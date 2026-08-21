package ing.croft.call.workflow

import ing.croft.call.Callee
import ing.croft.call.DialAdmission
import ing.croft.call.caps.Admit
import ing.croft.call.net.UrlHttpJson
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * The dial-composition journey (M4c): plan → mint → action, composed over
 * the real ports — the exact pipeline `MainViewModel.dialCallee` runs,
 * minus the native peer. The posture rows are the ones a device test
 * would otherwise have to reproduce by hand:
 *
 *   ticket in hand  → minted → Dial(token)
 *   grant revoked   → refused → Refuse, and NOTHING would dial
 *   admit down      → outage  → Dial(tokenless) with the note
 *   signed out, no secret → tokenless nudge with ZERO mint traffic
 */
@RunWith(RobolectricTestRunner::class)
class DialCompositionJourneyTest {

    private val calleeDid = "did:plc:calleecalleecallee"
    private val callerEndpoint =
        "14af214d000000000000000000000000000000000000000000000000000000ab"

    private fun fixture(): FixtureExchange = FixtureExchange().apply {
        accounts[calleeDid] = "callee.example"
        putEndpoint(calleeDid, "self", "ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00")
        putGrant(calleeDid, "party", ticketGrant("open sesame"))
    }

    private fun card(secret: String?) = Callee(
        endpointId = "ee00", relayUrl = null, handle = null,
        did = calleeDid, device = null, grant = "party", ticketSecret = secret,
    )

    private suspend fun run(
        fx: FixtureExchange,
        callee: Callee,
        signedIn: Boolean,
    ): DialAdmission.Action? {
        return when (val plan = DialAdmission.plan(callee, signedIn)) {
            is DialAdmission.Plan.DialTokenless ->
                DialAdmission.Action.Dial(authToken = null, note = plan.note)
            is DialAdmission.Plan.Mint -> {
                val proof = when (val p = plan.proof) {
                    is DialAdmission.ProofSource.Ticket -> Admit.Proof.Ticket(p.secret)
                    DialAdmission.ProofSource.ServiceAuth -> Admit.Proof.ServiceAuth("svc.jwt")
                }
                DialAdmission.action(
                    Admit.grantCall(
                        UrlHttpJson, admitBase = fx.base, calleeDid = plan.calleeDid,
                        grant = plan.grant, endpointId = callerEndpoint, proof = proof,
                    ),
                )
            }
        }
    }

    @Test
    fun `ticket in hand dials with a real minted token`() = runBlocking {
        fixture().use { fx ->
            val action = run(fx, card(secret = "open sesame"), signedIn = false)
            assertTrue("expected Dial, got $action", action is DialAdmission.Action.Dial)
            val dial = action as DialAdmission.Action.Dial
            assertTrue("a real token from the mint", !dial.authToken.isNullOrEmpty())
            assertEquals(null, dial.note)
        }
    }

    @Test
    fun `a revoked grant refuses and nothing dials`() = runBlocking {
        fixture().use { fx ->
            fx.deleteGrant(calleeDid, "party")
            val action = run(fx, card(secret = "open sesame"), signedIn = false)
            assertEquals(
                DialAdmission.Action.Refuse("this invite no longer exists"),
                action,
            )
        }
    }

    @Test
    fun `an admit outage dials tokenless and says so`() = runBlocking {
        fixture().use { fx ->
            fx.nextMintRefusal = "unavailable"
            val action = run(fx, card(secret = "open sesame"), signedIn = false)
            assertEquals(
                DialAdmission.Action.Dial(
                    authToken = null,
                    note = "admission service unreachable — dialing without a token",
                ),
                action,
            )
        }
    }

    @Test
    fun `signed out with no secret nudges and never touches the mint`() = runBlocking {
        fixture().use { fx ->
            val action = run(fx, card(secret = null), signedIn = false)
            assertEquals(
                DialAdmission.Action.Dial(
                    authToken = null,
                    note = "sign in to present this grant",
                ),
                action,
            )
            assertTrue("zero mint traffic", fx.mints.isEmpty())
        }
    }
}
