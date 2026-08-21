package ing.croft.call.identity

import android.content.Context
import android.content.SharedPreferences
import android.util.Log
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import ing.croft.call.caps.Http
import ing.croft.call.caps.HttpForm
import ing.croft.call.caps.OAuthFlow
import ing.croft.call.caps.Xrpc
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import java.math.BigInteger
import java.security.KeyFactory
import java.security.KeyPair
import java.security.SecureRandom
import java.security.spec.PKCS8EncodedKeySpec
import java.security.spec.X509EncodedKeySpec
import java.util.Base64

/**
 * The effect half of M3 identity proof: orchestrates [OAuthFlow] over the
 * live ports, launches the browser for the human tap, captures the
 * redirect, and holds the session durably. In-flight state (PKCE
 * verifier, CSRF state, the DPoP keypair the tokens will be bound to)
 * persists across the browser hop — the process may die while the user
 * is in the browser. Tokens and keys never appear in logs; DIDs may.
 *
 * Prefs are injected: production passes [encryptedPrefs] (same
 * EncryptedSharedPreferences pattern as [IdentityStore]); tests pass a
 * plain one (AndroidKeyStore is absent under Robolectric).
 */
class AuthManager(
    private val prefs: SharedPreferences,
    private val http: Http,
    private val form: HttpForm,
    private val openUrl: (String) -> Unit,
    private val nowMs: () -> Long = System::currentTimeMillis,
) {

    private val _provenDid = MutableStateFlow(prefs.getString(K_DID, null))

    /** The OAuth-proven caller DID, null when signed out. */
    val provenDid: StateFlow<String?> = _provenDid

    /**
     * Start the dance for [handle]: resolve its PDS, discover the auth
     * server, push the authorization request, and hand the authorize URL
     * to the browser. Returns when the browser is launched — the flow
     * resumes at [onRedirect].
     */
    suspend fun signIn(handle: String) {
        Log.i(TAG, "sign-in started for $handle")
        val did = Xrpc.resolveHandle(http, handle)
        val pds = Xrpc.resolvePds(http, did)
        val server = OAuthFlow.discover(http, pds)
        Log.i(TAG, "auth server ${server.issuer} for $did")

        val state = randomToken()
        val verifier = randomToken()
        val keyPair = ing.croft.call.caps.Dpop.generateKeyPair()
        val requestUri = OAuthFlow.pushAuthRequest(
            form, server, clientId = CLIENT_ID, redirectUri = REDIRECT_URI, scope = SCOPE,
            loginHint = handle, state = state, pkceVerifier = verifier,
            keyPair = keyPair, jti = randomToken(), nowMs = nowMs(),
        )
        Log.i(TAG, "PAR accepted; launching browser")

        prefs.edit()
            .putString(K_PENDING_STATE, state)
            .putString(K_PENDING_VERIFIER, verifier)
            .putString(K_PDS, pds)
            .putString(K_PRIVATE, b64(keyPair.private.encoded))
            .putString(K_PUBLIC, b64(keyPair.public.encoded))
            .putString(K_ISSUER, server.issuer)
            .putString(K_PAR, server.parEndpoint)
            .putString(K_AUTHORIZE, server.authorizeEndpoint)
            .putString(K_TOKEN, server.tokenEndpoint)
            .apply()

        openUrl(OAuthFlow.authorizeUrl(server, CLIENT_ID, requestUri))
    }

    /**
     * The redirect intent's data URI. Fails closed (and clears nothing)
     * on a state mismatch; on success the tokens, keypair and DID are
     * stored and [provenDid] flips.
     */
    suspend fun onRedirect(redirect: String) {
        Log.i(TAG, "redirect received")
        val state = prefs.getString(K_PENDING_STATE, null)
            ?: throw IllegalStateException("no sign-in in flight")
        val verifier = prefs.getString(K_PENDING_VERIFIER, null)
            ?: throw IllegalStateException("no PKCE verifier in flight")
        val code = OAuthFlow.parseRedirect(redirect, expectedState = state)
        val tokens = OAuthFlow.exchangeCode(
            form, server = storedServer(), clientId = CLIENT_ID, redirectUri = REDIRECT_URI,
            code = code, pkceVerifier = verifier, keyPair = storedKeyPair(),
            jti = randomToken(), nowMs = nowMs(),
        )
        prefs.edit()
            .putString(K_DID, tokens.did)
            .putString(K_ACCESS, tokens.accessToken)
            .putString(K_REFRESH, tokens.refreshToken)
            .putLong(K_EXPIRES_AT, nowMs() + tokens.expiresInSeconds * 1000)
            .remove(K_PENDING_STATE)
            .remove(K_PENDING_VERIFIER)
            .apply()
        _provenDid.value = tokens.did
        Log.i(TAG, "exchange succeeded: signed in as ${tokens.did}")
    }

    /**
     * A usable access token, refreshing first when the stored one is stale
     * (M4b; closes ROADMAP_TODO E113 — the mint is the first load-bearing
     * consumer of a live session). atproto refresh tokens are SINGLE-USE
     * and rotate: the new pair is persisted before this returns, so a
     * process death after a refresh never strands a spent token as the
     * stored one. Call on-foreground and before every mint.
     */
    suspend fun freshAccessToken(): String {
        val access = prefs.getString(K_ACCESS, null)
            ?: throw IllegalStateException("not signed in")
        val expiresAt = prefs.getLong(K_EXPIRES_AT, 0)
        if (nowMs() < expiresAt - EXPIRY_MARGIN_MS) return access

        Log.i(TAG, "access token stale; refreshing")
        val tokens = OAuthFlow.refresh(
            form, server = storedServer(), clientId = CLIENT_ID,
            refreshToken = required(K_REFRESH), keyPair = storedKeyPair(),
            jti = randomToken(), nowMs = nowMs(),
        )
        prefs.edit()
            .putString(K_ACCESS, tokens.accessToken)
            .putString(K_REFRESH, tokens.refreshToken)
            .putLong(K_EXPIRES_AT, nowMs() + tokens.expiresInSeconds * 1000)
            .apply()
        Log.i(TAG, "session refreshed")
        return tokens.accessToken
    }

    /**
     * A service-auth JWT proving this session's DID (M4c) — minted at the
     * PDS the session was established against (stored at sign-in, not
     * re-resolved), with a fresh access token (the before-mint half of
     * E113). The JWT is returned verbatim; never logged.
     */
    suspend fun serviceAuthProof(
        http: ing.croft.call.caps.HttpGet,
        aud: String,
        lxm: String,
    ): String = ing.croft.call.caps.ServiceAuth.fetch(
        http,
        pdsBase = required(K_PDS),
        accessToken = freshAccessToken(),
        keyPair = storedKeyPair(),
        aud = aud,
        lxm = lxm,
        jti = randomToken(),
        nowMs = nowMs(),
    )

    /** Drop the session: tokens, keys and DID all cleared. */
    fun signOut() {
        val was = _provenDid.value
        prefs.edit().clear().apply()
        _provenDid.value = null
        Log.i(TAG, "signed out${if (was != null) " ($was)" else ""}")
    }

    private fun storedServer() = OAuthFlow.AuthServer(
        issuer = required(K_ISSUER),
        parEndpoint = required(K_PAR),
        authorizeEndpoint = required(K_AUTHORIZE),
        tokenEndpoint = required(K_TOKEN),
    )

    private fun storedKeyPair(): KeyPair {
        val kf = KeyFactory.getInstance("EC")
        return KeyPair(
            kf.generatePublic(X509EncodedKeySpec(unb64(required(K_PUBLIC)))),
            kf.generatePrivate(PKCS8EncodedKeySpec(unb64(required(K_PRIVATE)))),
        )
    }

    private fun required(key: String): String =
        prefs.getString(key, null) ?: throw IllegalStateException("auth store missing $key")

    private fun randomToken(): String {
        val bytes = ByteArray(32)
        SecureRandom().nextBytes(bytes)
        return BigInteger(1, bytes).toString(36)
    }

    private fun b64(bytes: ByteArray): String = Base64.getEncoder().encodeToString(bytes)
    private fun unb64(s: String): ByteArray = Base64.getDecoder().decode(s)

    companion object {
        private const val TAG = "CroftCall"

        /** The hosted client metadata (Phase 0 D4 — live on connect Pages). */
        const val CLIENT_ID = "https://connect.croft.ing/oauth-client-metadata.json"

        /** Spec rule: custom scheme = client_id hostname reversed (D1). */
        const val REDIRECT_URI = "ing.croft.connect:/oauth"

        /**
         * M4 (plan O2, resolved from PDS source 2026-08-20): the mint's
         * caller proof is `getServiceAuth`, which under OAuth requires an
         * RPC permission the bare `atproto` scope does not carry
         * (`assertRpc` → ScopeMissingError). The granular form
         * (`rpc:ing.croft.relay.grantCall?aud=did:web:admit.croft.ing`) is
         * implemented upstream but NOT advertised by the bsky.social
         * entryway yet, so this is `transition:generic` (app-password
         * equivalence) until it is — narrow it when `scopes_supported`
         * grows rpc scopes. Must match the hosted client metadata.
         */
        const val SCOPE = "atproto transition:generic"

        fun isOAuthRedirect(url: String?): Boolean =
            url != null && url.startsWith("ing.croft.connect:")

        /** Production prefs: encrypted, same pattern as [IdentityStore]. */
        fun encryptedPrefs(context: Context): SharedPreferences =
            EncryptedSharedPreferences.create(
                context,
                "croftcall.auth",
                MasterKey.Builder(context).setKeyScheme(MasterKey.KeyScheme.AES256_GCM).build(),
                EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
                EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
            )

        private const val K_DID = "did"
        private const val K_ACCESS = "access_token"
        private const val K_EXPIRES_AT = "access_expires_at"
        private const val K_PDS = "session_pds"

        /** Refresh this long before nominal expiry — a token that dies
         *  mid-getServiceAuth helps nobody. */
        private const val EXPIRY_MARGIN_MS = 60_000L
        private const val K_REFRESH = "refresh_token"
        private const val K_PRIVATE = "dpop_private"
        private const val K_PUBLIC = "dpop_public"
        private const val K_PENDING_STATE = "pending_state"
        private const val K_PENDING_VERIFIER = "pending_verifier"
        private const val K_ISSUER = "as_issuer"
        private const val K_PAR = "as_par"
        private const val K_AUTHORIZE = "as_authorize"
        private const val K_TOKEN = "as_token"
    }
}
