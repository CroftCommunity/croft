package ing.croft.call.identity

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.test.core.app.ApplicationProvider
import ing.croft.call.caps.FormResponse
import ing.croft.call.caps.Http
import ing.croft.call.caps.HttpForm
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * AuthManager wiring (M3 Phase 3a): sign-in launches the browser at the
 * authorize URL, the redirect intent drives the code exchange to a stored
 * DID, the DID survives a new instance over the same prefs (process
 * restart), and sign-out clears it. Canned routes reuse the Phase 0
 * fixtures; prefs are plain SharedPreferences here (EncryptedShared-
 * Preferences needs AndroidKeyStore, absent under Robolectric — the
 * production factory is exercised on-device).
 */
@RunWith(RobolectricTestRunner::class)
class AuthManagerTest {

    private val did = "did:plc:l5xigmplwu7eyxjobjr23iza"
    private val pds = "https://fibercap.us-west.host.bsky.network"

    private fun fixture(name: String): String =
        javaClass.getResourceAsStream("/oauth/$name")!!.readBytes().decodeToString()

    private class FakeHttp(private val routes: Map<String, String>) : Http {
        override suspend fun getJson(url: String): String =
            routes.entries.firstOrNull { url.contains(it.key) }?.value
                ?: throw IllegalStateException("unrouted GET $url")
    }

    private class FakeForm(private val script: MutableList<FormResponse>) : HttpForm {
        data class Sent(val url: String, val fields: Map<String, String>)
        val sent = mutableListOf<Sent>()
        override suspend fun postForm(
            url: String, fields: Map<String, String>, headers: Map<String, String>,
        ): FormResponse {
            sent += Sent(url, fields)
            return script.removeAt(0)
        }
    }

    private fun http() = FakeHttp(
        mapOf(
            "resolveHandle" to """{"did":"$did"}""",
            "plc.directory" to
                """{"service":[{"id":"#atproto_pds","type":"AtprotoPersonalDataServer","serviceEndpoint":"$pds"}]}""",
            "$pds/.well-known/oauth-protected-resource" to fixture("oauth-protected-resource.json"),
            "bsky.social/.well-known/oauth-authorization-server" to fixture("oauth-authorization-server.json"),
        ),
    )

    private fun form() = FakeForm(
        mutableListOf(
            FormResponse(200, emptyMap(), """{"request_uri":"urn:r1","expires_in":60}"""),
            FormResponse(
                200, emptyMap(),
                """{"access_token":"at-1","token_type":"DPoP","expires_in":1799,
                    "refresh_token":"rt-1","scope":"atproto","sub":"$did"}""",
            ),
        ),
    )

    private fun prefs() = ApplicationProvider.getApplicationContext<Context>()
        .getSharedPreferences("auth-test", Context.MODE_PRIVATE)

    private fun manager(
        form: FakeForm,
        opened: MutableList<String> = mutableListOf(),
        nowMs: () -> Long = { 1_755_400_000_000 },
    ) = AuthManager(prefs(), http(), form, openUrl = { opened += it }, nowMs = nowMs)

    /** Sign in against the scripted form so the session is stored. */
    private suspend fun signedIn(form: FakeForm, nowMs: () -> Long = { 1_755_400_000_000 }): AuthManager {
        val auth = manager(form, nowMs = nowMs)
        auth.signIn("bobzmudacroft.bsky.social")
        val state = form.sent[0].fields.getValue("state")
        auth.onRedirect("ing.croft.connect:/oauth?code=c-1&state=$state")
        return auth
    }

    @Test
    fun `serviceAuthProof mints at the STORED pds with a fresh token`(): Unit = runBlocking {
        val form = form()
        val auth = signedIn(form)
        val gets = mutableListOf<Pair<String, Map<String, String>>>()
        val jwt = auth.serviceAuthProof(
            http = { url, headers ->
                gets += url to headers
                ing.croft.call.caps.FormResponse(200, emptyMap(), """{"token":"svc.jwt"}""")
            },
            aud = "did:web:admit.croft.ing",
            lxm = "ing.croft.relay.grantCall",
        )
        assertEquals("svc.jwt", jwt)
        val (url, headers) = gets.single()
        // The PDS resolved at sign-in is where the proof mints — stored, not
        // re-resolved (the session is bound to it).
        assertTrue(url.startsWith("$pds/xrpc/com.atproto.server.getServiceAuth?"))
        assertEquals("DPoP at-1", headers["Authorization"])
    }

