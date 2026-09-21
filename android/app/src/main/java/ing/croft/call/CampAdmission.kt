package ing.croft.call

import ing.croft.call.caps.Admit
import uniffi.croft_ffi.CampAction as CoreAction
import uniffi.croft_ffi.CampFailure as CoreFailure
import uniffi.croft_ffi.CampOutcome as CoreOutcome
import uniffi.croft_ffi.CampPass as CorePass
import uniffi.croft_ffi.CampPlan as CorePlan
import uniffi.croft_ffi.CampRefusal as CoreRefusal
import uniffi.croft_ffi.campAction
import uniffi.croft_ffi.campFailureNote
import uniffi.croft_ffi.campPlan

/**
 * The pure half of camp-at-attach (Phase 11 M4e, O1): whether this attach
 * presents a camping pass, and what each camp-mint outcome does to the camp.
 * Decisions only — no I/O, no clock reads; the ViewModel performs the plan
 * and feeds the outcome back, exactly [DialAdmission]'s shape.
 *
 * **D3.2 (2026-09-21): the decisions are `call-core`'s.** This object keeps
 * the shape the ViewModel and the enforcement matrix's `PIN:` rows were
 * written against and delegates every decision through the FFI
 * (`uniffi.croft_ffi.campPlan` / `campAction` / `campFailureNote`), so the
 * shipped app and the Rust rules are ONE implementation graded by the same
 * rows. The words come back from the core verbatim; nothing here re-words a
 * refusal.
 *
 * Posture (plan M4e), deliberately unlike dial:
 * - **the token is the cache** (O1 point 3): a minted pass is reused across
 *   reconnects until the core's re-mint margin before its expiry — the
 *   identity round-trip happens once per pass lifetime, not per connectivity
 *   flap;
 * - a camp REFUSAL still camps tokenless WITH the worded reason — in open
 *   mode reception must keep working, and under enforce the relay's refusal
 *   is the visible gate;
 * - an OUTAGE camps tokenless with the M4c-style note;
 * - signed-out has no camping identity under O1: tokenless, silently —
 *   exactly v0.4.0 in open mode.
 */
object CampAdmission {

    /** Re-mint when the pass is within this margin of its expiry. Mirrors
     *  the core's margin; the matrix's boundary rows (exactly at the margin,
     *  one past it) are what catch a drift between the two. */
    const val REMINT_MARGIN_MILLIS: Long = 60_000

    /** A minted camping pass: the opaque token and when the WIRE said it
     *  expires (`expiresIn` on the mint response — the token is never
     *  parsed, D3). */
    data class CampPass(val token: String, val expiresAtMillis: Long)

    /** What this attach should do about a camping pass. */
    sealed interface Plan {
        /** Camp with the cached pass; no network. */
        data class UseCached(val token: String) : Plan

        /** Call the camp mint, then act on the outcome. */
        data object Mint : Plan

        /** Camp with no token; [note] surfaces when there is one. */
        data class CampTokenless(val note: String?) : Plan
    }

    fun plan(signedIn: Boolean, cached: CampPass?, nowMs: Long): Plan =
        when (val p = campPlan(signedIn, cached?.let { CorePass(it.token, it.expiresAtMillis) }, nowMs)) {
            is CorePlan.UseCached -> Plan.UseCached(p.token)
            is CorePlan.Mint -> Plan.Mint
            is CorePlan.CampTokenless -> Plan.CampTokenless(p.note)
        }

    /** What a camp-mint outcome does to the camp. */
    sealed interface Action {
        /** Camp with the pass, and cache it for the next attach. */
        data class Camp(val authToken: String, val pass: CampPass) : Action

        /** Camp with no token; [note] surfaces when there is one. */
        data class CampTokenless(val note: String?) : Action
    }

    fun action(outcome: Admit.CampOutcome, nowMs: Long): Action =
        when (val a = campAction(outcome.toCore(), nowMs)) {
            is CoreAction.Camp -> Action.Camp(
                authToken = a.authToken,
                pass = CampPass(a.pass.token, a.pass.expiresAtMillis),
            )
            is CoreAction.CampTokenless -> Action.CampTokenless(a.note)
        }

    /**
     * Words for a camp attempt that threw, or null when there is nothing
     * honest to say. A [kotlinx.coroutines.CancellationException] means the
     * app rebound or backgrounded mid-attempt — a lifecycle event, not a
     * refusal — and rendering it as "camping pass setup failed" put a scary,
     * unactionable line on screen during device runs (2026-08-28).
     */
    fun failureNote(t: Throwable): String? = campFailureNote(
        if (t is kotlinx.coroutines.CancellationException) CoreFailure.Cancelled
        else CoreFailure.Error(t.message ?: t.toString()),
    )

    private fun Admit.CampOutcome.toCore(): CoreOutcome = when (this) {
        is Admit.CampOutcome.Minted -> CoreOutcome.Minted(token, expiresInSecs)
        is Admit.CampOutcome.Refused -> CoreOutcome.Refused(
            when (reason) {
                Admit.CampRefusal.NO_PROOF -> CoreRefusal.NO_PROOF
                Admit.CampRefusal.PROOF_UNSUPPORTED -> CoreRefusal.PROOF_UNSUPPORTED
                Admit.CampRefusal.JWT_INVALID -> CoreRefusal.JWT_INVALID
                Admit.CampRefusal.REPLAY -> CoreRefusal.REPLAY
                Admit.CampRefusal.UNKNOWN_KEY -> CoreRefusal.UNKNOWN_KEY
                Admit.CampRefusal.ENDPOINT_UNBOUND -> CoreRefusal.ENDPOINT_UNBOUND
                Admit.CampRefusal.UNKNOWN -> CoreRefusal.UNKNOWN
            },
        )
        Admit.CampOutcome.Unavailable -> CoreOutcome.Unavailable
        Admit.CampOutcome.BadRequest -> CoreOutcome.BadRequest
    }
}
