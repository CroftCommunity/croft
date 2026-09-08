package ing.croft.call

import ing.croft.call.caps.Admit

/**
 * The pure half of mint-at-dial (Phase 11 M4c): which proof this dial
 * presents, and what each mint outcome does to the dial. Decisions only —
 * no I/O; the ViewModel performs the plan and feeds the outcome back.
 *
 * Posture (plan M4c):
 * - a REFUSAL never dials — "not permitted" must never look like a
 *   network flake;
 * - an OUTAGE dials tokenless with a visible note: the relay is the
 *   actual gate (open in production today; under enforce it refuses the
 *   attach), and calling must not die because croft-admit is down;
 * - a v1 callee (no grant) dials tokenless silently, exactly as v0.4.0;
 * - a grant with no usable proof dials tokenless with a sign-in nudge —
 *   open-mode compatible; the enforce-mode refusal comes from the relay.
 */
object DialAdmission {

    /** Where the mint's proof comes from. */
    sealed interface ProofSource {
        /** The retained invite secret. */
        data class Ticket(val secret: String) : ProofSource

        /** A service-auth JWT from the caller's session (fetched at dial). */
        data object ServiceAuth : ProofSource
    }

    /** What this dial should do about admission. */
    sealed interface Plan {
        /** Call the mint with this proof, then act on the outcome. */
        data class Mint(
            val calleeDid: String,
            val grant: String,
            val proof: ProofSource,
        ) : Plan

        /** Dial with no token; [note] surfaces when there is one. */
        data class DialTokenless(val note: String?) : Plan
    }

    fun plan(callee: Callee, signedIn: Boolean): Plan {
        val grant = callee.grant
        val did = callee.did
        if (grant == null || did == null) return Plan.DialTokenless(note = null)
        callee.ticketSecret?.let {
            return Plan.Mint(calleeDid = did, grant = grant, proof = ProofSource.Ticket(it))
        }
        if (signedIn) return Plan.Mint(calleeDid = did, grant = grant, proof = ProofSource.ServiceAuth)
        return Plan.DialTokenless(note = "sign in to present this grant")
    }

    /** What a mint outcome does to the dial. */
    sealed interface Action {
        /** Proceed, with the token when there is one. */
        data class Dial(val authToken: String?, val note: String?) : Action

        /** Do not dial; show why. */
        data class Refuse(val message: String) : Action
    }

    fun action(outcome: Admit.Outcome): Action = when (outcome) {
        is Admit.Outcome.Minted -> Action.Dial(authToken = outcome.token, note = null)
        is Admit.Outcome.Refused -> Action.Refuse(
            when (outcome.reason) {
                Admit.Refusal.CAP_REVOKED -> "this invite has been revoked"
                Admit.Refusal.CAP_NOT_FOUND -> "this invite no longer exists"
                Admit.Refusal.CAP_MISMATCH -> "this invite does not admit you"
                Admit.Refusal.JWT_INVALID -> "identity proof was not accepted"
                Admit.Refusal.REPLAY -> "identity proof was already used — try again"
                Admit.Refusal.QUOTA_EXHAUSTED -> "the sponsoring account is out of quota"
                Admit.Refusal.NO_CAP -> "no invite was presented"
                Admit.Refusal.UNKNOWN -> "the call was not permitted"
            },
        )
        Admit.Outcome.Unavailable -> Action.Dial(
            authToken = null,
            note = "admission service unreachable — dialing without a token",
        )
        Admit.Outcome.BadRequest -> Action.Refuse("client error building the admission request")
    }

    /** What a dial must do to the endpoint's currently bound relay token. */
    sealed interface Rebind {
        /**
         * Leave the endpoint alone. Either the wanted token is already bound,
         * or binding the wanted token would *lower* our admission.
         */
        data object Keep : Rebind

        /** Bind [token]; the endpoint stops and re-attaches, and the camp gaps. */
        data class Swap(val token: String?) : Rebind
    }

    /**
     * A dial never lowers the endpoint's admission (§15.3).
     *
     * The relay auth token belongs to the *endpoint*, so changing it costs a
     * `stop()`/`start()` — the camped connection dies and re-attaches. Swapping
     * one admitting token for another is the designed M4c path and pays that
     * cost deliberately. Dropping to tokenless while holding a pass pays it for
     * nothing: a camped endpoint is already admitted, and under enforce the
     * tokenless re-attach is refused by definition.
     *
     * Measured before this existed (runbook §15.3, a physical phone on the
     * enforcing relay): the tokenless path called `rebindWithToken(null)` over a
     * live camping pass, the re-attach was refused ~20 times with `no_token`,
     * and the phone stayed unreachable for four minutes until it was restarted.
     * Open mode had hidden it completely — a tokenless re-attach was admitted
     * anyway.
     */
    fun rebind(current: String?, wanted: String?): Rebind = when {
        wanted == current -> Rebind.Keep
        // The only downgrade available: giving up a token we already hold.
        wanted == null -> Rebind.Keep
        else -> Rebind.Swap(wanted)
    }
}
