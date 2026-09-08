package ing.croft.call

import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * What a dial is allowed to do to the endpoint's bound relay token (§15.3).
 *
 * The relay auth token is a property of the *endpoint*, so changing it means
 * `stop()` + `start()` — the camped connection dies and re-attaches. Under
 * enforce that gap is real unreachability, and on 2026-09-08 it was measured
 * at four minutes on a physical phone: the tokenless dial path called
 * `rebindWithToken(null)` while the phone held a live camping pass, so the
 * re-attach presented no token at all and the relay refused it ~20 times
 * before an app restart recovered it.
 *
 * The rule these rows encode: **a dial never lowers the endpoint's admission.**
 * Swapping one admitting token for another is the designed M4c behaviour and
 * still costs a gap; dropping to tokenless while holding a pass is a strict
 * downgrade with no upside, because a camped endpoint is already admitted.
 */
@RunWith(RobolectricTestRunner::class)
class RebindPolicyTest {

    @Test
    fun `a tokenless dial keeps a live camping pass — reachability is never lowered`() {
        assertEquals(
            DialAdmission.Rebind.Keep,
            DialAdmission.rebind(current = "camp-pass", wanted = null),
        )
    }

    @Test
    fun `the same token is not a rebind`() {
        assertEquals(
            DialAdmission.Rebind.Keep,
            DialAdmission.rebind(current = "camp-pass", wanted = "camp-pass"),
        )
    }

    @Test
    fun `tokenless while already tokenless is not a rebind`() {
        assertEquals(
            DialAdmission.Rebind.Keep,
            DialAdmission.rebind(current = null, wanted = null),
        )
    }

    @Test
    fun `a minted call token swaps — a different admitting token is the M4c path`() {
        assertEquals(
            DialAdmission.Rebind.Swap("call-token"),
            DialAdmission.rebind(current = "camp-pass", wanted = "call-token"),
        )
    }

    @Test
    fun `an unbound endpoint binds the minted token`() {
        assertEquals(
            DialAdmission.Rebind.Swap("call-token"),
            DialAdmission.rebind(current = null, wanted = "call-token"),
        )
    }
}
