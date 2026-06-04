use chrono::{NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::common::{ImpactLevel, ProjectId, TagId, UrgencyLevel, UserId};

/// Opaque ID for a recurrence template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecurrenceTemplateId(pub Uuid);

impl RecurrenceTemplateId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for RecurrenceTemplateId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RecurrenceTemplateId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for RecurrenceTemplateId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse()?))
    }
}

/// A week-of-month selector for monthly-by-weekday recurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeekOfMonth {
    First,
    Second,
    Third,
    Fourth,
    Last,
}

/// Compact bitmask newtype for a set of weekdays.
/// Bit 0 = Monday, bit 1 = Tuesday, ..., bit 6 = Sunday.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeekdaySet(pub u8);

impl WeekdaySet {
    pub const fn empty() -> Self {
        Self(0)
    }

    pub fn insert(&mut self, day: Weekday) {
        self.0 |= 1 << Self::bit(day);
    }

    pub fn contains(&self, day: Weekday) -> bool {
        self.0 & (1 << Self::bit(day)) != 0
    }

    /// Iterate weekdays present in the set, in Mon→Sun order.
    pub fn iter(&self) -> impl Iterator<Item = Weekday> + '_ {
        [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
            Weekday::Sun,
        ]
        .into_iter()
        .filter(|d| self.contains(*d))
    }

    const fn bit(day: Weekday) -> u8 {
        match day {
            Weekday::Mon => 0,
            Weekday::Tue => 1,
            Weekday::Wed => 2,
            Weekday::Thu => 3,
            Weekday::Fri => 4,
            Weekday::Sat => 5,
            Weekday::Sun => 6,
        }
    }
}

/// The recurrence rule for a template.
///
/// Serde uses `"kind"` as the tag field and snake_case variant names.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecurrenceRule {
    /// Repeats every `interval` calendar days.
    Daily { interval: u8 },
    /// Repeats every `interval` weeks on the specified weekdays.
    Weekly { interval: u8, weekdays: WeekdaySet },
    /// Repeats every `interval` months on a fixed day-of-month (1–31).
    /// day=31 means "last day of month".
    /// For day 1–30: if the month has fewer days, the occurrence is skipped.
    MonthlyByDay { interval: u8, day: u8 },
    /// Repeats every `interval` months on the Nth weekday (e.g. "first Tuesday").
    MonthlyByWeekday {
        interval: u8,
        week: WeekOfMonth,
        weekday: Weekday,
    },
}

/// A persistent recurrence template owned by a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurrenceTemplate {
    pub id: RecurrenceTemplateId,
    pub user_id: UserId,
    pub title: String,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub project_id: Option<ProjectId>,
    pub urgency: UrgencyLevel,
    pub urgency_manual: bool,
    pub impact: ImpactLevel,
    pub estimated_hours: Option<f32>,
    pub tags: Vec<TagId>,
    pub rule: RecurrenceRule,
    /// First allowed occurrence date.
    pub starts_on: NaiveDate,
    /// Optional hard end date (inclusive). `None` means never.
    pub ends_on: Option<NaiveDate>,
    /// Optional occurrence count cap. `None` means never.
    /// When both `ends_on` and `max_occurrences` are set, `ends_on` takes precedence.
    pub max_occurrences: Option<u32>,
    /// Materialization watermark: all occurrences up to and including this date have been
    /// inserted into the `tasks` table.
    pub last_generated_through: Option<NaiveDate>,
    /// Soft-delete flag.
    pub active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test 14: JSON round-trip for each RecurrenceRule variant with correct "kind" discriminator.

    #[test]
    fn round_trip_daily() {
        let rule = RecurrenceRule::Daily { interval: 2 };
        let json = serde_json::to_string(&rule).unwrap();
        assert!(json.contains("\"kind\":\"daily\""), "json={json}");
        assert!(json.contains("\"interval\":2"), "json={json}");
        let back: RecurrenceRule = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rule);
    }

    #[test]
    fn round_trip_weekly() {
        let mut weekdays = WeekdaySet::empty();
        weekdays.insert(Weekday::Mon);
        weekdays.insert(Weekday::Fri);
        let rule = RecurrenceRule::Weekly { interval: 1, weekdays };
        let json = serde_json::to_string(&rule).unwrap();
        assert!(json.contains("\"kind\":\"weekly\""), "json={json}");
        let back: RecurrenceRule = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rule);
    }

    #[test]
    fn round_trip_monthly_by_day() {
        let rule = RecurrenceRule::MonthlyByDay { interval: 1, day: 15 };
        let json = serde_json::to_string(&rule).unwrap();
        assert!(json.contains("\"kind\":\"monthly_by_day\""), "json={json}");
        let back: RecurrenceRule = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rule);
    }

    #[test]
    fn round_trip_monthly_by_weekday() {
        let rule = RecurrenceRule::MonthlyByWeekday {
            interval: 1,
            week: WeekOfMonth::First,
            weekday: Weekday::Tue,
        };
        let json = serde_json::to_string(&rule).unwrap();
        assert!(json.contains("\"kind\":\"monthly_by_weekday\""), "json={json}");
        let back: RecurrenceRule = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rule);
    }

    // Test 15: WeekdaySet bitmask round-trip.

    #[test]
    fn weekday_set_insert_contains_iter() {
        let mut set = WeekdaySet::empty();
        set.insert(Weekday::Mon);
        set.insert(Weekday::Fri);

        assert!(set.contains(Weekday::Mon));
        assert!(set.contains(Weekday::Fri));
        assert!(!set.contains(Weekday::Tue));
        assert!(!set.contains(Weekday::Wed));
        assert!(!set.contains(Weekday::Thu));
        assert!(!set.contains(Weekday::Sat));
        assert!(!set.contains(Weekday::Sun));

        let days: Vec<Weekday> = set.iter().collect();
        assert_eq!(days, vec![Weekday::Mon, Weekday::Fri]);
    }

    #[test]
    fn weekday_set_all_days() {
        let mut set = WeekdaySet::empty();
        for day in [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
            Weekday::Sun,
        ] {
            set.insert(day);
        }
        let days: Vec<Weekday> = set.iter().collect();
        assert_eq!(days.len(), 7);
    }
}
