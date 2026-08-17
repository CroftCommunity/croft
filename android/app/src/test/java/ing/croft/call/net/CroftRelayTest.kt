package ing.croft.call.net

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * CroftRelay tests — the client half of the relay.croft.ing wiring (rung 3).
 * The values are pinned against what the relay itself advertises on its
 * front page (probed 2026-08-17): "iroh-relay (mode B): connect via
 * https://relay.croft.ing:8443 (relay) and udp/7824 (QUIC)". Nonstandard
 * ports, so drift here means a client that dials the wrong door.
 */
class CroftRelayTest {

    @Test
    fun `relay url carries the advertised nonstandard port`() {
        assertEquals("https://relay.croft.ing:8443", CroftRelay.config().url)
    }

    @Test
    fun `quic address discovery uses the advertised udp port`() {
        assertEquals(7824.toUShort(), CroftRelay.config().quicPort)
    }

    @Test
    fun `no auth token until the admission layer lands`() {
        // Phase 11 puts a bearer token here; until then the relay is open.
        assertNull(CroftRelay.config().authToken)
    }
}
