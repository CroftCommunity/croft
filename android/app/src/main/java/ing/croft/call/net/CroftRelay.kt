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
 * authToken becomes an `Authorization: Bearer` header on the relay upgrade
 * request (iroh-ffi src/relay.rs). Null = the pre-M4 posture (production
 * runs admission="open", which admits token-less connections); M4c passes
 * the token `/grantCall` minted for this endpoint.
 */
object CroftRelay {
    fun config(authToken: String? = null): RelayConfig = RelayConfig(
        url = ing.croft.call.BuildConfig.CROFT_RELAY_URL,
        quicPort = ing.croft.call.BuildConfig.CROFT_RELAY_QUIC_PORT.toUShort(),
        authToken = authToken,
    )

    /** croft-admit, the mint (declared at services/croft-admit.toml). */
    val ADMIT_BASE: String = ing.croft.call.BuildConfig.CROFT_ADMIT_BASE

    /** Service-auth audience: croft-admit's DID (D3). */
    const val ADMIT_AUD = "did:web:admit.croft.ing"

    /** Service-auth method binding (D3). */
    const val ADMIT_LXM = "ing.croft.relay.grantCall"
}
