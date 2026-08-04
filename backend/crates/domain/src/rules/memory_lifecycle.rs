//! The validation-queue and truth transitions of a memory. Pure: every function
//! takes the rows it needs and returns new values, so the write path can be
//! tested without a database.
//!
//! Two lifecycles live here and must not be confused (§6.3 of the design):
//!
//! - **`status`** is the validation queue: `pending` → `active` | `rejected`.
//! - **`invalidated_at` + `superseded_by`** is the truth: when a fact stopped
//!   being true, and what replaced it.
//!
//! [`merge`] collapses two rows into one — it ERASES history, and is only for
//! "same fact, better wording". [`supersede`] keeps both rows — it PRESERVES
//! history, and is for "the fact changed". Conflating them destroys the answer to
//! "why did we change our mind", which is half of what a secretary is for.

use chrono::{DateTime, Utc};

use crate::errors::DomainError;
use crate::rules::dedup::normalized_levenshtein;
use crate::types::memory::{Memory, MemoryId, MemoryKind, MemoryStatus};

/// Title similarity at or above which two memories are treated as near-duplicates.
pub const NEAR_DUPLICATE_THRESHOLD: f64 = 0.6;

/// Result of a merge: ONE row survives, the other is deleted. History is lost —
/// that is the point of the operation, and why it is not the default.
#[derive(Debug, Clone, PartialEq)]
pub struct MergeOutcome {
    /// The surviving row: the target's identity and temporality, the candidate's wording.
    pub survivor: Memory,
    /// The row the caller must delete.
    pub discarded: MemoryId,
}

/// Result of a supersession: BOTH rows survive. The old one carries the end of
/// its validity and a pointer to what replaced it.
#[derive(Debug, Clone, PartialEq)]
pub struct SupersedeOutcome {
    /// The old row, now carrying `invalidated_at` and `superseded_by`.
    pub invalidated: Memory,
    /// The new truth, now active.
    pub successor: Memory,
}

/// Accept a pending candidate: `pending` → `active`, optionally re-typing it.
/// Does not touch temporality — accepting does not change when the thing
/// happened, nor when aplan learned it.
///
/// Accepting also *answers* any supersession the candidate proposed, with "no":
/// this verb invalidates nothing, so both facts stay true and the claim is
/// [`spent`](spend_proposal).
pub fn accept(candidate: &Memory, kind_override: Option<MemoryKind>) -> Result<Memory, DomainError> {
    require_pending(candidate, "accepted")?;
    let mut accepted = candidate.clone();
    accepted.status = MemoryStatus::Active;
    if let Some(kind) = kind_override {
        accepted.kind = kind;
    }
    spend_proposal(&mut accepted);
    Ok(accepted)
}

/// Reject a pending candidate. The row is KEPT as a tombstone: without it the
/// consolidation job would re-propose the same candidate every evening.
///
/// The tombstone keeps the wording — that is what makes the loop converge — but
/// not the claim: nothing is left to settle.
pub fn reject(candidate: &Memory) -> Result<Memory, DomainError> {
    require_pending(candidate, "rejected")?;
    let mut rejected = candidate.clone();
    rejected.status = MemoryStatus::Rejected;
    spend_proposal(&mut rejected);
    Ok(rejected)
}

