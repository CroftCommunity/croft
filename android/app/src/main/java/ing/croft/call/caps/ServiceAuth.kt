package ing.croft.call.caps

import java.net.URLEncoder
import java.security.KeyPair

/**
 * The fourth seam to the network (M4b): an HTTP GET carrying headers,
 * returning status + headers + body without throwing on non-2xx — the
 * resource-server DPoP nonce dance reads `DPoP-Nonce` off a 401.
 */
fun interface HttpGet {
    suspend fun get(url: String, headers: Map<String, String>): FormResponse
}

/**
 * The caller proof D3 chose (Phase 11 M4b): `com.atproto.server.
 * getServiceAuth` on the caller's OWN PDS — the PDS mints a short-lived
 * JWT (aud-bound to croft-admit, method-bound via lxm, single-use jti)
 * that croft-admit verifies against the caller's DID document. The phone's
 * OAuth tokens never leave the phone↔PDS pair; only this derived proof
 * travels.
 *
 * Presentation is RFC 9449 resource-server DPoP: the access token under
 * the `DPoP` authorization scheme (never Bearer), the proof `ath`-bound to
 * it, and a 401 carrying `DPoP-Nonce` retried exactly once with the nonce
 * and a FRESH jti (proofs are single-use). The minted JWT is returned
 * verbatim and never parsed or logged here.
 */
object ServiceAuth {

    suspend fun fetch(
        http: HttpGet,
        pdsBase: String,
        accessToken: String,
        keyPair: KeyPair,
        aud: String,
        lxm: String,
        jti: String,
        nowMs: Long,
    ): String {
        val htu = "${pdsBase.trimEnd('/')}/xrpc/com.atproto.server.getServiceAuth"
        val url = "$htu?aud=${enc(aud)}&lxm=${enc(lxm)}"

        val first = http.get(url, headers(keyPair, htu, accessToken, jti, nowMs, nonce = null))
        val ok = if (first.status in 200..299) {
            first
        } else {
            val nonce = first.header("DPoP-Nonce")
                ?: throw IllegalStateException("HTTP ${first.status} from getServiceAuth: ${first.body}")
            val second =
                http.get(url, headers(keyPair, htu, accessToken, "$jti-n", nowMs, nonce))
            check(second.status in 200..299) {
                "HTTP ${second.status} from getServiceAuth after nonce retry: ${second.body}"
            }
            second
        }
        val token = org.json.JSONObject(ok.body).optString("token")
        check(token.isNotEmpty()) { "getServiceAuth answered without a token" }
        return token
    }

    private fun headers(
        keyPair: KeyPair,
        htu: String,
        accessToken: String,
        jti: String,
        nowMs: Long,
        nonce: String?,
    ): Map<String, String> = mapOf(
        "Authorization" to "DPoP $accessToken",
        "DPoP" to Dpop.proof(
            keyPair, htm = "GET", htu = htu, iatMs = nowMs, jti = jti,
            nonce = nonce, accessToken = accessToken,
        ),
    )

    private fun enc(s: String): String = URLEncoder.encode(s, "UTF-8")
}
