package ing.croft.call.net

import uniffi.croft_ffi.EndpointOptions

/**
 * The croft relay, as the client dials it. The relay's front page is the
 * source of truth for these values — https://relay.croft.ing/ advertises
 * "connect via https://relay.croft.ing:8443 (relay) and udp/7824 (QUIC)"
 * (probed 2026-08-17). Both ports are nonstandard, which is why the target
 * carries a QUIC port and not a bare URL.
 *
 * D3.3: the values reach the endpoint as OUR port's `EndpointOptions`
 * (`call-transport-iroh`, through `uniffi.croft_ffi.CallEndpoint`), not
 * upstream iroh-ffi's `RelayConfig`. The admission token rides the same
 * options at bind and `CallEndpoint.rebind` afterwards; null = tokenless,
 * which an enforcing relay refuses (the M4 posture).
 */
object CroftRelay {
    val URL: String = ing.croft.call.BuildConfig.CROFT_RELAY_URL
    val QUIC_PORT: UShort = ing.croft.call.BuildConfig.CROFT_RELAY_QUIC_PORT.toUShort()

    /** A relay to camp on, and how peers are found. */
    data class Target(val url: String, val quicPort: UShort, val discoveryN0: Boolean) {
        fun endpointOptions(secret: ByteArray?, token: String?): EndpointOptions = EndpointOptions(
            secretKey = secret,
            relayUrl = url,
            quicPort = quicPort,
            token = token,
            discoveryN0 = discoveryN0,
        )
    }

    /** Production: the croft relay with n0's discovery (DNS/pkarr). */
    val PRODUCTION = Target(URL, QUIC_PORT, discoveryN0 = true)

    fun endpointOptions(secret: ByteArray?, token: String?): EndpointOptions =
        PRODUCTION.endpointOptions(secret, token)

    /** croft-admit, the mint (declared at services/croft-admit.toml). */
    val ADMIT_BASE: String = ing.croft.call.BuildConfig.CROFT_ADMIT_BASE

    /** Service-auth audience: croft-admit's DID (D3). */
    const val ADMIT_AUD = "did:web:admit.croft.ing"

    /** Service-auth method binding (D3). */
    const val ADMIT_LXM = "ing.croft.relay.grantCall"

    /** The camp mint's method binding (M4e, O1). */
    const val ADMIT_CAMP_LXM = "ing.croft.relay.campToken"
}
