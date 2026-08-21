package ing.croft.call.caps

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import java.util.Base64

/**
 * The service-auth fetch (M4b): `com.atproto.server.getServiceAuth` on the
 * caller's OWN PDS, DPoP-authed — the caller proof D3 chose. The engine
 * builds the URL (aud + lxm encoded), presents the access token under the
 * `DPoP` authorization scheme with an ath-bound proof, honors the RFC 9449
 * resource-server nonce dance (retry once with the nonce and a fresh jti),
 * and returns the minted JWT verbatim — the client never parses it.
 */
@RunWith(RobolectricTestRunner::class)
class ServiceAuthTest {

    private class FakeGet(private val script: MutableList<FormResponse>) : HttpGet {
        data class Sent(val url: String, val headers: Map<String, String>)
        val sent = mutableListOf<Sent>()
        override suspend fun get(url: String, headers: Map<String, String>): FormResponse {
            sent += Sent(url, headers)
            return script.removeAt(0)
        }
    }

    private val keyPair = Dpop.generateKeyPair()

    private fun fetch(http: FakeGet) = runBlocking {
        ServiceAuth.fetch(
            http,
            pdsBase = "https://pds.example.com",
            accessToken = "access-tok",
            keyPair = keyPair,
            aud = "did:web:admit.croft.ing",
            lxm = "ing.croft.relay.grantCall",
            jti = "jti-1",
            nowMs = 1_700_000_000_000L,
        )
    }

    @Test
    fun `a minted proof comes back verbatim with the right request shape`() {
        val http = FakeGet(mutableListOf(FormResponse(200, emptyMap(), """{"token":"ey.svc.jwt"}""")))
        assertEquals("ey.svc.jwt", fetch(http))

        val sent = http.sent.single()
        assertTrue(sent.url.startsWith("https://pds.example.com/xrpc/com.atproto.server.getServiceAuth?"))
        assertTrue("aud encoded", sent.url.contains("aud=did%3Aweb%3Aadmit.croft.ing"))
        assertTrue("lxm named", sent.url.contains("lxm=ing.croft.relay.grantCall"))
        // The access token rides the DPoP scheme, never Bearer (RFC 9449).
        assertEquals("DPoP access-tok", sent.headers["Authorization"])
        // And the proof is ath-bound to that token.
        val claims = String(
            Base64.getUrlDecoder().decode(sent.headers.getValue("DPoP").split(".")[1]),
        )
        assertTrue("proof carries ath", claims.contains("\"ath\":"))
        assertTrue("proof is a GET proof", claims.contains("\"htm\":\"GET\""))
    }

    @Test
    fun `a nonce rejection is retried once with the nonce and a fresh jti`() {
        val http = FakeGet(
            mutableListOf(
                FormResponse(401, mapOf("DPoP-Nonce" to "n-1"), """{"error":"use_dpop_nonce"}"""),
                FormResponse(200, emptyMap(), """{"token":"ey.svc.jwt"}"""),
            ),
        )
        assertEquals("ey.svc.jwt", fetch(http))
        assertEquals(2, http.sent.size)
        val retryClaims = String(
            Base64.getUrlDecoder().decode(http.sent[1].headers.getValue("DPoP").split(".")[1]),
        )
        assertTrue(retryClaims.contains("\"nonce\":\"n-1\""))
        val firstJti = String(
            Base64.getUrlDecoder().decode(http.sent[0].headers.getValue("DPoP").split(".")[1]),
        )
        assertTrue("proofs are single-use: the retry must not reuse the jti",
            firstJti != retryClaims)
    }

    @Test
    fun `a refusal without a nonce fails loud, and a nonce retry that fails surfaces`() {
        assertThrows(IllegalStateException::class.java) {
            fetch(FakeGet(mutableListOf(FormResponse(401, emptyMap(), """{"error":"AuthRequired"}"""))))
        }
        assertThrows(IllegalStateException::class.java) {
            fetch(
                FakeGet(
                    mutableListOf(
                        FormResponse(401, mapOf("DPoP-Nonce" to "n-1"), "{}"),
                        FormResponse(401, emptyMap(), """{"error":"still no"}"""),
                    ),
                ),
            )
        }
    }

    @Test
    fun `a 200 with no token fails loud rather than minting an empty proof`() {
        assertThrows(IllegalStateException::class.java) {
            fetch(FakeGet(mutableListOf(FormResponse(200, emptyMap(), "{}"))))
        }
    }
}
