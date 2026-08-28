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

    // E135(a), measured on hardware 2026-08-28 (runbook §13 step 3). Two
    // candidate signals were tried and BOTH refuted by device runs before this
    // one: polling `addr().relayUrl()` reports the CONFIGURED relay while the
    // relay refuses every attach, and `watchHomeRelay` throws "there is no
    // reactor running" exactly like `conn.watchPaths()` (JOURNAL 2026-08-17).
    // The signal that actually discriminates is `Endpoint.online()`: it
    // returns promptly when attached and blocks (times out) when the relay
    // refuses — measured ONLINE against production and TIMEOUT against the
    // staging enforce listener, with the SAME configured url in both.

    @Test
    fun `online with a relay url is the attached relay`() {
        assertEquals(
            "https://relay.croft.ing:8443",
            CampPresence.attachedRelay(online = true, relayUrl = "https://relay.croft.ing:8443"),
        )
    }

    @Test
    fun `not online is not attached however loudly the url claims otherwise`() {
        assertEquals(
            null,
            CampPresence.attachedRelay(online = false, relayUrl = "https://relay.croft.ing:8444"),
        )
    }

    @Test
    fun `online without a url is not an attachment`() {
        assertEquals(null, CampPresence.attachedRelay(online = true, relayUrl = null))
        assertEquals(null, CampPresence.attachedRelay(online = true, relayUrl = ""))
    }
}
