package ing.croft.call.net

import computer.iroh.PathSnapshot

/**
 * One honest line about which path a connection is using, from the endpoint's
 * own path snapshots. The rule from the two-device call test runbook applies
 * here in code: report what the snapshot actually says, never infer — anything
 * short of a selected path with a definite type is "path unknown".
 */
object PathSummary {
    fun describe(paths: List<PathSnapshot>): String {
        val selected = paths.firstOrNull { it.isSelected } ?: return "path unknown"
        return when {
            selected.isIp -> "direct ${selected.remoteAddr}"
            selected.isRelay -> "relayed ${selected.remoteAddr}"
            else -> "path unknown"
        }
    }
}
