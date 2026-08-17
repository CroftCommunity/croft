package ing.croft.call.caps

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Ticket-secret verification (contract §2, ticket matcher) — the Kotlin
 * mirror of connect's resolver.js `sha256Hex`/`verifyTicketSecret`. The
 * vectors match connect/web-tests/redeem.test.js so the two halves are
 * provably checking the same thing.
 */
class TicketsTest {

    @Test
    fun `sha256 matches the canonical abc vector, lowercase hex`() {
        assertEquals(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            Tickets.sha256Hex("abc"),
        )
    }

    @Test
    fun `secret verifies against its own hash`() {
        val hash = Tickets.sha256Hex("s3cr3t")
        assertTrue(Tickets.verifyTicketSecret("s3cr3t", hash))
    }

    @Test
    fun `wrong secret is refused`() {
        val hash = Tickets.sha256Hex("s3cr3t")
        assertFalse(Tickets.verifyTicketSecret("not-it", hash))
    }

    @Test
    fun `stored hash is compared case-insensitively`() {
        // resolver.js lowercases the stored hash before comparing; a repo
        // that stored uppercase hex must still admit the right secret.
        val hash = Tickets.sha256Hex("s3cr3t").uppercase()
        assertTrue(Tickets.verifyTicketSecret("s3cr3t", hash))
    }
}
