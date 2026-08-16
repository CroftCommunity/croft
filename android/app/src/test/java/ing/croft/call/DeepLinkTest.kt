package ing.croft.call

import android.content.Intent
import android.net.Uri
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * DeepLink.parse tests — the Android half of the shared contract
 * (docs/contract.md). Runs under Robolectric so android.net.Uri does the real
 * scheme/host/query parsing and URL-decoding on the JVM, no device needed.
 */
@RunWith(RobolectricTestRunner::class)
class DeepLinkTest {

    private fun intentFor(uri: String) = Intent(Intent.ACTION_VIEW, Uri.parse(uri))

    @Test
    fun `parses a full valid link`() {
        val callee = DeepLink.parse(
            intentFor("croftcall://call?endpoint=ep-abc&relay=https%3A%2F%2Frelay.example&handle=alice.test&did=did%3Aplc%3Aabc"),
        )
        requireNotNull(callee)
        assertEquals("ep-abc", callee.endpointId)
        assertEquals("https://relay.example", callee.relayUrl)   // URL-decoded
        assertEquals("alice.test", callee.handle)
        assertEquals("did:plc:abc", callee.did)                  // URL-decoded
    }

    @Test
    fun `endpoint only is enough`() {
        val callee = DeepLink.parse(intentFor("croftcall://call?endpoint=solo"))
        requireNotNull(callee)
        assertEquals("solo", callee.endpointId)
        assertNull(callee.relayUrl)
        assertNull(callee.handle)
        assertNull(callee.did)
        assertNull(callee.device)
        assertNull(callee.grant)
    }

    @Test
    fun `captures device and grant when present (contract v2)`() {
        val callee = DeepLink.parse(
            intentFor("croftcall://call?endpoint=ep-abc&device=phone&grant=g1"),
        )
        requireNotNull(callee)
        assertEquals("ep-abc", callee.endpointId)
        assertEquals("phone", callee.device)
        assertEquals("g1", callee.grant)
    }

    @Test
    fun `missing endpoint yields null`() {
        assertNull(DeepLink.parse(intentFor("croftcall://call?handle=alice.test")))
    }

    @Test
    fun `blank endpoint yields null`() {
        assertNull(DeepLink.parse(intentFor("croftcall://call?endpoint=")))
    }

    @Test
    fun `wrong scheme yields null`() {
        assertNull(DeepLink.parse(intentFor("https://call?endpoint=ep-abc")))
    }

    @Test
    fun `wrong host yields null`() {
        assertNull(DeepLink.parse(intentFor("croftcall://other?endpoint=ep-abc")))
    }

    @Test
    fun `null intent and null data yield null`() {
        assertNull(DeepLink.parse(null))
        assertNull(DeepLink.parse(Intent(Intent.ACTION_VIEW)))
    }

    @Test
    fun `extra unknown params are ignored`() {
        val callee = DeepLink.parse(
            intentFor("croftcall://call?endpoint=ep-abc&foo=bar&utm_source=x"),
        )
        requireNotNull(callee)
        assertEquals("ep-abc", callee.endpointId)
    }
}
