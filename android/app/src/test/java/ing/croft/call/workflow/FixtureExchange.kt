package ing.croft.call.workflow

import ing.croft.call.caps.Http
import ing.croft.call.caps.Tickets
import okhttp3.mockwebserver.Dispatcher
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okhttp3.mockwebserver.RecordedRequest
import org.json.JSONArray
import org.json.JSONObject
import java.net.URLDecoder

/**
 * The workflow harness (M4, first-class per the plan): one in-JVM server
 * standing in for every backend the client talks to — AppView
 * (resolveHandle, getRelationships), plc.directory, the callee's PDS
 * (listRecords/getRecord), and croft-admit (/grantCall). State is MUTABLE
 * so tests can tell revocation stories: delete a grant mid-test and the
 * next mint refuses, exactly the row the relay side proves on its end
 * (croft-stack `tests/mint_binary.rs`).
 *
 * Same discipline as croft-stack's fixture servers: workflow tests drive
 * the REAL ports (UrlHttp over real sockets), so the whole journey —
 * URL building, wire parsing, engine, admission mapping — runs as shipped.
 * Only name resolution is faked, via [Rewired] (an /etc/hosts for tests);
 * the PDS needs no rewrite at all because the fixture's DID documents name
 * the fixture itself as the PDS.
 *
 * The /grantCall route MIRRORS the real server's contract (shapes and
 * refusal discriminants from croft-stack `mint.rs` — the external-API
 * rule). It is not the real binary; the cross-stack loop against real
 * croft-admit is plan chunk M4d(b).
 */
class FixtureExchange : AutoCloseable {

    /** did → handle (resolveHandle answers by reverse lookup). */
    val accounts = mutableMapOf<String, String>()

    /** "did/collection/rkey" → record value (JSON). Mutable: deleting an
     *  entry IS revocation. */
    val records = mutableMapOf<String, JSONObject>()

    /** Pairs (a, b) that are mutual, order-insensitive. */
    val mutuals = mutableSetOf<Set<String>>()

    /** Every /grantCall request body, for assertions. */
    val mints = mutableListOf<JSONObject>()

    /** Every PAR request's form fields, for assertions (state, scope, …). */
    val parRequests = mutableListOf<Map<String, String>>()

    /** Force the next /grantCall to refuse with this discriminant. */
    var nextMintRefusal: String? = null

    /** The admit mirror's clock for policy `expires` rules (epoch ms). */
    var nowMs: Long = 1_700_000_000_000L

    private var tokenSerial = 0

    private val server = MockWebServer().apply {
        dispatcher = object : Dispatcher() {
            override fun dispatch(request: RecordedRequest): MockResponse = route(request)
        }
        start()
    }

    /** The fixture's base URL — also every account's PDS endpoint. */
    val base: String = server.url("/").toString().trimEnd('/')

    override fun close() = server.shutdown()

    // ---- convenience state builders -----------------------------------------

    fun putGrant(did: String, rkey: String, value: JSONObject) {
        records["$did/ing.croft.call.grant/$rkey"] = value
    }

    fun deleteGrant(did: String, rkey: String) {
        records.remove("$did/ing.croft.call.grant/$rkey")
    }

    fun putEndpoint(did: String, rkey: String, endpointId: String) {
        records["$did/ing.croft.iroh.endpoint/$rkey"] =
            JSONObject().put("endpointId", endpointId).put("homeRelay", "$base/relay")
    }

    fun ticketGrant(secret: String, devices: List<String> = emptyList()): JSONObject =
        JSONObject()
            .put(
                "matcher",
                JSONObject().put("type", "ticket").put("secretHash", Tickets.sha256Hex(secret)),
            )
            .put("devices", JSONArray(devices))

    // ---- routing ------------------------------------------------------------

