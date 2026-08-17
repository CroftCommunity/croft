package ing.croft.call.caps

import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import java.security.MessageDigest
import java.util.Base64

/**
 * The atproto OAuth dance (M3 Phase 2) over canned routes built from the
 * Phase 0 D2 fixtures — the real bsky.social entryway metadata, saved
 * verbatim. Everything except the human browser tap is covered here:
 * discovery → PAR → authorize URL → redirect parse → code exchange →
 * refresh, plus the DPoP nonce retry (bounded) and fail-closed paths.
 * Runs under Robolectric for a real org.json (suite convention).
 */
@RunWith(RobolectricTestRunner::class)
class OAuthFlowTest {

    private val pdsUrl = "https://fibercap.us-west.host.bsky.network"
    private val clientId = "https://connect.croft.ing/oauth-client-metadata.json"
    private val redirectUri = "ing.croft.connect:/oauth"

    private fun fixture(name: String): String =
        javaClass.getResourceAsStream("/oauth/$name")!!.readBytes().decodeToString()

    private class FakeHttp(private val routes: Map<String, String>) : Http {
        override suspend fun getJson(url: String): String =
            routes.entries.firstOrNull { url.contains(it.key) }?.value
                ?: throw IllegalStateException("unrouted GET $url")
    }

    /** Scripted POST port: responses served in order; every request recorded. */
    private class FakeForm(private val script: MutableList<FormResponse>) : HttpForm {
        data class Sent(val url: String, val fields: Map<String, String>, val headers: Map<String, String>)
        val sent = mutableListOf<Sent>()
        override suspend fun postForm(
            url: String, fields: Map<String, String>, headers: Map<String, String>,
        ): FormResponse {
            sent += Sent(url, fields, headers)
            check(script.isNotEmpty()) { "unscripted POST $url" }
            return script.removeAt(0)
        }
    }

    private fun discoveryHttp() = FakeHttp(
        mapOf(
            "$pdsUrl/.well-known/oauth-protected-resource" to fixture("oauth-protected-resource.json"),
            "https://bsky.social/.well-known/oauth-authorization-server" to fixture("oauth-authorization-server.json"),
        ),
    )

    private fun ok(body: String, vararg headers: Pair<String, String>) =
        FormResponse(200, headers.toMap(), body)

    private fun tokenBody(did: String = "did:plc:l5xigmplwu7eyxjobjr23iza") =
        """{"access_token":"at-1","token_type":"DPoP","expires_in":1799,
            "refresh_token":"rt-1","scope":"atproto","sub":"$did"}"""

    // --- discovery ---

    @Test
    fun `discovery chains protected-resource to the auth server metadata`(): Unit = runBlocking {
        val server = OAuthFlow.discover(discoveryHttp(), pdsUrl)
        assertEquals("https://bsky.social", server.issuer)
        assertEquals("https://bsky.social/oauth/par", server.parEndpoint)
        assertEquals("https://bsky.social/oauth/authorize", server.authorizeEndpoint)
        assertEquals("https://bsky.social/oauth/token", server.tokenEndpoint)
    }

    @Test
    fun `discovery fails closed when the PDS names no auth server`(): Unit = runBlocking {
        val http = FakeHttp(mapOf("oauth-protected-resource" to """{"resource":"x"}"""))
        assertThrows(IllegalStateException::class.java) {
            runBlocking { OAuthFlow.discover(http, pdsUrl) }
        }
    }

    @Test
    fun `discovery fails closed on issuer mismatch`(): Unit = runBlocking {
        val http = FakeHttp(
            mapOf(
                "oauth-protected-resource" to fixture("oauth-protected-resource.json"),
                "oauth-authorization-server" to
                    fixture("oauth-authorization-server.json").replace(
                        "\"issuer\":\"https://bsky.social\"", "\"issuer\":\"https://evil.example\"",
                    ),
            ),
        )
        assertThrows(IllegalStateException::class.java) {
            runBlocking { OAuthFlow.discover(http, pdsUrl) }
        }
    }

    // --- PAR ---