/// Merge a pending candidate into an active memory: same fact, better wording.
///
/// The target keeps its identity and its temporality (`occurred_at`,
/// `recorded_at`) — only the wording changes, so the dates must not move. The
/// stakeholders are unioned: a rewording should never lose a person.
pub fn merge(candidate: &Memory, target: &Memory) -> Result<MergeOutcome, DomainError> {
    if candidate.id == target.id {
        return Err(DomainError::ValidationError(
            "a memory cannot be merged into itself".into(),
        ));
    }
    require_same_user(candidate, target)?;
    require_pending(candidate, "merged")?;
    if target.status != MemoryStatus::Active {
        return Err(DomainError::ValidationError(format!(
            "memory {} is {} and cannot receive a merge; only an active memory can",
            target.id,
            target.status.as_str()
        )));
    }
    if target.invalidated_at.is_some() {
        return Err(DomainError::MemoryAlreadyInvalidated(target.id));
    }

    let mut survivor = target.clone();
    survivor.title = candidate.title.clone();
    survivor.body = candidate.body.clone();
    for person in &candidate.stakeholders {
        if !survivor.stakeholders.contains(person) {
            survivor.stakeholders.push(person.clone());
        }
    }
    // Entity links are filled in when either side has one; the candidate wins a tie.
    if candidate.project_id.is_some() {
        survivor.project_id = candidate.project_id;
    }
    if candidate.task_id.is_some() {
        survivor.task_id = candidate.task_id;
    }
    // The candidate's claim is deliberately NOT inherited: a merge means "same
    // fact", so there is no contradiction left to settle. Inheriting it would also
    // be pathological in the common case — the near-duplicate gate offers merge and
    // supersede on the same pair, so the claim usually names the merge target, and
    // the survivor would end up proposing to supersede itself.
    spend_proposal(&mut survivor);

    Ok(MergeOutcome {
        survivor,
        discarded: candidate.id,
    })
}

/// Supersede an active memory by another: the fact CHANGED. The old row is
/// invalidated and points at its successor; both rows survive.
///
/// `chain_from_successor` is the list of ids reachable from `successor` by
/// following `superseded_by`, resolved by the caller (walking it is I/O). It is
/// what makes cycle detection possible without the domain touching a database.
///
/// Chains are legal: A superseded by B, then B superseded by C. Cycles are not,
/// and a memory may not supersede itself.
pub fn supersede(
    old: &Memory,
    successor: &Memory,
    chain_from_successor: &[MemoryId],
    now: DateTime<Utc>,
) -> Result<SupersedeOutcome, DomainError> {
    if old.id == successor.id {
        // The degenerate cycle: a self-loop.
        return Err(DomainError::MemorySupersessionCycle {
            old: old.id,
            new: successor.id,
        });
    }
    if chain_from_successor.contains(&old.id) {
        return Err(DomainError::MemorySupersessionCycle {
            old: old.id,
            new: successor.id,
        });
    }
    require_same_user(old, successor)?;

    // Decided: refuse to re-supersede. The row already has a successor, and
    // overwriting `superseded_by` would drop that link and fork the very history
    // the bi-temporal model exists to preserve. The caller must target the head
    // of the chain, which is what the error says.
    if old.invalidated_at.is_some() {
        return Err(DomainError::MemoryAlreadyInvalidated(old.id));
    }
    if old.status != MemoryStatus::Active {
        return Err(DomainError::ValidationError(format!(
            "memory {} is {} and holds no truth to invalidate; only an active memory can be superseded",
            old.id,
            old.status.as_str()
        )));
    }
    if successor.invalidated_at.is_some() {
        return Err(DomainError::ValidationError(format!(
            "memory {} is invalidated and cannot become the new truth",
            successor.id
        )));
    }
    if successor.status == MemoryStatus::Rejected {
        return Err(DomainError::ValidationError(format!(
            "memory {} is a rejected tombstone and cannot become the new truth",
            successor.id
        )));
    }

    let mut invalidated = old.clone();
    invalidated.invalidated_at = Some(now);
    invalidated.superseded_by = Some(successor.id);
    spend_proposal(&mut invalidated);

    let mut promoted = successor.clone();
    promoted.status = MemoryStatus::Active;
    // The claim has been HONOURED: `invalidated.superseded_by` now records the same
    // fact structurally. Keeping both would store one truth twice, in two forms
    // that can drift.
    spend_proposal(&mut promoted);

    Ok(SupersedeOutcome {
        invalidated,
        successor: promoted,
    })
}

/// Clear a supersession proposal that has been answered.
///
/// Every queue verdict — accept, reject, merge, supersede — is an answer, so every
/// one of them calls this. The invariant it upholds: a row that is no longer
/// `pending` carries no proposal, which is what lets any reader treat a proposal it
/// finds as a live question rather than an archaeological one.
fn spend_proposal(memory: &mut Memory) {
    memory.proposed_supersedes = None;
}

