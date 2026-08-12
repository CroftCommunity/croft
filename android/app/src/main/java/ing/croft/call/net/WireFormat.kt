package ing.croft.call.net

/**
 * Protocol identity for Croft Call, v0.
 *
 * ALPN (application-layer protocol negotiation) is the string iroh uses to
 * route an incoming connection to the right protocol handler; both sides must
 * bind/dial with the same value.
 *
 * v0 wire format is deliberately trivial, one hello frame each way, because
 * its only job is to prove "two endpoints found each other via a PDS record
 * and a deep link, authenticated, over relay.croft.ing." Media (iroh-roq /
 * Opus) replaces this in a later phase without changing anything upstream of
 * this file.
 *
 * Frame: u16 length (big-endian) + UTF-8 JSON {"hello": "<handle-or-anon>"}.
 */
object WireFormat {
    val ALPN: ByteArray = "croft-call/0".toByteArray(Charsets.UTF_8)

    fun encodeHello(from: String): ByteArray {
        val body = """{"hello":${jsonString(from)}}""".toByteArray(Charsets.UTF_8)
        require(body.size <= 0xFFFF) { "hello too large" }
        return ByteArray(2 + body.size).also {
            it[0] = (body.size ushr 8).toByte()
            it[1] = (body.size and 0xFF).toByte()
            body.copyInto(it, 2)
        }
    }

    fun frameLength(header: ByteArray): Int =
        ((header[0].toInt() and 0xFF) shl 8) or (header[1].toInt() and 0xFF)

    private fun jsonString(s: String): String =
        "\"" + s.replace("\\", "\\\\").replace("\"", "\\\"") + "\""
}
