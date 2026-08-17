package ing.croft.call.caps

import org.json.JSONObject
import java.net.URLDecoder
import java.net.URLEncoder
import java.security.KeyPair
import java.security.MessageDigest
import java.util.Base64

/**
 * The atproto OAuth dance (M3 Phase 2): server discovery → PAR →
 * authorize URL → redirect parse → code exchange → refresh. Pure given
 * its inputs — clock, state, PKCE verifier and jti are parameters the
 * effect edge (AuthManager) generates; network runs through the injected
 * [Http]/[HttpForm] ports. Every mismatch fails closed. Ground truth:
 * Phase 0 findings (atproto.com/specs/oauth + the live bsky.social
 * entryway metadata, saved as fixtures).
 */
object OAuthFlow {

    /** The auth-server endpoints discovered for a PDS. */
    data class AuthServer(
        val issuer: String,
        val parEndpoint: String,
        val authorizeEndpoint: String,
        val tokenEndpoint: String,
    )

    /** DPoP-bound session tokens; [did] is the token response's `sub`. */
    data class Tokens(
        val accessToken: String,
        val refreshToken: String,
        val did: String,
        val expiresInSeconds: Long,
    )

    /**
     * PDS → auth server: `/.well-known/oauth-protected-resource` names the
     * authorization server; its `/.well-known/oauth-authorization-server`
     * carries the endpoints. Fails closed if the PDS names no server or the
     * metadata's issuer does not match the server origin it came from.
     */
    suspend fun discover(http: Http, pdsUrl: String): AuthServer {
        val protected = JSONObject(
            http.getJson(pdsUrl.trimEnd('/') + "/.well-known/oauth-protected-resource"),
        )
        val servers = protected.optJSONArray("authorization_servers")
        check(servers != null && servers.length() > 0) { "PDS names no authorization server" }
        val origin = servers.getString(0).trimEnd('/')
        val meta = JSONObject(http.getJson("$origin/.well-known/oauth-authorization-server"))
        val issuer = meta.optString("issuer").trimEnd('/')
        check(issuer == origin) { "issuer mismatch: $issuer served from $origin" }
        return AuthServer(
            issuer = issuer,
            parEndpoint = required(meta, "pushed_authorization_request_endpoint"),
            authorizeEndpoint = required(meta, "authorization_endpoint"),
            tokenEndpoint = required(meta, "token_endpoint"),
        )
    }

    /** PAR (mandatory per the atproto profile). Returns the `request_uri`. */
    suspend fun pushAuthRequest(
        form: HttpForm,
        server: AuthServer,
        clientId: String,
        redirectUri: String,
        scope: String,
        loginHint: String?,
        state: String,
        pkceVerifier: String,
        keyPair: KeyPair,
        jti: String,
        nowMs: Long,
    ): String {
        val fields = buildMap {
            put("client_id", clientId)
            put("response_type", "code")
            put("redirect_uri", redirectUri)
            put("scope", scope)
            put("state", state)
            put("code_challenge", s256(pkceVerifier))
            put("code_challenge_method", "S256")
            if (loginHint != null) put("login_hint", loginHint)
        }
        val response = postWithDpop(form, server.parEndpoint, fields, keyPair, jti, nowMs)
        return required(JSONObject(response.body), "request_uri")
    }

    /** The browser-hop URL: authorize endpoint + client_id + request_uri. */
    fun authorizeUrl(server: AuthServer, clientId: String, requestUri: String): String =
        server.authorizeEndpoint +
            "?client_id=" + enc(clientId) +
            "&request_uri=" + enc(requestUri)