    @Test
    fun `PAR carries client_id, S256 PKCE, state, redirect, scope, login hint, and a DPoP proof`(): Unit =
        runBlocking {
            val server = OAuthFlow.discover(discoveryHttp(), pdsUrl)
            val form = FakeForm(mutableListOf(ok("""{"request_uri":"urn:ietf:params:oauth:request_uri:r1","expires_in":60}""")))
            val key = Dpop.generateKeyPair()
            val requestUri = OAuthFlow.pushAuthRequest(
                form, server, clientId = clientId, redirectUri = redirectUri, scope = "atproto",
                loginHint = "bobzmudacroft.bsky.social", state = "st-1", pkceVerifier = "v".repeat(43),
                keyPair = key, jti = "par-jti", nowMs = 1_755_400_000_000,
            )
            assertEquals("urn:ietf:params:oauth:request_uri:r1", requestUri)
            val sent = form.sent.single()
            assertEquals("https://bsky.social/oauth/par", sent.url)
            assertEquals(clientId, sent.fields["client_id"])
            assertEquals("code", sent.fields["response_type"])
            assertEquals(redirectUri, sent.fields["redirect_uri"])
            assertEquals("atproto", sent.fields["scope"])
            assertEquals("st-1", sent.fields["state"])
            assertEquals("bobzmudacroft.bsky.social", sent.fields["login_hint"])
            assertEquals("S256", sent.fields["code_challenge_method"])
            val expected = Base64.getUrlEncoder().withoutPadding().encodeToString(
                MessageDigest.getInstance("SHA-256").digest("v".repeat(43).toByteArray()),
            )
            assertEquals(expected, sent.fields["code_challenge"])
            assertTrue(sent.headers.containsKey("DPoP"))
        }

    // --- authorize URL + redirect ---

    @Test
    fun `authorize URL points at the endpoint with client_id and request_uri encoded`(): Unit = runBlocking {
        val server = OAuthFlow.discover(discoveryHttp(), pdsUrl)
        val url = OAuthFlow.authorizeUrl(server, clientId, "urn:ietf:params:oauth:request_uri:r1")
        assertTrue(url.startsWith("https://bsky.social/oauth/authorize?"))
        assertTrue(url.contains("request_uri=urn%3Aietf%3Aparams%3Aoauth%3Arequest_uri%3Ar1"))
        assertTrue(url.contains("client_id=https%3A%2F%2Fconnect.croft.ing%2Foauth-client-metadata.json"))
    }

    @Test
    fun `redirect parse returns the code when state matches and fails closed otherwise`() {
        val code = OAuthFlow.parseRedirect("$redirectUri?code=c-1&state=st-1&iss=https%3A%2F%2Fbsky.social", expectedState = "st-1")
        assertEquals("c-1", code)
        assertThrows(IllegalStateException::class.java) {
            OAuthFlow.parseRedirect("$redirectUri?code=c-1&state=WRONG", expectedState = "st-1")
        }
        assertThrows(IllegalStateException::class.java) {
            OAuthFlow.parseRedirect("$redirectUri?error=access_denied&state=st-1", expectedState = "st-1")
        }
    }

    // --- token exchange (the Phase 1 wiring test lives here) ---

    @Test
    fun `token exchange attaches a DPoP proof bound to the token endpoint`(): Unit = runBlocking {
        val server = OAuthFlow.discover(discoveryHttp(), pdsUrl)
        val form = FakeForm(mutableListOf(ok(tokenBody())))
        val key = Dpop.generateKeyPair()
        val tokens = OAuthFlow.exchangeCode(
            form, server, clientId = clientId, redirectUri = redirectUri, code = "c-1",
            pkceVerifier = "v".repeat(43), keyPair = key, jti = "tok-jti", nowMs = 1_755_400_000_000,
        )
        assertEquals("did:plc:l5xigmplwu7eyxjobjr23iza", tokens.did)
        assertEquals("at-1", tokens.accessToken)
        assertEquals("rt-1", tokens.refreshToken)
        val sent = form.sent.single()
        assertEquals("authorization_code", sent.fields["grant_type"])
        assertEquals("c-1", sent.fields["code"])
        assertEquals("v".repeat(43), sent.fields["code_verifier"])
        // The DPoP header is a real proof: claims bind POST + the token URL.
        val proof = sent.headers.getValue("DPoP")
        val claims = JSONObject(String(Base64.getUrlDecoder().decode(proof.split(".")[1])))
        assertEquals("POST", claims.getString("htm"))
        assertEquals("https://bsky.social/oauth/token", claims.getString("htu"))
    }

    @Test
    fun `a DPoP nonce challenge is retried once with the nonce and a fresh jti`(): Unit = runBlocking {
        val server = OAuthFlow.discover(discoveryHttp(), pdsUrl)
        val form = FakeForm(
            mutableListOf(
                FormResponse(400, mapOf("DPoP-Nonce" to "n-1"), """{"error":"use_dpop_nonce"}"""),
                ok(tokenBody()),
            ),
        )
        val tokens = OAuthFlow.exchangeCode(
            form, server, clientId = clientId, redirectUri = redirectUri, code = "c-1",
            pkceVerifier = "v".repeat(43), keyPair = Dpop.generateKeyPair(), jti = "j", nowMs = 0,
        )
        assertEquals("at-1", tokens.accessToken)
        assertEquals(2, form.sent.size)
        val d = Base64.getUrlDecoder()
        val first = JSONObject(String(d.decode(form.sent[0].headers.getValue("DPoP").split(".")[1])))
        val second = JSONObject(String(d.decode(form.sent[1].headers.getValue("DPoP").split(".")[1])))
        assertTrue(!first.has("nonce"))
        assertEquals("n-1", second.getString("nonce"))
        assertTrue(first.getString("jti") != second.getString("jti"))
    }

