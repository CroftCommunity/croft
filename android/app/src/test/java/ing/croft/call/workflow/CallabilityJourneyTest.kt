package ing.croft.call.workflow

import ing.croft.call.caps.Admit
import ing.croft.call.caps.Callability
import ing.croft.call.caps.CallerContext
import ing.croft.call.net.UrlHttp
import ing.croft.call.net.UrlHttpJson
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * User-flow journeys around callability (harness maturity pass): what the
 * caller SEES (the derived callability line) composed with what the mint
 * then DOES — including the disagreement cases, which are the point:
 * callability is a lookup-time derivation and the mint is the durable
 * gate, so the mint's answer wins and the UI must be built for that.
 */
@RunWith(RobolectricTestRunner::class)
class CallabilityJourneyTest {

    private val calleeDid = "did:plc:calleecalleecallee"
    private val callerDid = "did:plc:callercallercaller"
    private val strangerDid = "did:plc:strangerstranger"
    private val callerEndpoint =
        "14af214d000000000000000000000000000000000000000000000000000000ab"

    private fun fixture(): FixtureExchange = FixtureExchange().apply {
        accounts[calleeDid] = "callee.example"
        putEndpoint(calleeDid, "self", "ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00ee00")
        putGrant(
            calleeDid, "family",
            JSONObject().put(
                "matcher",
                JSONObject()
                    .put("type", "registeredCallers")
                    .put("dids", JSONArray().put(callerDid)),
            ),
        )
    }

    private suspend fun callability(fx: FixtureExchange, provenDid: String?) =
        Callability.resolve(
            Rewired(UrlHttp, fx.base),
            principal = "callee.example",
            context = CallerContext(provenDid = provenDid),
            now = 1_700_000_000_000L,
        )

    @Test
    fun `signed in and listed - callable, and the mint agrees`() = runBlocking {
        fixture().use { fx ->
            val state = callability(fx, provenDid = callerDid)
            assertTrue("expected Callable, got $state", state is Callability.State.Callable)
            assertEquals("family", (state as Callability.State.Callable).grant)

            val out = Admit.grantCall(
                UrlHttpJson, admitBase = fx.base, calleeDid = calleeDid,
                grant = state.grant, endpointId = callerEndpoint,
                proof = Admit.Proof.ServiceAuth("svc.jwt"),
            )
            assertTrue("the durable gate agrees: $out", out is Admit.Outcome.Minted)
        }
    }

    @Test
    fun `signed out - may-not-permit, honestly derived`() = runBlocking {
        fixture().use { fx ->
            assertEquals(Callability.State.MayNotPermit, callability(fx, provenDid = null))
        }
    }

    @Test
    fun `an expired policy hides the grant from callability AND revokes at mint`() = runBlocking {
        fixture().use { fx ->
            fx.records["$calleeDid/ing.croft.call.policy/past"] = JSONObject().put(
                "rules",
                JSONArray().put(JSONObject().put("type", "expires").put("at", "2020-01-01T00:00:00Z")),
            )
            fx.putGrant(
                calleeDid, "lapsed",
                fx.ticketGrant("s3cr3t").put("policyRef", "past"),
            )
            fx.deleteGrant(calleeDid, "family")

            // Lookup time: the lapsed grant is skipped — MayNotPermit.
            assertEquals(Callability.State.MayNotPermit, callability(fx, provenDid = callerDid))

            // Call time: the mint says revoked, the same fact, durably.
            val out = Admit.grantCall(
                UrlHttpJson, admitBase = fx.base, calleeDid = calleeDid,
                grant = "lapsed", endpointId = callerEndpoint,
                proof = Admit.Proof.Ticket("s3cr3t"),
            )
            assertEquals(Admit.Outcome.Refused(Admit.Refusal.CAP_REVOKED), out)
        }
    }

    @Test
    fun `callability can say yes and the mint still says no - the gate wins`() = runBlocking {
        // The disagreement case the UI must survive: derived state goes
        // stale the moment it is computed; the mint is fresh. Here the
        // grant vanishes between the look and the call.
        fixture().use { fx ->
            val state = callability(fx, provenDid = callerDid)
            assertTrue(state is Callability.State.Callable)

            fx.deleteGrant(calleeDid, "family")
            val out = Admit.grantCall(
                UrlHttpJson, admitBase = fx.base, calleeDid = calleeDid,
                grant = "family", endpointId = callerEndpoint,
                proof = Admit.Proof.ServiceAuth("svc.jwt"),
            )
            assertEquals(Admit.Outcome.Refused(Admit.Refusal.CAP_NOT_FOUND), out)
        }
    }

    @Test
    fun `an unlisted caller is refused at the gate whatever it presents`() = runBlocking {
        fixture().use { fx ->
            // Derivation for a stranger: not listed in any grant.
            assertEquals(Callability.State.MayNotPermit, callability(fx, provenDid = strangerDid))
            // The forced-refusal knob stands in for the real server's DID
            // check (the fixture's serviceAuth checks are structural; the
            // cryptographic refusal is croft-admit's, proven in M4d(b)).
            fx.nextMintRefusal = "cap_mismatch"
            val out = Admit.grantCall(
                UrlHttpJson, admitBase = fx.base, calleeDid = calleeDid,
                grant = "family", endpointId = callerEndpoint,
                proof = Admit.Proof.ServiceAuth("svc.jwt"),
            )
            assertEquals(Admit.Outcome.Refused(Admit.Refusal.CAP_MISMATCH), out)
        }
    }
}
