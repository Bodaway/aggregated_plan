use chrono::{Datelike, Duration, NaiveDate};

use crate::types::UrgencyLevel;

/// Count business days between two dates (excluding weekends).
/// Positive if `to` is after `from`, negative if `to` is before `from`.
pub fn count_business_days(from: NaiveDate, to: NaiveDate) -> i64 {
    if from == to {
        return 0;
    }
    let forward = to >= from;
    let (start, end) = if forward { (from, to) } else { (to, from) };
    let mut count: i64 = 0;
    let mut current = start;
    while current < end {
        current += Duration::days(1);
        if current.weekday().num_days_from_monday() < 5 {
            count += 1;
        }
    }
    if forward { count } else { -count }
}

/// R10-R14: Calculate urgency from deadline relative to today.
pub fn calculate_urgency(deadline: Option<NaiveDate>, today: NaiveDate) -> UrgencyLevel {
    match deadline {
        None => UrgencyLevel::Low,
        Some(d) => {
            let business_days = count_business_days(today, d);
            if business_days < 0 {
                UrgencyLevel::Critical
            } else if business_days <= 1 {
                UrgencyLevel::High
            } else if business_days <= 5 {
                UrgencyLevel::Medium
            } else {
                UrgencyLevel::Low
            }
        }
    }
}

/// R15: Resolve urgency — manual override takes precedence.
pub fn resolve_urgency(
    manual_urgency: Option<UrgencyLevel>,
    deadline: Option<NaiveDate>,
    today: NaiveDate,
) -> (UrgencyLevel, bool) {
    match manual_urgency {
        Some(u) => (u, true),
        None => (calculate_urgency(deadline, today), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    // ─── count_business_days ───

    #[test]
    fn count_business_days_same_day_is_zero() {
        let d = date(2026, 3, 9); // Monday
        assert_eq!(count_business_days(d, d), 0);
    }

    #[test]
    fn count_business_days_one_weekday() {
        // Monday to Tuesday = 1 business day
        assert_eq!(count_business_days(date(2026, 3, 9), date(2026, 3, 10)), 1);
    }

    #[test]
    fn count_business_days_skips_weekend() {
        // Friday to Monday = 1 business day (Sat/Sun skipped)
        assert_eq!(count_business_days(date(2026, 3, 6), date(2026, 3, 9)), 1);
    }

    #[test]
    fn count_business_days_full_week() {
        // Monday to next Monday = 5 business days
        assert_eq!(count_business_days(date(2026, 3, 9), date(2026, 3, 16)), 5);
    }

    #[test]
    fn count_business_days_negative_when_past() {
        // Tuesday to Monday (going backward) = -1
        assert_eq!(count_business_days(date(2026, 3, 10), date(2026, 3, 9)), -1);
    }

    #[test]
    fn count_business_days_two_weeks() {
        // Monday to Monday two weeks later = 10
        assert_eq!(count_business_days(date(2026, 3, 9), date(2026, 3, 23)), 10);
    }

    // ─── R10: no deadline → Low ───

    #[test]
    fn r10_no_deadline_returns_low() {
        let today = date(2026, 3, 7);
        assert_eq!(calculate_urgency(None, today), UrgencyLevel::Low);
    }

    // ─── R11: deadline > 5 business days → Low ───

    #[test]
    fn r11_deadline_far_away_returns_low() {
        let today = date(2026, 3, 9); // Monday
        // 10 business days away
        let deadline = date(2026, 3, 23);
        assert_eq!(calculate_urgency(Some(deadline), today), UrgencyLevel::Low);
    }

    #[test]
    fn r11_deadline_exactly_6_business_days_returns_low() {
        let today = date(2026, 3, 9); // Monday
        // 6 business days = next Tuesday (Mon-Fri = 5, +Mon = 6... wait, let me think)
        // Mon 9 -> Tue 10 (1) -> Wed 11 (2) -> Thu 12 (3) -> Fri 13 (4) -> Mon 16 (5) -> Tue 17 (6)
        let deadline = date(2026, 3, 17);
        assert_eq!(count_business_days(today, deadline), 6);
        assert_eq!(calculate_urgency(Some(deadline), today), UrgencyLevel::Low);
    }

    // ─── R12: deadline 2-5 business days → Medium ───

    #[test]
    fn r12_deadline_5_business_days_returns_medium() {
        let today = date(2026, 3, 9); // Monday
        let deadline = date(2026, 3, 16); // Next Monday = 5 biz days
        assert_eq!(count_business_days(today, deadline), 5);
        assert_eq!(calculate_urgency(Some(deadline), today), UrgencyLevel::Medium);
    }

    #[test]
    fn r12_deadline_2_business_days_returns_medium() {
        let today = date(2026, 3, 9); // Monday
        let deadline = date(2026, 3, 11); // Wednesday = 2 biz days
        assert_eq!(count_business_days(today, deadline), 2);
        assert_eq!(calculate_urgency(Some(deadline), today), UrgencyLevel::Medium);
    }

    // ─── R13: deadline 0-1 business days → High ───

    #[test]
    fn r13_deadline_1_business_day_returns_high() {
        let today = date(2026, 3, 9); // Monday
        let deadline = date(2026, 3, 10); // Tuesday = 1 biz day
        assert_eq!(calculate_urgency(Some(deadline), today), UrgencyLevel::High);
    }

    #[test]
    fn r13_deadline_today_returns_high() {
        let today = date(2026, 3, 9);
        assert_eq!(calculate_urgency(Some(today), today), UrgencyLevel::High);
    }

    // ─── R14: deadline overdue → Critical ───

    #[test]
    fn r14_deadline_overdue_returns_critical() {
        let today = date(2026, 3, 11); // Wednesday
        let deadline = date(2026, 3, 9); // Monday (past)
        assert_eq!(calculate_urgency(Some(deadline), today), UrgencyLevel::Critical);
    }

    // ─── R15: resolve_urgency ───

    #[test]
    fn r15_manual_override_takes_precedence() {
        let today = date(2026, 3, 9);
        let deadline = Some(date(2026, 3, 23)); // Far away → would be Low
        let (level, is_manual) =
            resolve_urgency(Some(UrgencyLevel::Critical), deadline, today);
        assert_eq!(level, UrgencyLevel::Critical);
        assert!(is_manual);
    }

    #[test]
    fn r15_auto_when_no_manual() {
        let today = date(2026, 3, 9);
        let deadline = Some(date(2026, 3, 10)); // 1 biz day → High
        let (level, is_manual) = resolve_urgency(None, deadline, today);
        assert_eq!(level, UrgencyLevel::High);
        assert!(!is_manual);
    }
}
