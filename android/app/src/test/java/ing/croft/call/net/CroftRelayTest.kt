package ing.croft.call.net

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * CroftRelay tests — the client half of the relay.croft.ing wiring (rung 3).
 * The values are pinned against what the relay itself advertises on its
 * front page (probed 2026-08-17): "iroh-relay (mode B): connect via
 * https://relay.croft.ing:8443 (relay) and udp/7824 (QUIC)". Nonstandard
 * ports, so drift here means a client that dials the wrong door. D3.3: the
 * values reach the endpoint as `EndpointOptions` (our port), not upstream's
 * `RelayConfig`; the token rides the same options, bound by `rebind`.
 */
class CroftRelayTest {

    @Test
    fun `relay url carries the advertised nonstandard port`() {
        assertEquals("https://relay.croft.ing:8443", CroftRelay.URL)
    }

    @Test
    fun `quic address discovery uses the advertised udp port`() {
        assertEquals(7824.toUShort(), CroftRelay.QUIC_PORT)
    }

    @Test
    fun `the endpoint options carry both, and the token when there is one`() {
        val tokenless = CroftRelay.endpointOptions(secret = null, token = null)
        assertEquals("https://relay.croft.ing:8443", tokenless.relayUrl)
        assertEquals(7824.toUShort(), tokenless.quicPort)
        assertEquals(null, tokenless.token)
        assertEquals("pass", CroftRelay.endpointOptions(secret = null, token = "pass").token)
    }
}
