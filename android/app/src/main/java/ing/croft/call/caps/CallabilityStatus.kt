package ing.croft.call.caps

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * The callability seam the UI consumes (M3 Phase 4). Resolution is lazy —
 * [lookup] runs only because a callee arrived or the user acted (decision
 * D1) — and the derived answer is TTL-cached per (principal, identity) via
 * [CallabilityCache], so a repeat inside the TTL costs no PDS lookup and a
 * different proven DID can never read another identity's answer. Pure
 * given its inputs (clock injected, network behind [Http]); the ViewModel
 * owns one instance and logs outcomes at the effect edge.
 */
class CallabilityStatus(
    private val http: Http,
    ttlMs: Long = DEFAULT_TTL_MS,
    private val nowMs: () -> Long = System::currentTimeMillis,
) {

    /** One lookup's answer; [fromCache] is for the caller's logging. */
    data class Lookup(val state: Callability.State, val fromCache: Boolean)

    private val cache = CallabilityCache(ttlMs)

    private val _state = MutableStateFlow<Callability.State?>(null)

    /** The latest derived state for the UI; null before any lookup. */
    val state: StateFlow<Callability.State?> = _state

    suspend fun lookup(principal: String, provenDid: String?, secret: String? = null): Lookup {
        val context = CallerContext(provenDid = provenDid, secret = secret)
        cache.get(principal, context, nowMs())?.let {
            _state.value = it
            return Lookup(it, fromCache = true)
        }
        val resolved = Callability.resolve(http, principal, context, nowMs())
        cache.put(principal, context, resolved, nowMs())
        _state.value = resolved
        return Lookup(resolved, fromCache = false)
    }

    companion object {
        /** Plan default: repeats within five minutes re-leak nothing. */
        const val DEFAULT_TTL_MS = 5 * 60_000L
    }
}
