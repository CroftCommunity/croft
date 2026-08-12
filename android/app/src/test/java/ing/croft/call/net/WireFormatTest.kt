package ing.croft.call.net

import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * WireFormat tests — the v0 hello frame (u16 big-endian length + UTF-8 JSON).
 * Pure Kotlin, no Android, so this is a plain JUnit test.
 */
class WireFormatTest {

    @Test
    fun `alpn is the v0 protocol string`() {
        assertEquals("croft-call/0", String(WireFormat.ALPN, Charsets.UTF_8))
    }

    @Test
    fun `encode then frameLength round-trips the body length`() {
        val frame = WireFormat.encodeHello("alice.test")
        val header = frame.copyOfRange(0, 2)
        val bodyLen = WireFormat.frameLength(header)
        assertEquals(frame.size - 2, bodyLen)
        // body decodes to the expected JSON
        val body = String(frame, 2, bodyLen, Charsets.UTF_8)
        assertEquals("""{"hello":"alice.test"}""", body)
    }

    @Test
    fun `big-endian length is encoded across both header bytes`() {
        // A body longer than 255 bytes exercises the high byte of the u16 length.
        val name = "x".repeat(300)
        val frame = WireFormat.encodeHello(name)
        val header = frame.copyOfRange(0, 2)
        val expected = frame.size - 2
        assertEquals(expected, WireFormat.frameLength(header))
        assertTrue("high byte should be set for >255 body", (header[0].toInt() and 0xFF) > 0)
    }

    @Test
    fun `oversize hello is rejected`() {
        // Body = {"hello":"..."} must stay within the u16 length field (<= 0xFFFF).
        val tooBig = "a".repeat(0x10000)
        assertThrows(IllegalArgumentException::class.java) {
            WireFormat.encodeHello(tooBig)
        }
    }

    @Test
    fun `json special characters are escaped`() {
        val frame = WireFormat.encodeHello("""a"b\c""")
        val body = String(frame, 2, WireFormat.frameLength(frame.copyOfRange(0, 2)), Charsets.UTF_8)
        // the quote and backslash are backslash-escaped inside the JSON string
        assertEquals("""{"hello":"a\"b\\c"}""", body)
    }
}
