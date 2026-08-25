package ing.croft.call.net

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * E130(a): the line status must not claim "camped on relay" the app cannot
 * see. §12 found the app saying "ready, camped on relay" while the enforce
 * relay was REFUSING its attach — under enforcement a signed-out callee
 * looks fine on-screen while unreachable. The pollable truth is the
 * endpoint's home-relay url (endpoint.addr().relayUrl(), the same
 * poll-not-callback pattern the path summary already uses); this mapper
 * turns it into honest words.
 */
class CampPresenceTest {

    @Test
    fun `a home relay present is the camped line`() {
        assertEquals(
            "ready, camped on relay",
            CampPresence.line(homeRelay = "https://relay.croft.ing:8444"),
        )
    }

    @Test
    fun `no home relay says NOT camped and what it costs — never silence`() {
        assertEquals(
            "ready — NOT camped on relay; calls cannot reach this device",
            CampPresence.line(homeRelay = null),
        )
        assertEquals(
            "ready — NOT camped on relay; calls cannot reach this device",
            CampPresence.line(homeRelay = ""),
        )
    }
}
