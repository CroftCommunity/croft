package ing.croft.call.net

import computer.iroh.RelayConfig

/**
 * The croft relay, as the client dials it. The relay's front page is the
 * source of truth for these values — https://relay.croft.ing/ advertises
 * "connect via https://relay.croft.ing:8443 (relay) and udp/7824 (QUIC)"
 * (probed 2026-08-17). Both ports are nonstandard, which is why this is a
 * RelayConfig and not a bare URL: RelayMode.customFromUrls would dial the
 * defaults and miss.
 *
 * authToken stays null until Phase 11 (the admission layer) issues one; the
 * field becomes an `Authorization: Bearer` header on the relay upgrade
 * request (iroh-ffi src/relay.rs).
 */
object CroftRelay {
    fun config(): RelayConfig = RelayConfig(
        url = "https://relay.croft.ing:8443",
        quicPort = 7824u,
        authToken = null,
    )
}
