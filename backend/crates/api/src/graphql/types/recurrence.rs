use async_graphql::{Enum, InputObject, Object, SimpleObject, ID};
use chrono::{DateTime, NaiveDate, Utc, Weekday};
use domain::types::recurrence::{RecurrenceRule, RecurrenceTemplate, WeekOfMonth, WeekdaySet};

use super::enums::{ImpactLevelGql, UrgencyLevelGql};

// ─── Weekday enum ─────────────────────────────────────────────────────────────

/// GraphQL enum for days of the week.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum WeekdayGql {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl From<Weekday> for WeekdayGql {
    fn from(d: Weekday) -> Self {
        match d {
            Weekday::Mon => WeekdayGql::Monday,
            Weekday::Tue => WeekdayGql::Tuesday,
            Weekday::Wed => WeekdayGql::Wednesday,
            Weekday::Thu => WeekdayGql::Thursday,
            Weekday::Fri => WeekdayGql::Friday,
            Weekday::Sat => WeekdayGql::Saturday,
            Weekday::Sun => WeekdayGql::Sunday,
        }
    }
}

impl From<WeekdayGql> for Weekday {
    fn from(d: WeekdayGql) -> Self {
        match d {
            WeekdayGql::Monday => Weekday::Mon,
            WeekdayGql::Tuesday => Weekday::Tue,
            WeekdayGql::Wednesday => Weekday::Wed,
            WeekdayGql::Thursday => Weekday::Thu,
            WeekdayGql::Friday => Weekday::Fri,
            WeekdayGql::Saturday => Weekday::Sat,
            WeekdayGql::Sunday => Weekday::Sun,
        }
    }
}

// ─── WeekOfMonth enum ─────────────────────────────────────────────────────────

/// GraphQL enum for the week-of-month position.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum WeekOfMonthGql {
    First,
    Second,
    Third,
    Fourth,
    Last,
}

impl From<WeekOfMonth> for WeekOfMonthGql {
    fn from(w: WeekOfMonth) -> Self {
        match w {
            WeekOfMonth::First => WeekOfMonthGql::First,
            WeekOfMonth::Second => WeekOfMonthGql::Second,
            WeekOfMonth::Third => WeekOfMonthGql::Third,
            WeekOfMonth::Fourth => WeekOfMonthGql::Fourth,
            WeekOfMonth::Last => WeekOfMonthGql::Last,
        }
    }
}

impl From<WeekOfMonthGql> for WeekOfMonth {
    fn from(w: WeekOfMonthGql) -> Self {
        match w {
            WeekOfMonthGql::First => WeekOfMonth::First,
            WeekOfMonthGql::Second => WeekOfMonth::Second,
            WeekOfMonthGql::Third => WeekOfMonth::Third,
            WeekOfMonthGql::Fourth => WeekOfMonth::Fourth,
            WeekOfMonthGql::Last => WeekOfMonth::Last,
        }
    }
}

// ─── RecurrenceRuleGql output type ────────────────────────────────────────────

/// Flat output type for a recurrence rule.
///
/// `kind` is one of: "daily" | "weekly" | "monthly_by_day" | "monthly_by_weekday".
/// The optional fields are only populated for the matching kind.
#[derive(SimpleObject)]
pub struct RecurrenceRuleGql {
    pub kind: String,
    pub interval: i32,
    /// Set for `weekly`: weekdays on which the task repeats.
    pub weekdays: Option<Vec<WeekdayGql>>,
    /// Set for `monthly_by_day`: calendar day (1–31; 31 = last day of month).
    pub day_of_month: Option<i32>,
    /// Set for `monthly_by_weekday`: which week-of-month.
    pub week: Option<WeekOfMonthGql>,
    /// Set for `monthly_by_weekday`: which weekday.
    pub weekday: Option<WeekdayGql>,
}

