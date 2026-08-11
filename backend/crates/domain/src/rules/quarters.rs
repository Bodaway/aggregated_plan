use crate::rules::presence::{covered_minutes, minutes_in, Lane, LaneKey};
use crate::rules::reconstruction::{apportion_to_target, Bucket, ReconstructionConfig};
use crate::types::common::Confidence;

/// A quarter-day: half of a configured window. Four per day with the defaults —
/// 08-10, 10-12, 13-15, 15-17 — each declaring its own length in hours.
///
/// The unit exists because it is the smallest slice a person can still remember
/// honestly at the end of the day. Finer grids move the guessing from the engine to the
/// user without making the answer truer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quarter {
    pub index: u8,
    pub start_min: i64,
    pub end_min: i64,
    pub hours: f64,
}

/// One lane's hours inside one quarter.
///
/// `presence_minutes` travels beside `hours` on purpose: a share is an apportionment of
/// contested time, and a reader who cannot see the weight behind the number has no way
/// to tell a well-evidenced hour from a coin toss.
#[derive(Debug, Clone, PartialEq)]
pub struct Share {
    pub lane: LaneKey,
    pub label: String,
    pub gryzzly_project_id: Option<String>,
    pub presence_minutes: i64,
    pub hours: f64,
    pub is_pinned: bool,
}

/// A share the user set by hand, held fixed while the rest of its quarter rebalances.
#[derive(Debug, Clone, PartialEq)]
pub struct Pin {
    pub lane: LaneKey,
    pub hours: f64,
}

/// Where a pin belongs, for a whole-day allocation.
#[derive(Debug, Clone, PartialEq)]
pub struct DayPin {
    pub quarter_index: u8,
    pub lane: LaneKey,
    pub hours: f64,
}

#[derive(Debug, Clone)]
pub struct QuarterAllocation {
    pub quarter: Quarter,
    pub shares: Vec<Share>,
    pub ooo_hours: f64,
    /// The quarter's length minus out-of-office time: what the shares sum to.
    pub declarable_hours: f64,
    pub confidence: Confidence,
}

#[derive(Debug, Clone)]
pub struct DayAllocation {
    pub quarters: Vec<QuarterAllocation>,
    pub total_hours: f64,
    pub day_confidence: Confidence,
}

/// Cut each configured window in half. Windows of odd length put the extra minute in
/// the second quarter, so the four always tile the day exactly.
pub fn quarters(cfg: &ReconstructionConfig) -> [Quarter; 4] {
    let halve = |start: u32, end: u32, first: u8| {
        let (s, e) = (start as i64 * 60, end as i64 * 60);
        let mid = s + (e - s) / 2;
        [
            Quarter { index: first, start_min: s, end_min: mid, hours: (mid - s) as f64 / 60.0 },
            Quarter {
                index: first + 1,
                start_min: mid,
                end_min: e,
                hours: (e - mid) as f64 / 60.0,
            },
        ]
    };
    let m = halve(cfg.morning.0, cfg.morning.1, 0);
    let a = halve(cfg.afternoon.0, cfg.afternoon.1, 2);
    [m[0], m[1], a[0], a[1]]
}

fn rank(c: Confidence) -> u8 {
    match c {
        Confidence::Low => 0,
        Confidence::Medium => 1,
        Confidence::High => 2,
    }
}

/// How much of the quarter's wall clock ANY lane accounts for.
///
/// Measured on the union, never the sum: three concurrent lanes do not make a quarter
/// three times better evidenced.
fn confidence_for(lanes: &[Lane], q: &Quarter) -> Confidence {
    let span = (q.end_min - q.start_min).max(1);
    let ratio = covered_minutes(lanes, q.start_min, q.end_min) as f64 / span as f64;
    if ratio >= 0.75 {
        Confidence::High
    } else if ratio >= 0.40 {
        Confidence::Medium
    } else {
        Confidence::Low
    }
}

fn unattributed_share(hours: f64) -> Share {
    Share {
        lane: LaneKey::Unattributed,
        label: "unattributed".to_string(),
        gryzzly_project_id: None,
        presence_minutes: 0,
        hours,
        is_pinned: false,
    }
}

