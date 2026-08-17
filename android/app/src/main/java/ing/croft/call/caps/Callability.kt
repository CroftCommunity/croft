package ing.croft.call.caps

import org.json.JSONObject

/** Who is asking — the call-side facts a matcher can consume (§2/§7).
 *  provenDid arrives with M3 (OAuth); until then lookups run unproven. */
data class CallerContext(
    val provenDid: String? = null,
    val secret: String? = null,
)

/**
 * The callability resolver (Phase 11 M2) — the rendered-principal seam.
 * Callability is derived, never looked up: "does any grant admit me, and do
 * its rules still hold" (handoff's key correction; matchers OR together).
 * Rules are checked with lookup-time facts only (usesSoFar is a call-time
 * fact the relay owns — here it is 0, matching the §6 redeem stance).
 */
object Callability {

    sealed interface State {
        /** No endpoint records — the normal case for most of the world. */
        data object NotListed : State

        /** Endpoints exist but no grant admits this caller. */
        data object MayNotPermit : State

        /** [grant] admits the caller; [devices] empty means all devices. */
        data class Callable(val grant: String, val devices: List<String>) : State
    }

    suspend fun resolve(
        http: Http,
        principal: String,
        context: CallerContext,
        now: Long,
    ): State {
        val did =
            if (principal.startsWith("did:")) principal
            else Xrpc.resolveHandle(http, principal)
        val pds = Xrpc.resolvePds(http, did)

        val endpoints = Xrpc.listEndpoints(http, pds, did)
        if (endpoints.isEmpty()) return State.NotListed

        for ((rkey, grant) in Xrpc.listGrants(http, pds, did)) {
            if (grant.policyRef.isNotEmpty()) {
                val policy = Xrpc.fetchPolicy(http, pds, did, grant.policyRef)
                if (!Rules.evaluate(policy.rules, now = now, usesSoFar = 0)) continue
            }
            if (Evaluate.matcher(http, grant.matcher, context, calleeDid = did)) {
                return State.Callable(grant = rkey, devices = grant.devices)
            }
        }
        return State.MayNotPermit
    }
}

/**
 * Matcher evaluation (§2) — the Kotlin mirror of resolver.js
 * `evaluateMatcher`/`areMutuals`. Fails closed: no identity, no secret, or
 * an unknown type all deny.
 */
object Evaluate {

    suspend fun matcher(
        http: Http,
        matcher: Matcher,
        context: CallerContext,
        calleeDid: String,
    ): Boolean = when (matcher) {
        is Matcher.Ticket ->
            context.secret != null &&
                Tickets.verifyTicketSecret(context.secret, matcher.secretHash)
        Matcher.Mutuals ->
            context.provenDid != null && areMutuals(http, context.provenDid, calleeDid)
        is Matcher.RegisteredCallers ->
            context.provenDid != null && context.provenDid in matcher.dids
        is Matcher.Unknown -> false
    }

    /**
     * Mutual iff the actor both follows and is followed by the other —
     * app.bsky.graph.getRelationships on the public AppView, no auth.
     */
    suspend fun areMutuals(http: Http, actorDid: String, otherDid: String): Boolean {
        val body = http.getJson(
            "${Xrpc.APPVIEW}/xrpc/app.bsky.graph.getRelationships" +
                "?actor=$actorDid&others=$otherDid",
        )
        val rel = JSONObject(body).optJSONArray("relationships")?.optJSONObject(0)
            ?: return false
        return rel.optString("following").isNotEmpty() &&
            rel.optString("followedBy").isNotEmpty()
    }
}
