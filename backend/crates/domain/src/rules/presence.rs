use std::collections::BTreeMap;

use chrono::{NaiveDateTime, Timelike};
use uuid::Uuid;

use crate::rules::worklog_time::MAX_CONTINUATION_GAP_MINUTES;

/// What produced a piece of evidence. Carried on the lane so a reader can tell a share
/// resting on measured time from one resting on an inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceKind {
    /// A worklog entry: a timestamp, written after the work it describes.
    Log,
    /// A git commit: likewise a timestamp, not a duration.
    Commit,
    /// A calendar meeting: a measured span.
    Meeting,
    /// A hand-run activity slot: a measured span the worklog projection cannot derive.
    ManualSlot,
}

/// What a lane is about.
///
/// `Unattributed` is not a task that failed to resolve — it is the residue of a quarter
/// no evidence accounts for, which must still declare its hours rather than quietly
/// shrink the day.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LaneKey {
    Task(Uuid),
    /// Evidence that belongs to no task: a meeting, or a commit that matched no Jira
    /// key. Keyed by an opaque source ref so two of them never collide.
    Source(String),
    Unattributed,
}

impl LaneKey {
    /// The persisted and wire form. Stable by contract: it is a database key
    /// (`timesheet_quarter_shares.lane_key`), a GraphQL argument and a CLI token.
    pub fn as_key(&self) -> String {
        match self {
            LaneKey::Task(id) => format!("task:{id}"),
            LaneKey::Source(r) => format!("src:{r}"),
            LaneKey::Unattributed => "unattributed".to_string(),
        }
    }

    pub fn parse(s: &str) -> Option<LaneKey> {
        if s == "unattributed" {
            return Some(LaneKey::Unattributed);
        }
        if let Some(rest) = s.strip_prefix("task:") {
            return Uuid::parse_str(rest).ok().map(LaneKey::Task);
        }
        s.strip_prefix("src:").map(|rest| LaneKey::Source(rest.to_string()))
    }

    pub fn task_id(&self) -> Option<Uuid> {
        match self {
            LaneKey::Task(id) => Some(*id),
            _ => None,
        }
    }
}

/// A timestamped trace of work: a worklog entry or a commit.
///
/// It is written AFTER the work, so it is evidence for the stretch BEFORE it — see
/// [`build_lanes`].
#[derive(Debug, Clone)]
pub struct EvidencePoint {
    pub at: NaiveDateTime,
    pub lane: LaneKey,
    pub label: String,
    pub gryzzly_project_id: Option<String>,
    pub kind: EvidenceKind,
}

/// Measured time: a calendar meeting, or a hand-run activity slot. Taken as it stands —
/// there is nothing to infer.
#[derive(Debug, Clone)]
pub struct EvidenceSpan {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub lane: LaneKey,
    pub label: String,
    pub gryzzly_project_id: Option<String>,
    pub kind: EvidenceKind,
}

/// One task's presence across the day, in local minutes from midnight.
///
/// Lanes OVERLAP, and that is the point: two sessions running at once produce two lanes
/// covering the same minutes. Presence is a **weight** for apportionment, never a claim
/// on the clock — no rule can recover how attention split between concurrent sessions,
/// so the honest move is to weight them and let the user arbitrate.
#[derive(Debug, Clone)]
pub struct Lane {
    pub key: LaneKey,
    pub label: String,
    pub gryzzly_project_id: Option<String>,
    /// Merged, window-clipped, sorted.
    pub intervals: Vec<(i64, i64)>,
    /// Minutes clipped away for falling outside the working windows. Reported rather
    /// than discarded: the previous engine dropped a day's evening work with no trace
    /// anywhere in the UI or the CLI, and nobody could see that it had.
    pub outside_minutes: i64,
    pub kinds: Vec<EvidenceKind>,
}

fn mins(dt: NaiveDateTime) -> i64 {
    dt.time().hour() as i64 * 60 + dt.time().minute() as i64
}

