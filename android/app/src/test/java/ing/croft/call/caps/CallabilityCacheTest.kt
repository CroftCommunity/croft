package ing.croft.call.caps

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * TTL cache tests — the second half of decision D1: resolution runs only on
 * explicit user action (lazy-on-tap), and the cache keeps a repeat action
 * against the same principal from re-leaking a PDS lookup inside the TTL.
 * The cache stores DERIVED state only, keyed by principal + caller identity
 * (a different proven identity must never see another identity's answer).
 * The clock is an input.
 */
class CallabilityCacheTest {

    private val now = 1_700_000_000_000L
    private val callable = Callability.State.Callable(grant = "g1", devices = emptyList())

    @Test
    fun `a fresh entry is returned inside the ttl`() {
        val cache = CallabilityCache(ttlMs = 60_000)
        cache.put("alice.test", CallerContext(), callable, now = now)
        assertEquals(callable, cache.get("alice.test", CallerContext(), now = now + 59_999))
    }

    @Test
    fun `an entry past the ttl is gone`() {
        val cache = CallabilityCache(ttlMs = 60_000)
        cache.put("alice.test", CallerContext(), callable, now = now)
        assertNull(cache.get("alice.test", CallerContext(), now = now + 60_001))
    }

    @Test
    fun `a different caller identity never sees another identity's answer`() {
        val cache = CallabilityCache(ttlMs = 60_000)
        cache.put("alice.test", CallerContext(provenDid = "did:plc:me"), callable, now = now)
        assertNull(cache.get("alice.test", CallerContext(), now = now))
        assertNull(cache.get("alice.test", CallerContext(provenDid = "did:plc:other"), now = now))
    }

    @Test
    fun `an unknown principal is a miss`() {
        val cache = CallabilityCache(ttlMs = 60_000)
        assertNull(cache.get("nobody.test", CallerContext(), now = now))
    }
}