impl From<&RecurrenceRule> for RecurrenceRuleGql {
    fn from(rule: &RecurrenceRule) -> Self {
        match rule {
            RecurrenceRule::Daily { interval } => RecurrenceRuleGql {
                kind: "daily".to_string(),
                interval: *interval as i32,
                weekdays: None,
                day_of_month: None,
                week: None,
                weekday: None,
            },
            RecurrenceRule::Weekly { interval, weekdays } => RecurrenceRuleGql {
                kind: "weekly".to_string(),
                interval: *interval as i32,
                weekdays: Some(weekdays.iter().map(WeekdayGql::from).collect()),
                day_of_month: None,
                week: None,
                weekday: None,
            },
            RecurrenceRule::MonthlyByDay { interval, day } => RecurrenceRuleGql {
                kind: "monthly_by_day".to_string(),
                interval: *interval as i32,
                weekdays: None,
                day_of_month: Some(*day as i32),
                week: None,
                weekday: None,
            },
            RecurrenceRule::MonthlyByWeekday { interval, week, weekday } => RecurrenceRuleGql {
                kind: "monthly_by_weekday".to_string(),
                interval: *interval as i32,
                weekdays: None,
                day_of_month: None,
                week: Some(WeekOfMonthGql::from(*week)),
                weekday: Some(WeekdayGql::from(*weekday)),
            },
        }
    }
}

// ─── RecurrenceRuleInput ──────────────────────────────────────────────────────

/// Flat input type for specifying a recurrence rule.
///
/// Set `kind` to one of: "daily" | "weekly" | "monthly_by_day" | "monthly_by_weekday".
/// The validator (`try_into_domain`) rejects inputs where required fields for the chosen
/// kind are missing or out of range.
#[derive(InputObject, Debug)]
pub struct RecurrenceRuleInput {
    pub kind: String,
    pub interval: i32,
    pub weekdays: Option<Vec<WeekdayGql>>,
    pub day_of_month: Option<i32>,
    pub week: Option<WeekOfMonthGql>,
    pub weekday: Option<WeekdayGql>,
}