fn require_pending(candidate: &Memory, verb: &str) -> Result<(), DomainError> {
    if candidate.status != MemoryStatus::Pending {
        return Err(DomainError::ValidationError(format!(
            "memory {} is {} and cannot be {verb}; only a pending candidate can",
            candidate.id,
            candidate.status.as_str()
        )));
    }
    Ok(())
}

fn require_same_user(a: &Memory, b: &Memory) -> Result<(), DomainError> {
    if a.user_id != b.user_id {
        return Err(DomainError::ValidationError(
            "memories belong to different users".into(),
        ));
    }
    Ok(())
}

/// Similarity of two memory titles in `[0, 1]`.
///
/// The maximum of two signals, because "better wording" takes two forms:
/// re-ordering the same words (token overlap catches it, edit distance does not)
/// and fixing a typo (edit distance catches it, token overlap does not).
pub fn title_similarity(a: &str, b: &str) -> f64 {
    let overlap = token_overlap(a, b);
    let edit = normalized_levenshtein(&a.to_lowercase(), &b.to_lowercase());
    overlap.max(edit)
}

/// Jaccard index over the lowercased alphanumeric tokens. No length filter:
/// dropping short tokens would make `wave 0` and `wave 1` identical.
fn token_overlap(a: &str, b: &str) -> f64 {
    let tokens_a = tokens(a);
    let tokens_b = tokens(b);
    if tokens_a.is_empty() && tokens_b.is_empty() {
        return 1.0;
    }
    if tokens_a.is_empty() || tokens_b.is_empty() {
        return 0.0;
    }
    let shared = tokens_a.iter().filter(|t| tokens_b.contains(t)).count();
    let union = tokens_a.len() + tokens_b.len() - shared;
    if union == 0 {
        return 1.0;
    }
    shared as f64 / union as f64
}

fn tokens(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for token in text.split(|c: char| !c.is_alphanumeric()) {
        if token.is_empty() {
            continue;
        }
        let lowered = token.to_lowercase();
        if !out.contains(&lowered) {
            out.push(lowered);
        }
    }
    out
}