    @Test
    fun `the nonce retry is bounded - a second consecutive 400 surfaces`(): Unit = runBlocking {
        val server = OAuthFlow.discover(discoveryHttp(), pdsUrl)
        val form = FakeForm(
            mutableListOf(
                FormResponse(400, mapOf("DPoP-Nonce" to "n-1"), """{"error":"use_dpop_nonce"}"""),
                FormResponse(400, mapOf("DPoP-Nonce" to "n-2"), """{"error":"use_dpop_nonce"}"""),
            ),
        )
        assertThrows(IllegalStateException::class.java) {
            runBlocking {
                OAuthFlow.exchangeCode(
                    form, server, clientId = clientId, redirectUri = redirectUri, code = "c",
                    pkceVerifier = "v".repeat(43), keyPair = Dpop.generateKeyPair(), jti = "j", nowMs = 0,
                )
            }
        }
        assertEquals(2, form.sent.size) // exactly one retry, not a loop
    }

    @Test
    fun `token responses missing sub or access_token fail closed`(): Unit = runBlocking {
        val server = OAuthFlow.discover(discoveryHttp(), pdsUrl)
        for (body in listOf(
            """{"access_token":"at","token_type":"DPoP","refresh_token":"rt"}""", // no sub
            """{"token_type":"DPoP","refresh_token":"rt","sub":"did:plc:x"}""",   // no access_token
        )) {
            val form = FakeForm(mutableListOf(ok(body)))
            assertThrows(IllegalStateException::class.java) {
                runBlocking {
                    OAuthFlow.exchangeCode(
                        form, server, clientId = clientId, redirectUri = redirectUri, code = "c",
                        pkceVerifier = "v".repeat(43), keyPair = Dpop.generateKeyPair(), jti = "j", nowMs = 0,
                    )
                }
            }
        }
    }

    // --- refresh ---

    @Test
    fun `refresh posts the refresh token with a DPoP proof and returns rotated tokens`(): Unit = runBlocking {
        val server = OAuthFlow.discover(discoveryHttp(), pdsUrl)
        val form = FakeForm(
            mutableListOf(ok(tokenBody().replace("at-1", "at-2").replace("rt-1", "rt-2"))),
        )
        val tokens = OAuthFlow.refresh(
            form, server, clientId = clientId, refreshToken = "rt-1",
            keyPair = Dpop.generateKeyPair(), jti = "r-jti", nowMs = 0,
        )
        assertEquals("at-2", tokens.accessToken)
        assertEquals("rt-2", tokens.refreshToken)
        val sent = form.sent.single()
        assertEquals("refresh_token", sent.fields["grant_type"])
        assertEquals("rt-1", sent.fields["refresh_token"])
        assertEquals(clientId, sent.fields["client_id"])
        assertTrue(sent.headers.containsKey("DPoP"))
    }

    // --- the whole chain (Phase 2 wiring test) ---

    @Test
    fun `the full dance runs discovery to tokens over canned routes`(): Unit = runBlocking {
        val server = OAuthFlow.discover(discoveryHttp(), pdsUrl)
        val form = FakeForm(
            mutableListOf(
                ok("""{"request_uri":"urn:r1","expires_in":60}"""),
                ok(tokenBody()),
            ),
        )
        val key = Dpop.generateKeyPair()
        val requestUri = OAuthFlow.pushAuthRequest(
            form, server, clientId = clientId, redirectUri = redirectUri, scope = "atproto",
            loginHint = "bobzmudacroft.bsky.social", state = "st-9", pkceVerifier = "w".repeat(43),
            keyPair = key, jti = "p", nowMs = 0,
        )
        val authorizeUrl = OAuthFlow.authorizeUrl(server, clientId, requestUri)
        assertTrue(authorizeUrl.contains("request_uri=urn%3Ar1"))
        // The human taps approve in a browser; the app gets the redirect back:
        val code = OAuthFlow.parseRedirect("$redirectUri?code=c-9&state=st-9", expectedState = "st-9")
        val tokens = OAuthFlow.exchangeCode(
            form, server, clientId = clientId, redirectUri = redirectUri, code = code,
            pkceVerifier = "w".repeat(43), keyPair = key, jti = "t", nowMs = 0,
        )
        assertEquals("did:plc:l5xigmplwu7eyxjobjr23iza", tokens.did)
    }
}
