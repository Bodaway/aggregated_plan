use chrono::{Duration, NaiveDate, NaiveDateTime, Timelike};

use crate::types::common::HalfDay;

/// What a block spanning a single timestamp occupies once persisted.
///
/// A lone entry in a half-day has `start == end`, and a zero-length slot would be
/// invisible to every hours computation. One minute is the smallest duration that
/// keeps it countable.
///
/// Here rather than at the persistence site because two callers now materialise
/// slots — the flush and the reattribution repair — and a projection defined twice
/// is a projection that will disagree with itself.
pub const MIN_BLOCK_MINUTES: i64 = 1;

/// How long a pause between two consecutive worklog entries may last while the work
/// still counts as continuing.
///
/// The user's rule: **a gap of more than this between two entries is time that was not
/// spent on the task.** Below or at the threshold the entries document one uninterrupted
/// stretch of work; above it, whatever happened in between belongs to something else —
/// a meeting, a break, another task — and charging it to this task would invent hours
/// nobody worked.
///
/// **Why forty-five minutes and not fifteen.** A worklog entry is an *event marker*, not
/// an activity sample: the logging instruction is one entry per finding, decision or
/// action, so two entries can legitimately sit forty minutes apart during a code read or
/// a build wait, with the work never interrupted. A fifteen-minute threshold assumes a
/// dense cadence the journal does not have, and under-counts badly — measured at −73% on
/// a real day. Forty-five excludes a genuine break while tolerating a sparse cadence.
/// The gap distribution of 2026-08-03 says the same (45 gaps): 43 of 15 minutes or less,
/// **none** between 16 and 30 minutes, two between 31 and 45, and one of 2h53 — the
/// inflection sits between 30 and 45, not at 15.
///
/// A constant rather than a configuration key: it is a business rule, and a threshold
/// that varied per user would make two people's hours incomparable.
pub const MAX_CONTINUATION_GAP_MINUTES: i64 = 45;

/// A derived block of worked time, expressed in the user's LOCAL wall-clock.
/// `start`/`end` are local naive datetimes; the caller maps them back to UTC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalBlock {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub date: NaiveDate,
    pub half_day: HalfDay,
}

/// Group LOCAL worklog timestamps into the stretches of work they document.
///
/// Two boundaries cut a block, and a half-day therefore holds **as many blocks as it
/// had stretches of work** — one per half-day was the old projection, and it charged
/// every idle hour between two entries as worked time:
///
/// - the (calendar day, half-day) it belongs to. Morning = hour < 13, Afternoon =
///   hour >= 13 (matches `workload::half_day_of`). A block never straddles the
///   boundary, because a persisted slot carries exactly one `half_day` and the
///   reattribution repair scopes itself by it.
/// - a gap of more than [`MAX_CONTINUATION_GAP_MINUTES`] to the next timestamp: past
///   that, the time was not spent on the task, so the block ends and the next one
///   starts at the entry that resumed the work.
///
/// A block runs from the first to the last timestamp of its stretch; for a stretch of
/// a single timestamp, `start == end` (the caller gives it a minimal non-zero duration
/// when persisting). Input order does not matter; output is sorted by (date, half_day
/// morning-before-afternoon, start).
pub fn derive_time_blocks(local_times: &[NaiveDateTime]) -> Vec<LocalBlock> {
    use crate::rules::workload::half_day_of;
    use std::collections::BTreeMap;

    // `bool` rather than `HalfDay` as the key so the map's own ordering puts the
    // morning before the afternoon of the same day.
    let mut groups: BTreeMap<(NaiveDate, bool), Vec<NaiveDateTime>> = BTreeMap::new();
    for &t in local_times {
        let is_pm = matches!(half_day_of(t.time().hour()), HalfDay::Afternoon);
        groups.entry((t.date(), is_pm)).or_default().push(t);
    }

    groups
        .into_iter()
        .flat_map(|((date, is_pm), times)| {
            let half_day = if is_pm { HalfDay::Afternoon } else { HalfDay::Morning };
            worked_stretches(times)
                .into_iter()
                .map(move |(start, end)| LocalBlock { start, end, date, half_day })
        })
        .collect()
}