/// The candidates that look like near-duplicates of `title`, most similar first.
///
/// This is the "never a silent add" gate of §6.3: FTS5 narrows the field, this
/// rule decides, and the human picks `merge` (a rewording) or `supersede` (a
/// changed fact). Nothing here tries to tell those two apart — that judgement is
/// semantic, and the backend holds no model.
pub fn near_duplicates<'a>(title: &str, candidates: &'a [Memory]) -> Vec<(&'a Memory, f64)> {
    let mut scored: Vec<(&Memory, f64)> = candidates
        .iter()
        .map(|memory| (memory, title_similarity(title, &memory.title)))
        .filter(|(_, score)| *score >= NEAR_DUPLICATE_THRESHOLD)
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::memory::{MemorySource, NewMemory};
    use uuid::Uuid;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-03T09:00:00+00:00")
            .expect("valid fixture")
            .with_timezone(&Utc)
    }

    fn memory(user_id: Uuid, title: &str, status: MemoryStatus) -> Memory {
        Memory::new(
            user_id,
            NewMemory {
                kind: MemoryKind::Decision,
                title: title.into(),
                body: Some("because Pierre asked".into()),
                occurred_at: Some(now() - chrono::Duration::days(10)),
                source: MemorySource::ClaudeSession,
                source_ref: None,
                status,
                proposed_supersedes: None,
                project_id: None,
                task_id: None,
                stakeholders: vec![],
            },
            now(),
        )
        .expect("valid fixture")
    }

    // ─── accept / reject ────────────────────────────────────────────────────

    #[test]
    fn accept_promotes_a_pending_candidate() {
        let uid = Uuid::new_v4();
        let candidate = memory(uid, "Wave 0 limited to the Microsoft AI scope", MemoryStatus::Pending);
        let accepted = accept(&candidate, None).expect("accepts");
        assert_eq!(accepted.status, MemoryStatus::Active);
        assert!(accepted.is_recallable());
        assert_eq!(accepted.id, candidate.id, "accepting keeps the identity");
        assert_eq!(accepted.occurred_at, candidate.occurred_at);
        assert_eq!(accepted.recorded_at, candidate.recorded_at);
    }

    #[test]
    fn accept_can_retype_the_candidate() {
        let uid = Uuid::new_v4();
        let candidate = memory(uid, "the mcp crate does not compile", MemoryStatus::Pending);
        let accepted = accept(&candidate, Some(MemoryKind::Fact)).expect("accepts");
        assert_eq!(accepted.kind, MemoryKind::Fact);
    }

    #[test]
    fn accept_refuses_anything_not_pending() {
        let uid = Uuid::new_v4();
        for status in [MemoryStatus::Active, MemoryStatus::Rejected] {
            let m = memory(uid, "already decided", status);
            assert!(matches!(
                accept(&m, None).unwrap_err(),
                DomainError::ValidationError(_)
            ));
        }
    }

    #[test]
    fn reject_keeps_the_row_as_a_tombstone() {
        let uid = Uuid::new_v4();
        let candidate = memory(uid, "not worth remembering", MemoryStatus::Pending);
        let rejected = reject(&candidate).expect("rejects");
        assert_eq!(rejected.status, MemoryStatus::Rejected);
        assert_eq!(rejected.id, candidate.id, "the tombstone keeps the id");
        assert!(!rejected.is_recallable());
        assert_eq!(
            rejected.invalidated_at, None,
            "rejection is a queue verdict, not an end of validity"
        );
    }

    #[test]
    fn reject_refuses_an_active_memory() {
        let uid = Uuid::new_v4();
        let active = memory(uid, "an established fact", MemoryStatus::Active);
        assert!(matches!(
            reject(&active).unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    // ─── the supersession proposal is spent by the queue verdict ────────────

    /// A candidate carrying a proposal, i.e. the shape the consolidation writes.
    fn candidate_proposing(user_id: Uuid, title: &str, older: MemoryId) -> Memory {
        let mut candidate = memory(user_id, title, MemoryStatus::Pending);
        candidate.proposed_supersedes = Some(older);
        candidate
    }

    /// Accepting is the human answering "no, this is a new fact, keep both" — the
    /// verb invalidates nothing. Leaving the claim on the row afterwards would keep
    /// announcing a conflict that was just waved through, and `recall` would keep
    /// printing it.
    #[test]
    fn accept_spends_the_supersession_proposal() {
        let uid = Uuid::new_v4();
        let older = Uuid::new_v4();
        let accepted = accept(&candidate_proposing(uid, "a new scope", older), None).expect("accepts");
        assert_eq!(accepted.status, MemoryStatus::Active);
        assert_eq!(
            accepted.proposed_supersedes, None,
            "an active memory never carries a pending claim"
        );
        assert_eq!(
            accepted.invalidated_at, None,
            "accepting is not superseding: nothing was invalidated"
        );
    }

    #[test]
    fn reject_spends_the_supersession_proposal() {
        let uid = Uuid::new_v4();
        let rejected =
            reject(&candidate_proposing(uid, "a new scope", Uuid::new_v4())).expect("rejects");
        assert_eq!(rejected.status, MemoryStatus::Rejected);
        assert_eq!(
            rejected.proposed_supersedes, None,
            "the tombstone keeps the wording, not the claim"
        );
    }

    /// The trap this closes: the near-duplicate gate offers `merge` and
    /// `supersede` on the SAME pair, so the memory a candidate proposes to
    /// supersede is usually the very one it gets merged into. Inheriting the claim
    /// would leave the survivor proposing to supersede itself.
    #[test]
    fn merge_does_not_carry_the_candidates_proposal_into_the_survivor() {
        let uid = Uuid::new_v4();
        let target = memory(uid, "Wave 0 limited to Microsoft AI", MemoryStatus::Active);
        let candidate = candidate_proposing(uid, "Wave 0 scope, restated", target.id);

        let outcome = merge(&candidate, &target).expect("merges");
        assert_eq!(
            outcome.survivor.proposed_supersedes, None,
            "the survivor must not propose to supersede itself"
        );
    }

    /// Applying the supersession is what the proposal asked for. Keeping it would
    /// leave the same fact recorded twice — once structurally on the old row, once
    /// as a claim on the new one — and two representations drift.
    #[test]
    fn supersede_spends_the_proposal_it_realises() {
        let uid = Uuid::new_v4();
        let old = memory(uid, "Wave 0 limited to Microsoft AI", MemoryStatus::Active);
        let successor = candidate_proposing(uid, "Wave 0 extended to the platform", old.id);

        let outcome = supersede(&old, &successor, &[], now()).expect("supersedes");
        assert_eq!(outcome.successor.proposed_supersedes, None);
        assert_eq!(
            outcome.invalidated.superseded_by,
            Some(successor.id),
            "the claim is replaced by the real link, not lost"
        );
        assert_eq!(outcome.invalidated.proposed_supersedes, None);
    }

    // ─── merge vs supersede: the distinction that carries the bi-temporal ───

    #[test]
    fn merge_keeps_one_row_and_erases_no_history_because_there_is_none_to_keep() {
        let uid = Uuid::new_v4();
        let target = memory(uid, "Wave 0 limited to Microsoft AI", MemoryStatus::Active);
        let mut candidate = memory(uid, "Wave 0 is limited to the Microsoft AI scope", MemoryStatus::Pending);
        candidate.stakeholders = vec!["Pierre".into()];

        let outcome = merge(&candidate, &target).expect("merges");
        assert_eq!(outcome.survivor.id, target.id, "the target's identity survives");
        assert_eq!(outcome.discarded, candidate.id, "the candidate row is dropped");
        assert_eq!(
            outcome.survivor.title, candidate.title,
            "the better wording wins"
        );
        assert_eq!(outcome.survivor.body, candidate.body);
        assert_eq!(
            outcome.survivor.occurred_at, target.occurred_at,
            "a rewording must not move the temporality"
        );
        assert_eq!(outcome.survivor.recorded_at, target.recorded_at);
        assert_eq!(outcome.survivor.invalidated_at, None);
        assert_eq!(outcome.survivor.superseded_by, None);
        assert_eq!(
            outcome.survivor.stakeholders,
            vec!["Pierre".to_string()],
            "a rewording must not lose a person"
        );
    }

    /// The pin the design asks for: the same pair of rows, merged and superseded,
    /// must produce different worlds. Merge = one row, no invalidation. Supersede
    /// = two rows, one invalidated and pointing at the other.
    #[test]
    fn merge_and_supersede_are_not_interchangeable() {
        let uid = Uuid::new_v4();
        let old = memory(uid, "Wave 0 limited to Microsoft AI", MemoryStatus::Active);
        let new = memory(uid, "Wave 0 extended to the whole platform", MemoryStatus::Pending);

        let merged = merge(&new, &old).expect("merges");
        let superseded = supersede(&old, &new, &[], now()).expect("supersedes");

        // Merge: one survivor, nothing invalidated, no successor pointer, and the
        // old wording is gone for good.
        assert_eq!(merged.survivor.invalidated_at, None);
        assert_eq!(merged.survivor.superseded_by, None);
        assert_eq!(merged.discarded, new.id);
        assert_eq!(merged.survivor.title, new.title);

        // Supersede: both rows survive, the old one keeps its own wording, and
        // the chain records why the truth moved.
        assert_eq!(superseded.invalidated.id, old.id);
        assert_eq!(superseded.invalidated.title, old.title, "history is preserved");
        assert_eq!(superseded.invalidated.invalidated_at, Some(now()));
        assert_eq!(superseded.invalidated.superseded_by, Some(new.id));
        assert_eq!(superseded.successor.id, new.id);
        assert_eq!(superseded.successor.status, MemoryStatus::Active);
        assert!(!superseded.invalidated.is_recallable(), "the old fact is hidden");
        assert!(superseded.successor.is_recallable(), "the new fact is recalled");
    }

    #[test]
    fn merge_refuses_itself_a_non_pending_candidate_and_a_dead_target() {
        let uid = Uuid::new_v4();
        let target = memory(uid, "an active fact", MemoryStatus::Active);
        let candidate = memory(uid, "a candidate", MemoryStatus::Pending);

        assert!(matches!(
            merge(&candidate, &candidate).unwrap_err(),
            DomainError::ValidationError(_)
        ));
        assert!(matches!(
            merge(&target, &target).unwrap_err(),
            DomainError::ValidationError(_)
        ));

        let active_candidate = memory(uid, "already active", MemoryStatus::Active);
        assert!(matches!(
            merge(&active_candidate, &target).unwrap_err(),
            DomainError::ValidationError(_)
        ));

        let pending_target = memory(uid, "still pending", MemoryStatus::Pending);
        assert!(matches!(
            merge(&candidate, &pending_target).unwrap_err(),
            DomainError::ValidationError(_)
        ));

        let mut dead_target = target.clone();
        dead_target.invalidated_at = Some(now());
        assert_eq!(
            merge(&candidate, &dead_target).unwrap_err(),
            DomainError::MemoryAlreadyInvalidated(dead_target.id)
        );
    }

    #[test]
    fn merge_refuses_to_cross_users() {
        let candidate = memory(Uuid::new_v4(), "a candidate", MemoryStatus::Pending);
        let target = memory(Uuid::new_v4(), "another user's fact", MemoryStatus::Active);
        assert!(matches!(
            merge(&candidate, &target).unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    // ─── supersession: chains legal, cycles not ─────────────────────────────

    #[test]
    fn a_chain_of_supersessions_is_legal() {
        let uid = Uuid::new_v4();
        let a = memory(uid, "scope is X", MemoryStatus::Active);
        let b = memory(uid, "scope is Y", MemoryStatus::Pending);
        let first = supersede(&a, &b, &[], now()).expect("A superseded by B");

        // B is now the active head; C replaces it in turn.
        let c = memory(uid, "scope is Z", MemoryStatus::Pending);
        let second = supersede(&first.successor, &c, &[], now()).expect("B superseded by C");

        assert_eq!(first.invalidated.superseded_by, Some(b.id));
        assert_eq!(second.invalidated.id, b.id);
        assert_eq!(second.invalidated.superseded_by, Some(c.id));
        assert!(second.successor.is_recallable());
    }

    #[test]
    fn a_memory_cannot_supersede_itself() {
        let uid = Uuid::new_v4();
        let m = memory(uid, "scope is X", MemoryStatus::Active);
        assert_eq!(
            supersede(&m, &m, &[], now()).unwrap_err(),
            DomainError::MemorySupersessionCycle { old: m.id, new: m.id }
        );
    }

    #[test]
    fn a_cycle_through_the_chain_is_refused() {
        let uid = Uuid::new_v4();
        // A was superseded by B, B by C. Trying to supersede C by A would close
        // the loop: A is reachable from... itself, via C.
        let a = memory(uid, "scope is X", MemoryStatus::Active);
        let c = memory(uid, "scope is Z", MemoryStatus::Active);
        let chain_from_a = vec![Uuid::new_v4(), c.id];
        assert_eq!(
            supersede(&c, &a, &chain_from_a, now()).unwrap_err(),
            DomainError::MemorySupersessionCycle { old: c.id, new: a.id }
        );
    }

    /// Decision recorded in the report: re-superseding an already-invalidated row
    /// is refused, because overwriting `superseded_by` would drop the existing
    /// link and fork the history the model exists to keep.
    #[test]
    fn an_already_invalidated_memory_cannot_be_superseded_again() {
        let uid = Uuid::new_v4();
        let a = memory(uid, "scope is X", MemoryStatus::Active);
        let b = memory(uid, "scope is Y", MemoryStatus::Pending);
        let first = supersede(&a, &b, &[], now()).expect("A superseded by B");

        let c = memory(uid, "scope is Z", MemoryStatus::Pending);
        assert_eq!(
            supersede(&first.invalidated, &c, &[], now()).unwrap_err(),
            DomainError::MemoryAlreadyInvalidated(a.id),
            "the caller must target B, the head of the chain"
        );
    }

    #[test]
    fn only_an_active_memory_can_be_superseded() {
        let uid = Uuid::new_v4();
        let successor = memory(uid, "the new truth", MemoryStatus::Pending);
        for status in [MemoryStatus::Pending, MemoryStatus::Rejected] {
            let old = memory(uid, "not established", status);
            assert!(matches!(
                supersede(&old, &successor, &[], now()).unwrap_err(),
                DomainError::ValidationError(_)
            ));
        }
    }

    #[test]
    fn a_dead_or_rejected_row_cannot_become_the_new_truth() {
        let uid = Uuid::new_v4();
        let old = memory(uid, "scope is X", MemoryStatus::Active);

        let mut dead = memory(uid, "scope is Y", MemoryStatus::Active);
        dead.invalidated_at = Some(now());
        assert!(matches!(
            supersede(&old, &dead, &[], now()).unwrap_err(),
            DomainError::ValidationError(_)
        ));

        let tombstone = memory(uid, "scope is Y", MemoryStatus::Rejected);
        assert!(matches!(
            supersede(&old, &tombstone, &[], now()).unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    #[test]
    fn supersede_refuses_to_cross_users() {
        let old = memory(Uuid::new_v4(), "scope is X", MemoryStatus::Active);
        let successor = memory(Uuid::new_v4(), "scope is Y", MemoryStatus::Pending);
        assert!(matches!(
            supersede(&old, &successor, &[], now()).unwrap_err(),
            DomainError::ValidationError(_)
        ));
    }

    #[test]
    fn supersede_promotes_an_already_active_successor_unchanged() {
        // The `aplan memory supersede` path: both rows are already active.
        let uid = Uuid::new_v4();
        let old = memory(uid, "scope is X", MemoryStatus::Active);
        let successor = memory(uid, "scope is Y", MemoryStatus::Active);
        let outcome = supersede(&old, &successor, &[], now()).expect("supersedes");
        assert_eq!(outcome.successor.status, MemoryStatus::Active);
        assert_eq!(outcome.successor.id, successor.id);
    }

    // ─── near-duplicate detection ──────────────────────────────────────────

    #[test]
    fn a_reordered_rewording_is_a_near_duplicate() {
        let score = title_similarity(
            "Wave 0 limited to the Microsoft AI scope",
            "Wave 0 scope limited to Microsoft AI",
        );
        assert!(
            score >= NEAR_DUPLICATE_THRESHOLD,
            "reordering must be caught, got {score}"
        );
    }

    #[test]
    fn a_typo_fix_is_a_near_duplicate() {
        let score = title_similarity("Wave 0 limitee au perimetre", "Wave 0 limite au perimetre");
        assert!(score >= NEAR_DUPLICATE_THRESHOLD, "got {score}");
    }

    #[test]
    fn an_unrelated_title_is_not_a_near_duplicate() {
        let score = title_similarity(
            "Wave 0 limited to the Microsoft AI scope",
            "Cartier certificate must be renewed",
        );
        assert!(score < NEAR_DUPLICATE_THRESHOLD, "got {score}");
    }

    #[test]
    fn titles_differing_only_by_a_number_are_not_collapsed() {
        // No short-token filter, so `0` and `1` still separate these.
        let score = title_similarity("wave 0 scope", "wave 1 scope");
        assert!(score < 1.0, "got {score}");
    }

    #[test]
    fn identical_titles_score_one() {
        assert!((title_similarity("same thing", "same thing") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn near_duplicates_returns_the_most_similar_first_and_drops_the_rest() {
        let uid = Uuid::new_v4();
        let close = memory(uid, "Wave 0 scope limited to Microsoft AI", MemoryStatus::Active);
        let exact = memory(uid, "Wave 0 limited to the Microsoft AI scope", MemoryStatus::Active);
        let unrelated = memory(uid, "Cartier certificate must be renewed", MemoryStatus::Active);
        let pool = vec![close.clone(), exact.clone(), unrelated];

        let found = near_duplicates("Wave 0 limited to the Microsoft AI scope", &pool);
        assert_eq!(found.len(), 2, "the unrelated title is filtered out");
        assert_eq!(found[0].0.id, exact.id, "the exact match ranks first");
        assert_eq!(found[1].0.id, close.id);
    }

    #[test]
    fn near_duplicates_of_nothing_is_nothing() {
        assert!(near_duplicates("anything", &[]).is_empty());
    }
}
