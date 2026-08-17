package ing.croft.call.caps

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * XRPC discovery tests — the Kotlin mirror of resolver.js's fetch layer,
 * with the same injected-fetch shape (routes keyed by URL substring, first
 * match wins) as connect/web-tests. Runs under Robolectric for org.json.
 */
@RunWith(RobolectricTestRunner::class)
class XrpcTest {

    private class FakeHttp(private val routes: Map<String, String>) : Http {
        val calls = mutableListOf<String>()
        override suspend fun getJson(url: String): String {
            calls += url
            routes.entries.firstOrNull { url.contains(it.key) }?.let { return it.value }
            throw IllegalStateException("no route for $url")
        }
    }

    @Test
    fun `resolveHandle normalizes and returns the did`() = runBlocking {
        val http = FakeHttp(mapOf("resolveHandle" to """{"did":"did:plc:abc"}"""))
        assertEquals("did:plc:abc", Xrpc.resolveHandle(http, "@Alice.Test "))
        assertTrue(http.calls[0].contains("handle=alice.test"))
    }

    @Test
    fun `resolvePds reads the atproto_pds service from a plc did doc`() = runBlocking {
        val http = FakeHttp(mapOf(
            "plc.directory" to
                """{"service":[{"id":"#atproto_pds","type":"AtprotoPersonalDataServer","serviceEndpoint":"https://pds.example.com"}]}""",
        ))
        assertEquals("https://pds.example.com", Xrpc.resolvePds(http, "did:plc:abc"))
    }

    @Test
    fun `resolvePds refuses an unsupported did method`() {
        val http = FakeHttp(emptyMap())
        assertThrows(IllegalArgumentException::class.java) {
            runBlocking { Xrpc.resolvePds(http, "did:key:zabc") }
        }
    }

    @Test
    fun `resolvePds refuses a doc with no pds service`() {
        val http = FakeHttp(mapOf("plc.directory" to """{"service":[]}"""))
        assertThrows(IllegalStateException::class.java) {
            runBlocking { Xrpc.resolvePds(http, "did:plc:abc") }
        }
    }

    @Test
    fun `fetchEndpoint returns the endpoint and refuses one without endpointId`(): Unit = runBlocking {
        val http = FakeHttp(mapOf(
            "getRecord" to """{"value":{"endpointId":"ep1","homeRelay":"https://r.example"}}""",
        ))
        val ep = Xrpc.fetchEndpoint(http, "https://pds.example.com", "did:plc:abc", "self")
        assertEquals("ep1", ep.endpointId)
        assertEquals("https://r.example", ep.homeRelay)
        assertTrue(http.calls[0].contains("collection=ing.croft.iroh.endpoint"))

        val bad = FakeHttp(mapOf("getRecord" to """{"value":{"label":"no id"}}"""))
        assertThrows(IllegalStateException::class.java) {
            runBlocking { Xrpc.fetchEndpoint(bad, "https://pds.example.com", "did:plc:abc", "self") }
        }
    }

    @Test
    fun `fetchGrant parses a ticket matcher and refuses a grant without one`(): Unit = runBlocking {
        val http = FakeHttp(mapOf(
            "getRecord" to
                """{"value":{"matcher":{"type":"ticket","secretHash":"ab"},"devices":["phone"],"policyRef":"p1"}}""",
        ))
        val g = Xrpc.fetchGrant(http, "https://pds.example.com", "did:plc:abc", "g1")
        assertEquals(Matcher.Ticket(secretHash = "ab"), g.matcher)
        assertEquals(listOf("phone"), g.devices)
        assertEquals("p1", g.policyRef)
        assertTrue(http.calls[0].contains("collection=ing.croft.call.grant"))

        val bad = FakeHttp(mapOf("getRecord" to """{"value":{}}"""))
        assertThrows(IllegalStateException::class.java) {
            runBlocking { Xrpc.fetchGrant(bad, "https://pds.example.com", "did:plc:abc", "g1") }
        }
    }

    @Test
    fun `an unknown matcher type is preserved as Unknown, not guessed`() = runBlocking {
        val http = FakeHttp(mapOf(
            "getRecord" to """{"value":{"matcher":{"type":"followsMe"}}}""",
        ))
        val g = Xrpc.fetchGrant(http, "https://pds.example.com", "did:plc:abc", "g1")
        assertEquals(Matcher.Unknown(type = "followsMe"), g.matcher)
    }

    @Test
    fun `fetchPolicy parses tagged rules, unknown types preserved`() = runBlocking {
        val http = FakeHttp(mapOf(
            "getRecord" to
                """{"value":{"rules":[{"type":"expires","at":"2027-01-01T00:00:00Z"},{"type":"maxUses","n":3},{"type":"burnOnSuccess"},{"type":"frobnicate"}]}}""",
        ))
        val p = Xrpc.fetchPolicy(http, "https://pds.example.com", "did:plc:abc", "p1")
        assertEquals(
            listOf(
                Rule.Expires(atEpochMs = 1_798_761_600_000L),
                Rule.MaxUses(n = 3),
                Rule.BurnOnSuccess,
                Rule.Unknown(type = "frobnicate"),
            ),
            p.rules,
        )
        assertTrue(http.calls[0].contains("collection=ing.croft.call.policy"))
    }
}
