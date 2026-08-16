package ing.croft.call

import android.content.Intent
import android.net.Uri

/**
 * The callee, as delivered by the Croft Exchange lookup page via
 * croftcall://call?endpoint=...&relay=...&handle=...&did=...&device=...&grant=...
 *
 * Only [endpointId] is required to dial: iroh converts an endpoint id into a
 * dialable address, using discovery (or the relay hint) for the rest.
 *
 * [device] and [grant] (connect contract v2 §5) identify which device was
 * resolved and which grant authorized the call; they are carried so a later
 * call-time re-check (contract §7) can re-validate the grant. Absent for a v1
 * link — the call path does not need them.
 */
data class Callee(
    val endpointId: String,
    val relayUrl: String?,
    val handle: String?,
    val did: String?,
    val device: String?,
    val grant: String?,
)

object DeepLink {
    const val SCHEME = "croftcall"
    const val HOST = "call"

    fun parse(intent: Intent?): Callee? {
        val uri: Uri = intent?.data ?: return null
        if (uri.scheme != SCHEME || uri.host != HOST) return null
        val endpoint = uri.getQueryParameter("endpoint")?.trim().orEmpty()
        if (endpoint.isEmpty()) return null
        return Callee(
            endpointId = endpoint,
            relayUrl = uri.getQueryParameter("relay")?.trim()?.ifEmpty { null },
            handle = uri.getQueryParameter("handle")?.trim()?.ifEmpty { null },
            did = uri.getQueryParameter("did")?.trim()?.ifEmpty { null },
            device = uri.getQueryParameter("device")?.trim()?.ifEmpty { null },
            grant = uri.getQueryParameter("grant")?.trim()?.ifEmpty { null },
        )
    }
}
