package ing.croft.call

import ing.croft.call.caps.Admit
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * The pure half of mint-at-dial (M4c): which proof a dial presents, and
 * what each mint outcome does to the dial. The posture rows are the
 * design (plan M4c + the Unavailable decision):
 *
 *  - a REFUSAL never dials — "not permitted" must never look like
 *    "network flake";
 *  - an OUTAGE dials tokenless with a visible note — the relay is the
 *    actual gate (open today; under enforce it will refuse the attach),
 *    and calling must not die because croft-admit is down;
 *  - a v1 callee (no grant) dials tokenless silently, exactly as v0.4.0;
 *  - a grant with no usable proof (no secret, signed out) dials tokenless
 *    with a sign-in nudge rather than refusing — open-mode compatible,
 *    and the enforce-mode refusal will come from the relay with truth.
 */
@RunWith(RobolectricTestRunner::class)
class DialAdmissionTest {

    private fun callee(
        grant: String? = "party",
        did: String? = "did:plc:callee",
        secret: String? = null,
    ) = Callee(
        endpointId = "ee00", relayUrl = null, handle = null,
        did = did, device = null, grant = grant, ticketSecret = secret,
    )

    // ---- the plan: which proof ----------------------------------------------

    @Test
    fun `a ticket secret is the proof whenever the card carries one`() {
        val plan = DialAdmission.plan(callee(secret = "s3cr3t"), signedIn = true)
        assertEquals(
            DialAdmission.Plan.Mint(
                calleeDid = "did:plc:callee", grant = "party",
                proof = DialAdmission.ProofSource.Ticket("s3cr3t"),
            ),
            plan,
        )
    }

    @Test
    fun `a signed-in caller proves identity when there is no secret`() {
        assertEquals(
            DialAdmission.Plan.Mint(
                calleeDid = "did:plc:callee", grant = "party",
                proof = DialAdmission.ProofSource.ServiceAuth,
            ),
            DialAdmission.plan(callee(), signedIn = true),
        )
    }

    @Test
    fun `no grant means the v1 tokenless dial, silently`() {
        assertEquals(
            DialAdmission.Plan.DialTokenless(note = null),
            DialAdmission.plan(callee(grant = null), signedIn = true),
        )
        // A grant without a DID cannot be minted against either.
        assertEquals(
            DialAdmission.Plan.DialTokenless(note = null),
            DialAdmission.plan(callee(did = null), signedIn = true),
        )
    }

    @Test
    fun `a grant with no usable proof dials tokenless with a sign-in nudge`() {
        val plan = DialAdmission.plan(callee(), signedIn = false)
        assertEquals(
            DialAdmission.Plan.DialTokenless(note = "sign in to present this grant"),
            plan,
        )
    }

    // ---- the action: what each outcome does ---------------------------------

    @Test
    fun `a minted token dials with it`() {
        assertEquals(
            DialAdmission.Action.Dial(authToken = "tok", note = null),
            DialAdmission.action(Admit.Outcome.Minted("tok")),
        )
    }

    @Test
    fun `every refusal blocks the dial with its own honest message`() {
        val rows = mapOf(
            Admit.Refusal.CAP_REVOKED to "this invite has been revoked",
            Admit.Refusal.CAP_NOT_FOUND to "this invite no longer exists",
            Admit.Refusal.CAP_MISMATCH to "this invite does not admit you",
            Admit.Refusal.JWT_INVALID to "identity proof was not accepted",
            Admit.Refusal.REPLAY to "identity proof was already used — try again",
            Admit.Refusal.QUOTA_EXHAUSTED to "the sponsoring account is out of quota",
            Admit.Refusal.NO_CAP to "no invite was presented",
            Admit.Refusal.UNKNOWN to "the call was not permitted",
        )
        for ((reason, message) in rows) {
            assertEquals(
                "reason $reason",
                DialAdmission.Action.Refuse(message),
                DialAdmission.action(Admit.Outcome.Refused(reason)),
            )
        }
    }

    @Test
    fun `an admit outage dials tokenless and says so`() {
        assertEquals(
            DialAdmission.Action.Dial(
                authToken = null,
                note = "admission service unreachable — dialing without a token",
            ),
            DialAdmission.action(Admit.Outcome.Unavailable),
        )
    }

    @Test
    fun `a bad request is a client defect and blocks the dial`() {
        assertEquals(
            DialAdmission.Action.Refuse("client error building the admission request"),
            DialAdmission.action(Admit.Outcome.BadRequest),
        )
    }
}
