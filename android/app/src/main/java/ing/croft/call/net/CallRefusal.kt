package ing.croft.call.net

import uniffi.croft_ffi.CallException

/**
 * The words off a refusal that crossed the FFI (D3 follow-up).
 *
 * `CallException` is a sealed hierarchy of generated classes with no shared
 * accessor — uniffi renders each Rust variant as its own Kotlin class carrying
 * that variant's fields, and builds the exception's `message` FROM those
 * fields ("reason=…"). So a screen that renders `e.message` renders a field
 * dump: the Samsung showed "dial failed: reason=dial failed: no answer within
 * 20s" on 2026-09-21 (runbook §17). The `reason` every variant carries is the
 * sentence; it is reachable only by matching — the social module's
 * `FfiRefusal` shape, for the same cause.
 *
 * The `else` branch is deliberately not a friendly placeholder: a refusal
 * nobody has written words for should look like an oversight, not like a
 * considered message.
 */
object CallRefusal {
    fun words(e: CallException): String = when (e) {
        is CallException.NotSignedIn -> e.reason
        is CallException.SessionDead -> e.reason
        is CallException.NotBound -> e.reason
        is CallException.NoSuchDevice -> e.reason
        is CallException.Closed -> e.reason
        is CallException.Network -> e.reason
        is CallException.State -> e.reason
        is CallException.Transport -> e.reason
        else -> e.toString()
    }

    /**
     * A dial that threw, in words the matrix accepts: the port's own
     * sentence when it is ours (already "dial failed: …"), a prefixed message
     * for any other throwable, and a named blank for one that says nothing —
     * "dial failed: null" reached a real screen on 2026-09-08 (§15.3).
     */
    fun dialFailure(t: Throwable): String {
        if (t is CallException) return words(t)
        val why = t.message?.takeIf { it.isNotBlank() }
            ?: "the connection was refused and gave no reason (${t.javaClass.simpleName})"
        return "dial failed: $why"
    }
}