    private fun route(request: RecordedRequest): MockResponse {
        val path = request.requestUrl?.encodedPath.orEmpty()
        val q: (String) -> String = { request.requestUrl?.queryParameter(it).orEmpty() }
        return try {
            when {
                path == "/xrpc/com.atproto.identity.resolveHandle" -> {
                    val did = accounts.entries.firstOrNull { it.value == q("handle") }?.key
                    if (did == null) json(400, """{"error":"InvalidRequest"}""")
                    else json(200, JSONObject().put("did", did).toString())
                }
                path == "/xrpc/app.bsky.graph.getRelationships" -> {
                    val rel = JSONObject().put("following", "at://x/follow/1")
                    if (setOf(q("actor"), q("others")) in mutuals) {
                        rel.put("followedBy", "at://y/follow/2")
                    }
                    json(200, JSONObject().put("relationships", JSONArray().put(rel)).toString())
                }
                path.startsWith("/plc/") -> {
                    val did = URLDecoder.decode(path.removePrefix("/plc/"), "UTF-8")
                    if (did !in accounts) json(404, """{"error":"not found"}""")
                    else json(200, didDoc(did))
                }
                path == "/xrpc/com.atproto.repo.getRecord" -> {
                    val value = records["${q("repo")}/${q("collection")}/${q("rkey")}"]
                    if (value == null) json(400, """{"error":"RecordNotFound"}""")
                    else json(200, JSONObject().put("value", value).toString())
                }
                path == "/xrpc/com.atproto.repo.listRecords" -> {
                    val prefix = "${q("repo")}/${q("collection")}/"
                    val rows = JSONArray()
                    for ((key, value) in records) {
                        if (key.startsWith(prefix)) {
                            rows.put(JSONObject().put("uri", "at://$key").put("value", value))
                        }
                    }
                    json(200, JSONObject().put("records", rows).toString())
                }
                path == "/xrpc/com.atproto.server.getServiceAuth" -> serviceAuth(request)
                path == "/.well-known/oauth-protected-resource" ->
                    json(200, JSONObject().put("authorization_servers", JSONArray().put(base)).toString())
                path == "/.well-known/oauth-authorization-server" ->
                    json(
                        200,
                        JSONObject()
                            .put("issuer", base)
                            .put("pushed_authorization_request_endpoint", "$base/oauth/par")
                            .put("authorization_endpoint", "$base/oauth/authorize")
                            .put("token_endpoint", "$base/oauth/token")
                            .toString(),
                    )
                path == "/oauth/par" -> par(request)
                path == "/oauth/token" -> token(request)
                path == "/grantCall" -> grantCall(request)
                else -> json(404, """{"error":"no fixture route for $path"}""")
            }
        } catch (t: Throwable) {
            json(500, """{"error":"fixture: ${t.message}"}""")
        }
    }

    /** RFC 9449 §8: a DPoP request whose proof lacks our nonce is bounced
     *  with the nonce; the engine retries once. Applied to PAR, token, and
     *  getServiceAuth alike — the real entryway does. */
    private fun nonceGate(request: RecordedRequest): MockResponse? {
        val proofClaims = request.getHeader("DPoP")?.split(".")?.getOrNull(1)
            ?.let { String(java.util.Base64.getUrlDecoder().decode(it)) }.orEmpty()
        if (proofClaims.contains("\"nonce\":\"$SERVICE_NONCE\"")) return null
        return json(400, """{"error":"use_dpop_nonce"}""").setHeader("DPoP-Nonce", SERVICE_NONCE)
    }

    private fun formFields(request: RecordedRequest): Map<String, String> =
        request.body.readUtf8().split('&').filter { it.contains('=') }.associate {
            val (k, v) = it.split('=', limit = 2)
            k to URLDecoder.decode(v, "UTF-8")
        }

    private fun par(request: RecordedRequest): MockResponse {
        nonceGate(request)?.let { return it }
        val fields = formFields(request)
        parRequests += fields
        return json(200, """{"request_uri":"urn:fx:r${parRequests.size}","expires_in":60}""")
    }

    /** The token endpoint: code exchange and single-use refresh rotation,
     *  both minting a numbered pair so tests can watch the rotation. */
    private fun token(request: RecordedRequest): MockResponse {
        nonceGate(request)?.let { return it }
        val fields = formFields(request)
        val did = accounts.keys.firstOrNull() ?: "did:plc:nobody"
        return when (fields["grant_type"]) {
            "authorization_code", "refresh_token" -> {
                tokenSerial += 1
                json(
                    200,
                    JSONObject()
                        .put("access_token", "fx-at-$tokenSerial")
                        .put("refresh_token", "fx-rt-$tokenSerial")
                        .put("token_type", "DPoP")
                        .put("expires_in", 1799)
                        .put("scope", fields["scope"] ?: "atproto")
                        .put("sub", fields["_sub"] ?: did)
                        .toString(),
                )
            }
            else -> json(400, """{"error":"unsupported_grant_type"}""")
        }
    }

