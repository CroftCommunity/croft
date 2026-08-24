package ing.croft.call.net

/**
 * The three call-endings (E129), as words. Pure — CallPeer feeds it the
 * local-hang-up flag and whatever reason `Connection.closed()` reported.
 *
 * We never classify upstream's reason string (its text is not our
 * contract); a non-local ending passes it through verbatim, so the words
 * carry the transport's truth without us guessing remote-end from error.
 */
object CallEnding {
    fun ended(
        endpointId: String,
        peer: String,
        localHangUp: Boolean,
        reason: String?,
    ): CallPeer.State.Ended {
        val message = when {
            localHangUp -> "you ended the call"
            reason.isNullOrBlank() -> "connection lost"
            else -> "call ended: $reason"
        }
        return CallPeer.State.Ended(endpointId, peer, message)
    }
}
