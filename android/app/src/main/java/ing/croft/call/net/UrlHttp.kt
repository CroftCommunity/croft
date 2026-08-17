package ing.croft.call.net

import ing.croft.call.caps.Http
import java.net.HttpURLConnection
import java.net.URL

/**
 * The real [Http] port: one GET, JSON accepted, and a thrown exception on
 * any non-2xx status (the engine's fail-closed contract — resolver.js's
 * getJson does the same). No retries, no caching; policy belongs to callers.
 */
object UrlHttp : Http {
    override suspend fun getJson(url: String): String {
        val conn = URL(url).openConnection() as HttpURLConnection
        try {
            conn.requestMethod = "GET"
            conn.setRequestProperty("Accept", "application/json")
            conn.connectTimeout = 15_000
            conn.readTimeout = 15_000
            val status = conn.responseCode
            if (status !in 200..299) {
                val detail = conn.errorStream?.bufferedReader()?.use { it.readText() }.orEmpty()
                throw IllegalStateException("HTTP $status for $url${if (detail.isNotEmpty()) ": ${detail.take(200)}" else ""}")
            }
            return conn.inputStream.bufferedReader().use { it.readText() }
        } finally {
            conn.disconnect()
        }
    }
}
