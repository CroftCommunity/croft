//! **The admission decision — the A3 invariant as code (E117 P4, ADR-0003).**
//!
//! [`evaluate_admission`] is the only thing in the system that can answer
//! "admit?". The KeyLayer port carries artifacts and parsed claims (data,
//! never a decision); this function consumes those claims plus
//! adapter-assembled governance context and returns either a named refusal
//! or a [`MergeApproval`] — the one value the port's merge accepts, and a
//! value nothing outside this module can construct. The approval carries the
//! minted [`AdmissionFact`] inside it, so §11.7's merge-rule clause (a merge
//! that would not deposit its fact MUST be refused) is unwritable to
//! violate: no approval without a fact, no merge without an approval.
//!
//! Position discipline (S26): `subject_standing` is the subject's standing
//! **at the commit's causal position**, resolved by the adapter's own fold —
//! never the evaluator's head, never a claim from the wire. The context
//! field is named for the obligation.

use thiserror::Error;

use crate::model::{Hash, PrincipalId};
use crate::project::head_currency::{admits_membership_origination, HeadCurrency, Stalled};

/// A governance-issued re-entry token's identity (the PSK id we mint).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TokenId([u8; 32]);

impl TokenId {
    /// Wrap raw token-id bytes.
    #[must_use]
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The raw bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// A chain issuance fact, as the adapter derives it from the governance log:
/// token `token` was issued to `lineage`. Revocation is a chain fact, not an
/// erasure — a revoked issuance still exists; it just no longer admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IssuanceFact {
    /// The token issued.
    pub token: TokenId,
    /// The one lineage this token may admit (§11.7: persona-bound by
    /// cross-check, never bearer possession).
    pub lineage: PrincipalId,
    /// Whether a revocation fact exists at the evaluated position.
    pub revoked: bool,
}

/// The subject's standing at the commit's causal position, per the
/// evaluator's own fold (the adapter's obligation — S26).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectStanding {
    /// Standing intact: no exclusion, no open contradiction.
    Good,
    /// Banned at the evaluated position.
    Excluded,
    /// The subject's standing slot is CONTESTED (E108): an open
    /// contradiction the gate must not resolve by admitting.
    Contested,
}

/// Parsed claims from the key layer: what a `NewMemberCommit` *says*, as
/// data. The port extracts these; it never acts on them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionClaims {
    /// The lineage the joiner's leaf credential resolves to.
    pub joiner_lineage: PrincipalId,
    /// The token the commit presents (its PSK id).
    pub presented_token: TokenId,
    /// The commit's content address — the admission *event*'s identity
    /// (§7.3.4: per-acceptor facts corroborate one event, never rival it).
    pub commit_content_address: Hash,
    /// The governance position the commit claims — a locator, never an
    /// authorization input (§7.4.3); the adapter resolves standing against
    /// its own fold to this position.
    pub commit_position: u64,
}

/// Everything the decision needs, assembled by the adapter from governance
/// state it already owns — the `FoldContext` pattern.
#[derive(Debug)]
pub struct AdmissionContext<'a> {
    /// Issuance facts derived from the governance log.
    pub issuance: &'a [IssuanceFact],
    /// The subject's standing at the commit's causal position.
    pub subject_standing: SubjectStanding,
    /// The evaluator's head-currency state (behind-via-traffic detection).
    pub currency: HeadCurrency,
    /// Distinct-lineage vouchers for the evaluator's head (C3's HeadAck
    /// count — the §7.4 corroborated-fresh input).
    pub freshness: u64,
    /// Current member count, from which the freshness quorum k derives.
    pub member_count: u64,
    /// The acceptor's own governance frontier, committed into the fact
    /// (§7.5.1 classifies a concurrently-stale acceptor by it).
    pub acceptor_frontier: u64,
}