    /**
     * The PDS service-auth mint, with the RFC 9449 resource-server nonce
     * dance the real bsky PDS performs: a proof without our nonce gets a
     * 401 + `DPoP-Nonce`; a nonce-carrying retry with a DPoP-scheme access
     * token mints. Structural checks only — the cryptographic half is the
     * real PDS's (and croft-admit verifies the signature on its side).
     */
    private fun serviceAuth(request: RecordedRequest): MockResponse {
        val authz = request.getHeader("Authorization").orEmpty()
        if (!authz.startsWith("DPoP ")) {
            return json(401, """{"error":"AuthRequired"}""")
        }
        val proofClaims = request.getHeader("DPoP")?.split(".")?.getOrNull(1)
            ?.let { String(java.util.Base64.getUrlDecoder().decode(it)) }.orEmpty()
        if (!proofClaims.contains("\"nonce\":\"$SERVICE_NONCE\"")) {
            return json(401, """{"error":"use_dpop_nonce"}""")
                .setHeader("DPoP-Nonce", SERVICE_NONCE)
        }
        return json(200, JSONObject().put("token", "svc-jwt-fixture").toString())
    }

    /**
     * The admit mirror: fresh grant read (revocation bites the next mint),
     * ticket possession or a structurally-present serviceAuth (the real
     * signature verification is the server's — chunk M4d(b) proves it),
     * refusal discriminants verbatim from `mint.rs`.
     */
    private fun grantCall(request: RecordedRequest): MockResponse {
        val body = JSONObject(request.body.readUtf8())
        mints += body
        nextMintRefusal?.let {
            nextMintRefusal = null
            return json(if (it == "unavailable") 503 else 403, """{"error":"$it"}""")
        }
        if (body.optString("grant").isEmpty() || !body.has("proof")) {
            return json(403, """{"error":"no_cap"}""")
        }
        val grant = records["${body.optString("callee")}/ing.croft.call.grant/${body.optString("grant")}"]
            ?: return json(403, """{"error":"cap_not_found"}""")
        val proof = body.getJSONObject("proof")
        val matcher = grant.getJSONObject("matcher")
        val admitted = when (matcher.optString("type")) {
            "ticket" ->
                proof.has("ticket") &&
                    Tickets.sha256Hex(proof.getString("ticket")) ==
                    matcher.getString("secretHash").lowercase()
            "registeredCallers", "mutuals" -> proof.has("serviceAuth")
            else -> false
        }
        if (!admitted) return json(403, """{"error":"cap_mismatch"}""")
        // Policy rules, like the real mint: a failed rule is revocation.
        val policyRef = grant.optString("policyRef")
        if (policyRef.isNotEmpty()) {
            val policy = records["${body.optString("callee")}/ing.croft.call.policy/$policyRef"]
                ?: return json(403, """{"error":"cap_revoked"}""")
            val rules = policy.optJSONArray("rules") ?: JSONArray()
            for (i in 0 until rules.length()) {
                val rule = rules.getJSONObject(i)
                if (rule.optString("type") == "expires" &&
                    nowMs > java.time.Instant.parse(rule.getString("at")).toEpochMilli()
                ) {
                    return json(403, """{"error":"cap_revoked"}""")
                }
            }
        }
        return json(200, JSONObject().put("token", "fixture-token-${mints.size}").toString())
    }

    private fun didDoc(did: String): String =
        JSONObject()
            .put("id", did)
            .put(
                "service",
                JSONArray().put(
                    JSONObject()
                        .put("id", "#atproto_pds")
                        .put("type", "AtprotoPersonalDataServer")
                        .put("serviceEndpoint", base),
                ),
            )
            .toString()

    private fun json(status: Int, body: String): MockResponse =
        MockResponse()
            .setResponseCode(status)
            .setHeader("Content-Type", "application/json")
            .setBody(body)

    companion object {
        /** The fixture PDS's DPoP nonce (see [serviceAuth]). */
        const val SERVICE_NONCE = "fx-nonce-1"
    }
}

/**
 * An /etc/hosts for workflow tests: rewrites the two hardcoded production
 * hosts (plc.directory, the public AppView) onto the fixture, then delegates
 * to the REAL port — real sockets, real encoding, real parsing. The PDS
 * never needs rewriting: the fixture's DID documents name the fixture
 * itself as the PDS, so the engine walks there on its own.
 */
class Rewired(private val delegate: Http, private val fixtureBase: String) : Http {
    override suspend fun getJson(url: String): String = delegate.getJson(rewrite(url))

    private fun rewrite(url: String): String = when {
        url.startsWith("https://plc.directory/") ->
            url.replace("https://plc.directory/", "$fixtureBase/plc/")
        url.startsWith("https://public.api.bsky.app/") ->
            url.replace("https://public.api.bsky.app/", "$fixtureBase/")
        else -> url
    }
}
