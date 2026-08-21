package ing.croft.call.workflow

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import ing.croft.call.identity.AuthManager
import ing.croft.call.net.UrlHttp
import ing.croft.call.net.UrlHttpForm
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * The session journey (harness maturity pass, 2026-08-20): the whole OAuth
 * arc — discovery → PAR → browser hand-off → redirect → code exchange →
 * staleness → single-use refresh rotation — over REAL sockets against the
 * fixture authorization server, with the RFC 9449 nonce dance live on both
 * the PAR and token endpoints. Until this test, the dance had only ever
 * run against in-memory fakes (AuthManagerTest) or the real bsky.social
 * entryway (a phone, a human, and a browser); this is the first place the
 * shipped ports walk it unattended.
 *
 * It also pins O2 into the harness: the PAR request must ask for
 * `atproto transition:generic` — the scope getServiceAuth needs — so a
 * regression to the identity-only scope fails HERE, not on a device.
 */
@RunWith(RobolectricTestRunner::class)
class SessionJourneyTest {

    private val callerDid = "did:plc:callercallercaller"

    @Test
    fun `sign-in, staleness, and rotation — the whole session arc over real sockets`() {
        FixtureExchange().use { fx ->
            fx.accounts[callerDid] = "caller.example"

            var now = 1_755_400_000_000L
            val opened = mutableListOf<String>()
            val prefs = ApplicationProvider.getApplicationContext<Context>()
                .getSharedPreferences("session-journey", Context.MODE_PRIVATE)
            val auth = AuthManager(
                prefs,
                http = Rewired(UrlHttp, fx.base),
                form = UrlHttpForm,
                openUrl = { opened += it },
                nowMs = { now },
            )

            runBlocking {
                // Sign in: discovery + PAR (nonce-danced) land a browser URL.
                auth.signIn("caller.example")
                val browserUrl = opened.single()
                assertTrue(browserUrl.startsWith("${fx.base}/oauth/authorize?"))

                // O2, pinned: the auth request carries the mint-capable scope.
                val par = fx.parRequests.last()
                assertEquals("atproto transition:generic", par["scope"])

                // The browser round-trip returns; the code exchanges.
                auth.onRedirect("ing.croft.connect:/oauth?code=c-1&state=${par["state"]}")
                assertEquals(callerDid, auth.provenDid.value)

                // Fresh: served from the store.
                assertEquals("fx-at-1", auth.freshAccessToken())

                // Stale: one rotation, and the NEW pair is durable.
                now += 1_900_000
                assertEquals("fx-at-2", auth.freshAccessToken())
                now += 10_000
                assertEquals("fx-at-2", auth.freshAccessToken())
            }
        }
    }
}
