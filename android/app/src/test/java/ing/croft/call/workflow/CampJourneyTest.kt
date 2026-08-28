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

    /**
     * The WHOLE workflow end to end, no canned session: OAuth sign-in
     * (discovery → PAR → redirect → exchange, nonce-danced) → the session's
     * own camp proof, method-bound to campToken — never grantCall — → the
     * pass, bound and cached → the cache rides reconnects free → the margin
     * forces a real re-mint over the wire.
     */
    @Test
    fun `oauth session to camping pass to expiry re-mint — the full arc`() {
        FixtureExchange().use { fx ->
            fx.accounts[calleeDid] = "callee.example"
            fx.putEndpoint(calleeDid, "home", homeEndpoint)

            var clock = now
            val opened = mutableListOf<String>()
            val prefs = androidx.test.core.app.ApplicationProvider
                .getApplicationContext<android.content.Context>()
                .getSharedPreferences("camp-journey", android.content.Context.MODE_PRIVATE)
            val auth = ing.croft.call.identity.AuthManager(
                prefs,
                http = Rewired(ing.croft.call.net.UrlHttp, fx.base),
                form = ing.croft.call.net.UrlHttpForm,
                openUrl = { opened += it },
                nowMs = { clock },
            )

            runBlocking {
                // The session, over real sockets — the same arc a phone walks.
                auth.signIn("callee.example")
                val state = fx.parRequests.last()["state"]
                auth.onRedirect("ing.croft.connect:/oauth?code=c-1&state=$state")
                assertEquals(calleeDid, auth.provenDid.value)

                // The camp proof comes from the SESSION, bound to campToken.
                val jwt = auth.serviceAuthProof(
                    UrlHttpGet, "did:web:admit.croft.ing", "ing.croft.relay.campToken",
                )
                assertEquals("ing.croft.relay.campToken", fx.serviceAuthLxms.last())

                val out = Admit.campToken(
                    UrlHttpJson, admitBase = fx.base,
                    endpointId = homeEndpoint, serviceAuthJwt = jwt,
                )
                val camp =
                    CampAdmission.action(out, nowMs = clock) as CampAdmission.Action.Camp
                assertEquals(1, fx.camps.size)

                // Reconnects ride the cache — no network.
                assertEquals(
                    CampAdmission.Plan.UseCached(camp.authToken),
                    CampAdmission.plan(signedIn = true, cached = camp.pass, nowMs = clock),
                )
                assertEquals(1, fx.camps.size)

                // Past the margin, the plan demands a REAL re-mint — and the
                // wire sees it.
                clock = camp.pass.expiresAtMillis - CampAdmission.REMINT_MARGIN_MILLIS
                assertEquals(
                    CampAdmission.Plan.Mint,
                    CampAdmission.plan(signedIn = true, cached = camp.pass, nowMs = clock),
                )
                val again = Admit.campToken(
                    UrlHttpJson, admitBase = fx.base,
                    endpointId = homeEndpoint,
                    serviceAuthJwt = auth.serviceAuthProof(
                        UrlHttpGet, "did:web:admit.croft.ing", "ing.croft.relay.campToken",
                    ),
                )
                assertTrue(again is Admit.CampOutcome.Minted)
                assertEquals(2, fx.camps.size)
            }
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

    /**
     * The RECOVERY arc, which is the one production actually hit
     * (2026-08-28, runbook §13 step 3): a device whose account has NEVER
     * published it is refused `endpoint_unbound` — the caller phone sat in
     * exactly this state through the whole first bake, camping tokenless
     * while looking fine — and publishing the record makes the very next
     * mint camp with a real pass. The existing arc above covers the
     * revocation direction; this covers the repair, which is what an
     * operator or a user actually does about it.
     */
    @Test
    fun `an unpublished device is refused until the record exists, then camps`() =
        runBlocking {
            FixtureExchange().use { fx ->
                fx.accounts[calleeDid] = "callee.example"
                // No putEndpoint: the account publishes nothing at all.

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

                // The repair: publish this device under the account.
                fx.putEndpoint(calleeDid, "home", homeEndpoint)

                val out = Admit.campToken(
                    UrlHttpJson,
                    admitBase = fx.base,
                    endpointId = homeEndpoint,
                    serviceAuthJwt = campProof(fx),
                )
                val camp = CampAdmission.action(out, nowMs = now) as CampAdmission.Action.Camp
                assertEquals((out as Admit.CampOutcome.Minted).token, camp.authToken)
            }
        }

    /**
     * Silence is the success signal — and it nearly read as failure during
     * the first production bake (runbook §13 results): a successful mint logs
     * nothing client-side, nothing at the admit, and nothing at the relay
     * until the connection closes. The posture must therefore carry NO note
     * on success, so that any note on screen means something actually went wrong.
     */
    @Test
    fun `a successful camp says nothing — a note would mean something is wrong`() =
        runBlocking {
            FixtureExchange().use { fx ->
                fx.accounts[calleeDid] = "callee.example"
                fx.putEndpoint(calleeDid, "home", homeEndpoint)

                val out = Admit.campToken(
                    UrlHttpJson,
                    admitBase = fx.base,
                    endpointId = homeEndpoint,
                    serviceAuthJwt = campProof(fx),
                )
                val camp = CampAdmission.action(out, nowMs = now)
                assertTrue(camp is CampAdmission.Action.Camp)
                // No words anywhere in the success path.
                assertEquals(null, CampAdmission.failureNote(kotlinx.coroutines.CancellationException()))
            }
        }

    /**
     * Holding a pass is NOT being camped, and the screen must not conflate
     * them. Measured on hardware 2026-08-28: the phone held a valid
     * production pass while the staging enforce listener refused every
     * attach (wrong signing key), and the honest line has to follow
     * reachability — `Endpoint.online()` — not possession of a token and not
     * the configured relay url.
     */
    @Test
    fun `a held pass is not a camped claim — a refused attach still reads NOT camped`() =
        runBlocking {
            FixtureExchange().use { fx ->
                fx.accounts[calleeDid] = "callee.example"
                fx.putEndpoint(calleeDid, "home", homeEndpoint)

                val out = Admit.campToken(
                    UrlHttpJson,
                    admitBase = fx.base,
                    endpointId = homeEndpoint,
                    serviceAuthJwt = campProof(fx),
                )
                val camp = CampAdmission.action(out, nowMs = now) as CampAdmission.Action.Camp
                assertTrue(camp.authToken.isNotEmpty())

                // The relay refuses the attach anyway: online() never resolves.
                val refusedLine = ing.croft.call.net.CampPresence.line(
                    ing.croft.call.net.CampPresence.attachedRelay(
                        online = false,
                        relayUrl = "https://relay.croft.ing:8444",
                    ),
                )
                assertEquals(
                    "ready — NOT camped on relay; calls cannot reach this device",
                    refusedLine,
                )
                // …and when the attach succeeds, the same pass reads camped.
                assertEquals(
                    "ready, camped on relay",
                    ing.croft.call.net.CampPresence.line(
                        ing.croft.call.net.CampPresence.attachedRelay(
                            online = true,
                            relayUrl = "https://relay.croft.ing:8443",
                        ),
                    ),
                )
            }
        }
}
