package ing.croft.call.net

import ing.croft.call.caps.FormResponse
import ing.croft.call.caps.Http
import ing.croft.call.caps.HttpForm
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

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

/**
 * The real [HttpGet] port (M4b): a GET carrying headers that returns
 * status + headers + body without throwing on non-2xx — the resource-server
 * DPoP nonce dance reads `DPoP-Nonce` off a 401 (port contract in
 * caps/ServiceAuth.kt).
 */
object UrlHttpGet : ing.croft.call.caps.HttpGet {
    override suspend fun get(url: String, headers: Map<String, String>): FormResponse {
        val conn = URL(url).openConnection() as HttpURLConnection
        try {
            conn.requestMethod = "GET"
            conn.setRequestProperty("Accept", "application/json")
            for ((k, v) in headers) conn.setRequestProperty(k, v)
            conn.connectTimeout = 15_000
            conn.readTimeout = 15_000
            val status = conn.responseCode
            val body = (if (status in 200..299) conn.inputStream else conn.errorStream)
                ?.bufferedReader()?.use { it.readText() }.orEmpty()
            val flat = conn.headerFields
                .filterKeys { it != null }
                .mapKeys { it.key!! }
                .mapValues { it.value.firstOrNull().orEmpty() }
            return FormResponse(status = status, headers = flat, body = body)
        } finally {
            conn.disconnect()
        }
    }
}

/**
 * The real [HttpJson] port (M4): a JSON POST that, like [UrlHttpForm],
 * returns status + body *without* throwing on non-2xx — the admit engine
 * maps a 403 body to a typed refusal (port contract in caps/Admit.kt).
 */
object UrlHttpJson : ing.croft.call.caps.HttpJson {
    override suspend fun postJson(url: String, body: String): FormResponse {
        val conn = URL(url).openConnection() as HttpURLConnection
        try {
            conn.requestMethod = "POST"
            conn.doOutput = true
            conn.setRequestProperty("Content-Type", "application/json")
            conn.setRequestProperty("Accept", "application/json")
            conn.connectTimeout = 15_000
            conn.readTimeout = 15_000
            conn.outputStream.use { it.write(body.toByteArray()) }
            val status = conn.responseCode
            val text = (if (status in 200..299) conn.inputStream else conn.errorStream)
                ?.bufferedReader()?.use { it.readText() }.orEmpty()
            val flat = conn.headerFields
                .filterKeys { it != null }
                .mapKeys { it.key!! }
                .mapValues { it.value.firstOrNull().orEmpty() }
            return FormResponse(status = status, headers = flat, body = text)
        } finally {
            conn.disconnect()
        }
    }
}

/**
 * The real [HttpForm] port: application/x-www-form-urlencoded POST that
 * returns status + headers + body *without* throwing on non-2xx — the
 * OAuth engine reads the DPoP-Nonce header off 400 responses (port
 * contract in caps/HttpForm.kt). Policy still belongs to callers.
 */
object UrlHttpForm : HttpForm {
    override suspend fun postForm(
        url: String,
        fields: Map<String, String>,
        headers: Map<String, String>,
    ): FormResponse {
        val conn = URL(url).openConnection() as HttpURLConnection
        try {
            conn.requestMethod = "POST"
            conn.doOutput = true
            conn.setRequestProperty("Content-Type", "application/x-www-form-urlencoded")
            conn.setRequestProperty("Accept", "application/json")
            for ((k, v) in headers) conn.setRequestProperty(k, v)
            conn.connectTimeout = 15_000
            conn.readTimeout = 15_000
            val encoded = fields.entries.joinToString("&") { (k, v) ->
                URLEncoder.encode(k, "UTF-8") + "=" + URLEncoder.encode(v, "UTF-8")
            }
            conn.outputStream.use { it.write(encoded.toByteArray()) }
            val status = conn.responseCode
            val body = (if (status in 200..299) conn.inputStream else conn.errorStream)
                ?.bufferedReader()?.use { it.readText() }.orEmpty()
            val flat = conn.headerFields
                .filterKeys { it != null }
                .mapKeys { it.key!! }
                .mapValues { it.value.firstOrNull().orEmpty() }
            return FormResponse(status = status, headers = flat, body = body)
        } finally {
            conn.disconnect()
        }
    }
}