    @Test
    fun `a fresh access token is served from the store with no network`(): Unit = runBlocking {
        val form = form()
        val auth = signedIn(form)
        val posts = form.sent.size
        assertEquals("at-1", auth.freshAccessToken())
        assertEquals("no token-endpoint traffic for a fresh token", posts, form.sent.size)
    }

    @Test
    fun `a stale token refreshes once and the rotated pair is durable`(): Unit = runBlocking {
        // Sign in at t0 (expires_in 1799s), then move the clock past expiry.
        // The script: PAR, exchange, then ONE refresh.
        val scripted = FakeForm(
            mutableListOf(
                FormResponse(200, emptyMap(), """{"request_uri":"urn:r1","expires_in":60}"""),
                FormResponse(
                    200, emptyMap(),
                    """{"access_token":"at-1","token_type":"DPoP","expires_in":1799,
                        "refresh_token":"rt-1","scope":"atproto","sub":"$did"}""",
                ),
                FormResponse(
                    200, emptyMap(),
                    """{"access_token":"at-2","token_type":"DPoP","expires_in":1799,
                        "refresh_token":"rt-2","scope":"atproto","sub":"$did"}""",
                ),
            ),
        )
        var now = 1_755_400_000_000L
        val auth = signedIn(scripted, nowMs = { now })
        now += 1_900_000 // past the 1799s lifetime

        assertEquals("at-2", auth.freshAccessToken())
        // The refresh was a rotation grant against the token endpoint...
        val refresh = scripted.sent.last()
        assertEquals("refresh_token", refresh.fields["grant_type"])
        assertEquals("rt-1", refresh.fields["refresh_token"])
        // ...and the NEW pair is durable: a fresh call needs no more traffic,
        // even from a new instance over the same prefs.
        val posts = scripted.sent.size
        assertEquals("at-2", auth.freshAccessToken())
        assertEquals(posts, scripted.sent.size)
    }

    @Test
    fun `sign-in then redirect intent lands a stored proven DID`(): Unit = runBlocking {
        val opened = mutableListOf<String>()
        val form = form()
        val auth = manager(form, opened)
        assertNull(auth.provenDid.value)

        auth.signIn("bobzmudacroft.bsky.social")
        val browserUrl = opened.single()
        assertTrue(browserUrl.startsWith("https://bsky.social/oauth/authorize?"))
        assertTrue(browserUrl.contains("request_uri=urn%3Ar1"))

        // The browser round-trip comes back as an intent; the state the
        // engine sent to PAR must match the one on the redirect.
        val state = form.sent[0].fields.getValue("state")
        val intent = Intent(Intent.ACTION_VIEW, Uri.parse("ing.croft.connect:/oauth?code=c-1&state=$state"))
        assertTrue(AuthManager.isOAuthRedirect(intent.data.toString()))
        auth.onRedirect(intent.data.toString())

        assertEquals(did, auth.provenDid.value)
    }

    @Test
    fun `the proven DID survives a new instance over the same prefs`(): Unit = runBlocking {
        val form = form()
        val auth = manager(form)
        auth.signIn("bobzmudacroft.bsky.social")
        auth.onRedirect("ing.croft.connect:/oauth?code=c-1&state=" + form.sent[0].fields.getValue("state"))

        val reborn = manager(FakeForm(mutableListOf()))
        assertEquals(did, reborn.provenDid.value)
    }

    @Test
    fun `sign-out clears the identity durably`(): Unit = runBlocking {
        val form = form()
        val auth = manager(form)
        auth.signIn("bobzmudacroft.bsky.social")
        auth.onRedirect("ing.croft.connect:/oauth?code=c-1&state=" + form.sent[0].fields.getValue("state"))

        auth.signOut()
        assertNull(auth.provenDid.value)
        assertNull(manager(FakeForm(mutableListOf())).provenDid.value)
    }

    @Test
    fun `a redirect with a stale state is refused and leaves no identity`(): Unit = runBlocking {
        val form = form()
        val auth = manager(form)
        auth.signIn("bobzmudacroft.bsky.social")
        var thrown = false
        try {
            auth.onRedirect("ing.croft.connect:/oauth?code=c-1&state=NOT-THE-STATE")
        } catch (_: IllegalStateException) {
            thrown = true
        }
        assertTrue(thrown)
        assertNull(auth.provenDid.value)
    }

    @Test
    fun `only the reverse-domain scheme is recognized as an oauth redirect`() {
        assertTrue(AuthManager.isOAuthRedirect("ing.croft.connect:/oauth?code=x&state=y"))
        assertTrue(!AuthManager.isOAuthRedirect("croftcall://call?peer=x"))
        assertTrue(!AuthManager.isOAuthRedirect("https://connect.croft.ing/redeem?repo=x"))
    }
}
