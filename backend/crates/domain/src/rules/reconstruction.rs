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

#[derive(Debug, Clone)]
pub struct DayInputs {
    pub date: NaiveDate,
    pub meetings: Vec<MeetingBlock>,
    pub signals: Vec<Signal>,
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
    // Invariant: leftover == sum of fractional remainders < number of unpinned buckets,
    // so each bucket receives at most one bonus unit (the idx % len wrap never triggers).
    debug_assert!(leftover <= floors.len() as i64, "leftover ({leftover}) exceeds bucket count {} — floors invariant broken", floors.len());
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

use chrono::Timelike;
use std::collections::HashMap;

/// A half-day window in local wall-clock minutes-from-midnight.
struct Window {
    start_min: i64,
    end_min: i64,
}

fn windows(cfg: &ReconstructionConfig) -> [Window; 2] {
    [
        Window { start_min: cfg.morning.0 as i64 * 60, end_min: cfg.morning.1 as i64 * 60 },
        Window { start_min: cfg.afternoon.0 as i64 * 60, end_min: cfg.afternoon.1 as i64 * 60 },
    ]
}

fn mins(dt: NaiveDateTime) -> i64 {
    dt.time().hour() as i64 * 60 + dt.time().minute() as i64
}

/// Reconstruct one day from its LOCAL-time signals and meetings.
pub fn reconstruct_day(inputs: &DayInputs, cfg: &ReconstructionConfig) -> ReconstructedDay {
    let mut blocks: Vec<AttributedBlock> = Vec::new();
    let mut unresolved: Vec<UnresolvedSignal> = Vec::new();

    // Out-of-office anchors suppress target scaling for the half-days they cover.
    let mut ooo_windows: Vec<(i64, i64)> = Vec::new();

    for w in windows(cfg).iter() {
        // Meetings clipped to this window.
        let mut anchors: Vec<(i64, i64, &MeetingBlock)> = inputs
            .meetings
            .iter()
            .filter_map(|m| {
                let s = mins(m.start).max(w.start_min);
                let e = mins(m.end).min(w.end_min);
                if e > s {
                    Some((s, e, m))
                } else {
                    None
                }
            })
            .collect();
        anchors.sort_by_key(|a| a.0);

        // Earlier meeting wins contested intervals: truncate later overlaps.
        let mut cursor = w.start_min;
        let mut fixed: Vec<(i64, i64, &MeetingBlock)> = Vec::new();
        for (s, e, m) in anchors {
            let s = s.max(cursor);
            if e > s {
                fixed.push((s, e, m));
                cursor = e;
            }
        }
        for (s, e, m) in &fixed {
            let kind = match m.kind {
                MeetingKind::Work => BlockKind::Meeting,
                MeetingKind::OutOfOffice => {
                    ooo_windows.push((*s, *e));
                    BlockKind::OutOfOffice
                }
            };
            if matches!(kind, BlockKind::OutOfOffice) {
                continue; // OOO consumes time but is never attributed to a project
            }
            blocks.push(AttributedBlock {
                start: inputs.date.and_hms_opt((*s / 60) as u32, (*s % 60) as u32, 0).unwrap(),
                end: inputs.date.and_hms_opt((*e / 60) as u32, (*e % 60) as u32, 0).unwrap(),
                gryzzly_project_id: m.gryzzly_project_id.clone(),
                kind: BlockKind::Meeting,
                hours: (e - s) as f64 / 60.0,
                source_refs: vec![m.source_ref.clone()],
            });
        }

        // Free intervals = window minus fixed meeting anchors.
        let mut free: Vec<(i64, i64)> = Vec::new();
        let mut c = w.start_min;
        for (s, e, _) in &fixed {
            if *s > c {
                free.push((c, *s));
            }
            c = (*e).max(c);
        }
        if c < w.end_min {
            free.push((c, w.end_min));
        }

        // Signals in this window, sorted by time.
        let mut sigs: Vec<&Signal> = inputs
            .signals
            .iter()
            .filter(|s| mins(s.at) >= w.start_min && mins(s.at) < w.end_min)
            .collect();
        sigs.sort_by_key(|s| mins(s.at));

        // Carry-forward within each free interval.
        for (fs, fe) in &free {
            let in_iv: Vec<&Signal> = sigs
                .iter()
                .copied()
                .filter(|s| {
                    let m = mins(s.at);
                    m >= *fs && m < *fe
                })
                .collect();
            if in_iv.is_empty() {
                continue;
            }
            for (i, s) in in_iv.iter().enumerate() {
                let start_min = if i == 0 { *fs } else { mins(s.at) };
                let end_min = if i + 1 < in_iv.len() { mins(in_iv[i + 1].at) } else { *fe };
                if end_min <= start_min {
                    continue;
                }
                if s.gryzzly_project_id.is_none() {
                    unresolved.push(UnresolvedSignal {
                        source_ref: s.source_ref.clone(),
                        label: s.label.clone(),
                        at: s.at,
                    });
                }
                blocks.push(AttributedBlock {
                    start: inputs.date.and_hms_opt((start_min / 60) as u32, (start_min % 60) as u32, 0).unwrap(),
                    end: inputs.date.and_hms_opt((end_min / 60) as u32, (end_min % 60) as u32, 0).unwrap(),
                    gryzzly_project_id: s.gryzzly_project_id.clone(),
                    kind: BlockKind::Work,
                    hours: (end_min - start_min) as f64 / 60.0,
                    source_refs: vec![s.source_ref.clone()],
                });
            }
        }
    }

    // Aggregate raw hours by project (None = unattributed).
    let mut raw: HashMap<Option<String>, (f64, Vec<String>)> = HashMap::new();
    for blk in &blocks {
        let entry = raw.entry(blk.gryzzly_project_id.clone()).or_insert((0.0, vec![]));
        entry.0 += blk.hours;
        entry.1.extend(blk.source_refs.iter().cloned());
    }
    let raw_total: f64 = raw.values().map(|(h, _)| *h).sum();

    let ooo_hours: f64 = ooo_windows.iter().map(|(s, e)| (e - s) as f64 / 60.0).sum();
    let low_signal = is_low_signal(&inputs.signals, cfg.min_signal_hours);

    // Guardrails + normalization (Task 6 replaces finalize_day's body).
    finalize_day(inputs.date, raw, raw_total, low_signal, ooo_hours, unresolved, blocks, cfg)
}

/// A day is "low signal" when it has fewer than 2 work signals, or all its signals
/// fall within a wall-clock span shorter than `min_span_hours` — not enough spread
/// to trust a full-day reconstruction. Measured from RAW signal timestamps, never
/// from carry-forward-inflated block hours. Meetings are anchors, not counted here.
fn is_low_signal(signals: &[Signal], min_span_hours: f64) -> bool {
    if signals.len() < 2 {
        return true;
    }
    let mut min_m = i64::MAX;
    let mut max_m = i64::MIN;
    for s in signals {
        let m = mins(s.at);
        min_m = min_m.min(m);
        max_m = max_m.max(m);
    }
    ((max_m - min_m) as f64 / 60.0) < min_span_hours
}

fn finalize_day(
    date: NaiveDate,
    raw: HashMap<Option<String>, (f64, Vec<String>)>,
    raw_total: f64,
    low_signal: bool,
    ooo_hours: f64,
    unresolved: Vec<UnresolvedSignal>,
    blocks: Vec<AttributedBlock>,
    cfg: &ReconstructionConfig,
) -> ReconstructedDay {
    if raw_total <= 0.0 {
        return ReconstructedDay {
            date, allocations: vec![], unattributed_hours: 0.0, unresolved,
            total_hours: 0.0, day_confidence: Confidence::Low, blocks,
        };
    }

    let unit = cfg.rounding_hours.max(f64::EPSILON);
    let round = |h: f64| (h / unit).round() * unit;

    if low_signal || ooo_hours > 0.0 {
        // GUARDED: never inflate projects. Keep raw project hours (rounded); fill up
        // to the billable target with quarantined unattributed hours.
        let billable_target = (cfg.daily_target_hours - ooo_hours).max(0.0);
        let mut allocations = Vec::new();
        let mut raw_unattr = 0.0;
        let mut sum_projects = 0.0;
        for (k, (h, refs)) in &raw {
            let rounded = round(*h);
            match k {
                Some(pid) => {
                    sum_projects += rounded;
                    allocations.push(ProjectAllocation {
                        gryzzly_project_id: pid.clone(),
                        hours: rounded,
                        confidence: Confidence::High,
                        source_refs: refs.clone(),
                    });
                }
                None => raw_unattr += rounded,
            }
        }
        // Fill only the gap up to the billable target (never negative → never caps real work).
        let fill = round((billable_target - sum_projects).max(0.0));
        let unattributed_hours = raw_unattr + fill;
        let total_hours = sum_projects + unattributed_hours;
        let day_confidence = if low_signal { Confidence::Low } else { Confidence::Medium };
        return ReconstructedDay {
            date, allocations, unattributed_hours, unresolved, total_hours, day_confidence, blocks,
        };
    }

    // HIGH-SIGNAL, no off-time: scale every bucket to the full target.
    let buckets: Vec<Bucket> = raw
        .iter()
        .map(|(k, (h, _))| Bucket { key: k.clone(), hours: *h, pinned: false })
        .collect();
    let apportioned = apportion_to_target(&buckets, cfg.daily_target_hours, cfg.rounding_hours);
    let mut allocations = Vec::new();
    let mut unattributed_hours = 0.0;
    for bkt in &apportioned {
        match &bkt.key {
            Some(pid) => {
                let refs = raw.get(&Some(pid.clone())).map(|(_, r)| r.clone()).unwrap_or_default();
                allocations.push(ProjectAllocation {
                    gryzzly_project_id: pid.clone(),
                    hours: bkt.hours,
                    confidence: Confidence::High,
                    source_refs: refs,
                });
            }
            None => unattributed_hours += bkt.hours,
        }
    }
    let total_hours: f64 = allocations.iter().map(|a| a.hours).sum::<f64>() + unattributed_hours;
    ReconstructedDay {
        date, allocations, unattributed_hours, unresolved, total_hours,
        day_confidence: Confidence::High, blocks,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditedLine {
    pub gryzzly_project_id: Option<String>,
    pub hours: f64,
    pub is_pinned: bool,
}

/// Re-apply target-rounding to user-edited lines: pinned lines are frozen,
/// unpinned lines + unattributed absorb the difference so the total == target.
pub fn renormalize_lines(lines: &[EditedLine], target: f64, rounding: f64) -> Vec<EditedLine> {
    let buckets: Vec<Bucket> = lines
        .iter()
        .map(|l| Bucket { key: l.gryzzly_project_id.clone(), hours: l.hours, pinned: l.is_pinned })
        .collect();
    apportion_to_target(&buckets, target, rounding)
        .into_iter()
        .map(|b| EditedLine { gryzzly_project_id: b.key, hours: b.hours, is_pinned: b.pinned })
        .collect()
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
    use chrono::NaiveDate;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 6, 8).unwrap()
    }
    fn at(h: u32, m: u32) -> NaiveDateTime {
        day().and_hms_opt(h, m, 0).unwrap()
    }
    fn sig(h: u32, m: u32, project: Option<&str>) -> Signal {
        Signal {
            at: at(h, m),
            gryzzly_project_id: project.map(|s| s.to_string()),
            kind: SignalKind::Log,
            label: format!("log {h}:{m}"),
            source_ref: format!("wl-{h}{m}"),
        }
    }
    fn meeting(sh: u32, eh: u32, project: Option<&str>, kind: MeetingKind) -> MeetingBlock {
        MeetingBlock {
            start: at(sh, 0),
            end: at(eh, 0),
            gryzzly_project_id: project.map(|s| s.to_string()),
            kind,
            title: "meet".into(),
            source_ref: format!("mtg-{sh}"),
        }
    }

    #[test]
    fn empty_day_yields_zero_total_low_confidence() {
        let out = reconstruct_day(
            &DayInputs { date: day(), meetings: vec![], signals: vec![] },
            &ReconstructionConfig::default(),
        );
        assert_eq!(out.total_hours, 0.0);
        assert_eq!(out.day_confidence, Confidence::Low);
        assert!(out.allocations.is_empty());
    }

    #[test]
    fn two_project_signals_split_and_scale_to_target() {
        // Morning log on p1 at 09:00; afternoon log on p2 at 14:00. Enough span (>2h).
        let out = reconstruct_day(
            &DayInputs {
                date: day(),
                meetings: vec![],
                signals: vec![sig(9, 0, Some("p1")), sig(14, 0, Some("p2"))],
            },
            &ReconstructionConfig::default(),
        );
        assert!((out.total_hours - 7.5).abs() < 1e-9, "total {}", out.total_hours);
        assert_eq!(out.day_confidence, Confidence::High);
        let p1 = out.allocations.iter().find(|a| a.gryzzly_project_id == "p1");
        let p2 = out.allocations.iter().find(|a| a.gryzzly_project_id == "p2");
        assert!(p1.is_some() && p2.is_some());
    }

    #[test]
    fn unresolved_signal_goes_to_unattributed_not_a_project() {
        let out = reconstruct_day(
            &DayInputs {
                date: day(),
                meetings: vec![],
                signals: vec![sig(9, 0, Some("p1")), sig(10, 0, None), sig(14, 0, Some("p1"))],
            },
            &ReconstructionConfig::default(),
        );
        assert!(out.unattributed_hours > 0.0);
        assert!(out.unresolved.iter().any(|u| u.source_ref == "wl-100"));
    }

    #[test]
    fn meeting_anchor_counts_toward_its_project() {
        // Only a 2h meeting on p_meet in the morning, plus one afternoon log on p1.
        let out = reconstruct_day(
            &DayInputs {
                date: day(),
                meetings: vec![meeting(9, 11, Some("p_meet"), MeetingKind::Work)],
                signals: vec![sig(14, 0, Some("p1"))],
            },
            &ReconstructionConfig::default(),
        );
        assert!(out.allocations.iter().any(|a| a.gryzzly_project_id == "p_meet"));
        assert!((out.total_hours - 7.5).abs() < 1e-9);
    }

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

    #[test]
    fn low_signal_day_quarantines_to_unattributed_not_projects() {
        // A single work signal → low_signal by count (< 2), regardless of span/threshold.
        let cfg = ReconstructionConfig::default();
        let out = reconstruct_day(
            &DayInputs { date: day(), meetings: vec![], signals: vec![sig(9, 0, Some("p1"))] },
            &cfg,
        );
        // p1 keeps only its raw carry-forward hours (~4h morning); the fill is quarantined.
        assert_eq!(out.day_confidence, Confidence::Low);
        let p1 = out.allocations.iter().find(|a| a.gryzzly_project_id == "p1").unwrap();
        assert!(p1.hours < out.total_hours, "p1 should not absorb the whole day");
        assert!(out.unattributed_hours > 0.0);
        assert!((out.total_hours - cfg.daily_target_hours).abs() < 1e-9);
    }

    #[test]
    fn out_of_office_day_bills_worked_time_not_full_target() {
        // Morning OOO (4h off) + full afternoon of work on p1 (back-fill 13:00→17:00 = 4h).
        // billable_target = 7.5 - 4 = 3.5; worked = 4.0 → total = max(worked, billable) = 4.0.
        let out = reconstruct_day(
            &DayInputs {
                date: day(),
                meetings: vec![meeting(8, 12, None, MeetingKind::OutOfOffice)],
                signals: vec![sig(14, 0, Some("p1"))],
            },
            &ReconstructionConfig::default(),
        );
        assert!(out.total_hours < 7.5, "OOO day must not be scaled to full target");
        assert!((out.total_hours - 4.0).abs() < 1e-9, "should bill the 4h afternoon, got {}", out.total_hours);
        // Morning OOO time is never attributed to a project.
        assert!(out.allocations.iter().all(|a| a.hours <= 4.0 + 1e-9));
    }

    #[test]
    fn renormalize_respects_pinned_and_sums_to_target() {
        let lines = vec![
            EditedLine { gryzzly_project_id: Some("a".into()), hours: 3.0, is_pinned: true },
            EditedLine { gryzzly_project_id: Some("b".into()), hours: 1.0, is_pinned: false },
            EditedLine { gryzzly_project_id: None, hours: 1.0, is_pinned: false },
        ];
        let out = renormalize_lines(&lines, 7.5, 0.25);
        let a = out.iter().find(|l| l.gryzzly_project_id.as_deref() == Some("a")).unwrap();
        assert!((a.hours - 3.0).abs() < 1e-9);
        let total: f64 = out.iter().map(|l| l.hours).sum();
        assert!((total - 7.5).abs() < 1e-9);
    }

    #[test]
    fn overlapping_meetings_earlier_wins() {
        // Two overlapping WORK meetings in the morning:
        //   p_a: 09:00–11:00 (2h)
        //   p_b: 10:00–12:00 (2h raw, but 10:00–11:00 is contested)
        // Earlier-wins truncation: p_a keeps 09:00–11:00 (2h), cursor moves to 11:00.
        // p_b is clipped to max(10:00, 11:00)=11:00 → 11:00–12:00 (1h).
        // No signals → low_signal day (guarded branch). Projects keep raw meeting hours.
        let out = reconstruct_day(
            &DayInputs {
                date: day(),
                meetings: vec![
                    meeting(9, 11, Some("p_a"), MeetingKind::Work),
                    meeting(10, 12, Some("p_b"), MeetingKind::Work),
                ],
                signals: vec![],
            },
            &ReconstructionConfig::default(),
        );
        // Assert on raw blocks (meeting attributed hours) rather than allocations,
        // since this is a low-signal day and the guarded branch keeps raw hours.
        let pa_meeting_hours: f64 = out.blocks.iter()
            .filter(|blk| blk.gryzzly_project_id.as_deref() == Some("p_a") && blk.kind == BlockKind::Meeting)
            .map(|blk| blk.hours)
            .sum();
        let pb_meeting_hours: f64 = out.blocks.iter()
            .filter(|blk| blk.gryzzly_project_id.as_deref() == Some("p_b") && blk.kind == BlockKind::Meeting)
            .map(|blk| blk.hours)
            .sum();
        assert!((pa_meeting_hours - 2.0).abs() < 1e-9, "p_a should have 2h (09:00–11:00), got {}", pa_meeting_hours);
        assert!((pb_meeting_hours - 1.0).abs() < 1e-9, "p_b should have 1h (11:00–12:00), got {}", pb_meeting_hours);
    }
}
