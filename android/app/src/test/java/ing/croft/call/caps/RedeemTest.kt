package ing.croft.call.caps

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * redeemTicket orchestration tests — the Kotlin mirror of resolver.js
 * `redeemTicket` (contract §6): parse the invite link, resolve the repo,
 * verify the ticket secret against the grant, enforce redeem-time rules
 * (`expires` only — use-based rules are call-time), read the chosen device's
 * endpoint, and hand back a dialable result carrying grant + device for the
 * call-time re-check. Fails closed at every step.
 */
@RunWith(RobolectricTestRunner::class)
class RedeemTest {

    private val secret = "s3cr3t"
    private val secretHash = Tickets.sha256Hex(secret)
    private val now = 1_700_000_000_000L

    private class FakeHttp(private val routes: Map<String, String>) : Http {
        val calls = mutableListOf<String>()
        override suspend fun getJson(url: String): String {
            calls += url
            routes.entries.firstOrNull { url.contains(it.key) }?.let { return it.value }
            throw IllegalStateException("no route for $url")
        }
    }

    private fun happyRoutes(
        grantJson: String =
            """{"value":{"matcher":{"type":"ticket","secretHash":"$secretHash"},"devices":["phone"]}}""",
    ) = mapOf(
        "resolveHandle" to """{"did":"did:plc:callee"}""",
        "plc.directory" to
            """{"service":[{"id":"#atproto_pds","serviceEndpoint":"https://pds.example.com"}]}""",
        "ing.croft.call.grant" to grantJson,
        "ing.croft.iroh.endpoint" to
            """{"value":{"endpointId":"ep-callee","homeRelay":"https://relay.croft.ing:8443"}}""",
    )

    @Test
    fun `redeems a ticket invite to a dialable endpoint carrying grant and device`() = runBlocking {
        val http = FakeHttp(happyRoutes())
        val r = Redeem.redeemTicket(
            http,
            "https://connect.croft.ing/redeem?repo=alice.test&grant=g1#$secret",
            now = now,
        )
        assertEquals("ep-callee", r.endpointId)
        assertEquals("https://relay.croft.ing:8443", r.homeRelay)
        assertEquals("did:plc:callee", r.did)
        assertEquals("phone", r.device)   // grant.devices[0] when the link names none
        assertEquals("g1", r.grant)
        // M4: the secret is the call-time ticket proof — a redeem that drops
        // it forces the user back to the invite link at every dial.
        assertEquals(secret, r.secret)
    }

    @Test
    fun `a did repo skips handle resolution`() = runBlocking {
        val http = FakeHttp(happyRoutes())
        val r = Redeem.redeemTicket(
            http,
            "https://connect.croft.ing/redeem?repo=did:plc:callee&grant=g1#$secret",
            now = now,
        )
        assertEquals("did:plc:callee", r.did)
        assertEquals(emptyList<String>(), http.calls.filter { it.contains("resolveHandle") })
    }

    @Test
    fun `the link's device hint wins over the grant's device list`() = runBlocking {
        val http = FakeHttp(happyRoutes())
        val r = Redeem.redeemTicket(
            http,
            "https://connect.croft.ing/redeem?repo=alice.test&grant=g1&device=laptop#$secret",
            now = now,
        )
        assertEquals("laptop", r.device)
    }

    @Test
    fun `a grant with no device list falls back to self`() = runBlocking {
        val http = FakeHttp(happyRoutes(
            grantJson = """{"value":{"matcher":{"type":"ticket","secretHash":"$secretHash"}}}""",
        ))
        val r = Redeem.redeemTicket(
            http,
            "https://connect.croft.ing/redeem?repo=alice.test&grant=g1#$secret",
            now = now,
        )
        assertEquals("self", r.device)
    }

    @Test
    fun `a non-ticket grant is refused`() {
        val http = FakeHttp(happyRoutes(grantJson = """{"value":{"matcher":{"type":"mutuals"}}}"""))
        assertThrows(IllegalStateException::class.java) {
            runBlocking {
                Redeem.redeemTicket(
                    http, "https://connect.croft.ing/redeem?repo=alice.test&grant=g1#$secret", now = now,
                )
            }
        }
    }

    @Test
    fun `a missing secret is refused`() {
        val http = FakeHttp(happyRoutes())
        assertThrows(IllegalStateException::class.java) {
            runBlocking {
                Redeem.redeemTicket(
                    http, "https://connect.croft.ing/redeem?repo=alice.test&grant=g1", now = now,
                )
            }
        }
    }

    @Test
    fun `a wrong secret is refused`() {
        val http = FakeHttp(happyRoutes())
        assertThrows(IllegalStateException::class.java) {
            runBlocking {
                Redeem.redeemTicket(
                    http, "https://connect.croft.ing/redeem?repo=alice.test&grant=g1#not-it", now = now,
                )
            }
        }
    }

    @Test
    fun `an expired policy refuses at redeem time`() {
        val http = FakeHttp(
            happyRoutes(
                grantJson =
                    """{"value":{"matcher":{"type":"ticket","secretHash":"$secretHash"},"policyRef":"p1"}}""",
            ) + mapOf(
                "ing.croft.call.policy" to
                    """{"value":{"rules":[{"type":"expires","at":"2020-01-01T00:00:00Z"}]}}""",
            ),
        )
        assertThrows(IllegalStateException::class.java) {
            runBlocking {
                Redeem.redeemTicket(
                    http, "https://connect.croft.ing/redeem?repo=alice.test&grant=g1#$secret", now = now,
                )
            }
        }
    }

    @Test
    fun `use-based rules are deliberately ignored at redeem time`() = runBlocking {
        // maxUses/burnOnSuccess are call-time facts (§7); a redeem must not
        // refuse on them — mirror of resolver.js enforceRedeemTimeRules.
        val http = FakeHttp(
            happyRoutes(
                grantJson =
                    """{"value":{"matcher":{"type":"ticket","secretHash":"$secretHash"},"policyRef":"p1"}}""",
            ) + mapOf(
                "ing.croft.call.policy" to
                    """{"value":{"rules":[{"type":"maxUses","n":0},{"type":"burnOnSuccess"}]}}""",
            ),
        )
        val r = Redeem.redeemTicket(
            http, "https://connect.croft.ing/redeem?repo=alice.test&grant=g1#$secret", now = now,
        )
        assertEquals("ep-callee", r.endpointId)
    }
}
