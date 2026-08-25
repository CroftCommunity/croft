package ing.croft.call.net

/**
 * E130(a): honest words for the camping claim. The §12 run found the app
 * saying "ready, camped on relay" while the enforce relay was refusing its
 * attach — the pollable truth is the endpoint's home-relay url; these are
 * the only two lines it maps to.
 */
object CampPresence {
    fun line(homeRelay: String?): String =
        if (homeRelay.isNullOrEmpty()) {
            "ready — NOT camped on relay; calls cannot reach this device"
        } else {
            "ready, camped on relay"
        }
}
