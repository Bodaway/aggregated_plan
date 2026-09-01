use chrono::{DateTime, NaiveDate, Utc};

use crate::types::TaskStatus;

/// R73: how late a task is, and on which commitment.
///
/// The two levels do not say the same thing: an overrun `planned_start` is a scheduling
/// slip that binds only the user, while an overrun `deadline` is a broken commitment that
/// binds a third party.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overdue {
    None,
    /// `planned_start` is in the past — scheduling slip.
    Planned { days: i64 },
    /// `deadline` is in the past — broken commitment.
    Deadline { days: i64 },
}

/// R73: classify how late a task is, as of `today`.
///
/// `Deadline` wins when both levels apply: the graver one absorbs the lesser instead of
/// stacking two markers on one card. `Done` and `Cancelled` are never late. `days` counts
/// calendar days — not business days — because a broken commitment does not pause over
/// the weekend.
pub fn classify(
    planned_start: Option<DateTime<Utc>>,
    deadline: Option<NaiveDate>,
    status: TaskStatus,
    today: NaiveDate,
) -> Overdue {
    if matches!(status, TaskStatus::Done | TaskStatus::Cancelled) {
        return Overdue::None;
    }

    if let Some(d) = deadline.filter(|d| *d < today) {
        return Overdue::Deadline { days: (today - d).num_days() };
    }

    if let Some(p) = planned_start.map(|dt| dt.date_naive()).filter(|p| *p < today) {
        return Overdue::Planned { days: (today - p).num_days() };
    }

    Overdue::None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid date")
    }

    fn at(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        date(y, m, d).and_hms_opt(8, 0, 0).expect("valid time").and_utc()
    }

    const TODAY: (i32, u32, u32) = (2026, 3, 16); // a Monday

    #[test]
    fn classify_table() {
        let today = date(TODAY.0, TODAY.1, TODAY.2);

        let cases: Vec<(&str, Option<DateTime<Utc>>, Option<NaiveDate>, TaskStatus, Overdue)> = vec![
            ("no date at all is never late", None, None, TaskStatus::Todo, Overdue::None),
            (
                "planned today is not late",
                Some(at(2026, 3, 16)),
                None,
                TaskStatus::Todo,
                Overdue::None,
            ),
            (
                "planned tomorrow is not late",
                Some(at(2026, 3, 17)),
                None,
                TaskStatus::Todo,
                Overdue::None,
            ),
            (
                "planned yesterday is one day late",
                Some(at(2026, 3, 15)),
                None,
                TaskStatus::Todo,
                Overdue::Planned { days: 1 },
            ),
            (
                "deadline today is not late",
                None,
                Some(date(2026, 3, 16)),
                TaskStatus::Todo,
                Overdue::None,
            ),
            (
                "deadline yesterday is one day late, without any planned_start",
                None,
                Some(date(2026, 3, 15)),
                TaskStatus::Todo,
                Overdue::Deadline { days: 1 },
            ),
            (
                "deadline wins over planned when both are overrun",
                Some(at(2026, 3, 2)),
                Some(date(2026, 3, 11)),
                TaskStatus::Todo,
                Overdue::Deadline { days: 5 },
            ),
            (
                "an overrun deadline wins even when planned_start is still ahead",
                Some(at(2026, 3, 20)),
                Some(date(2026, 3, 13)),
                TaskStatus::Todo,
                Overdue::Deadline { days: 3 },
            ),
            (
                "a future deadline falls back on the overrun planned_start",
                Some(at(2026, 3, 12)),
                Some(date(2026, 3, 20)),
                TaskStatus::Todo,
                Overdue::Planned { days: 4 },
            ),
            (
                "days count calendar days, weekend included",
                Some(at(2026, 3, 13)), // Friday, today is Monday
                None,
                TaskStatus::Todo,
                Overdue::Planned { days: 3 },
            ),
            (
                "in progress is late like todo",
                Some(at(2026, 3, 9)),
                None,
                TaskStatus::InProgress,
                Overdue::Planned { days: 7 },
            ),
            (
                "blocked is late like todo",
                None,
                Some(date(2026, 3, 9)),
                TaskStatus::Blocked,
                Overdue::Deadline { days: 7 },
            ),
            (
                "done is never late",
                Some(at(2026, 2, 2)),
                Some(date(2026, 2, 9)),
                TaskStatus::Done,
                Overdue::None,
            ),
            (
                "cancelled is never late",
                Some(at(2026, 2, 2)),
                Some(date(2026, 2, 9)),
                TaskStatus::Cancelled,
                Overdue::None,
            ),
        ];

        for (label, planned_start, deadline, status, expected) in cases {
            assert_eq!(
                classify(planned_start, deadline, status, today),
                expected,
                "case: {label}"
            );
        }
    }

    #[test]
    fn planned_start_is_compared_on_its_utc_date() {
        let today = date(2026, 3, 16);
        // 23:59 UTC on the day before is still the day before.
        let late_evening = date(2026, 3, 15).and_hms_opt(23, 59, 0).expect("valid").and_utc();
        assert_eq!(
            classify(Some(late_evening), None, TaskStatus::Todo, today),
            Overdue::Planned { days: 1 }
        );
        // 00:01 UTC today is today.
        let early_morning = date(2026, 3, 16).and_hms_opt(0, 1, 0).expect("valid").and_utc();
        assert_eq!(
            classify(Some(early_morning), None, TaskStatus::Todo, today),
            Overdue::None
        );
    }

    #[test]
    fn days_span_months_and_years() {
        let today = date(2026, 1, 5);
        assert_eq!(
            classify(None, Some(date(2025, 12, 26)), TaskStatus::Todo, today),
            Overdue::Deadline { days: 10 }
        );
    }
}
