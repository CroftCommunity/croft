package ing.croft.call.caps

import org.json.JSONObject
import java.net.URLEncoder
import java.time.Instant

/**
 * The one seam to the network. Implementations perform an HTTP GET and
 * return the response body; they must throw on a non-2xx status. Injected
 * everywhere (mirroring resolver.js's `fetchImpl` parameter) so the engine
 * stays testable with canned routes and portable to the shared core.
 */
fun interface Http {
    suspend fun getJson(url: String): String
}

/**
 * XRPC discovery — the Kotlin mirror of resolver.js's fetch layer:
 * handle → DID → PDS → endpoint/grant/policy records. All reads are
 * unauthenticated (the contract keeps the records world-readable).
 */
object Xrpc {
    const val APPVIEW = "https://public.api.bsky.app"
    const val ENDPOINT_COLLECTION = "ing.croft.iroh.endpoint"
    const val GRANT_COLLECTION = "ing.croft.call.grant"
    const val POLICY_COLLECTION = "ing.croft.call.policy"

    /** Bare lowercase handle (drops a leading @) → DID via the public AppView. */
    suspend fun resolveHandle(http: Http, handle: String): String {
        val clean = handle.trim().removePrefix("@").lowercase()
        val body = http.getJson(
            "$APPVIEW/xrpc/com.atproto.identity.resolveHandle?handle=${enc(clean)}",
        )
        return JSONObject(body).getString("did")
    }

    /**
     * DID → PDS base URL. `did:plc:` via plc.directory; `did:web:` via the
     * host's /.well-known/did.json. Anything else is refused.
     */
    suspend fun resolvePds(http: Http, did: String): String {
        val docUrl = when {
            did.startsWith("did:plc:") -> "https://plc.directory/${enc(did)}"
            did.startsWith("did:web:") -> {
                val host = did.removePrefix("did:web:").substringBefore(':')
                "https://$host/.well-known/did.json"
            }
            else -> throw IllegalArgumentException("unsupported DID method: $did")
        }
        val doc = JSONObject(http.getJson(docUrl))
        val services = doc.optJSONArray("service") ?: return failNoPds()
        for (i in 0 until services.length()) {
            val svc = services.getJSONObject(i)
            val id = svc.optString("id")
            if (id == "#atproto_pds" || id.endsWith("#atproto_pds") ||
                svc.optString("type") == "AtprotoPersonalDataServer"
            ) {
                return svc.getString("serviceEndpoint")
            }
        }
        return failNoPds()
    }

    /** One device's endpoint record (§1). Refuses a record with no endpointId. */
    suspend fun fetchEndpoint(http: Http, pdsUrl: String, did: String, rkey: String): Endpoint {
        val value = recordValue(http, pdsUrl, did, ENDPOINT_COLLECTION, rkey)
        val endpointId = value.optString("endpointId")
        check(endpointId.isNotEmpty()) { "endpoint record missing endpointId" }
        return Endpoint(
            endpointId = endpointId,
            homeRelay = value.optString("homeRelay"),
            label = value.optString("label"),
        )
    }

    /** A grant record (§2). Refuses a grant with no matcher. */
    suspend fun fetchGrant(http: Http, pdsUrl: String, did: String, rkey: String): Grant {
        val value = recordValue(http, pdsUrl, did, GRANT_COLLECTION, rkey)
        val m = value.optJSONObject("matcher")
            ?: throw IllegalStateException("grant record missing matcher")
        return Grant(
            matcher = parseMatcher(m),
            devices = stringList(value, "devices"),
            policyRef = value.optString("policyRef"),
        )
    }

    /** A policy record (§3). Unknown rule types are preserved, not dropped. */
    suspend fun fetchPolicy(http: Http, pdsUrl: String, did: String, rkey: String): Policy {
        val value = recordValue(http, pdsUrl, did, POLICY_COLLECTION, rkey)
        val rulesJson = value.optJSONArray("rules")
        val rules = buildList {
            for (i in 0 until (rulesJson?.length() ?: 0)) {
                add(parseRule(rulesJson!!.getJSONObject(i)))
            }
        }
        return Policy(rules = rules, label = value.optString("label"))
    }

