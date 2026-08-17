package ing.croft.call.caps

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * The callability seam the UI consumes (M3 Phase 4): lazy lookup on
 * callee arrival, TTL-cached per (principal, identity) so a repeat costs
 * no network, and identity-keyed so signed-in and signed-out answers
 * never bleed into each other. Clock injected; canned routes carry the
 * live fixtures' record shapes (m3registered names the caller DID).
 * Runs under Robolectric for a real org.json (suite convention).
 */
@RunWith(RobolectricTestRunner::class)
class CallabilityStatusTest {

    private val callee = "did:plc:xyfhcaweaeyew3zrgk6jaln7"
    private val caller = "did:plc:l5xigmplwu7eyxjobjr23iza"
    private val pds = "https://stropharia.us-west.host.bsky.network"

    private class FakeHttp(private val routes: Map<String, String>) : Http {
        var calls = 0
        override suspend fun getJson(url: String): String {
            calls++
            return routes.entries.firstOrNull { url.contains(it.key) }?.value
                ?: throw IllegalStateException("unrouted GET $url")
        }
    }

    private fun http() = FakeHttp(
        mapOf(
            "plc.directory" to
                """{"service":[{"id":"#atproto_pds","type":"AtprotoPersonalDataServer","serviceEndpoint":"$pds"}]}""",
            "collection=ing.croft.iroh.endpoint" to
                """{"records":[{"uri":"at://$callee/ing.croft.iroh.endpoint/self",
                    "value":{"endpointId":"14af","homeRelay":"https://relay.croft.ing:8443"}}]}""",
            "collection=ing.croft.call.grant" to
                """{"records":[{"uri":"at://$callee/ing.croft.call.grant/m3registered",
                    "value":{"matcher":{"type":"registeredCallers","dids":["$caller"]},"devices":["self"]}}]}""",
        ),
    )

    @Test
    fun `signed out is MayNotPermit and a repeat inside the TTL costs no network`(): Unit = runBlocking {
        val http = http()
        var now = 1_000L
        val status = CallabilityStatus(http, ttlMs = 300_000, nowMs = { now })

        val first = status.lookup(callee, provenDid = null)
        assertEquals(Callability.State.MayNotPermit, first.state)
        assertTrue(!first.fromCache)
        assertEquals(first.state, status.state.value)

        val callsAfterFirst = http.calls
        now += 60_000 // one minute later, well inside the TTL
        val second = status.lookup(callee, provenDid = null)
        assertTrue(second.fromCache)
        assertEquals(callsAfterFirst, http.calls) // no new network
    }

    @Test
    fun `signing in flips the same principal to Callable via its grant`(): Unit = runBlocking {
        val http = http()
        val status = CallabilityStatus(http, ttlMs = 300_000, nowMs = { 1_000L })

        assertEquals(Callability.State.MayNotPermit, status.lookup(callee, provenDid = null).state)
        val flipped = status.lookup(callee, provenDid = caller)
        assertTrue(!flipped.fromCache) // different identity key — never the cached answer
        assertEquals(
            Callability.State.Callable(grant = "m3registered", devices = listOf("self")),
            flipped.state,
        )
    }

    @Test
    fun `signing out flips back immediately - the signed-out key is distinct`(): Unit = runBlocking {
        val http = http()
        val status = CallabilityStatus(http, ttlMs = 300_000, nowMs = { 1_000L })

        status.lookup(callee, provenDid = caller)
        val back = status.lookup(callee, provenDid = null)
        assertEquals(Callability.State.MayNotPermit, back.state)
    }

    @Test
    fun `past the TTL the lookup goes back to the network`(): Unit = runBlocking {
        val http = http()
        var now = 1_000L
        val status = CallabilityStatus(http, ttlMs = 300_000, nowMs = { now })

        status.lookup(callee, provenDid = null)
        val callsAfterFirst = http.calls
        now += 300_001 // just past the TTL boundary
        val again = status.lookup(callee, provenDid = null)
        assertTrue(!again.fromCache)
        assertTrue(http.calls > callsAfterFirst)
    }
}
