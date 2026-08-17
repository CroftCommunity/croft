package ing.croft.call.caps

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * Callability resolver tests (Phase 11 M2) — the rendered-principal seam.
 * Callability is DERIVED, not looked up (the handoff's key correction):
 * "does any grant admit me, and do its rules still hold." Three outcomes:
 *   NotListed     — no endpoint records (the normal case)
 *   Callable      — endpoints exist AND some grant admits this caller now
 *   MayNotPermit  — endpoints exist but no grant admits this caller
 * Fails closed: with no proven identity and no secret, ticket and identity
 * grants alike deny — a pre-OAuth client honestly sees MayNotPermit.
 */
@RunWith(RobolectricTestRunner::class)
class CallabilityTest {

    private val now = 1_700_000_000_000L

    private class FakeHttp(private val routes: Map<String, String>) : Http {
        val calls = mutableListOf<String>()
        override suspend fun getJson(url: String): String {
            calls += url
            routes.entries.firstOrNull { url.contains(it.key) }?.let { return it.value }
            throw IllegalStateException("no route for $url")
        }
    }

    private val discovery = mapOf(
        "resolveHandle" to """{"did":"did:plc:callee"}""",
        "plc.directory" to
            """{"service":[{"id":"#atproto_pds","serviceEndpoint":"https://pds.example.com"}]}""",
    )

    private fun endpointList(vararg rkeys: String): String {
        val records = rkeys.joinToString(",") {
            """{"uri":"at://did:plc:callee/ing.croft.iroh.endpoint/$it","value":{"endpointId":"ep-$it","homeRelay":"https://r.example"}}"""
        }
        return """{"records":[$records]}"""
    }

    @Test
    fun `no endpoint records derives NotListed`() = runBlocking {
        val http = FakeHttp(discovery + mapOf(
            "ing.croft.iroh.endpoint" to """{"records":[]}""",
        ))
        val c = Callability.resolve(http, "alice.test", CallerContext(), now)
        assertEquals(Callability.State.NotListed, c)
    }

    @Test
    fun `endpoints but no grants derives MayNotPermit`() = runBlocking {
        val http = FakeHttp(discovery + mapOf(
            "ing.croft.iroh.endpoint" to endpointList("self"),
            "ing.croft.call.grant" to """{"records":[]}""",
        ))
        val c = Callability.resolve(http, "alice.test", CallerContext(), now)
        assertEquals(Callability.State.MayNotPermit, c)
    }

    @Test
    fun `a registeredCallers grant admits a proven caller`() = runBlocking {
        val http = FakeHttp(discovery + mapOf(
            "ing.croft.iroh.endpoint" to endpointList("self", "phone"),
            "ing.croft.call.grant" to
                """{"records":[{"uri":"at://did:plc:callee/ing.croft.call.grant/g1","value":{"matcher":{"type":"registeredCallers","dids":["did:plc:me"]},"devices":["phone"]}}]}""",
        ))
        val c = Callability.resolve(
            http, "alice.test", CallerContext(provenDid = "did:plc:me"), now,
        )
        assertEquals(
            Callability.State.Callable(grant = "g1", devices = listOf("phone")),
            c,
        )
    }

    @Test
    fun `the same grant refuses an unproven or unlisted caller`() = runBlocking {
        val routes = discovery + mapOf(
            "ing.croft.iroh.endpoint" to endpointList("self"),
            "ing.croft.call.grant" to
                """{"records":[{"uri":"at://did:plc:callee/ing.croft.call.grant/g1","value":{"matcher":{"type":"registeredCallers","dids":["did:plc:me"]}}}]}""",
        )
        assertEquals(
            Callability.State.MayNotPermit,
            Callability.resolve(FakeHttp(routes), "alice.test", CallerContext(), now),
        )
        assertEquals(
            Callability.State.MayNotPermit,
            Callability.resolve(
                FakeHttp(routes), "alice.test", CallerContext(provenDid = "did:plc:stranger"), now,
            ),
        )
    }

    @Test
    fun `a mutuals grant admits when the appview says both directions follow`() = runBlocking {
        val http = FakeHttp(discovery + mapOf(
            "ing.croft.iroh.endpoint" to endpointList("self"),
            "ing.croft.call.grant" to
                """{"records":[{"uri":"at://did:plc:callee/ing.croft.call.grant/g1","value":{"matcher":{"type":"mutuals"}}}]}""",
            "getRelationships" to
                """{"relationships":[{"following":"at://f1","followedBy":"at://f2"}]}""",
        ))
        val c = Callability.resolve(
            http, "alice.test", CallerContext(provenDid = "did:plc:me"), now,
        )
        assertEquals(
            Callability.State.Callable(grant = "g1", devices = emptyList()),
            c,
        )
    }

    @Test
    fun `an expired grant cannot admit`() = runBlocking {
        val http = FakeHttp(discovery + mapOf(
            "ing.croft.iroh.endpoint" to endpointList("self"),
            "ing.croft.call.grant" to
                """{"records":[{"uri":"at://did:plc:callee/ing.croft.call.grant/g1","value":{"matcher":{"type":"registeredCallers","dids":["did:plc:me"]},"policyRef":"p1"}}]}""",
            "ing.croft.call.policy" to
                """{"value":{"rules":[{"type":"expires","at":"2020-01-01T00:00:00Z"}]}}""",
        ))
        val c = Callability.resolve(
            http, "alice.test", CallerContext(provenDid = "did:plc:me"), now,
        )
        assertEquals(Callability.State.MayNotPermit, c)
    }
}