impl RecurrenceRuleInput {
    /// Validate and convert to a domain `RecurrenceRule`.
    pub fn try_into_domain(self) -> async_graphql::Result<RecurrenceRule> {
        if self.interval < 1 {
            return Err(async_graphql::Error::new("interval must be >= 1"));
        }
        let interval = u8::try_from(self.interval)
            .map_err(|_| async_graphql::Error::new("interval must be between 1 and 255"))?;
        match self.kind.to_ascii_lowercase().as_str() {
            "daily" => Ok(RecurrenceRule::Daily { interval }),
            "weekly" => {
                let raw = self.weekdays.unwrap_or_default();
                if raw.is_empty() {
                    return Err(async_graphql::Error::new(
                        "weekly rule requires at least one weekday",
                    ));
                }
                let mut weekdays = WeekdaySet::empty();
                for d in raw {
                    weekdays.insert(Weekday::from(d));
                }
                Ok(RecurrenceRule::Weekly { interval, weekdays })
            }
            "monthly_by_day" => {
                let day = self.day_of_month.ok_or_else(|| {
                    async_graphql::Error::new(
                        "monthly_by_day rule requires day_of_month",
                    )
                })?;
                if !(1..=31).contains(&day) {
                    return Err(async_graphql::Error::new(
                        "day_of_month must be between 1 and 31",
                    ));
                }
                // day is validated 1..=31 above, so try_from is defence-in-depth.
                let day = u8::try_from(day)
                    .map_err(|_| async_graphql::Error::new("day_of_month must be between 1 and 31"))?;
                Ok(RecurrenceRule::MonthlyByDay { interval, day })
            }
            "monthly_by_weekday" => {
                let week = self.week.ok_or_else(|| {
                    async_graphql::Error::new(
                        "monthly_by_weekday rule requires week",
                    )
                })?;
                let weekday_gql = self.weekday.ok_or_else(|| {
                    async_graphql::Error::new(
                        "monthly_by_weekday rule requires weekday",
                    )
                })?;
                Ok(RecurrenceRule::MonthlyByWeekday {
                    interval,
                    week: WeekOfMonth::from(week),
                    weekday: Weekday::from(weekday_gql),
                })
            }
            other => Err(async_graphql::Error::new(format!(
                "unknown recurrence kind: \"{other}\"; expected daily | weekly | monthly_by_day | monthly_by_weekday"
            ))),
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn daily_input(interval: i32) -> RecurrenceRuleInput {
        RecurrenceRuleInput {
            kind: "daily".to_string(),
            interval,
            weekdays: None,
            day_of_month: None,
            week: None,
            weekday: None,
        }
    }

    #[test]
    fn interval_zero_rejected() {
        let result = daily_input(0).try_into_domain();
        assert!(result.is_err(), "interval=0 should be rejected");
        assert!(result.unwrap_err().message.contains("interval must be >= 1"));
    }

    #[test]
    fn interval_256_rejected() {
        let result = daily_input(256).try_into_domain();
        assert!(result.is_err(), "interval=256 should be rejected");
        assert!(result.unwrap_err().message.contains("interval must be between 1 and 255"));
    }

    #[test]
    fn interval_1_accepted() {
        let result = daily_input(1).try_into_domain();
        assert!(result.is_ok(), "interval=1 should be accepted: {:?}", result.err());
        assert!(matches!(result.unwrap(), RecurrenceRule::Daily { interval: 1 }));
    }

    #[test]
    fn interval_255_accepted() {
        let result = daily_input(255).try_into_domain();
        assert!(result.is_ok(), "interval=255 should be accepted: {:?}", result.err());
        assert!(matches!(result.unwrap(), RecurrenceRule::Daily { interval: 255 }));
    }
}

// ─── RecurrenceTemplateGql output type ───────────────────────────────────────

/// GraphQL output type for a recurrence template.
pub struct RecurrenceTemplateGql(pub RecurrenceTemplate);

#[Object]
impl RecurrenceTemplateGql {
    async fn id(&self) -> ID {
        ID(self.0.id.to_string())
    }

    async fn user_id(&self) -> ID {
        ID(self.0.user_id.to_string())
    }

    async fn title(&self) -> &str {
        &self.0.title
    }

    async fn description(&self) -> Option<&str> {
        self.0.description.as_deref()
    }

    async fn notes(&self) -> Option<&str> {
        self.0.notes.as_deref()
    }

    async fn project_id(&self) -> Option<ID> {
        self.0.project_id.map(|id| ID(id.to_string()))
    }

    async fn urgency(&self) -> UrgencyLevelGql {
        self.0.urgency.into()
    }

    async fn urgency_manual(&self) -> bool {
        self.0.urgency_manual
    }

    async fn impact(&self) -> ImpactLevelGql {
        self.0.impact.into()
    }

    async fn estimated_hours(&self) -> Option<f32> {
        self.0.estimated_hours
    }

    /// Tag IDs associated with the template.
    async fn tag_ids(&self) -> Vec<ID> {
        self.0.tags.iter().map(|t| ID(t.to_string())).collect()
    }

    async fn rule(&self) -> RecurrenceRuleGql {
        RecurrenceRuleGql::from(&self.0.rule)
    }

    async fn starts_on(&self) -> NaiveDate {
        self.0.starts_on
    }

    async fn ends_on(&self) -> Option<NaiveDate> {
        self.0.ends_on
    }

    async fn max_occurrences(&self) -> Option<i32> {
        self.0.max_occurrences.map(|n| n as i32)
    }

    async fn last_generated_through(&self) -> Option<NaiveDate> {
        self.0.last_generated_through
    }

    async fn active(&self) -> bool {
        self.0.active
    }

    async fn created_at(&self) -> DateTime<Utc> {
        self.0.created_at
    }

    async fn updated_at(&self) -> DateTime<Utc> {
        self.0.updated_at
    }
}

// ─── Input types for mutations ────────────────────────────────────────────────

/// Input for creating a new recurring task template.
#[derive(InputObject, Debug)]
pub struct CreateRecurringTaskInput {
    pub title: String,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub project_id: Option<ID>,
    pub urgency: UrgencyLevelGql,
    pub impact: ImpactLevelGql,
    pub estimated_hours: Option<f32>,
    pub tag_ids: Option<Vec<ID>>,
    pub rule: RecurrenceRuleInput,
    pub starts_on: NaiveDate,
    pub ends_on: Option<NaiveDate>,
    pub max_occurrences: Option<i32>,
}

/// Input for updating a recurring task template.
///
/// Every field is `Option`-wrapped: `None` means "leave unchanged".
/// For nullable domain fields (description, notes, etc.) the inner value
/// follows the same `Option<Option<T>>` pattern used by `UpdateTaskInput`.
#[derive(InputObject, Debug)]
pub struct UpdateRecurringTaskInput {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub notes: Option<Option<String>>,
    pub project_id: Option<Option<ID>>,
    pub urgency: Option<UrgencyLevelGql>,
    pub impact: Option<ImpactLevelGql>,
    pub estimated_hours: Option<Option<f32>>,
    pub tag_ids: Option<Vec<ID>>,
    pub rule: Option<RecurrenceRuleInput>,
    pub starts_on: Option<NaiveDate>,
    pub ends_on: Option<Option<NaiveDate>>,
    pub max_occurrences: Option<Option<i32>>,
}
