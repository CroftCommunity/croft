package ing.croft.call.net

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The three call-endings (E129), as words: hang-up, remote-end, error. The
 * pure mapper is the whole decision — CallPeer feeds it the local-hang-up
 * flag and whatever reason `Connection.closed()` returned, and the state
 * carries honest words. We never CLASSIFY the remote-vs-error string
 * (upstream's text is not our contract); we pass it through verbatim, so
 * the words carry the truth without us guessing at it.
 */
class CallEndingTest {

    private val ep = "cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33cc33"
    private val peer = "aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11"

    @Test
    fun `a local hang-up says you ended it, whatever the transport reports`() {
        assertEquals(
            CallPeer.State.Ended(ep, peer, "you ended the call"),
            CallEnding.ended(ep, peer, localHangUp = true, reason = "closed by us"),
        )
        assertEquals(
            CallPeer.State.Ended(ep, peer, "you ended the call"),
            CallEnding.ended(ep, peer, localHangUp = true, reason = null),
        )
    }

    @Test
    fun `a remote or transport ending carries the transport's own words`() {
        assertEquals(
            CallPeer.State.Ended(ep, peer, "call ended: closed by peer: 0"),
            CallEnding.ended(ep, peer, localHangUp = false, reason = "closed by peer: 0"),
        )
        assertEquals(
            CallPeer.State.Ended(ep, peer, "call ended: timed out"),
            CallEnding.ended(ep, peer, localHangUp = false, reason = "timed out"),
        )
    }

    @Test
    fun `an ending with no reason at all is a lost connection, said plainly`() {
        assertEquals(
            CallPeer.State.Ended(ep, peer, "connection lost"),
            CallEnding.ended(ep, peer, localHangUp = false, reason = null),
        )
        assertEquals(
            CallPeer.State.Ended(ep, peer, "connection lost"),
            CallEnding.ended(ep, peer, localHangUp = false, reason = "  "),
        )
    }

    @Test
    fun `an ended state still knows its own endpoint — the device stays callable`() {
        val ended = CallEnding.ended(ep, peer, localHangUp = true, reason = null)
        assertEquals(ep, ended.endpointId)
    }
}
