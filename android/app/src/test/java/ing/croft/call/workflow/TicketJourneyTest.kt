package ing.croft.call.workflow

import ing.croft.call.caps.Admit
import ing.croft.call.caps.Redeem
import ing.croft.call.net.UrlHttp
import ing.croft.call.net.UrlHttpJson
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * Workflow tests (M4): whole journeys over the REAL ports against the
 * [FixtureExchange] — invite link in, relay token out — not unit steps.
 * These exist because the admission story only means something end to end:
 * a redeem that works and a mint that works can still compose into a dial
 * that never gets a token.
 *
 * Journeys:
 *   1. The happy path: invite link → redeem (real HTTP: handle → DID doc →
 *      PDS records) → mint (real HTTP: /grantCall) → an opaque token, with
 *      the mint request carrying exactly what the redeem produced.
 *   2. Revocation: the callee deletes the grant AFTER a successful redeem;
 *      the mint — a fresh read on the admit side — refuses `cap_revoked`
 *      territory (`cap_not_found` here: the fixture, like a restarted
 *      admit, has no memory of the grant). Either way: NO token.
 *   3. A wrong secret dies at redeem, before any mint traffic exists.
 */
@RunWith(RobolectricTestRunner::class)
class TicketJourneyTest {

    private val calleeDid = "did:plc:calleecalleecallee"
    private val callerEndpoint =
        "14af214d000000000000000000000000000000000000000000000000000000ab"
    private val secret = "open sesame"

    private fun fixture(): FixtureExchange = FixtureExchange().apply {
        accounts[calleeDid] = "callee.example"
        putEndpoint(calleeDid, "self", "ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00")
        putGrant(calleeDid, "party", ticketGrant(secret))
    }

    private fun invite(fx: FixtureExchange, secret: String) =
        // The producer URL-encodes the fragment (contract §4; the page's
        // buildInviteLink does encodeURIComponent) — the parser decodes.
        "https://connect.croft.ing/redeem?repo=callee.example&grant=party#" +
            java.net.URLEncoder.encode(secret, "UTF-8").replace("+", "%20")

    @Test
    fun `invite link to relay token, end to end over real HTTP`() = runBlocking {
        fixture().use { fx ->
            val http = Rewired(UrlHttp, fx.base)

            val redeemed = Redeem.redeemTicket(http, invite(fx, secret), now = 1_700_000_000_000L)
            assertTrue(redeemed.endpointId.startsWith("ee00"))
            assertEquals(secret, redeemed.secret)

            val out = Admit.grantCall(
                UrlHttpJson,
                admitBase = fx.base,
                calleeDid = redeemed.did,
                grant = redeemed.grant,
                endpointId = callerEndpoint,
                proof = Admit.Proof.Ticket(redeemed.secret),
            )
            assertTrue("expected a token, got $out", out is Admit.Outcome.Minted)

            // The mint saw exactly what the redeem produced — no re-derivation.
            val sent = fx.mints.single()
            assertEquals(calleeDid, sent.getString("callee"))
            assertEquals("party", sent.getString("grant"))
            assertEquals(callerEndpoint, sent.getString("endpoint"))
        }
    }

    @Test
    fun `a grant deleted after redeem yields no token at mint time`() = runBlocking {
        fixture().use { fx ->
            val http = Rewired(UrlHttp, fx.base)
            val redeemed = Redeem.redeemTicket(http, invite(fx, secret), now = 1_700_000_000_000L)

            // The callee revokes: deletes the grant record. The redeem
            // already happened; only a FRESH read at mint catches this.
            fx.deleteGrant(calleeDid, "party")

            val out = Admit.grantCall(
                UrlHttpJson,
                admitBase = fx.base,
                calleeDid = redeemed.did,
                grant = redeemed.grant,
                endpointId = callerEndpoint,
                proof = Admit.Proof.Ticket(redeemed.secret),
            )
            assertEquals(Admit.Outcome.Refused(Admit.Refusal.CAP_NOT_FOUND), out)
        }
    }

    @Test
    fun `a wrong secret dies at redeem and no mint traffic ever exists`() {
        fixture().use { fx ->
            val http = Rewired(UrlHttp, fx.base)
            assertThrows(IllegalStateException::class.java) {
                runBlocking {
                    Redeem.redeemTicket(http, invite(fx, "wrong"), now = 1_700_000_000_000L)
                }
            }
            assertTrue("no /grantCall may have been attempted", fx.mints.isEmpty())
        }
    }
}
