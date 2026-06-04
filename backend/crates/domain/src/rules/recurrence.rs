use chrono::{Datelike, Duration, NaiveDate, Weekday};

use crate::types::{RecurrenceRule, WeekOfMonth, WeekdaySet};

impl RecurrenceRule {
    /// Returns the next occurrence strictly after `previous`, on or after `starts_on`.
    ///
    /// If `previous < starts_on`, returns the first occurrence on or after `starts_on`.
    pub fn next_after(&self, starts_on: NaiveDate, previous: NaiveDate) -> Option<NaiveDate> {
        // The candidate search window: start from starts_on or the day after previous,
        // whichever is later.
        let search_from = if previous < starts_on {
            starts_on
        } else {
            previous + Duration::days(1)
        };

        // Compute a dynamic search horizon based on the rule interval so that large
        // intervals (e.g. Daily { interval: 30 }) are still found within one window.
        let horizon_days = match self {
            RecurrenceRule::Daily { interval } => (*interval as i64).max(1) * 2 + 7,
            RecurrenceRule::Weekly { interval, .. } => (*interval as i64).max(1) * 7 * 2 + 7,
            RecurrenceRule::MonthlyByDay { interval, .. }
            | RecurrenceRule::MonthlyByWeekday { interval, .. } => {
                (*interval as i64).max(1) * 32 + 31
            }
        };
        let horizon = search_from + Duration::days(horizon_days);
        let candidates = self.occurrences_in(starts_on, search_from, horizon);
        candidates.into_iter().next()
    }

