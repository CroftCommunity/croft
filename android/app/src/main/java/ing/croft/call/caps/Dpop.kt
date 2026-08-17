package ing.croft.call.caps

import java.security.KeyPair
import java.security.KeyPairGenerator
import java.security.Signature
import java.security.interfaces.ECPublicKey
import java.security.spec.ECGenParameterSpec
import java.util.Base64

/**
 * DPoP proof construction (RFC 9449) for the atproto OAuth dance —
 * Phase 11 M3. Pure given its inputs: key, method, url, clock, jti and
 * optional server nonce in; compact JWS out. Hand-rolled (Phase 0 D3
 * decision): java.security signs ES256 natively, so no JOSE dependency —
 * the only subtlety is converting the DER signature to the raw 64-byte
 * r||s form JOSE requires, where short and sign-padded DER integers are
 * live branches, not theory.
 */
object Dpop {

    private val b64 = Base64.getUrlEncoder().withoutPadding()

    /** A fresh P-256 keypair. The caller owns its lifetime — DPoP binds
     *  tokens to this key, so it must persist with the tokens. */
    fun generateKeyPair(): KeyPair =
        KeyPairGenerator.getInstance("EC")
            .apply { initialize(ECGenParameterSpec("secp256r1")) }
            .generateKeyPair()

    /**
     * Build one DPoP proof JWS. [iatMs] is wall-clock millis (truncated to
     * the seconds NumericDate RFC 7519 wants); [nonce] is the server nonce
     * from a `DPoP-Nonce` response header, absent on the first request.
     */
    fun proof(
        keyPair: KeyPair,
        htm: String,
        htu: String,
        iatMs: Long,
        jti: String,
        nonce: String? = null,
    ): String {
        val pub = keyPair.public as ECPublicKey
        val header = buildString {
            append("{\"typ\":\"dpop+jwt\",\"alg\":\"ES256\",\"jwk\":{")
            append("\"kty\":\"EC\",\"crv\":\"P-256\"")
            append(",\"x\":\"").append(b64.encodeToString(coord32(pub.w.affineX.toByteArray()))).append('"')
            append(",\"y\":\"").append(b64.encodeToString(coord32(pub.w.affineY.toByteArray()))).append('"')
            append("}}")
        }
        val claims = buildString {
            append("{\"jti\":\"").append(jti).append('"')
            append(",\"htm\":\"").append(htm).append('"')
            append(",\"htu\":\"").append(htu).append('"')
            append(",\"iat\":").append(iatMs / 1000)
            if (nonce != null) append(",\"nonce\":\"").append(nonce).append('"')
            append('}')
        }
        val signingInput =
            b64.encodeToString(header.toByteArray()) + "." + b64.encodeToString(claims.toByteArray())
        val signer = Signature.getInstance("SHA256withECDSA")
        signer.initSign(keyPair.private)
        signer.update(signingInput.toByteArray())
        return signingInput + "." + b64.encodeToString(derToJose(signer.sign()))
    }

    /**
     * DER `SEQUENCE { INTEGER r, INTEGER s }` → raw 64-byte r||s. Each DER
     * integer may be 33 bytes (leading 0x00 sign byte — stripped) or fewer
     * than 32 (left-padded with zeros). P-256 signatures are < 128 bytes
     * total, so all DER lengths here are single-byte.
     */
    fun derToJose(der: ByteArray): ByteArray {
        check(der[0].toInt() == 0x30) { "not a DER sequence" }
        val out = ByteArray(64)
        val rLen = der[3].toInt() and 0xff
        copyInt(der, from = 4, len = rLen, out = out, at = 0)
        val sLen = der[4 + rLen + 1].toInt() and 0xff
        copyInt(der, from = 4 + rLen + 2, len = sLen, out = out, at = 32)
        return out
    }

    /** Raw 64-byte r||s → DER, for verifying with java.security. */
    fun joseToDer(raw: ByteArray): ByteArray {
        check(raw.size == 64) { "raw ES256 signature must be 64 bytes" }
        val r = toDerInt(raw, 0)
        val s = toDerInt(raw, 32)
        return byteArrayOf(0x30, (r.size + s.size).toByte()) + r + s
    }

    private fun copyInt(src: ByteArray, from: Int, len: Int, out: ByteArray, at: Int) {
        var start = from
        var n = len
        if (n == 33) { start++; n = 32 } // drop the DER sign byte
        check(n <= 32) { "DER integer longer than 33 bytes" }
        System.arraycopy(src, start, out, at + (32 - n), n)
    }

    private fun toDerInt(raw: ByteArray, off: Int): ByteArray {
        var start = off
        while (start < off + 31 && raw[start].toInt() == 0) start++
        val len = off + 32 - start
        val pad = raw[start].toInt() and 0x80 != 0
        val out = ByteArray(2 + len + if (pad) 1 else 0)
        out[0] = 0x02
        out[1] = (len + if (pad) 1 else 0).toByte()
        System.arraycopy(raw, start, out, 2 + if (pad) 1 else 0, len)
        return out
    }

    /** EC affine coordinate → exactly 32 bytes: BigInteger.toByteArray may
     *  carry a sign byte (33) or drop leading zeros (<32). */
    private fun coord32(be: ByteArray): ByteArray {
        val out = ByteArray(32)
        val copy = minOf(be.size, 32)
        System.arraycopy(be, be.size - copy, out, 32 - copy, copy)
        return out
    }
}
