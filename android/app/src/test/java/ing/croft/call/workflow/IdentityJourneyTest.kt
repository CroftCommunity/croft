package ing.croft.call.workflow

import ing.croft.call.caps.Admit
import ing.croft.call.caps.Dpop
import ing.croft.call.caps.ServiceAuth
import ing.croft.call.net.UrlHttpGet
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
 * The identity journey (M4b): a signed-in caller turns its OAuth session
 * into a service-auth proof at its own PDS — surviving the RFC 9449
 * resource-server nonce dance over real sockets — and presents that proof
 * to the mint for a registeredCallers grant. Real ports throughout
 * ([UrlHttpGet], [UrlHttpJson]); the fixture's checks are structural (the
 * cryptographic halves belong to the real PDS and croft-admit — plan
 * chunk M4d(b)).
 */
@RunWith(RobolectricTestRunner::class)
class IdentityJourneyTest {

    private val calleeDid = "did:plc:calleecalleecallee"
    private val callerDid = "did:plc:callercallercaller"
    private val callerEndpoint =
        "14af214d000000000000000000000000000000000000000000000000000000ab"

    @Test
    fun `oauth session to service-auth proof to relay token`() = runBlocking {
        FixtureExchange().use { fx ->
            fx.accounts[calleeDid] = "callee.example"
            fx.putGrant(
                calleeDid, "family",
                JSONObject().put(
                    "matcher",
                    JSONObject()
                        .put("type", "registeredCallers")
                        .put("dids", JSONArray().put(callerDid)),
                ),
            )

            // The proof: getServiceAuth at the caller's PDS (the fixture),
            // DPoP-authed, first attempt bounced for the nonce.
            val jwt = ServiceAuth.fetch(
                UrlHttpGet,
                pdsBase = fx.base,
                accessToken = "access-tok",
                keyPair = Dpop.generateKeyPair(),
                aud = "did:web:admit.croft.ing",
                lxm = "ing.croft.relay.grantCall",
                jti = "journey-jti",
                nowMs = 1_700_000_000_000L,
            )
            assertEquals("svc-jwt-fixture", jwt)

            // The mint: the proof rides as serviceAuth; a token comes back.
            val out = Admit.grantCall(
                UrlHttpJson,
                admitBase = fx.base,
                calleeDid = calleeDid,
                grant = "family",
                endpointId = callerEndpoint,
                proof = Admit.Proof.ServiceAuth(jwt),
            )
            assertTrue("expected a token, got $out", out is Admit.Outcome.Minted)
            assertEquals(jwt, fx.mints.single().getJSONObject("proof").getString("serviceAuth"))
        }
    }
}
