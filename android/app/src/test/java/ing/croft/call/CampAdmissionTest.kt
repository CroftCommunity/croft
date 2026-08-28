package ing.croft.call

import ing.croft.call.caps.Admit
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The pure half of camp-at-attach (M4e, O1): whether this attach presents a
 * camping pass, and what each camp-mint outcome does to the camp. Decisions
 * only — no I/O, no clock reads; `nowMs` is injected.
 *
 * Posture (plan M4e), deliberately unlike dial:
 * - **the token is the cache** (O1 point 3): a minted pass is reused across
 *   reconnects until a safety margin before its expiry;
 * - a camp REFUSAL still camps tokenless WITH the worded reason — in open
 *   mode reception must keep working, and under enforce the relay's refusal
 *   is the visible gate;
 * - an OUTAGE camps tokenless with the M4c-style note;
 * - signed-out simply has no camping identity under O1 — tokenless, no note
 *   (exactly v0.4.0 in open mode; the enforce refusal comes from the relay).
 */
class CampAdmissionTest {

    private val now = 1_700_000_000_000L

    // ---- plan ----------------------------------------------------------------

    @Test
    fun `signed-out camps tokenless with no note`() {
        assertEquals(
            CampAdmission.Plan.CampTokenless(note = null),
            CampAdmission.plan(signedIn = false, cached = null, nowMs = now),
        )
    }

    @Test
    fun `signed-in with no cached pass mints`() {
        assertEquals(
            CampAdmission.Plan.Mint,
            CampAdmission.plan(signedIn = true, cached = null, nowMs = now),
        )
    }

    @Test
    fun `a live cached pass is reused — the token is the cache`() {
        val pass = CampAdmission.CampPass(token = "opaque", expiresAtMillis = now + 3_600_000)
        assertEquals(
            CampAdmission.Plan.UseCached("opaque"),
            CampAdmission.plan(signedIn = true, cached = pass, nowMs = now),
        )
    }

    @Test
    fun `a pass inside the re-mint margin mints fresh instead of riding expiry`() {
        val pass = CampAdmission.CampPass(
            token = "opaque",
            expiresAtMillis = now + CampAdmission.REMINT_MARGIN_MILLIS - 1,
        )
        assertEquals(
            CampAdmission.Plan.Mint,
            CampAdmission.plan(signedIn = true, cached = pass, nowMs = now),
        )
    }

    @Test
    fun `a pass exactly at the margin boundary still mints`() {
        val pass = CampAdmission.CampPass(
            token = "opaque",
            expiresAtMillis = now + CampAdmission.REMINT_MARGIN_MILLIS,
        )
        assertEquals(
            CampAdmission.Plan.Mint,
            CampAdmission.plan(signedIn = true, cached = pass, nowMs = now),
        )
    }

    @Test
    fun `signing out drops the pass — a cached pass without a session does not camp`() {
        val pass = CampAdmission.CampPass(token = "opaque", expiresAtMillis = now + 3_600_000)
        assertEquals(
            CampAdmission.Plan.CampTokenless(note = null),
            CampAdmission.plan(signedIn = false, cached = pass, nowMs = now),
        )
    }

    // ---- action --------------------------------------------------------------

    @Test
    fun `a mint camps with the token and caches it by the wire's expiresIn`() {
        val action = CampAdmission.action(
            Admit.CampOutcome.Minted(token = "opaque", expiresInSecs = 43_200),
            nowMs = now,
        )
        assertEquals(
            CampAdmission.Action.Camp(
                authToken = "opaque",
                pass = CampAdmission.CampPass("opaque", now + 43_200 * 1000),
            ),
            action,
        )
    }

    @Test
    fun `a refusal camps tokenless with words — reception must not die quietly`() {
        val action = CampAdmission.action(
            Admit.CampOutcome.Refused(Admit.CampRefusal.ENDPOINT_UNBOUND),
            nowMs = now,
        )
        val tokenless = action as CampAdmission.Action.CampTokenless
        assertEquals("this device is not published by your account", tokenless.note)
    }

    @Test
    fun `each refusal reason has its own words`() {
        fun note(r: Admit.CampRefusal): String? =
            (
                CampAdmission.action(Admit.CampOutcome.Refused(r), nowMs = now)
                    as CampAdmission.Action.CampTokenless
                ).note
        assertEquals("identity proof was not accepted", note(Admit.CampRefusal.JWT_INVALID))
        assertEquals(
            "identity proof was already used — try again",
            note(Admit.CampRefusal.REPLAY),
        )
        val distinct = Admit.CampRefusal.entries.map { note(it) }
        assertTrue("every refusal carries words", distinct.all { !it.isNullOrEmpty() })
    }

    @Test
    fun `an outage camps tokenless with the availability note`() {
        assertEquals(
            CampAdmission.Action.CampTokenless(
                note = "admission service unreachable — camping without a pass",
            ),
            CampAdmission.action(Admit.CampOutcome.Unavailable, nowMs = now),
        )
    }

    @Test
    fun `a client defect camps tokenless and says so`() {
        val action = CampAdmission.action(Admit.CampOutcome.BadRequest, nowMs = now)
        val tokenless = action as CampAdmission.Action.CampTokenless
        assertTrue(tokenless.note!!.isNotEmpty())
    }

    // Device runs 2026-08-28 showed the words "camping pass setup failed: Job
    // was cancelled" reaching the SCREEN whenever a rebind cancelled the camp
    // coroutine — a lifecycle event rendered as a failure the user can do
    // nothing about. Cancellation is not a refusal and must not speak.

    @Test
    fun `a cancelled camp is not a failure and says nothing`() {
        assertEquals(
            null,
            CampAdmission.failureNote(kotlinx.coroutines.CancellationException("Job was cancelled")),
        )
    }

    @Test
    fun `a real failure still says what went wrong`() {
        assertEquals(
            "camping pass setup failed: boom",
            CampAdmission.failureNote(IllegalStateException("boom")),
        )
    }
}
