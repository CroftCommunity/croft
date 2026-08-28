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

    fun line(homeRelay: String?): String =
        if (homeRelay.isNullOrEmpty()) {
            "ready — NOT camped on relay; calls cannot reach this device"
        } else {
            "ready, camped on relay"
        }
}
