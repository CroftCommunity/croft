package ing.croft.call.net

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.croft_ffi.CallException

/**
 * The words off a refusal that crossed the FFI. Seen on the Samsung's screen
 * 2026-09-21 (runbook §17): "dial failed: reason=dial failed: no answer within
 * 20s" — uniffi builds a generated exception's `message` from the variant's
 * FIELDS ("reason=…"), and CallPeer prefixed it again. The matrix wants words,
 * not a field dump: read the `reason` a CallException carries, and only fall
 * back to the message for a throwable that is not ours.
 */
class CallRefusalTest {

    @Test
    fun `a typed refusal's words are its reason, not the field dump`() {
        assertEquals(
            "dial failed: no answer within 20s",
            CallRefusal.words(CallException.Transport(reason = "dial failed: no answer within 20s")),
        )
        assertEquals(
            "no endpoint is bound yet — camp first",
            CallRefusal.words(CallException.NotBound(reason = "no endpoint is bound yet — camp first")),
        )
    }

    @Test
    fun `a dial failure keeps the port's own sentence once, and names a silent throwable`() {
        assertEquals(
            "dial failed: no answer within 20s",
            CallRefusal.dialFailure(CallException.Transport(reason = "dial failed: no answer within 20s")),
        )
        assertEquals(
            "dial failed: boom",
            CallRefusal.dialFailure(IllegalStateException("boom")),
        )
        assertEquals(
            "dial failed: the connection was refused and gave no reason (IllegalStateException)",
            CallRefusal.dialFailure(IllegalStateException("  ")),
        )
    }
}