    /** All of a repo's devices (§1) via listRecords; malformed records
     *  (no endpointId) are skipped, mirroring resolver.js listEndpoints. */
    suspend fun listEndpoints(http: Http, pdsUrl: String, did: String): List<Endpoint> =
        listValues(http, pdsUrl, did, ENDPOINT_COLLECTION)
            .filter { (_, v) -> v.optString("endpointId").isNotEmpty() }
            .map { (rkey, v) ->
                Endpoint(
                    endpointId = v.getString("endpointId"),
                    homeRelay = v.optString("homeRelay"),
                    label = v.optString("label").ifEmpty { rkey },
                )
            }

    /** All of a repo's grants (§2) as rkey → Grant; records with no matcher
     *  are skipped (malformed, not deniable — they can never admit anyway). */
    suspend fun listGrants(http: Http, pdsUrl: String, did: String): List<Pair<String, Grant>> =
        listValues(http, pdsUrl, did, GRANT_COLLECTION)
            .mapNotNull { (rkey, v) ->
                val m = v.optJSONObject("matcher") ?: return@mapNotNull null
                rkey to Grant(
                    matcher = parseMatcher(m),
                    devices = stringList(v, "devices"),
                    policyRef = v.optString("policyRef"),
                )
            }

    private suspend fun listValues(
        http: Http, pdsUrl: String, did: String, collection: String,
    ): List<Pair<String, JSONObject>> {
        val body = http.getJson(
            pdsUrl.trimEnd('/') +
                "/xrpc/com.atproto.repo.listRecords?repo=${enc(did)}&collection=$collection",
        )
        val records = JSONObject(body).optJSONArray("records") ?: return emptyList()
        return buildList {
            for (i in 0 until records.length()) {
                val rec = records.getJSONObject(i)
                val rkey = rec.optString("uri").substringAfterLast('/')
                add(rkey to (rec.optJSONObject("value") ?: JSONObject()))
            }
        }
    }

    private fun parseMatcher(m: JSONObject): Matcher = when (val type = m.optString("type")) {
        "ticket" -> Matcher.Ticket(secretHash = m.getString("secretHash"))
        "mutuals" -> Matcher.Mutuals
        "registeredCallers" -> Matcher.RegisteredCallers(dids = stringList(m, "dids"))
        else -> Matcher.Unknown(type = type)
    }

    private fun parseRule(r: JSONObject): Rule = when (val type = r.optString("type")) {
        "expires" -> Rule.Expires(atEpochMs = Instant.parse(r.getString("at")).toEpochMilli())
        "maxUses" -> Rule.MaxUses(n = r.getInt("n"))
        "burnOnSuccess" -> Rule.BurnOnSuccess
        else -> Rule.Unknown(type = type)
    }

    private suspend fun recordValue(
        http: Http, pdsUrl: String, did: String, collection: String, rkey: String,
    ): JSONObject {
        val body = http.getJson(
            pdsUrl.trimEnd('/') +
                "/xrpc/com.atproto.repo.getRecord?repo=${enc(did)}" +
                "&collection=$collection&rkey=${enc(rkey)}",
        )
        return JSONObject(body).optJSONObject("value") ?: JSONObject()
    }

    private fun stringList(o: JSONObject, key: String): List<String> {
        val arr = o.optJSONArray(key) ?: return emptyList()
        return buildList { for (i in 0 until arr.length()) add(arr.getString(i)) }
    }

    private fun failNoPds(): Nothing =
        throw IllegalStateException("no PDS service in DID document")

    private fun enc(s: String): String = URLEncoder.encode(s, "UTF-8")
}
