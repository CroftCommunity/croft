// D3.1: the croft core is reachable from the CALLING app — through the same
// generated bindings the social module drives, on the JVM, hermetically.
//
// Calling nothing in production yet; this is the wiring proof the plan asks
// for before any behaviour moves: the library loads, a CallSession opens, and
// its view crosses with the words the core committed to. The on-device half —
// the arm64 .so dlopen'd on a real phone — is env/build-croft-ffi-android.sh.

package ing.croft.call.net

import java.nio.file.Files
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import uniffi.croft_ffi.CallOptions
import uniffi.croft_ffi.CallSession

class FfiWiringTest {
    @Test
    fun `the core opens a calling session from the calling app and says what it knows`() {
        val dir = Files.createTempDirectory("croft-call-ffi-wiring")
        val session = CallSession.open(
            CallOptions(
                stateDir = dir.toString(),
                relay = "https://192.0.2.1:443",
                label = "android-test",
                discoveryN0 = false,
                attachPatienceSecs = 1u,
            ),
        )
        val view = session.view()
        assertFalse(view.signedIn)
        assertEquals("not signed in", view.session)
        assertTrue(view.presence.contains("NOT camped"), view.presence)
        assertTrue(view.presence.contains("calls cannot reach"), view.presence)
        assertNull(view.endpointId, "nothing is bound until camp")
        session.shutDown()
        dir.toFile().deleteRecursively()
    }
}
