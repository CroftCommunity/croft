package ing.croft.call.caps

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import java.security.KeyFactory
import java.security.Signature
import java.security.interfaces.ECPublicKey
import java.security.spec.X509EncodedKeySpec
import java.util.Base64

/**
 * The DPoP proof builder (RFC 9449) — Phase 11 M3 Phase 1. The proof is a
 * compact JWS: base64url(header).base64url(claims).base64url(raw-ES256-sig).
 * Phase 0's spike proved java.security suffices and that the DER→raw
 * left-pad branch is hit in practice (~1 in 128 signatures), so the
 * conversion is pinned by fixed vectors here, not just random roundtrips.
 * Runs under Robolectric for a real org.json (suite convention).
 */
@RunWith(RobolectricTestRunner::class)
class DpopTest {

    private val b64 = Base64.getUrlDecoder()

    private fun parts(jws: String): Triple<JSONObject, JSONObject, ByteArray> {
        val p = jws.split(".")
        assertEquals(3, p.size)
        return Triple(
            JSONObject(String(b64.decode(p[0]))),
            JSONObject(String(b64.decode(p[1]))),
            b64.decode(p[2]),
        )
    }

    @Test
    fun `header carries dpop typ, ES256, and a P-256 public jwk`() {
        val key = Dpop.generateKeyPair()
        val (header, _, _) = parts(
            Dpop.proof(key, htm = "POST", htu = "https://as.example/oauth/par", iatMs = 1_755_400_000_000, jti = "j1"),
        )
        assertEquals("dpop+jwt", header.getString("typ"))
        assertEquals("ES256", header.getString("alg"))
        val jwk = header.getJSONObject("jwk")
        assertEquals("EC", jwk.getString("kty"))
        assertEquals("P-256", jwk.getString("crv"))
        // 32 bytes unpadded base64url == exactly 43 chars, for every key.
        assertEquals(43, jwk.getString("x").length)
        assertEquals(43, jwk.getString("y").length)
    }

    @Test
    fun `claims carry jti, htm, htu, and iat in seconds`() {
        val key = Dpop.generateKeyPair()
        val (_, claims, _) = parts(
            Dpop.proof(key, htm = "POST", htu = "https://as.example/oauth/token", iatMs = 1_755_400_123_999, jti = "j2"),
        )
        assertEquals("j2", claims.getString("jti"))
        assertEquals("POST", claims.getString("htm"))
        assertEquals("https://as.example/oauth/token", claims.getString("htu"))
        assertEquals(1_755_400_123L, claims.getLong("iat")) // ms truncated to seconds
        assertFalse(claims.has("nonce"))
    }

    @Test
    fun `server nonce is included when provided`() {
        val key = Dpop.generateKeyPair()
        val (_, claims, _) = parts(
            Dpop.proof(key, htm = "POST", htu = "https://x/t", iatMs = 0, jti = "j", nonce = "server-nonce-1"),
        )
        assertEquals("server-nonce-1", claims.getString("nonce"))
    }

    @Test
    fun `signature is raw 64 bytes and verifies with the key in the header`() {
        val key = Dpop.generateKeyPair()
        val jws = Dpop.proof(key, htm = "POST", htu = "https://x/t", iatMs = 1, jti = "j")
        val (header, _, sig) = parts(jws)
        assertEquals(64, sig.size)

        // Rebuild the public key from the jwk the header itself carries —
        // the proof must be verifiable by a party that has only the JWS.
        val jwk = header.getJSONObject("jwk")
        val pub = ecPublicKey(b64.decode(jwk.getString("x")), b64.decode(jwk.getString("y")))
        val signingInput = jws.substringBeforeLast(".").toByteArray()
        val verifier = Signature.getInstance("SHA256withECDSA")
        verifier.initVerify(pub)
        verifier.update(signingInput)
        assertTrue(verifier.verify(Dpop.joseToDer(sig)))
    }

    @Test
    fun `every part is base64url without padding`() {
        val key = Dpop.generateKeyPair()
        // 60 proofs: statistically crosses the short-r/s branch a few times,
        // and every output must still be clean base64url.
        repeat(60) { i ->
            val jws = Dpop.proof(key, htm = "GET", htu = "https://x/$i", iatMs = i.toLong(), jti = "j$i")
            assertFalse(jws.contains("="))
            assertFalse(jws.contains("+"))
            assertFalse(jws.contains("/"))
        }
    }

    // --- DER→raw conversion, pinned by construction (Phase 0: the left-pad
    // branch is live; random-key tests alone would pass flakily) ---

    private fun derInt(bytes: ByteArray): ByteArray =
        byteArrayOf(0x02, bytes.size.toByte()) + bytes

    private fun derSig(r: ByteArray, s: ByteArray): ByteArray {
        val body = derInt(r) + derInt(s)
        return byteArrayOf(0x30, body.size.toByte()) + body
    }

    @Test
    fun `derToJose left-pads a short r to 32 bytes`() {
        val r = ByteArray(31) { 0x11 } // 31-byte integer: needs one pad byte
        val s = ByteArray(32) { 0x22 }
        val raw = Dpop.derToJose(derSig(r, s))
        assertEquals(64, raw.size)
        assertEquals(0, raw[0].toInt())          // the pad
        assertEquals(0x11, raw[1].toInt())
        assertEquals(0x22, raw[32].toInt())
    }

    @Test
    fun `derToJose strips the sign byte of a 33-byte integer`() {
        val r = ByteArray(32) { 0x7f }
        val sHigh = byteArrayOf(0x00) + ByteArray(32) { 0x99.toByte() } // sign-padded
        val raw = Dpop.derToJose(derSig(r, sHigh))
        assertEquals(64, raw.size)
        assertEquals(0x7f, raw[0].toInt())
        assertEquals(0x99.toByte(), raw[32])     // sign byte gone
    }

    @Test
    fun `derToJose and joseToDer roundtrip across all paddings`() {
        val cases = listOf(
            ByteArray(31) { 0x11 } to ByteArray(30) { 0x22 },              // both short
            ByteArray(32) { 0x7f } to ByteArray(32) { 0x6e },              // both plain
            (byteArrayOf(0x00) + ByteArray(32) { 0xC5.toByte() }) to ByteArray(32) { 0x41 }, // r sign-padded
        )
        for ((r, s) in cases) {
            val der = derSig(r, s)
            val raw = Dpop.derToJose(der)
            assertEquals(64, raw.size)
            // Roundtrip must re-produce a DER encoding whose raw form matches.
            assertEquals(raw.toList(), Dpop.derToJose(Dpop.joseToDer(raw)).toList())
        }
    }

    private fun ecPublicKey(x: ByteArray, y: ByteArray): ECPublicKey {
        // Uncompressed SEC1 point inside a minimal X.509 SubjectPublicKeyInfo
        // for P-256 (fixed 26-byte prefix), so KeyFactory can load it.
        val prefix = byteArrayOf(
            0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86.toByte(), 0x48, 0xce.toByte(),
            0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a, 0x86.toByte(), 0x48, 0xce.toByte(), 0x3d,
            0x03, 0x01, 0x07, 0x03, 0x42, 0x00, 0x04,
        )
        val spki = prefix + x + y
        return KeyFactory.getInstance("EC")
            .generatePublic(X509EncodedKeySpec(spki)) as ECPublicKey
    }
}
