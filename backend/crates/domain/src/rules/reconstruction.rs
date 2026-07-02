use chrono::{NaiveDate, NaiveDateTime};

use crate::types::common::Confidence;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    Log,
    Commit,
}

#[derive(Debug, Clone)]
pub struct Signal {
    pub at: NaiveDateTime,
    pub gryzzly_project_id: Option<String>,
    pub kind: SignalKind,
    pub label: String,
    pub source_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeetingKind {
    Work,
    OutOfOffice,
}

#[derive(Debug, Clone)]
pub struct MeetingBlock {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub gryzzly_project_id: Option<String>,
    pub kind: MeetingKind,
    pub title: String,
    pub source_ref: String,
}

#[derive(Debug, Clone, Copy)]
pub struct ReconstructionConfig {
    pub morning: (u32, u32),
    pub afternoon: (u32, u32),
    pub daily_target_hours: f64,
    pub rounding_hours: f64,
    pub min_signal_hours: f64,
}

impl Default for ReconstructionConfig {
    fn default() -> Self {
        Self {
            morning: (8, 12),
            afternoon: (13, 17),
            daily_target_hours: 7.5,
            rounding_hours: 0.25,
            min_signal_hours: 2.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Meeting,
    Work,
    OutOfOffice,
}

#[derive(Debug, Clone)]
pub struct AttributedBlock {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub gryzzly_project_id: Option<String>,
    pub kind: BlockKind,
    pub hours: f64,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectAllocation {
    pub gryzzly_project_id: String,
    pub hours: f64,
    pub confidence: Confidence,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct UnresolvedSignal {
    pub source_ref: String,
    pub label: String,
    pub at: NaiveDateTime,
}

#[derive(Debug, Clone)]
pub struct ReconstructedDay {
    pub date: NaiveDate,
    pub allocations: Vec<ProjectAllocation>,
    pub unattributed_hours: f64,
    pub unresolved: Vec<UnresolvedSignal>,
    pub total_hours: f64,
    pub day_confidence: Confidence,
    pub blocks: Vec<AttributedBlock>,
}

/// A weighted bucket for apportionment. `key = None` is the unattributed bucket.
#[derive(Debug, Clone, PartialEq)]
pub struct Bucket {
    pub key: Option<String>,
    pub hours: f64,
    pub pinned: bool,
}

/// Round every bucket to a multiple of `rounding` such that the total equals
/// `target` exactly (largest-remainder apportionment). Pinned buckets keep their
/// value (rounded to the increment) and are excluded from redistribution; the
/// leftover is spread across UNpinned buckets by largest fractional remainder.
/// If unpinned buckets can't absorb the leftover (all pinned), the residual is
/// appended to the unattributed bucket (key=None), created if absent.
/// If PINNED buckets alone exceed `target`, unpinned buckets are zeroed and the
/// returned total equals the pinned sum — the caller MUST validate pinned <= target
/// (see `save_timesheet_draft` in Task 13) before relying on the total == target invariant.
pub fn apportion_to_target(buckets: &[Bucket], target: f64, rounding: f64) -> Vec<Bucket> {
    let unit = rounding.max(f64::EPSILON);
    let target_units = (target / unit).round() as i64;

    // Pinned buckets: snap to nearest unit, reserve their units.
    let mut out: Vec<Bucket> = Vec::with_capacity(buckets.len());
    let mut pinned_units = 0i64;
    for b in buckets.iter().filter(|b| b.pinned) {
        let u = (b.hours / unit).round().max(0.0) as i64;
        pinned_units += u;
        out.push(Bucket { key: b.key.clone(), hours: u as f64 * unit, pinned: true });
    }

    let unpinned: Vec<&Bucket> = buckets.iter().filter(|b| !b.pinned).collect();
    let remaining_units = (target_units - pinned_units).max(0);

    let raw_sum: f64 = unpinned.iter().map(|b| b.hours.max(0.0)).sum();
    if unpinned.is_empty() || raw_sum <= 0.0 {
        // Nothing unpinned to scale — dump any remaining units on unattributed.
        if remaining_units > 0 {
            push_or_merge_unattributed(&mut out, remaining_units as f64 * unit);
        }
        return out;
    }

    // Scale unpinned to the remaining units, floor to integer units, distribute leftover.
    let scale = remaining_units as f64 / (raw_sum / unit);
    let mut floors: Vec<(usize, i64, f64)> = Vec::with_capacity(unpinned.len());
    let mut used = 0i64;
    for (i, b) in unpinned.iter().enumerate() {
        let scaled = (b.hours.max(0.0) / unit) * scale;
        let f = scaled.floor() as i64;
        let rem = scaled - f as f64;
        used += f;
        floors.push((i, f, rem));
    }
    let mut leftover = remaining_units - used;
    // Give leftover units to the largest remainders (stable by index on ties).
    let mut order: Vec<usize> = (0..floors.len()).collect();
    order.sort_by(|&a, &b| {
        floors[b].2
            .partial_cmp(&floors[a].2)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    let mut idx = 0;
    while leftover > 0 && !order.is_empty() {
        let target_i = order[idx % order.len()];
        floors[target_i].1 += 1;
        leftover -= 1;
        idx += 1;
    }
    for (i, units, _) in floors {
        out.push(Bucket {
            key: unpinned[i].key.clone(),
            hours: units as f64 * unit,
            pinned: false,
        });
    }
    out
}

fn push_or_merge_unattributed(out: &mut Vec<Bucket>, add_hours: f64) {
    if let Some(b) = out.iter_mut().find(|b| b.key.is_none()) {
        b.hours += add_hours;
    } else {
        out.push(Bucket { key: None, hours: add_hours, pinned: false });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(key: Option<&str>, hours: f64, pinned: bool) -> Bucket {
        Bucket { key: key.map(|s| s.to_string()), hours, pinned }
    }

    fn total(bs: &[Bucket]) -> f64 {
        bs.iter().map(|b| b.hours).sum()
    }

    #[test]
    fn apportion_sums_exactly_to_target() {
        let out = apportion_to_target(
            &[b(Some("a"), 1.0, false), b(Some("b"), 2.0, false)],
            7.5,
            0.25,
        );
        assert!((total(&out) - 7.5).abs() < 1e-9, "total was {}", total(&out));
    }

    #[test]
    fn apportion_rounds_to_increment() {
        let out = apportion_to_target(&[b(Some("a"), 1.0, false), b(Some("b"), 1.0, false)], 7.5, 0.25);
        for bucket in &out {
            let units = bucket.hours / 0.25;
            assert!((units - units.round()).abs() < 1e-9, "{} not a 0.25 multiple", bucket.hours);
        }
    }

    #[test]
    fn pinned_bucket_is_frozen_others_absorb_remainder() {
        let out = apportion_to_target(
            &[b(Some("a"), 3.0, true), b(Some("b"), 1.0, false), b(Some("c"), 1.0, false)],
            7.5,
            0.25,
        );
        let a = out.iter().find(|x| x.key.as_deref() == Some("a")).unwrap();
        assert!((a.hours - 3.0).abs() < 1e-9, "pinned a should stay 3.0, got {}", a.hours);
        assert!((total(&out) - 7.5).abs() < 1e-9);
    }

    #[test]
    fn all_pinned_residual_goes_to_unattributed() {
        let out = apportion_to_target(&[b(Some("a"), 3.0, true)], 7.5, 0.25);
        let un = out.iter().find(|x| x.key.is_none()).unwrap();
        assert!((un.hours - 4.5).abs() < 1e-9, "unattributed should absorb 4.5, got {}", un.hours);
        assert!((total(&out) - 7.5).abs() < 1e-9);
    }

    #[test]
    fn over_pinned_zeroes_unpinned_and_keeps_pinned() {
        // Pinned lines total 8.5h > 7.5h target: unpinned zeroed, pinned preserved.
        // (The use case rejects this before persisting; the pure fn stays total-honest.)
        let out = apportion_to_target(
            &[b(Some("a"), 5.0, true), b(Some("b"), 3.5, true), b(Some("c"), 2.0, false)],
            7.5,
            0.25,
        );
        let a = out.iter().find(|x| x.key.as_deref() == Some("a")).unwrap();
        let bb = out.iter().find(|x| x.key.as_deref() == Some("b")).unwrap();
        let c = out.iter().find(|x| x.key.as_deref() == Some("c")).unwrap();
        assert!((a.hours - 5.0).abs() < 1e-9);
        assert!((bb.hours - 3.5).abs() < 1e-9);
        assert!((c.hours - 0.0).abs() < 1e-9, "unpinned zeroed when pinned exceed target");
    }
}
