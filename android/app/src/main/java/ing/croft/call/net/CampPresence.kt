package ing.croft.call.net

/**
 * E130(a): honest words for the camping claim. The §12 run found the app
 * saying "ready, camped on relay" while the enforce relay was refusing its
 * attach — the pollable truth is the endpoint's home-relay url; these are
 * the only two lines it maps to.
 */
object CampPresence {
    /**
     * The relay this endpoint is ACTUALLY attached to, or null when it is
     * attached to none.
     *
     * `relayUrl` alone cannot answer this: measured on hardware 2026-08-28
     * (runbook §13 step 3), it reports the CONFIGURED relay while an
     * enforcing relay refuses every attach. `online` is the endpoint's own
     * reachability answer (`Endpoint.online()`, prompt when attached, blocked
     * when refused), so the url only becomes an attachment when online agrees.
     */
    fun attachedRelay(online: Boolean, relayUrl: String?): String? =
        if (online) relayUrl?.takeIf { it.isNotBlank() } else null

    /**
     * The attach log line. When NOT attached it carries iroh's last relay
     * error verbatim (`RelayStatus::last_error()` through the port), so the
     * next device run says WHY the connection dropped — the relay journal
     * (2026-09-21/23) showed the Pixel closing its own relay connection every
     * few minutes on Wi-Fi and LTE alike, and no run could name the reason.
     */
    fun attachLog(homeRelay: String?, lastError: String?): String =
        when {
            !homeRelay.isNullOrEmpty() -> "home relay: $homeRelay"
            lastError.isNullOrBlank() -> "home relay: NOT ATTACHED"
            else -> "home relay: NOT ATTACHED (last relay error: $lastError)"
        }

    /**
     * One relay status transition as a log line. RUN 2026-09-28: the Pixel at
     * rest re-made its relay connection seven times in 31 min with 0.5 s
     * gaps that the 5 s attach probe never saw; the port records each with
     * iroh's reason, and this is how it reaches logcat.
     */
    fun transitionLog(connected: Boolean, relayUrl: String?, error: String?): String {
        val where = relayUrl?.takeIf { it.isNotBlank() }?.let { " $it" } ?: ""
        return if (connected) {
            "relay transition: attached$where"
        } else {
            "relay transition: DETACHED$where (${error?.takeIf { it.isNotBlank() } ?: "no reason given"})"
        }
    }

    fun line(homeRelay: String?): String =
        if (homeRelay.isNullOrEmpty()) {
            "ready — NOT camped on relay; calls cannot reach this device"
        } else {
            "ready, camped on relay"
        }
}