/// Turn evidence into per-lane presence.
///
/// A point at `T` covers `[max(T - MAX_CONTINUATION_GAP_MINUTES, previous point of the
/// SAME lane), T]`. Two clips, two distinct jobs: the lane's own previous point stops
/// consecutive entries counting the same minute twice, and the 45-minute cap stops a
/// lone entry claiming a whole morning.
///
/// Clipping against OTHER lanes is precisely the carry-forward rule this replaces — the
/// one that credited a three-hour stretch to whichever task logged first after a silence
/// — so it must never be reintroduced.
///
/// Spans are taken as measured. Everything is merged per lane, then clipped to
/// `windows`; what falls outside is accumulated into `outside_minutes`, not lost.
pub fn build_lanes(
    points: &[EvidencePoint],
    spans: &[EvidenceSpan],
    windows: &[(i64, i64)],
) -> Vec<Lane> {
    struct Acc {
        label: String,
        project: Option<String>,
        raw: Vec<(i64, i64)>,
        kinds: Vec<EvidenceKind>,
    }
    let mut acc: BTreeMap<LaneKey, Acc> = BTreeMap::new();

    let mut sorted: Vec<&EvidencePoint> = points.iter().collect();
    sorted.sort_by_key(|p| (p.lane.clone(), mins(p.at)));

    let mut prev: BTreeMap<LaneKey, i64> = BTreeMap::new();
    for p in sorted {
        let end = mins(p.at);
        let floor = prev.get(&p.lane).copied().unwrap_or(i64::MIN);
        let start = (end - MAX_CONTINUATION_GAP_MINUTES).max(floor);
        prev.insert(p.lane.clone(), end);
        let e = acc.entry(p.lane.clone()).or_insert_with(|| Acc {
            label: p.label.clone(),
            project: p.gryzzly_project_id.clone(),
            raw: vec![],
            kinds: vec![],
        });
        if !e.kinds.contains(&p.kind) {
            e.kinds.push(p.kind);
        }
        if end > start {
            e.raw.push((start, end));
        }
    }

    for s in spans {
        let e = acc.entry(s.lane.clone()).or_insert_with(|| Acc {
            label: s.label.clone(),
            project: s.gryzzly_project_id.clone(),
            raw: vec![],
            kinds: vec![],
        });
        if !e.kinds.contains(&s.kind) {
            e.kinds.push(s.kind);
        }
        let (a, b) = (mins(s.start), mins(s.end));
        if b > a {
            e.raw.push((a, b));
        }
    }

    acc.into_iter()
        .map(|(key, a)| {
            let merged = merge(a.raw);
            let total: i64 = merged.iter().map(|(s, e)| e - s).sum();
            let intervals = clip(&merged, windows);
            let inside: i64 = intervals.iter().map(|(s, e)| e - s).sum();
            Lane {
                key,
                label: a.label,
                gryzzly_project_id: a.project,
                intervals,
                outside_minutes: total - inside,
                kinds: a.kinds,
            }
        })
        .collect()
}

fn merge(mut v: Vec<(i64, i64)>) -> Vec<(i64, i64)> {
    v.sort();
    let mut out: Vec<(i64, i64)> = Vec::with_capacity(v.len());
    for (s, e) in v {
        match out.last_mut() {
            Some(last) if s <= last.1 => last.1 = last.1.max(e),
            _ => out.push((s, e)),
        }
    }
    out
}

fn clip(v: &[(i64, i64)], windows: &[(i64, i64)]) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    for (s, e) in v {
        for (ws, we) in windows {
            let a = (*s).max(*ws);
            let b = (*e).min(*we);
            if b > a {
                out.push((a, b));
            }
        }
    }
    out.sort();
    out
}

/// How many minutes of this lane fall inside `[start_min, end_min)`.
pub fn minutes_in(lane: &Lane, start_min: i64, end_min: i64) -> i64 {
    lane.intervals
        .iter()
        .map(|(s, e)| ((*e).min(end_min) - (*s).max(start_min)).max(0))
        .sum()
}