/// The stretches a set of timestamps documents, ascending: consecutive timestamps stay
/// together while they are at most [`MAX_CONTINUATION_GAP_MINUTES`] apart, and a longer
/// pause closes the stretch.
fn worked_stretches(mut times: Vec<NaiveDateTime>) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    times.sort_unstable();
    let max_gap = Duration::minutes(MAX_CONTINUATION_GAP_MINUTES);

    times.into_iter().fold(Vec::new(), |mut stretches, t| {
        match stretches.last_mut() {
            Some((_, end)) if t - *end <= max_gap => *end = t,
            _ => stretches.push((t, t)),
        }
        stretches
    })
}

/// The duration a block occupies once persisted: its own span, floored at
/// [`MIN_BLOCK_MINUTES`].
pub fn block_duration(block: &LocalBlock) -> Duration {
    let span = block.end - block.start;
    let floor = Duration::minutes(MIN_BLOCK_MINUTES);
    if span < floor {
        floor
    } else {
        span
    }
}

/// Hours a set of blocks accounts for, each counted with [`block_duration`].
///
/// Minutes, not seconds, so this matches to the minute what the activity report
/// computes from the slots these blocks become.
///
/// `+ 0.0` is not noise: the additive identity of a float `Sum` is `-0.0`, so an
/// empty set of blocks would serialize as `-0.0` in the JSON a caller reads. Adding
/// a positive zero normalises the sign without touching any other value.
pub fn total_block_hours(blocks: &[LocalBlock]) -> f64 {
    blocks
        .iter()
        .map(|block| block_duration(block).num_minutes() as f64 / 60.0)
        .sum::<f64>()
        + 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn dt(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, min, 0)
            .unwrap()
    }

    /// Same, to the second: the gap rule cuts on a minute *and one second*, so the
    /// cases that pin it cannot be written in whole minutes.
    fn dts(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, min, s)
            .unwrap()
    }

    #[test]
    fn empty_input_yields_no_blocks() {
        assert!(derive_time_blocks(&[]).is_empty());
    }

    /// Timestamps that follow one another closely are one stretch of work, whatever
    /// their number.
    #[test]
    fn a_continuous_morning_is_a_single_block() {
        let times = vec![
            dt(2026, 6, 8, 10, 0),
            dt(2026, 6, 8, 10, 12),
            dt(2026, 6, 8, 10, 20),
        ];
        let blocks = derive_time_blocks(&times);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start, dt(2026, 6, 8, 10, 0));
        assert_eq!(blocks[0].end, dt(2026, 6, 8, 10, 20));
        assert_eq!(blocks[0].date, NaiveDate::from_ymd_opt(2026, 6, 8).unwrap());
        assert_eq!(blocks[0].half_day, HalfDay::Morning);
    }

    /// The half-day boundary is a cut of its own: a persisted slot carries exactly one
    /// `half_day`, so no block may straddle 13:00 even when the entries are close
    /// enough to be one stretch.
    #[test]
    fn a_stretch_never_straddles_the_half_day_boundary() {
        let times = vec![dt(2026, 6, 8, 12, 55), dt(2026, 6, 8, 13, 5)];
        let blocks = derive_time_blocks(&times);
        assert_eq!(blocks.len(), 2, "10 minutes apart, but across 13:00");
        assert_eq!(blocks[0].half_day, HalfDay::Morning);
        assert_eq!(blocks[0].start, dt(2026, 6, 8, 12, 55));
        assert_eq!(blocks[1].half_day, HalfDay::Afternoon);
        assert_eq!(blocks[1].start, dt(2026, 6, 8, 13, 5));
    }

    #[test]
    fn crossing_noon_splits_into_two_blocks() {
        let times = vec![dt(2026, 6, 8, 11, 0), dt(2026, 6, 8, 14, 0)];
        let blocks = derive_time_blocks(&times);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].half_day, HalfDay::Morning);
        assert_eq!(blocks[0].start, dt(2026, 6, 8, 11, 0));
        assert_eq!(blocks[0].end, dt(2026, 6, 8, 11, 0));
        assert_eq!(blocks[1].half_day, HalfDay::Afternoon);
        assert_eq!(blocks[1].start, dt(2026, 6, 8, 14, 0));
    }

    #[test]
    fn multi_day_only_days_with_entries() {
        let times = vec![
            dt(2026, 6, 8, 14, 2),
            dt(2026, 6, 8, 14, 15),
            dt(2026, 6, 10, 9, 10),
            dt(2026, 6, 10, 9, 20),
        ];
        let blocks = derive_time_blocks(&times);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].date, NaiveDate::from_ymd_opt(2026, 6, 8).unwrap());
        assert_eq!(blocks[0].half_day, HalfDay::Afternoon);
        assert_eq!(blocks[1].date, NaiveDate::from_ymd_opt(2026, 6, 10).unwrap());
        assert_eq!(blocks[1].half_day, HalfDay::Morning);
    }

    #[test]
    fn single_entry_block_has_equal_start_end() {
        let times = vec![dt(2026, 6, 8, 9, 0)];
        let blocks = derive_time_blocks(&times);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start, blocks[0].end);
    }

    #[test]
    fn unsorted_input_is_handled() {
        let times = vec![dt(2026, 6, 8, 10, 12), dt(2026, 6, 8, 10, 0)];
        let blocks = derive_time_blocks(&times);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start, dt(2026, 6, 8, 10, 0));
        assert_eq!(blocks[0].end, dt(2026, 6, 8, 10, 12));
    }

    /// A long stretch is still one block as long as no pause exceeds the threshold —
    /// here entries exactly [`MAX_CONTINUATION_GAP_MINUTES`] apart from 09:00 to 11:30.
    #[test]
    fn a_block_spanning_real_time_keeps_its_own_duration() {
        let blocks = derive_time_blocks(&every_quarter_hour(dt(2026, 6, 8, 9, 0), 10));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].end, dt(2026, 6, 8, 11, 30));
        assert_eq!(block_duration(&blocks[0]), Duration::minutes(150));
        assert_eq!(total_block_hours(&blocks), 2.5);
    }

    /// `steps + 1` timestamps a quarter of an hour apart — a cadence well inside
    /// [`MAX_CONTINUATION_GAP_MINUTES`], so the whole series is one stretch of work.
    /// Deliberately not expressed with the threshold: these fixtures must keep their
    /// span, and a step tied to the threshold would push them past noon if it grew.
    fn every_quarter_hour(start: NaiveDateTime, steps: i64) -> Vec<NaiveDateTime> {
        (0..=steps)
            .map(|i| start + Duration::minutes(15 * i))
            .collect()
    }

    /// A lone entry would otherwise account for zero hours, and the half-day it
    /// documents would vanish from every total.
    #[test]
    fn a_single_timestamp_block_still_counts_for_the_minimum() {
        let blocks = derive_time_blocks(&[dt(2026, 6, 8, 9, 0)]);
        assert_eq!(
            block_duration(&blocks[0]),
            Duration::minutes(MIN_BLOCK_MINUTES)
        );
        assert!(total_block_hours(&blocks) > 0.0);
    }

    #[test]
    fn hours_add_up_across_half_days() {
        let mut times = every_quarter_hour(dt(2026, 6, 8, 9, 0), 8); // 09:00 → 11:00
        times.extend(every_quarter_hour(dt(2026, 6, 8, 14, 0), 4)); // 14:00 → 15:00
        let blocks = derive_time_blocks(&times);
        assert_eq!(blocks.len(), 2);
        assert_eq!(total_block_hours(&blocks), 3.0);
    }

    // ─── The gap rule ────────────────────────────────────────────────────────

    /// The rule in the user's terms: past [`MAX_CONTINUATION_GAP_MINUTES`], the time
    /// between two entries was not spent on the task, so the half-day carries several
    /// blocks and the idle stretch is charged to nobody.
    #[test]
    fn a_gap_beyond_the_threshold_splits_a_half_day_in_two() {
        let times = vec![
            dts(2026, 8, 3, 14, 0, 0),
            dts(2026, 8, 3, 14, 10, 0),  // +10'00 → same block
            dts(2026, 8, 3, 14, 56, 1),  // +46'01 → a new block
            dts(2026, 8, 3, 14, 59, 30), // +3'29  → same block
        ];
        let blocks = derive_time_blocks(&times);
        assert_eq!(blocks.len(), 2, "the 46-minute gap is not worked time");
        assert_eq!(blocks[0].start, dts(2026, 8, 3, 14, 0, 0));
        assert_eq!(blocks[0].end, dts(2026, 8, 3, 14, 10, 0));
        assert_eq!(blocks[1].start, dts(2026, 8, 3, 14, 56, 1));
        assert_eq!(blocks[1].end, dts(2026, 8, 3, 14, 59, 30));
        assert!(blocks.iter().all(|b| b.half_day == HalfDay::Afternoon));
    }

    /// The boundary, from below: a gap *equal* to the threshold is still the same
    /// stretch of work — a code read or a build wait logged at either end of it.
    #[test]
    fn a_gap_of_exactly_the_threshold_coalesces() {
        let blocks = derive_time_blocks(&[
            dts(2026, 8, 3, 14, 0, 0),
            dts(2026, 8, 3, 14, 45, 0),
        ]);
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            block_duration(&blocks[0]),
            Duration::minutes(MAX_CONTINUATION_GAP_MINUTES)
        );
    }

    /// The boundary, from above: one second more and the work stopped in between.
    #[test]
    fn a_gap_one_second_beyond_the_threshold_splits() {
        let blocks = derive_time_blocks(&[
            dts(2026, 8, 3, 14, 0, 0),
            dts(2026, 8, 3, 14, 45, 1),
        ]);
        assert_eq!(blocks.len(), 2, "45 min + 1 s is not a continuation");
        assert_eq!(blocks[0].start, blocks[0].end);
        assert_eq!(blocks[1].start, blocks[1].end);
        assert_eq!(
            total_block_hours(&blocks),
            2.0 * MIN_BLOCK_MINUTES as f64 / 60.0,
            "two lone timestamps, each worth the minimum"
        );
    }

    /// Several blocks in one half-day are ordered by start, so the slots written from
    /// them read as the day happened.
    #[test]
    fn blocks_of_the_same_half_day_come_out_in_chronological_order() {
        let blocks = derive_time_blocks(&[
            dt(2026, 8, 3, 16, 0),
            dt(2026, 8, 3, 14, 0),
            dt(2026, 8, 3, 14, 10),
            dt(2026, 8, 3, 16, 10),
        ]);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].start, dt(2026, 8, 3, 14, 0));
        assert_eq!(blocks[0].end, dt(2026, 8, 3, 14, 10));
        assert_eq!(blocks[1].start, dt(2026, 8, 3, 16, 0));
        assert_eq!(blocks[1].end, dt(2026, 8, 3, 16, 10));
    }

    /// The hours of a split half-day are the worked stretches only — never the span
    /// from the first entry to the last.
    #[test]
    fn a_split_half_day_charges_the_stretches_and_not_the_idle_time() {
        let mut times = every_quarter_hour(dt(2026, 8, 3, 14, 0), 2); // 30 min of work
        times.extend(every_quarter_hour(dt(2026, 8, 3, 16, 0), 2)); // 30 min of work
        let blocks = derive_time_blocks(&times);
        assert_eq!(blocks.len(), 2);
        assert_eq!(
            total_block_hours(&blocks),
            1.0,
            "the 90 idle minutes between 14:30 and 16:00 belong to nobody"
        );
    }

    /// The afternoon that motivated the rule: every worklog timestamp recorded on
    /// 2026-08-03, in Europe/Paris wall-clock (the day's entries as stored, +02:00).
    ///
    /// One block per half-day charged the whole span — 14:56:15 → 21:34:45, **6h38** —
    /// as worked time, though the entries stop for nearly three hours after 18:41:01.
    /// The gap rule keeps the afternoon's one real stretch of work and drops that stop,
    /// charging **3h45**. The day's longest interruption *inside* the stretch is 43'02,
    /// which a sparse logging cadence explains and the threshold therefore tolerates.
    #[test]
    fn the_afternoon_of_2026_08_03_charges_only_the_stretches_worked() {
        let times = vec![
            dts(2026, 8, 3, 14, 56, 15),
            dts(2026, 8, 3, 15, 11, 10), // +14'55 → same block
            dts(2026, 8, 3, 15, 20, 33), // +9'23  → same block
            dts(2026, 8, 3, 15, 57, 41), // +37'08 → same block
            dts(2026, 8, 3, 16, 6, 28),  // +8'47  → same block
            dts(2026, 8, 3, 16, 7, 47),
            dts(2026, 8, 3, 16, 11, 44),
            dts(2026, 8, 3, 16, 14, 48),
            dts(2026, 8, 3, 16, 15, 27),
            dts(2026, 8, 3, 16, 16, 3),
            dts(2026, 8, 3, 16, 18, 52),
            dts(2026, 8, 3, 16, 31, 23),
            dts(2026, 8, 3, 16, 46, 33), // +15'10 → same block
            dts(2026, 8, 3, 16, 48, 41),
            dts(2026, 8, 3, 16, 58, 20),
            dts(2026, 8, 3, 16, 58, 21),
            dts(2026, 8, 3, 17, 4, 44),
            dts(2026, 8, 3, 17, 9, 21),
            dts(2026, 8, 3, 17, 16, 58),
            dts(2026, 8, 3, 17, 22, 3),
            dts(2026, 8, 3, 17, 37, 5), // +15'02 → same block
            dts(2026, 8, 3, 17, 44, 12),
            dts(2026, 8, 3, 17, 47, 9),
            dts(2026, 8, 3, 17, 57, 59), // +10'50 → same block
            dts(2026, 8, 3, 18, 41, 1),  // +43'02 → same block
            dts(2026, 8, 3, 21, 34, 45), // +2h53  → a new block
        ];
        let blocks = derive_time_blocks(&times);
        let spans: Vec<(NaiveDateTime, NaiveDateTime)> =
            blocks.iter().map(|b| (b.start, b.end)).collect();
        assert_eq!(
            spans,
            vec![
                (dts(2026, 8, 3, 14, 56, 15), dts(2026, 8, 3, 18, 41, 1)),
                (dts(2026, 8, 3, 21, 34, 45), dts(2026, 8, 3, 21, 34, 45)),
            ]
        );
        assert!(blocks.iter().all(|b| b.half_day == HalfDay::Afternoon));
        // 224 minutes worked plus a lone entry's minute, where one block charged 398.
        assert_eq!(total_block_hours(&blocks), 225.0 / 60.0);
    }

    /// `assert_eq!` cannot catch this on its own — `-0.0 == 0.0` — and a `-0.0` in the
    /// JSON payload is what a reader sees when a task ends the day with no time left.
    #[test]
    fn no_blocks_is_a_positive_zero_not_a_negative_one() {
        assert_eq!(total_block_hours(&[]), 0.0);
        assert!(
            total_block_hours(&[]).is_sign_positive(),
            "the float Sum identity is -0.0, which serializes as -0.0"
        );
    }
}