    /// All occurrences in `[from, to]` (inclusive bounds).
    ///
    /// Returns an empty vec when `from > to`.
    pub fn occurrences_in(
        &self,
        starts_on: NaiveDate,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Vec<NaiveDate> {
        if from > to {
            return vec![];
        }
        match self {
            RecurrenceRule::Daily { interval } => {
                daily_occurrences(starts_on, from, to, *interval)
            }
            RecurrenceRule::Weekly { interval, weekdays } => {
                weekly_occurrences(starts_on, from, to, *interval, *weekdays)
            }
            RecurrenceRule::MonthlyByDay { interval, day } => {
                monthly_by_day_occurrences(starts_on, from, to, *interval, *day)
            }
            RecurrenceRule::MonthlyByWeekday {
                interval,
                week,
                weekday,
            } => monthly_by_weekday_occurrences(starts_on, from, to, *interval, *week, *weekday),
        }
    }
}

// --- Daily ---

fn daily_occurrences(
    starts_on: NaiveDate,
    from: NaiveDate,
    to: NaiveDate,
    interval: u8,
) -> Vec<NaiveDate> {
    let interval = interval.max(1) as i64;
    let mut results = Vec::new();

    // Find the first occurrence on or after `from` that is aligned with `starts_on`.
    // The sequence is: starts_on, starts_on+interval, starts_on+2*interval, ...
    let days_since_start = (from - starts_on).num_days();
    let offset = if days_since_start <= 0 {
        0
    } else {
        // Ceiling division: how many full intervals fit before `from`?
        let periods = (days_since_start + interval - 1) / interval;
        periods * interval
    };

    let mut date = starts_on + Duration::days(offset);
    // Ensure we are at or after `from` (offset arithmetic above handles this, but guard anyway).
    while date < from {
        date += Duration::days(interval);
    }

    while date <= to {
        results.push(date);
        date += Duration::days(interval);
    }
    results
}

// --- Weekly ---

fn weekly_occurrences(
    starts_on: NaiveDate,
    from: NaiveDate,
    to: NaiveDate,
    interval: u8,
    weekdays: WeekdaySet,
) -> Vec<NaiveDate> {
    let interval = interval.max(1) as i64;
    let week_days_interval = interval * 7;

    // Find the Monday of the starts_on week. Weekly recurrence anchors to this week.
    let starts_week_monday = monday_of_week(starts_on);

    // Find how many full `interval`-week periods fit before the week containing `from`.
    let from_week_monday = monday_of_week(from);
    let weeks_diff = (from_week_monday - starts_week_monday).num_days() / 7;
    // Align to the nearest earlier active week.
    let periods_back = weeks_diff / interval;
    let anchor_monday = starts_week_monday + Duration::days(periods_back * week_days_interval);

    // Walk forward week by week, in steps of `interval` weeks.
    let mut results = Vec::new();
    let mut week_monday = anchor_monday;

    // Safety: never iterate more than we need
    while week_monday <= to + Duration::days(7) {
        for day in weekdays.iter() {
            let candidate = week_monday + Duration::days(day_offset_from_monday(day));
            if candidate >= from && candidate <= to {
                results.push(candidate);
            }
        }
        week_monday += Duration::days(week_days_interval);
        // Stop when we've moved past `to`.
        if week_monday > to + Duration::days(7) {
            break;
        }
    }

    results.sort_unstable();
    results
}

fn monday_of_week(date: NaiveDate) -> NaiveDate {
    let dow = date.weekday().num_days_from_monday() as i64;
    date - Duration::days(dow)
}

fn day_offset_from_monday(day: Weekday) -> i64 {
    day.num_days_from_monday() as i64
}

// --- Monthly by day-of-month ---

fn monthly_by_day_occurrences(
    starts_on: NaiveDate,
    from: NaiveDate,
    to: NaiveDate,
    interval: u8,
    day: u8,
) -> Vec<NaiveDate> {
    let interval = interval.max(1) as u32;
    let mut results = Vec::new();

    // Walk months starting from the month of starts_on.
    let start_year = starts_on.year();
    let start_month = starts_on.month();

    // Enumerate months from start_year/start_month in steps of `interval`.
    // We upper-bound by scanning until we exceed `to`.
    let mut year = start_year;
    let mut month = start_month;

    loop {
        if let Some(occurrence) = month_day_occurrence(year, month, day) {
            if occurrence > to {
                break;
            }
            if occurrence >= from {
                results.push(occurrence);
            }
        }
        // Advance by `interval` months.
        let total_months = (year as u32) * 12 + (month - 1) + interval;
        year = (total_months / 12) as i32;
        month = total_months % 12 + 1;

        // Safety: don't loop forever — stop when the month start is already past `to`.
        if NaiveDate::from_ymd_opt(year, month, 1).is_none_or(|d| d > to) {
            break;
        }
    }

    results
}

/// Returns the occurrence date for a given year/month and day rule.
/// day=31 → last day of month.
/// day 1..=30 → exact day if it exists, else None (skip).
fn month_day_occurrence(year: i32, month: u32, day: u8) -> Option<NaiveDate> {
    let last_day = days_in_month(year, month);
    let target_day = if day == 31 { last_day } else { day as u32 };
    if target_day > last_day {
        None // skip months with fewer days (rule for day 1..=30)
    } else {
        NaiveDate::from_ymd_opt(year, month, target_day)
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    // The first day of the next month minus one day gives the last day of this month.
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .expect("valid date")
        .pred_opt()
        .expect("valid pred")
        .day()
}

// --- Monthly by Nth weekday ---

fn monthly_by_weekday_occurrences(
    starts_on: NaiveDate,
    from: NaiveDate,
    to: NaiveDate,
    interval: u8,
    week: WeekOfMonth,
    weekday: Weekday,
) -> Vec<NaiveDate> {
    let interval = interval.max(1) as u32;
    let mut results = Vec::new();

    let start_year = starts_on.year();
    let start_month = starts_on.month();

    let mut year = start_year;
    let mut month = start_month;

    loop {
        if let Some(occurrence) = nth_weekday_in_month(year, month, week, weekday) {
            if occurrence > to {
                break;
            }
            if occurrence >= from {
                results.push(occurrence);
            }
        }
        let total_months = (year as u32) * 12 + (month - 1) + interval;
        year = (total_months / 12) as i32;
        month = total_months % 12 + 1;

        if NaiveDate::from_ymd_opt(year, month, 1).is_none_or(|d| d > to) {
            break;
        }
    }

    results
}

/// Finds the Nth (or last) `weekday` in the given `year`/`month`.
fn nth_weekday_in_month(
    year: i32,
    month: u32,
    week: WeekOfMonth,
    weekday: Weekday,
) -> Option<NaiveDate> {
    let last_day = days_in_month(year, month);

    match week {
        WeekOfMonth::Last => {
            // Walk backwards from the last day.
            for d in (1..=last_day).rev() {
                let date = NaiveDate::from_ymd_opt(year, month, d)?;
                if date.weekday() == weekday {
                    return Some(date);
                }
            }
            None
        }
        _ => {
            // Which occurrence number are we targeting?
            let n: u32 = match week {
                WeekOfMonth::First => 1,
                WeekOfMonth::Second => 2,
                WeekOfMonth::Third => 3,
                WeekOfMonth::Fourth => 4,
                WeekOfMonth::Last => unreachable!(),
            };
            let mut count = 0u32;
            for d in 1..=last_day {
                let date = NaiveDate::from_ymd_opt(year, month, d)?;
                if date.weekday() == weekday {
                    count += 1;
                    if count == n {
                        return Some(date);
                    }
                }
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn weekdays(days: &[Weekday]) -> WeekdaySet {
        let mut set = WeekdaySet::empty();
        for &day in days {
            set.insert(day);
        }
        set
    }

    #[test]
    fn daily_interval_1_next_after() {
        let rule = RecurrenceRule::Daily { interval: 1 };
        let result = rule.next_after(d(2026, 1, 1), d(2026, 1, 5));
        assert_eq!(result, Some(d(2026, 1, 6)));
    }

    #[test]
    fn daily_interval_3_occurrences_in() {
        let rule = RecurrenceRule::Daily { interval: 3 };
        let results = rule.occurrences_in(d(2026, 1, 1), d(2026, 1, 1), d(2026, 1, 15));
        assert_eq!(
            results,
            vec![
                d(2026, 1, 1),
                d(2026, 1, 4),
                d(2026, 1, 7),
                d(2026, 1, 10),
                d(2026, 1, 13),
            ]
        );
    }

    #[test]
    fn weekly_interval_1_mon_wed_fri() {
        let rule = RecurrenceRule::Weekly {
            interval: 1,
            weekdays: weekdays(&[Weekday::Mon, Weekday::Wed, Weekday::Fri]),
        };
        // 2026-04-27 is a Monday.
        let results = rule.occurrences_in(d(2026, 4, 27), d(2026, 4, 27), d(2026, 5, 3));
        assert_eq!(
            results,
            vec![d(2026, 4, 27), d(2026, 4, 29), d(2026, 5, 1)]
        );
    }

    #[test]
    fn weekly_interval_2_fri_biweekly() {
        let rule = RecurrenceRule::Weekly {
            interval: 2,
            weekdays: weekdays(&[Weekday::Fri]),
        };
        let results = rule.occurrences_in(d(2026, 4, 3), d(2026, 4, 3), d(2026, 5, 31));
        assert_eq!(
            results,
            vec![
                d(2026, 4, 3),
                d(2026, 4, 17),
                d(2026, 5, 1),
                d(2026, 5, 15),
                d(2026, 5, 29),
            ]
        );
    }

    #[test]
    fn monthly_by_day_15() {
        let rule = RecurrenceRule::MonthlyByDay { interval: 1, day: 15 };
        let results = rule.occurrences_in(d(2026, 1, 1), d(2026, 1, 1), d(2026, 4, 30));
        assert_eq!(
            results,
            vec![d(2026, 1, 15), d(2026, 2, 15), d(2026, 3, 15), d(2026, 4, 15)]
        );
    }

    #[test]
    fn monthly_by_day_31_last_day() {
        let rule = RecurrenceRule::MonthlyByDay { interval: 1, day: 31 };
        let results = rule.occurrences_in(d(2026, 1, 1), d(2026, 1, 1), d(2026, 4, 30));
        assert_eq!(
            results,
            vec![d(2026, 1, 31), d(2026, 2, 28), d(2026, 3, 31), d(2026, 4, 30)]
        );
    }

    #[test]
    fn monthly_by_day_30_skips_feb() {
        let rule = RecurrenceRule::MonthlyByDay { interval: 1, day: 30 };
        let results = rule.occurrences_in(d(2026, 1, 1), d(2026, 1, 1), d(2026, 4, 30));
        // Feb 2026 has 28 days → skip; Jan-30, Mar-30, Apr-30
        assert_eq!(
            results,
            vec![d(2026, 1, 30), d(2026, 3, 30), d(2026, 4, 30)]
        );
    }

    #[test]
    fn monthly_by_day_29_leap_year() {
        let rule = RecurrenceRule::MonthlyByDay { interval: 1, day: 29 };
        let results = rule.occurrences_in(d(2024, 1, 1), d(2024, 1, 1), d(2024, 4, 30));
        // 2024 is a leap year → Feb has 29 days
        assert_eq!(
            results,
            vec![d(2024, 1, 29), d(2024, 2, 29), d(2024, 3, 29), d(2024, 4, 29)]
        );
    }

    #[test]
    fn monthly_by_weekday_first_tuesday() {
        let rule = RecurrenceRule::MonthlyByWeekday {
            interval: 1,
            week: WeekOfMonth::First,
            weekday: Weekday::Tue,
        };
        let results = rule.occurrences_in(d(2026, 1, 1), d(2026, 1, 1), d(2026, 4, 30));
        // First Tuesday of each month:
        // Jan 2026: Tue Jan 6
        // Feb 2026: Tue Feb 3
        // Mar 2026: Tue Mar 3
        // Apr 2026: Tue Apr 7
        assert_eq!(
            results,
            vec![d(2026, 1, 6), d(2026, 2, 3), d(2026, 3, 3), d(2026, 4, 7)]
        );
    }

    #[test]
    fn monthly_by_weekday_last_friday() {
        let rule = RecurrenceRule::MonthlyByWeekday {
            interval: 1,
            week: WeekOfMonth::Last,
            weekday: Weekday::Fri,
        };
        let results = rule.occurrences_in(d(2026, 1, 1), d(2026, 1, 1), d(2026, 3, 31));
        // Last Friday of each month:
        // Jan 2026: Fri Jan 30
        // Feb 2026: Fri Feb 27
        // Mar 2026: Fri Mar 27
        assert_eq!(
            results,
            vec![d(2026, 1, 30), d(2026, 2, 27), d(2026, 3, 27)]
        );
    }

    #[test]
    fn next_after_previous_before_starts_on() {
        let rule = RecurrenceRule::Daily { interval: 1 };
        // previous is before starts_on
        let result = rule.next_after(d(2026, 3, 10), d(2026, 1, 1));
        assert_eq!(result, Some(d(2026, 3, 10)));
    }

    #[test]
    fn daily_year_boundary() {
        let rule = RecurrenceRule::Daily { interval: 1 };
        let results = rule.occurrences_in(d(2025, 12, 30), d(2025, 12, 30), d(2026, 1, 2));
        assert_eq!(
            results,
            vec![
                d(2025, 12, 30),
                d(2025, 12, 31),
                d(2026, 1, 1),
                d(2026, 1, 2),
            ]
        );
    }

    #[test]
    fn empty_range_returns_empty() {
        let rule = RecurrenceRule::Daily { interval: 1 };
        let results = rule.occurrences_in(d(2026, 1, 1), d(2026, 6, 1), d(2026, 5, 1));
        assert!(results.is_empty());
    }

    // Additional: Monthly interval=2 sanity check
    #[test]
    fn monthly_by_day_interval_2() {
        let rule = RecurrenceRule::MonthlyByDay { interval: 2, day: 1 };
        let results = rule.occurrences_in(d(2026, 1, 1), d(2026, 1, 1), d(2026, 7, 1));
        assert_eq!(
            results,
            vec![d(2026, 1, 1), d(2026, 3, 1), d(2026, 5, 1), d(2026, 7, 1)]
        );
    }

    #[test]
    fn daily_interval_30_next_after_large_gap() {
        let rule = RecurrenceRule::Daily { interval: 30 };
        // starts_on 2026-01-01, previous 2026-01-01 → next should be 2026-01-31
        let result = rule.next_after(d(2026, 1, 1), d(2026, 1, 1));
        assert_eq!(result, Some(d(2026, 1, 31)));

        // Chained: from Jan 31, next should be Mar 2
        let result2 = rule.next_after(d(2026, 1, 1), d(2026, 1, 31));
        assert_eq!(result2, Some(d(2026, 3, 2)));
    }
}