/// Apportion one quarter's declarable hours across the lanes present in it, weighted by
/// presence minutes, rounded to the increment, summing exactly to the declarable hours.
///
/// Out-of-office minutes shrink the quarter first — an afternoon half spent away is not
/// billable time to be shared out. A quarter nothing accounts for becomes one
/// unattributed share rather than silently shortening the day, which is how the previous
/// engine let a whole afternoon disappear without saying so.
pub fn allocate_quarter(
    q: &Quarter,
    lanes: &[Lane],
    pins: &[Pin],
    ooo_minutes: i64,
    rounding: f64,
) -> QuarterAllocation {
    let unit = rounding.max(f64::EPSILON);
    let round = |h: f64| (h / unit).round() * unit;
    let ooo_hours = round((ooo_minutes as f64 / 60.0).min(q.hours));
    let declarable = round((q.hours - ooo_hours).max(0.0));

    if declarable <= 0.0 {
        return QuarterAllocation {
            quarter: *q,
            shares: vec![],
            ooo_hours,
            declarable_hours: 0.0,
            // An off quarter is known, not doubtful — the calendar said so.
            confidence: Confidence::High,
        };
    }

    let present: Vec<(&Lane, i64)> = lanes
        .iter()
        .map(|l| (l, minutes_in(l, q.start_min, q.end_min)))
        // A pinned lane stays in the quarter even if a re-reconstruct left it with no
        // presence: the user's decision outranks the evidence that suggested it.
        .filter(|(l, m)| *m > 0 || pins.iter().any(|p| p.lane == l.key))
        .collect();

    if present.is_empty() {
        return QuarterAllocation {
            quarter: *q,
            shares: vec![unattributed_share(declarable)],
            ooo_hours,
            declarable_hours: declarable,
            confidence: confidence_for(lanes, q),
        };
    }

    let buckets: Vec<Bucket> = present
        .iter()
        .map(|(l, m)| {
            let pin = pins.iter().find(|p| p.lane == l.key);
            Bucket {
                key: Some(l.key.as_key()),
                hours: pin.map(|p| p.hours).unwrap_or(*m as f64),
                pinned: pin.is_some(),
            }
        })
        .collect();

    let apportioned = apportion_to_target(&buckets, declarable, unit);

    let mut shares: Vec<Share> = Vec::with_capacity(apportioned.len());
    for b in &apportioned {
        let parsed = b.key.as_deref().and_then(LaneKey::parse);
        let (Some(key), Some((lane, minutes))) = (
            parsed.clone(),
            parsed.as_ref().and_then(|k| present.iter().find(|(l, _)| l.key == *k)),
        ) else {
            // `apportion_to_target` parks any residual it cannot place on the
            // unattributed bucket (key = None). Carry it rather than lose the hours.
            if b.hours > 0.0 {
                shares.push(unattributed_share(b.hours));
            }
            continue;
        };
        shares.push(Share {
            lane: key,
            label: lane.label.clone(),
            gryzzly_project_id: lane.gryzzly_project_id.clone(),
            presence_minutes: *minutes,
            hours: b.hours,
            is_pinned: b.pinned,
        });
    }
    // A lane rounded down to nothing did happen, but declaring 0.00 h clutters the day.
    // A pinned zero is a deliberate "not this one" and stays visible.
    shares.retain(|s| s.hours > 0.0 || s.is_pinned);
    shares.sort_by(|a, b| {
        b.hours
            .partial_cmp(&a.hours)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.presence_minutes.cmp(&a.presence_minutes))
            .then(a.label.cmp(&b.label))
    });

    QuarterAllocation {
        quarter: *q,
        shares,
        ooo_hours,
        declarable_hours: declarable,
        confidence: confidence_for(lanes, q),
    }
}

