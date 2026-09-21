package ing.croft.call.net

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.croft_ffi.CallEnding as Ending

/**
 * The three call-endings (E129), as words: hang-up, remote-end, error. The
 * pure mapper is the whole decision — CallPeer feeds it the TYPED ending the
 * core observed (D3.3: `ActiveCall.ending()`, never a string it has to
 * classify), and the state carries honest words. A peer's reason passes
 * through verbatim, so the words carry the transport's truth without us
 * guessing at it.
 */
class CallEndingTest {

    private val ep = "cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33"
    private val peer = "aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11"

    @Test
    fun `a local hang-up says you ended it`() {
        assertEquals(
            CallPeer.State.Ended(ep, peer, "you ended the call"),
            CallEnding.ended(ep, peer, Ending.LocalHangUp),
        )
    }

    @Test
    fun `a peer's close carries its own reason and code, verbatim`() {
        // The words the phones showed on 2026-09-14 (§16), byte for byte.
        assertEquals(
            CallPeer.State.Ended(ep, peer, "call ended: closed by peer: hangup (code 0)"),
            CallEnding.ended(ep, peer, Ending.ClosedByPeer(code = 0uL, reason = "hangup")),
        )
    }

    @Test
    fun `a lost connection carries the transport's words, or says lost plainly`() {
        assertEquals(
            CallPeer.State.Ended(ep, peer, "call ended: timed out"),
            CallEnding.ended(ep, peer, Ending.Lost(reason = "timed out")),
        )
        assertEquals(
            CallPeer.State.Ended(ep, peer, "connection lost"),
            CallEnding.ended(ep, peer, Ending.Lost(reason = "  ")),
        )
    }

    @Test
    fun `an ended state still knows its own endpoint — the device stays callable`() {
        val ended = CallEnding.ended(ep, peer, Ending.LocalHangUp)
        assertEquals(ep, ended.endpointId)
    }
}