/// Why an admission is refused before it can seat anyone. Every variant is a
/// distinct product-renderable state; refusing loudly with the reason is the
/// house posture.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum AdmissionRefusal {
    /// The presented token has no issuance fact on the chain: holding PSK
    /// bytes is not holding a fact (S24 arm d).
    #[error("no issuance fact exists for the presented token")]
    NoIssuanceFact,
    /// The issuance fact exists and is revoked at the evaluated position.
    #[error("the token's issuance is revoked")]
    IssuanceRevoked,
    /// The token was issued to a different lineage than the joiner's
    /// credential resolves to.
    #[error("the token was issued to a different lineage")]
    LineageMismatch {
        /// The lineage the issuance fact names.
        issued_to: PrincipalId,
    },
    /// The subject is banned at the evaluated position; both proofs may be
    /// honest, standing refuses (S25's merge-side enforcement).
    #[error("the subject's standing is excluded at the evaluated position")]
    StandingExcluded,
    /// The subject's standing slot is CONTESTED; admitting would manufacture
    /// a verdict the fold refused to make (E108).
    #[error("the subject's standing is contested; no admission through an open contradiction")]
    StandingContested,
    /// The §7.3.8 finality gate: the evaluator cannot corroborate the
    /// subject's standing as fresh — the merge stalls, fail closed.
    #[error("the merge stalls: {0:?}")]
    Stalled(Stalled),
}

/// The admission fact the approval carries: an R6-shaped acceptance record
/// that **opens a membership span** — never a slot-competing membership
/// addition (§11.7's comparator placement). The acceptor's identity is the
/// envelope author when the adapter deposits this to the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionFact {
    /// The admission event's identity: the merged commit's content address.
    pub event: Hash,
    /// The lineage admitted — the span this fact opens is that lineage's.
    pub merged_lineage: PrincipalId,
    /// The token redeemed.
    pub redeemed_token: TokenId,
    /// The acceptor's governance frontier at merge — its frontier
    /// commitment.
    pub acceptor_frontier: u64,
}

/// The permission slip: the one value [`crate::ports`]' KeyLayer merge will
/// accept, constructible only by [`evaluate_admission`]. The private field
/// is the enforcement — an adapter cannot express "admit" on its own, and a
/// merge without its fact cannot be written.
#[derive(Debug)]
pub struct MergeApproval {
    fact: AdmissionFact,
}

impl MergeApproval {
    /// The admission fact this approval carries; the shell persists it to
    /// the governance log with the merge, or neither happens.
    #[must_use]
    pub fn fact(&self) -> &AdmissionFact {
        &self.fact
    }
}

/// Decide an admission: the §11.7 cross-check plus standing plus the §7.3.8
/// finality gate, in one pure function.
///
/// # Errors
/// A named [`AdmissionRefusal`] for every path that must not seat the
/// joiner; refusing loudly with the reason is the contract.
pub fn evaluate_admission(
    claims: &AdmissionClaims,
    ctx: &AdmissionContext<'_>,
) -> Result<MergeApproval, AdmissionRefusal> {
    // The finality gate first: merging is irreversible, so an evaluator that
    // cannot corroborate freshness has no business reading the rest.
    admits_membership_origination(&ctx.currency, ctx.freshness, ctx.member_count)
        .map_err(AdmissionRefusal::Stalled)?;

    match ctx.subject_standing {
        SubjectStanding::Excluded => return Err(AdmissionRefusal::StandingExcluded),
        SubjectStanding::Contested => return Err(AdmissionRefusal::StandingContested),
        SubjectStanding::Good => {}
    }

    let issuance = ctx
        .issuance
        .iter()
        .find(|f| f.token == claims.presented_token)
        .ok_or(AdmissionRefusal::NoIssuanceFact)?;
    if issuance.revoked {
        return Err(AdmissionRefusal::IssuanceRevoked);
    }
    if issuance.lineage != claims.joiner_lineage {
        return Err(AdmissionRefusal::LineageMismatch {
            issued_to: issuance.lineage,
        });
    }

    Ok(MergeApproval {
        fact: AdmissionFact {
            event: claims.commit_content_address,
            merged_lineage: claims.joiner_lineage,
            redeemed_token: claims.presented_token,
            acceptor_frontier: ctx.acceptor_frontier,
        },
    })
}

