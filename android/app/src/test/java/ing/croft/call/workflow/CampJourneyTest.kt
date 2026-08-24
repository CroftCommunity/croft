package ing.croft.call.workflow

import ing.croft.call.CampAdmission
import ing.croft.call.caps.Admit
import ing.croft.call.caps.Dpop
import ing.croft.call.caps.ServiceAuth
import ing.croft.call.net.UrlHttpGet
import ing.croft.call.net.UrlHttpJson
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * The camp journey (M4e, O1): the callee's own pass to camp on an enforce
 * relay, over real sockets against the fixture — the session's service-auth
 * proof (`lxm = ing.croft.relay.campToken`) rides to `/campToken`, and the
 * pass comes back with the wire's `expiresIn` so the client can cache the
 * opaque token (the token is the cache).
 *
 * The revocation story this file exists for: unpublishing the device's
 * endpoint record makes the NEXT camp mint refuse `endpoint_unbound` — and
 * the composed posture camps tokenless with words, never silently.
 */
@RunWith(RobolectricTestRunner::class)
class CampJourneyTest {

    private val calleeDid = "did:plc:calleecalleecallee"
    private val homeEndpoint =
        "cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33"
    private val now = 1_700_000_000_000L

    private suspend fun campProof(fx: FixtureExchange): String = ServiceAuth.fetch(
        UrlHttpGet,
        pdsBase = fx.base,
        accessToken = "access-tok",
        keyPair = Dpop.generateKeyPair(),
        aud = "did:web:admit.croft.ing",
        lxm = "ing.croft.relay.campToken",
        jti = "camp-journey-jti",
        nowMs = now,
    )

    @Test
    fun `session to camp proof to camping pass — then unpublish revokes the next mint`() =
        runBlocking {
            FixtureExchange().use { fx ->
                fx.accounts[calleeDid] = "callee.example"
                fx.putEndpoint(calleeDid, "home", homeEndpoint)

                val jwt = campProof(fx)
                val out = Admit.campToken(
                    UrlHttpJson,
                    admitBase = fx.base,
                    endpointId = homeEndpoint,
                    serviceAuthJwt = jwt,
                )
                val minted = out as Admit.CampOutcome.Minted
                assertEquals(43_200L, minted.expiresInSecs)
                assertEquals(
                    jwt,
                    fx.camps.single().getJSONObject("proof").getString("serviceAuth"),
                )

                // The composed posture: mint -> camp WITH the pass, cached to
                // the wire's expiry.
                val action = CampAdmission.action(out, nowMs = now)
                val camp = action as CampAdmission.Action.Camp
                assertEquals(minted.token, camp.authToken)
                assertEquals(now + 43_200 * 1000, camp.pass.expiresAtMillis)

                // The cached pass is reused without a network hit.
                assertEquals(
                    CampAdmission.Plan.UseCached(minted.token),
                    CampAdmission.plan(signedIn = true, cached = camp.pass, nowMs = now),
                )
                assertEquals(1, fx.camps.size)

                // Unpublish the device: the NEXT mint refuses, with words —
                // and still camps tokenless (reception must not die quietly).
                fx.records.remove("$calleeDid/ing.croft.iroh.endpoint/home")
                val refused = Admit.campToken(
                    UrlHttpJson,
                    admitBase = fx.base,
                    endpointId = homeEndpoint,
                    serviceAuthJwt = campProof(fx),
                )
                assertEquals(
                    Admit.CampOutcome.Refused(Admit.CampRefusal.ENDPOINT_UNBOUND),
                    refused,
                )
                val tokenless =
                    CampAdmission.action(refused, nowMs = now) as CampAdmission.Action.CampTokenless
                assertEquals("this device is not published by your account", tokenless.note)
            }
        }

    @Test
    fun `an admit outage camps tokenless with the availability note`() = runBlocking {
        FixtureExchange().use { fx ->
            fx.accounts[calleeDid] = "callee.example"
            fx.putEndpoint(calleeDid, "home", homeEndpoint)
            fx.nextCampRefusal = "unavailable"

            val out = Admit.campToken(
                UrlHttpJson,
                admitBase = fx.base,
                endpointId = homeEndpoint,
                serviceAuthJwt = campProof(fx),
            )
            assertEquals(Admit.CampOutcome.Unavailable, out)
            val tokenless =
                CampAdmission.action(out, nowMs = now) as CampAdmission.Action.CampTokenless
            assertTrue(tokenless.note!!.contains("unreachable"))
        }
    }
}