    /**
     * Parse the redirect the browser sends back. Fails closed on a state
     * mismatch (CSRF) or an `error` response; returns the code. String
     * parsing, not android.net.Uri — this is engine code.
     */
    fun parseRedirect(redirect: String, expectedState: String): String {
        val query = redirect.substringAfter('?', missingDelimiterValue = "")
        val params = query.split('&').filter { it.isNotEmpty() }.associate {
            val k = it.substringBefore('=')
            val v = URLDecoder.decode(it.substringAfter('=', missingDelimiterValue = ""), "UTF-8")
            k to v
        }
        check(params["error"] == null) { "authorization failed: ${params["error"]}" }
        check(params["state"] == expectedState) { "state mismatch on redirect" }
        return params["code"] ?: throw IllegalStateException("redirect carries no code")
    }

    /** Authorization-code exchange; returns DPoP-bound tokens + the DID. */
    suspend fun exchangeCode(
        form: HttpForm,
        server: AuthServer,
        clientId: String,
        redirectUri: String,
        code: String,
        pkceVerifier: String,
        keyPair: KeyPair,
        jti: String,
        nowMs: Long,
    ): Tokens = tokenRequest(
        form, server, keyPair, jti, nowMs,
        fields = mapOf(
            "grant_type" to "authorization_code",
            "client_id" to clientId,
            "redirect_uri" to redirectUri,
            "code" to code,
            "code_verifier" to pkceVerifier,
        ),
    )

    /** Refresh-token rotation (atproto refresh tokens are single-use). */
    suspend fun refresh(
        form: HttpForm,
        server: AuthServer,
        clientId: String,
        refreshToken: String,
        keyPair: KeyPair,
        jti: String,
        nowMs: Long,
    ): Tokens = tokenRequest(
        form, server, keyPair, jti, nowMs,
        fields = mapOf(
            "grant_type" to "refresh_token",
            "client_id" to clientId,
            "refresh_token" to refreshToken,
        ),
    )

    private suspend fun tokenRequest(
        form: HttpForm,
        server: AuthServer,
        keyPair: KeyPair,
        jti: String,
        nowMs: Long,
        fields: Map<String, String>,
    ): Tokens {
        val response = postWithDpop(form, server.tokenEndpoint, fields, keyPair, jti, nowMs)
        val json = JSONObject(response.body)
        return Tokens(
            accessToken = required(json, "access_token"),
            refreshToken = required(json, "refresh_token"),
            did = required(json, "sub"),
            expiresInSeconds = json.optLong("expires_in", 0),
        )
    }

    /**
     * POST with a DPoP proof, honoring the RFC 9449 §8 nonce dance: a
     * rejection carrying a `DPoP-Nonce` header is retried exactly once
     * with the nonce (and a fresh jti — proofs are single-use). A second
     * failure surfaces; anything non-2xx without a nonce surfaces at once.
     */
    private suspend fun postWithDpop(
        form: HttpForm,
        url: String,
        fields: Map<String, String>,
        keyPair: KeyPair,
        jti: String,
        nowMs: Long,
    ): FormResponse {
        val first = form.postForm(
            url, fields,
            headers = mapOf("DPoP" to Dpop.proof(keyPair, "POST", url, nowMs, jti)),
        )
        if (first.status in 200..299) return first
        val nonce = first.header("DPoP-Nonce")
            ?: throw IllegalStateException("HTTP ${first.status} from $url: ${first.body}")
        val second = form.postForm(
            url, fields,
            headers = mapOf("DPoP" to Dpop.proof(keyPair, "POST", url, nowMs, "$jti-n", nonce = nonce)),
        )
        check(second.status in 200..299) { "HTTP ${second.status} from $url after nonce retry: ${second.body}" }
        return second
    }

    private fun s256(verifier: String): String =
        Base64.getUrlEncoder().withoutPadding().encodeToString(
            MessageDigest.getInstance("SHA-256").digest(verifier.toByteArray()),
        )

    private fun required(json: JSONObject, key: String): String {
        val value = json.optString(key)
        check(value.isNotEmpty()) { "response missing $key" }
        return value
    }

    private fun enc(s: String): String = URLEncoder.encode(s, "UTF-8")
}