/// Distinct wall-clock minutes covered by ANY lane in the range — a union, never a sum.
///
/// This is what quarter confidence is measured on: three concurrent lanes do not make a
/// quarter three times better evidenced, and summing them would report 245 minutes of
/// coverage for a 120-minute quarter.
pub fn covered_minutes(lanes: &[Lane], start_min: i64, end_min: i64) -> i64 {
    let mut all: Vec<(i64, i64)> = Vec::new();
    for l in lanes {
        for (s, e) in &l.intervals {
            let a = (*s).max(start_min);
            let b = (*e).min(end_min);
            if b > a {
                all.push((a, b));
            }
        }
    }
    merge(all).iter().map(|(s, e)| e - s).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    /// The default working windows, in local minutes from midnight: 08:00-12:00 and
    /// 13:00-17:00.
    const DAY: [(i64, i64); 2] = [(480, 720), (780, 1020)];

    fn at(h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 8, 10).unwrap().and_hms_opt(h, m, 0).unwrap()
    }

    fn task(n: u128) -> LaneKey {
        LaneKey::Task(Uuid::from_u128(n))
    }

    fn point(h: u32, m: u32, lane: LaneKey) -> EvidencePoint {
        EvidencePoint {
            at: at(h, m),
            lane,
            label: "t".into(),
            gryzzly_project_id: Some("p1".into()),
            kind: EvidenceKind::Log,
        }
    }

    #[test]
    fn a_lone_point_casts_a_full_back_shadow() {
        let lanes = build_lanes(&[point(10, 0, task(1))], &[], &DAY);
        assert_eq!(lanes[0].intervals, vec![(600 - MAX_CONTINUATION_GAP_MINUTES, 600)]);
    }

    #[test]
    fn a_shadow_is_clipped_at_the_lanes_own_previous_point() {
        // 10:00 then 10:20 — the second reaches back 20 minutes, not 45.
        let lanes = build_lanes(&[point(10, 0, task(1)), point(10, 20, task(1))], &[], &DAY);
        assert_eq!(lanes[0].intervals, vec![(555, 620)], "09:15-10:00 merged with 10:00-10:20");
        assert_eq!(minutes_in(&lanes[0], 480, 720), 65, "never 45 + 45");
    }

    #[test]
    fn another_lanes_point_never_clips_this_lanes_shadow() {
        // The carry-forward bug in one test: task 2 logging at 10:10 must not shorten
        // task 1's shadow, and task 1 must not shorten task 2's.
        let lanes = build_lanes(&[point(10, 0, task(1)), point(10, 10, task(2))], &[], &DAY);
        let l1 = lanes.iter().find(|l| l.key == task(1)).unwrap();
        let l2 = lanes.iter().find(|l| l.key == task(2)).unwrap();
        assert_eq!(l1.intervals, vec![(555, 600)]);
        assert_eq!(l2.intervals, vec![(565, 610)]);
    }

    #[test]
    fn overlapping_lanes_are_kept_whole_and_covered_minutes_is_a_union() {
        let lanes = build_lanes(&[point(10, 0, task(1)), point(10, 10, task(2))], &[], &DAY);
        let summed: i64 = lanes.iter().map(|l| minutes_in(l, 480, 720)).sum();
        assert_eq!(summed, 90, "presence is a weight: overlap counts in both lanes");
        assert_eq!(covered_minutes(&lanes, 480, 720), 55, "wall clock 09:15-10:10 counts once");
    }

    #[test]
    fn a_span_is_taken_as_measured_not_shadowed() {
        let spans = vec![EvidenceSpan {
            start: at(9, 0),
            end: at(10, 0),
            lane: LaneKey::Source("mtg:1".into()),
            label: "Weekly".into(),
            gryzzly_project_id: None,
            kind: EvidenceKind::Meeting,
        }];
        let lanes = build_lanes(&[], &spans, &DAY);
        assert_eq!(lanes[0].intervals, vec![(540, 600)]);
    }

    #[test]
    fn evidence_outside_the_windows_is_reported_not_dropped() {
        // 18:30 is past the 17:00 edge; its whole shadow lands outside the working day.
        let lanes = build_lanes(&[point(18, 30, task(1))], &[], &DAY);
        assert!(lanes[0].intervals.is_empty());
        assert_eq!(lanes[0].outside_minutes, MAX_CONTINUATION_GAP_MINUTES);
    }

    #[test]
    fn a_shadow_straddling_a_window_edge_keeps_only_the_inside() {
        // 13:10 reaches back to 12:25 — 12:25-13:00 is lunch, 13:00-13:10 is inside.
        let lanes = build_lanes(&[point(13, 10, task(1))], &[], &DAY);
        assert_eq!(lanes[0].intervals, vec![(780, 790)]);
        assert_eq!(lanes[0].outside_minutes, 35);
    }

    #[test]
    fn duplicate_timestamps_do_not_double_count() {
        let lanes = build_lanes(
            &[point(10, 0, task(1)), point(10, 0, task(1)), point(10, 0, task(1))],
            &[],
            &DAY,
        );
        assert_eq!(minutes_in(&lanes[0], 480, 720), MAX_CONTINUATION_GAP_MINUTES);
    }

    #[test]
    fn lane_key_round_trips_through_its_string_form() {
        for k in [task(7), LaneKey::Source("mtg:abc".into()), LaneKey::Unattributed] {
            assert_eq!(LaneKey::parse(&k.as_key()), Some(k));
        }
    }
}
