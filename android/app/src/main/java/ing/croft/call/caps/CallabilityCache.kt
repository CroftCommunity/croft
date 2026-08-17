package ing.croft.call.caps

/**
 * The TTL half of decision D1: lazy-on-tap resolution, with derived state
 * cached so a repeat action against the same principal inside the TTL does
 * not re-leak a PDS lookup. Keys include the caller identity — a different
 * proven DID must never see another identity's answer. Stores derived
 * [Callability.State] only, never raw records. The clock is an input.
 */
class CallabilityCache(private val ttlMs: Long) {

    private data class Entry(val state: Callability.State, val expiresAt: Long)

    private val entries = mutableMapOf<String, Entry>()

    fun get(principal: String, context: CallerContext, now: Long): Callability.State? {
        val entry = entries[key(principal, context)] ?: return null
        if (now > entry.expiresAt) return null
        return entry.state
    }

    fun put(principal: String, context: CallerContext, state: Callability.State, now: Long) {
        entries[key(principal, context)] = Entry(state, expiresAt = now + ttlMs)
    }

    private fun key(principal: String, context: CallerContext): String =
        "$principal|${context.provenDid.orEmpty()}"
}
