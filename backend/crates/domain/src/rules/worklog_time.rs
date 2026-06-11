use chrono::{NaiveDate, NaiveDateTime, Timelike};

use crate::types::common::HalfDay;

/// A derived block of worked time, expressed in the user's LOCAL wall-clock.
/// `start`/`end` are local naive datetimes; the caller maps them back to UTC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalBlock {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub date: NaiveDate,
    pub half_day: HalfDay,
}

/// Group LOCAL worklog timestamps into one block per (calendar day, half-day).
/// Morning = hour < 13, Afternoon = hour >= 13 (matches `workload::half_day_of`).
/// A group's block runs from its earliest to its latest timestamp. For a group
/// with a single timestamp, `start == end` (the caller gives it a minimal
/// non-zero duration when persisting). Input order does not matter; output is
/// sorted by (date, half_day morning-before-afternoon, start).
pub fn derive_time_blocks(local_times: &[NaiveDateTime]) -> Vec<LocalBlock> {
    use crate::rules::workload::half_day_of;
    use std::collections::BTreeMap;

    let mut groups: BTreeMap<(NaiveDate, bool), (NaiveDateTime, NaiveDateTime)> = BTreeMap::new();

    for &t in local_times {
        let date = t.date();
        let half_day = half_day_of(t.time().hour());
        let is_pm = matches!(half_day, HalfDay::Afternoon);
        let entry = groups.entry((date, is_pm)).or_insert((t, t));
        if t < entry.0 {
            entry.0 = t;
        }
        if t > entry.1 {
            entry.1 = t;
        }
    }

    groups
        .into_iter()
        .map(|((date, is_pm), (start, end))| LocalBlock {
            start,
            end,
            date,
            half_day: if is_pm { HalfDay::Afternoon } else { HalfDay::Morning },
        })
        .collect()
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

    #[test]
    fn empty_input_yields_no_blocks() {
        assert!(derive_time_blocks(&[]).is_empty());
    }

    #[test]
    fn single_morning_day_one_block() {
        let times = vec![dt(2026, 6, 8, 10, 0), dt(2026, 6, 8, 11, 30)];
        let blocks = derive_time_blocks(&times);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start, dt(2026, 6, 8, 10, 0));
        assert_eq!(blocks[0].end, dt(2026, 6, 8, 11, 30));
        assert_eq!(blocks[0].date, NaiveDate::from_ymd_opt(2026, 6, 8).unwrap());
        assert_eq!(blocks[0].half_day, HalfDay::Morning);
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
            dt(2026, 6, 8, 15, 30),
            dt(2026, 6, 10, 9, 10),
            dt(2026, 6, 10, 11, 45),
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
        let times = vec![dt(2026, 6, 8, 11, 30), dt(2026, 6, 8, 10, 0)];
        let blocks = derive_time_blocks(&times);
        assert_eq!(blocks[0].start, dt(2026, 6, 8, 10, 0));
        assert_eq!(blocks[0].end, dt(2026, 6, 8, 11, 30));
    }
}
