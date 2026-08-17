package ing.croft.call.caps

import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

/**
 * Invite-link parsing (contract §4) — the Kotlin mirror of resolver.js
 * `parseInviteLink`. The load-bearing rule: a ticket's secret travels in the
 * URL FRAGMENT (never the query, so it never reaches a server), and repo +
 * grant are required.
 */
class InviteLinkTest {

    @Test
    fun `parses repo, grant, device and fragment secret`() {
        val p = InviteLink.parse(
            "https://connect.croft.ing/redeem?repo=did:plc:callee&grant=g1&device=phone#s3cr3t",
        )
        assertEquals("did:plc:callee", p.repo)
        assertEquals("g1", p.grant)
        assertEquals("phone", p.device)
        assertEquals("s3cr3t", p.secret)
    }

    @Test
    fun `device and secret are optional`() {
        val p = InviteLink.parse("https://connect.croft.ing/redeem?repo=alice.test&grant=g1")
        assertEquals("alice.test", p.repo)
        assertEquals("g1", p.grant)
        assertEquals("", p.device)
        assertEquals("", p.secret)
    }

    @Test
    fun `fragment secret is url-decoded`() {
        val p = InviteLink.parse(
            "https://connect.croft.ing/redeem?repo=did:plc:x&grant=g1#a%20secret%2Fwith%3Dchars",
        )
        assertEquals("a secret/with=chars", p.secret)
    }

    @Test
    fun `missing repo is refused`() {
        assertThrows(IllegalArgumentException::class.java) {
            InviteLink.parse("https://connect.croft.ing/redeem?grant=g1#s")
        }
    }

    @Test
    fun `missing grant is refused`() {
        assertThrows(IllegalArgumentException::class.java) {
            InviteLink.parse("https://connect.croft.ing/redeem?repo=did:plc:x#s")
        }
    }
}
