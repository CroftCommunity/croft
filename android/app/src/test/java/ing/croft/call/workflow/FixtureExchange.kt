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
                path == "/grantCall" -> grantCall(request)
                else -> json(404, """{"error":"no fixture route for $path"}""")
            }
        } catch (t: Throwable) {
            json(500, """{"error":"fixture: ${t.message}"}""")
        }
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
