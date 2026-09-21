package ing.croft.call

import ing.croft.call.caps.Admit
import uniffi.croft_ffi.DialAction as CoreAction
import uniffi.croft_ffi.DialCallee as CoreCallee
import uniffi.croft_ffi.DialOutcome as CoreOutcome
import uniffi.croft_ffi.DialPlan as CorePlan
import uniffi.croft_ffi.DialRefusal as CoreRefusal
import uniffi.croft_ffi.ProofSource as CoreProof
import uniffi.croft_ffi.RebindDecision
import uniffi.croft_ffi.dialAction
import uniffi.croft_ffi.dialPlan
import uniffi.croft_ffi.dialRebind

/**
 * The pure half of mint-at-dial (Phase 11 M4c): which proof this dial
 * presents, and what each mint outcome does to the dial. Decisions only —
 * no I/O; the ViewModel performs the plan and feeds the outcome back.
 *
 * **D3.2 (2026-09-21): the decisions are `call-core`'s.** The shape stays
 * (the ViewModel and the matrix's `PIN:` rows are written against it) and
 * every decision goes through the FFI (`uniffi.croft_ffi.dialPlan` /
 * `dialAction` / `dialRebind`); the words come back verbatim.
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

    fun plan(callee: Callee, signedIn: Boolean): Plan =
        when (val p = dialPlan(CoreCallee(callee.did, callee.grant, callee.ticketSecret), signedIn)) {
            is CorePlan.Mint -> Plan.Mint(
                calleeDid = p.calleeDid,
                grant = p.grant,
                proof = when (val source = p.proof) {
                    is CoreProof.Ticket -> ProofSource.Ticket(source.secret)
                    is CoreProof.ServiceAuth -> ProofSource.ServiceAuth
                },
            )
            is CorePlan.DialTokenless -> Plan.DialTokenless(p.note)
        }

    /** What a mint outcome does to the dial. */
    sealed interface Action {
        /** Proceed, with the token when there is one. */
        data class Dial(val authToken: String?, val note: String?) : Action

        /** Do not dial; show why. */
        data class Refuse(val message: String) : Action
    }

    fun action(outcome: Admit.Outcome): Action =
        when (val a = dialAction(outcome.toCore())) {
            is CoreAction.Dial -> Action.Dial(authToken = a.authToken, note = a.note)
            is CoreAction.Refuse -> Action.Refuse(a.message)
        }

    /** What a dial must do to the endpoint's currently bound relay token. */
    sealed interface Rebind {
        /**
         * Leave the endpoint alone. Either the wanted token is already bound,
         * or binding the wanted token would *lower* our admission.
         */
        data object Keep : Rebind

        /** Bind [token]; the endpoint stops and re-attaches, and the camp
         *  gaps. Never null (D3's note): the only downgrade available is
         *  giving up a token, and that is a [Keep]. */
        data class Swap(val token: String) : Rebind
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
     * anyway. The rule is `call_core::dial::rebind`, R0, enforced at the
     * Rust port's endpoint too.
     */
    fun rebind(current: String?, wanted: String?): Rebind =
        when (val d = dialRebind(current, wanted)) {
            is RebindDecision.Keep -> Rebind.Keep
            is RebindDecision.Swap -> Rebind.Swap(d.token)
        }

    private fun Admit.Outcome.toCore(): CoreOutcome = when (this) {
        is Admit.Outcome.Minted -> CoreOutcome.Minted(token)
        is Admit.Outcome.Refused -> CoreOutcome.Refused(
            when (reason) {
                Admit.Refusal.NO_CAP -> CoreRefusal.NO_CAP
                Admit.Refusal.CAP_NOT_FOUND -> CoreRefusal.CAP_NOT_FOUND
                Admit.Refusal.CAP_REVOKED -> CoreRefusal.CAP_REVOKED
                Admit.Refusal.CAP_MISMATCH -> CoreRefusal.CAP_MISMATCH
                Admit.Refusal.JWT_INVALID -> CoreRefusal.JWT_INVALID
                Admit.Refusal.REPLAY -> CoreRefusal.REPLAY
                Admit.Refusal.QUOTA_EXHAUSTED -> CoreRefusal.QUOTA_EXHAUSTED
                Admit.Refusal.UNKNOWN -> CoreRefusal.UNKNOWN
            },
        )
        Admit.Outcome.Unavailable -> CoreOutcome.Unavailable
        Admit.Outcome.BadRequest -> CoreOutcome.BadRequest
    }
}