/// Allocate the whole day. `ooo` holds out-of-office ranges in local minutes.
///
/// The day total is the sum of the quarters, NOT `daily_target_hours`: a quarter that
/// sums to its own length by construction cannot also sum to a scaled fraction of it.
/// The target becomes a check the caller reports on, not a factor applied here.
pub fn allocate_day(
    lanes: &[Lane],
    pins: &[DayPin],
    ooo: &[(i64, i64)],
    cfg: &ReconstructionConfig,
) -> DayAllocation {
    let qs = quarters(cfg);
    let mut out = Vec::with_capacity(qs.len());
    for q in qs.iter() {
        let ooo_minutes: i64 = ooo
            .iter()
            .map(|(s, e)| ((*e).min(q.end_min) - (*s).max(q.start_min)).max(0))
            .sum();
        let local: Vec<Pin> = pins
            .iter()
            .filter(|p| p.quarter_index == q.index)
            .map(|p| Pin { lane: p.lane.clone(), hours: p.hours })
            .collect();
        out.push(allocate_quarter(q, lanes, &local, ooo_minutes, cfg.rounding_hours));
    }
    let total_hours = out.iter().flat_map(|q| &q.shares).map(|s| s.hours).sum();
    let day_confidence = out
        .iter()
        .map(|q| q.confidence)
        .min_by_key(|c| rank(*c))
        .unwrap_or(Confidence::Low);
    DayAllocation { quarters: out, total_hours, day_confidence }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::presence::{build_lanes, EvidenceKind, EvidencePoint};
    use chrono::NaiveDate;
    use uuid::Uuid;

    fn cfg() -> ReconstructionConfig {
        ReconstructionConfig::default()
    }

    fn lane(n: u128, intervals: Vec<(i64, i64)>) -> Lane {
        Lane {
            key: LaneKey::Task(Uuid::from_u128(n)),
            label: format!("task {n}"),
            gryzzly_project_id: Some(format!("p{n}")),
            intervals,
            outside_minutes: 0,
            kinds: vec![EvidenceKind::Log],
        }
    }

    #[test]
    fn the_day_is_cut_into_four_two_hour_quarters() {
        let q = quarters(&cfg());
        assert_eq!(q[0].start_min, 480);
        assert_eq!(q[0].end_min, 600);
        assert_eq!(q[1].end_min, 720);
        assert_eq!(q[2].start_min, 780);
        assert_eq!(q[3].end_min, 1020);
        assert!(q.iter().all(|x| (x.hours - 2.0).abs() < 1e-9));
    }

    #[test]
    fn a_quarter_is_shared_in_proportion_to_presence_and_sums_to_its_length() {
        // 2026-08-10 Q4: 98 / 76 / 71 minutes of presence inside 15:00-17:00.
        let lanes = vec![
            lane(1, vec![(917, 1015)]),
            lane(2, vec![(938, 1014)]),
            lane(3, vec![(938, 1009)]),
        ];
        let a = allocate_quarter(&quarters(&cfg())[3], &lanes, &[], 0, 0.25);
        let hours: Vec<f64> = a.shares.iter().map(|s| s.hours).collect();
        assert!(
            (hours.iter().sum::<f64>() - 2.0).abs() < 1e-9,
            "a quarter always sums to its own length"
        );
        assert!(
            hours.iter().all(|h| ((h / 0.25) - (h / 0.25).round()).abs() < 1e-9),
            "every share lands on the rounding increment"
        );
        let s1 = a.shares.iter().find(|s| s.lane == LaneKey::Task(Uuid::from_u128(1))).unwrap();
        assert!(s1.hours >= 0.75, "the most-present lane leads");
        assert!(a.shares.iter().all(|s| s.hours > 0.0), "a lane that ran gets hours");
    }

    #[test]
    fn a_quarter_with_no_presence_is_unattributed_whole() {
        let a = allocate_quarter(&quarters(&cfg())[0], &[], &[], 0, 0.25);
        assert_eq!(a.shares.len(), 1);
        assert_eq!(a.shares[0].lane, LaneKey::Unattributed);
        assert!((a.shares[0].hours - 2.0).abs() < 1e-9);
        assert!(matches!(a.confidence, Confidence::Low));
    }

    #[test]
    fn a_pinned_share_is_held_and_the_rest_rebalance() {
        let lanes = vec![lane(1, vec![(900, 1000)]), lane(2, vec![(900, 940)])];
        let pins = vec![Pin { lane: LaneKey::Task(Uuid::from_u128(2)), hours: 1.5 }];
        let a = allocate_quarter(&quarters(&cfg())[3], &lanes, &pins, 0, 0.25);
        let s2 = a.shares.iter().find(|s| s.lane == LaneKey::Task(Uuid::from_u128(2))).unwrap();
        assert!((s2.hours - 1.5).abs() < 1e-9);
        assert!(s2.is_pinned);
        assert!((a.shares.iter().map(|s| s.hours).sum::<f64>() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn out_of_office_shrinks_the_quarter() {
        let lanes = vec![lane(1, vec![(480, 540)])];
        let a = allocate_quarter(&quarters(&cfg())[0], &lanes, &[], 60, 0.25);
        assert!((a.declarable_hours - 1.0).abs() < 1e-9);
        assert!((a.shares.iter().map(|s| s.hours).sum::<f64>() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_fully_out_of_office_quarter_declares_nothing() {
        let a = allocate_quarter(&quarters(&cfg())[0], &[], &[], 120, 0.25);
        assert!(a.shares.is_empty());
        assert!((a.declarable_hours - 0.0).abs() < 1e-9);
        assert!(
            matches!(a.confidence, Confidence::High),
            "an off quarter is known, not doubtful"
        );
    }

    #[test]
    fn confidence_reads_the_union_of_lanes_not_their_sum() {
        // Two lanes covering the SAME 60 of 120 minutes: 50% covered → Medium, not High.
        let lanes = vec![lane(1, vec![(900, 960)]), lane(2, vec![(900, 960)])];
        let a = allocate_quarter(&quarters(&cfg())[3], &lanes, &[], 0, 0.25);
        assert!(matches!(a.confidence, Confidence::Medium));
    }

    #[test]
    fn the_day_totals_the_quarters_and_takes_their_worst_confidence() {
        let lanes = vec![lane(1, vec![(480, 600), (600, 720), (780, 900)])];
        let day = allocate_day(&lanes, &[], &[], &cfg());
        assert_eq!(day.quarters.len(), 4);
        assert!((day.total_hours - 8.0).abs() < 1e-9);
        assert!(
            matches!(day.day_confidence, Confidence::Low),
            "the unevidenced last quarter drags the day down"
        );
    }

    /// The regression this whole design exists for. Three sessions ran concurrently on
    /// 2026-08-10; the carry-forward engine gave the third one 0.29 h for the entire day
    /// because its entries fell between the other two tasks' entries.
    #[test]
    fn concurrent_sessions_each_declare_real_hours() {
        let d = NaiveDate::from_ymd_opt(2026, 8, 10).unwrap();
        let p = |h: u32, m: u32, n: u128| EvidencePoint {
            at: d.and_hms_opt(h, m, 0).unwrap(),
            lane: LaneKey::Task(Uuid::from_u128(n)),
            label: format!("task {n}"),
            gryzzly_project_id: Some(format!("p{n}")),
            kind: EvidenceKind::Log,
        };
        let points = vec![
            p(14, 9, 1),
            p(16, 2, 1),
            p(16, 37, 1),
            p(16, 55, 1),
            p(16, 23, 2),
            p(16, 49, 2),
            p(16, 23, 3),
            p(16, 30, 3),
            p(16, 36, 3),
            p(16, 41, 3),
            p(16, 47, 3),
            p(16, 53, 3),
        ];
        let lanes = build_lanes(&points, &[], &[(480, 720), (780, 1020)]);
        let day = allocate_day(&lanes, &[], &[], &cfg());
        let per_task = |n: u128| -> f64 {
            day.quarters
                .iter()
                .flat_map(|q| &q.shares)
                .filter(|s| s.lane == LaneKey::Task(Uuid::from_u128(n)))
                .map(|s| s.hours)
                .sum()
        };
        assert!(per_task(2) >= 0.5, "a session that ran all afternoon declares real hours");
        assert!(per_task(3) >= 0.5, "not 0.29h for a day of work");
        assert!((day.total_hours - 8.0).abs() < 1e-9);
    }
}