/// Derive the issuance facts from the governance log — the chain is the
/// admission context's source, so nothing arrives out-of-band: every
/// `TokenIssuance` becomes an [`IssuanceFact`], and a `TokenRevocation`
/// naming its token flips `revoked` without erasing it (§11.7: revocation
/// is a chain fact the policy check consults, needing no key-deletion
/// race).
#[must_use]
pub fn issuance_view(log: &[(Hash, crate::model::AssertionEnvelope)]) -> Vec<IssuanceFact> {
    use crate::model::AssertionType;
    let mut facts: Vec<IssuanceFact> = Vec::new();
    for (_, e) in log {
        match e.assertion_type {
            AssertionType::TokenIssuance if e.payload.len() >= 64 => {
                let mut t = [0u8; 32];
                t.copy_from_slice(&e.payload[..32]);
                let mut l = [0u8; 32];
                l.copy_from_slice(&e.payload[32..64]);
                facts.push(IssuanceFact {
                    token: TokenId::new(t),
                    lineage: PrincipalId::new(l),
                    revoked: false,
                });
            }
            AssertionType::TokenRevocation if e.payload.len() >= 32 => {
                let mut t = [0u8; 32];
                t.copy_from_slice(&e.payload[..32]);
                let revoked_token = TokenId::new(t);
                for f in facts.iter_mut().filter(|f| f.token == revoked_token) {
                    f.revoked = true;
                }
            }
            _ => {}
        }
    }
    facts
}

/// Why an invite enactment is refused. The refusal is about the DECISION
/// layer: the enactment waits for the fold, or the fold is contested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum InviteRefusal {
    /// The fold has not seated this invitee: the governance decision has not
    /// happened (or has not reached this node) — the enactment waits.
    #[error("no folded MembershipAdd decision seats {invitee:?}; the enactment waits")]
    NotDecided {
        /// The invitee whose decision is missing.
        invitee: PrincipalId,
    },
    /// The invitee's standing slot is CONTESTED: no enactment through an
    /// open contradiction (E108's rule, on this path too).
    #[error("{invitee:?}'s standing is contested; no enactment through an open contradiction")]
    Contested {
        /// The contested invitee.
        invitee: PrincipalId,
    },
}

/// The invite path's permission slip: the one value the KeyLayer's
/// `add_with_welcome` accepts, constructible only here — and minted only
/// when the fold has already seated the invitee. MLS seating follows the
/// fold, never precedes it (S21's govern step, unskippable by
/// construction).
#[derive(Debug)]
pub struct InviteApproval {
    invitee: PrincipalId,
}

impl InviteApproval {
    /// The principal this slip seats.
    #[must_use]
    pub fn invitee(&self) -> &PrincipalId {
        &self.invitee
    }
}

/// Mint the invite-enactment slip: the folded `MembershipAdd` decision is
/// the authorization, read from the group state at the enacting node's
/// fold.
///
/// # Errors
/// [`InviteRefusal::NotDecided`] when the fold has not seated the invitee;
/// [`InviteRefusal::Contested`] when the invitee's slot is an open
/// contradiction.
pub fn authorize_invite_enactment(
    invitee: &PrincipalId,
    state: &crate::model::GroupState,
) -> Result<InviteApproval, InviteRefusal> {
    match state.membership(invitee) {
        crate::model::MembershipView::Member(_) => Ok(InviteApproval { invitee: *invitee }),
        crate::model::MembershipView::Contested(_) => {
            Err(InviteRefusal::Contested { invitee: *invitee })
        }
        crate::model::MembershipView::NotMember => {
            Err(InviteRefusal::NotDecided { invitee: *invitee })
        }
    }
}
