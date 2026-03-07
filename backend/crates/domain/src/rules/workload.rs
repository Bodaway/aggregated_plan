use chrono::{DateTime, Utc};

use crate::types::HalfDay;

/// Calculate meeting duration in hours from start and end times.
pub fn meeting_hours(start: DateTime<Utc>, end: DateTime<Utc>) -> f64 {
    (end - start).num_minutes() as f64 / 60.0
}

/// R16: Detect overload when total planned + meeting hours exceed weekly capacity.
/// Returns `Some(excess)` if overloaded, `None` otherwise.
pub fn detect_overload(
    planned_task_hours: f64,
    meeting_hours: f64,
    weekly_capacity_hours: f64,
) -> Option<f64> {
    let total = planned_task_hours + meeting_hours;
    if total > weekly_capacity_hours {
        Some(total - weekly_capacity_hours)
    } else {
        None
    }
}

/// Determine whether an hour falls in the morning or afternoon half-day.
/// Morning: hours < 13, Afternoon: hours >= 13.
pub fn half_day_of(hour: u32) -> HalfDay {
    if hour < 13 {
        HalfDay::Morning
    } else {
        HalfDay::Afternoon
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    // ─── meeting_hours ───

    #[test]
    fn meeting_hours_one_hour() {
        let start = Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 3, 9, 11, 0, 0).unwrap();
        assert!((meeting_hours(start, end) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn meeting_hours_ninety_minutes() {
        let start = Utc.with_ymd_and_hms(2026, 3, 9, 14, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 3, 9, 15, 30, 0).unwrap();
        assert!((meeting_hours(start, end) - 1.5).abs() < f64::EPSILON);
    }

    // ─── detect_overload ───

    #[test]
    fn overload_under_capacity() {
        assert_eq!(detect_overload(20.0, 5.0, 40.0), None);
    }

    #[test]
    fn overload_at_capacity() {
        assert_eq!(detect_overload(30.0, 10.0, 40.0), None);
    }

    #[test]
    fn overload_over_capacity() {
        let result = detect_overload(35.0, 10.0, 40.0);
        assert!(result.is_some());
        assert!((result.unwrap() - 5.0).abs() < f64::EPSILON);
    }

    // ─── half_day_of ───

    #[test]
    fn half_day_morning_hours() {
        assert_eq!(half_day_of(0), HalfDay::Morning);
        assert_eq!(half_day_of(8), HalfDay::Morning);
        assert_eq!(half_day_of(12), HalfDay::Morning);
    }

    #[test]
    fn half_day_afternoon_hours() {
        assert_eq!(half_day_of(13), HalfDay::Afternoon);
        assert_eq!(half_day_of(14), HalfDay::Afternoon);
        assert_eq!(half_day_of(23), HalfDay::Afternoon);
    }
}
