package ing.croft.call

import ing.croft.call.caps.Admit

/**
 * The pure half of camp-at-attach (Phase 11 M4e, O1): whether this attach
 * presents a camping pass, and what each camp-mint outcome does to the camp.
 * Decisions only — no I/O, no clock reads; the ViewModel performs the plan
 * and feeds the outcome back, exactly [DialAdmission]'s shape.
 *
 * Posture (plan M4e), deliberately unlike dial:
 * - **the token is the cache** (O1 point 3): a minted pass is reused across
 *   reconnects until [REMINT_MARGIN_MILLIS] before its expiry — the identity
 *   round-trip happens once per pass lifetime, not per connectivity flap;
 * - a camp REFUSAL still camps tokenless WITH the worded reason — in open
 *   mode reception must keep working, and under enforce the relay's refusal
 *   is the visible gate;
 * - an OUTAGE camps tokenless with the M4c-style note;
 * - signed-out has no camping identity under O1: tokenless, silently —
 *   exactly v0.4.0 in open mode.
 */
object CampAdmission {

    /** Re-mint when the pass is within this margin of its expiry. */
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

    fun plan(signedIn: Boolean, cached: CampPass?, nowMs: Long): Plan {
        if (!signedIn) return Plan.CampTokenless(note = null)
        if (cached != null && nowMs + REMINT_MARGIN_MILLIS < cached.expiresAtMillis) {
            return Plan.UseCached(cached.token)
        }
        return Plan.Mint
    }

    /** What a camp-mint outcome does to the camp. */
    sealed interface Action {
        /** Camp with the pass, and cache it for the next attach. */
        data class Camp(val authToken: String, val pass: CampPass) : Action

        /** Camp with no token; [note] surfaces when there is one. */
        data class CampTokenless(val note: String?) : Action
    }

    fun action(outcome: Admit.CampOutcome, nowMs: Long): Action = when (outcome) {
        is Admit.CampOutcome.Minted -> Action.Camp(
            authToken = outcome.token,
            pass = CampPass(outcome.token, nowMs + outcome.expiresInSecs * 1000),
        )
        is Admit.CampOutcome.Refused -> Action.CampTokenless(
            when (outcome.reason) {
                Admit.CampRefusal.ENDPOINT_UNBOUND -> "this device is not published by your account"
                Admit.CampRefusal.JWT_INVALID -> "identity proof was not accepted"
                Admit.CampRefusal.REPLAY -> "identity proof was already used — try again"
                Admit.CampRefusal.PROOF_UNSUPPORTED ->
                    "the admission service does not accept identity proofs"
                Admit.CampRefusal.NO_PROOF -> "no identity proof was presented"
                Admit.CampRefusal.UNKNOWN_KEY -> "this key is not trusted by the admission service"
                Admit.CampRefusal.UNKNOWN -> "the camping pass was refused"
            },
        )
        Admit.CampOutcome.Unavailable -> Action.CampTokenless(
            note = "admission service unreachable — camping without a pass",
        )
        Admit.CampOutcome.BadRequest -> Action.CampTokenless(
            note = "client error building the camping request",
        )
    }
}
