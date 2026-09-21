package ing.croft.call.net

import uniffi.croft_ffi.CallEnding as Ending

/**
 * The three call-endings (E129), as words. Pure — CallPeer feeds it the
 * TYPED ending the core observed (`ActiveCall.ending()`, D3.3), so nothing
 * here classifies a string: a local hang-up is the core's own fact, a
 * peer's close passes its reason and code through verbatim, and a lost
 * connection carries the transport's words or says so plainly.
 */
object CallEnding {
    fun ended(endpointId: String, peer: String, ending: Ending): CallPeer.State.Ended {
        val message = when (ending) {
            is Ending.LocalHangUp -> "you ended the call"
            is Ending.ClosedByPeer -> "call ended: closed by peer: ${ending.reason} (code ${ending.code})"
            is Ending.Lost ->
                if (ending.reason.isBlank()) "connection lost" else "call ended: ${ending.reason}"
        }
        return CallPeer.State.Ended(endpointId, peer, message)
    }
}
