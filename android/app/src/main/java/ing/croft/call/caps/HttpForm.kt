package ing.croft.call.caps

/**
 * A form-POST response with status and headers intact. Unlike the GET port
 * ([Http], body-only, throws on non-2xx), the OAuth token endpoints put
 * protocol data in the *headers of error responses*: the DPoP server nonce
 * arrives as a `DPoP-Nonce` header on a 400 the flow must read and retry.
 * So this port never throws on status — the engine decides.
 */
data class FormResponse(
    val status: Int,
    val headers: Map<String, String>,
    val body: String,
) {
    /** Case-insensitive header lookup (HTTP header names are). */
    fun header(name: String): String? =
        headers.entries.firstOrNull { it.key.equals(name, ignoreCase = true) }?.value
}

/**
 * The second seam to the network (M3): an HTTP form POST. Injected like
 * [Http]; implementations URL-encode [fields] as
 * application/x-www-form-urlencoded and attach [headers] verbatim.
 */
fun interface HttpForm {
    suspend fun postForm(
        url: String,
        fields: Map<String, String>,
        headers: Map<String, String>,
    ): FormResponse
}
