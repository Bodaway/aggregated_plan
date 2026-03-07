use chrono::{Datelike, Duration, NaiveDate, NaiveTime};
use domain::types::*;

use crate::errors::AppError;
use crate::repositories::*;

/// Default weekly capacity in half-day slots (Mon-Fri x Morning/Afternoon = 10).
const DEFAULT_WEEKLY_CAPACITY_SLOTS: i32 = 10;

/// Each half-day slot spans 4 hours.
const HALF_DAY_HOURS: f64 = 4.0;

/// Morning slot boundaries.
const MORNING_START_HOUR: u32 = 8;
const MORNING_END_HOUR: u32 = 12;

/// Afternoon slot boundaries.
const AFTERNOON_START_HOUR: u32 = 13;
const AFTERNOON_END_HOUR: u32 = 17;

/// Aggregated data for the daily dashboard view.
pub struct DailyDashboard {
    pub date: NaiveDate,
    pub tasks: Vec<Task>,
    pub meetings: Vec<Meeting>,
    pub alerts: Vec<Alert>,
    pub weekly_workload: WeeklyWorkload,
    pub sync_statuses: Vec<SyncStatus>,
}

/// Weekly workload summary.
pub struct WeeklyWorkload {
    pub week_start: NaiveDate,
    pub capacity: i32,
    pub half_days: Vec<HalfDaySlotData>,
    pub total_planned: f64,
    pub total_meetings: f64,
    pub overload: Option<f64>,
}

/// A single half-day slot in the workload view.
pub struct HalfDaySlotData {
    pub date: NaiveDate,
    pub half_day: HalfDay,
    pub meetings: Vec<Meeting>,
    pub tasks: Vec<Task>,
    pub consumption: f64,
    pub is_free: bool,
}

/// Fetch all data needed for the daily dashboard: tasks (sorted by priority),
/// meetings, unresolved alerts, sync statuses, and weekly workload.
pub async fn get_daily_dashboard(
    task_repo: &dyn TaskRepository,
    meeting_repo: &dyn MeetingRepository,
    alert_repo: &dyn AlertRepository,
    sync_repo: &dyn SyncStatusRepository,
    user_id: UserId,
    date: NaiveDate,
) -> Result<DailyDashboard, AppError> {
    // 1. Tasks for the user: those with deadlines on the given date or in TODO/IN_PROGRESS status
    let active_filter = TaskFilter {
        status: Some(vec![TaskStatus::Todo, TaskStatus::InProgress]),
        ..TaskFilter::empty()
    };
    let mut tasks = task_repo.find_by_user(user_id, &active_filter).await?;

    // Also include tasks with deadline on this exact date that may be in other statuses
    let deadline_tasks = task_repo
        .find_by_date_range(user_id, date, date)
        .await?;
    for dt in deadline_tasks {
        if !tasks.iter().any(|t| t.id == dt.id) {
            tasks.push(dt);
        }
    }

    // 2. Meetings for the user on the given date
    let meetings = meeting_repo
        .find_by_user_and_date(user_id, date)
        .await?;

    // 3. Unresolved alerts
    let alerts = alert_repo.find_unresolved(user_id).await?;

    // 4. Sync statuses
    let sync_statuses = sync_repo.find_by_user(user_id).await?;

    // 5. Weekly workload
    let week_start = week_start_for(date);
    let week_end = week_start + Duration::days(4); // Friday
    let week_meetings = meeting_repo
        .find_by_user_and_range(user_id, week_start, week_end)
        .await?;
    let week_tasks = task_repo
        .find_by_date_range(user_id, week_start, week_end)
        .await?;

    let weekly_workload = compute_weekly_workload(week_start, &week_meetings, &week_tasks);

    Ok(DailyDashboard {
        date,
        tasks,
        meetings,
        alerts,
        weekly_workload,
        sync_statuses,
    })
}

/// Compute only the weekly workload for a given week_start (Monday).
pub async fn get_weekly_workload(
    task_repo: &dyn TaskRepository,
    meeting_repo: &dyn MeetingRepository,
    user_id: UserId,
    week_start: NaiveDate,
) -> Result<WeeklyWorkload, AppError> {
    let week_end = week_start + Duration::days(4); // Friday
    let week_meetings = meeting_repo
        .find_by_user_and_range(user_id, week_start, week_end)
        .await?;
    let week_tasks = task_repo
        .find_by_date_range(user_id, week_start, week_end)
        .await?;

    Ok(compute_weekly_workload(week_start, &week_meetings, &week_tasks))
}

/// Compute the Monday of the week containing the given date.
pub fn week_start_for(date: NaiveDate) -> NaiveDate {
    let weekday = date.weekday();
    let days_since_monday = weekday.num_days_from_monday() as i64;
    date - Duration::days(days_since_monday)
}

/// Compute the weekly workload from meetings and tasks.
fn compute_weekly_workload(
    week_start: NaiveDate,
    meetings: &[Meeting],
    tasks: &[Task],
) -> WeeklyWorkload {
    let capacity = DEFAULT_WEEKLY_CAPACITY_SLOTS;
    let mut half_days = Vec::with_capacity(10);
    let mut total_meeting_hours = 0.0_f64;

    // For each weekday Mon-Fri, create Morning and Afternoon slots
    for day_offset in 0..5 {
        let slot_date = week_start + Duration::days(day_offset);

        for &half_day in &[HalfDay::Morning, HalfDay::Afternoon] {
            let (slot_start_hour, slot_end_hour) = match half_day {
                HalfDay::Morning => (MORNING_START_HOUR, MORNING_END_HOUR),
                HalfDay::Afternoon => (AFTERNOON_START_HOUR, AFTERNOON_END_HOUR),
            };

            let slot_start_time = NaiveTime::from_hms_opt(slot_start_hour, 0, 0).unwrap();
            let slot_end_time = NaiveTime::from_hms_opt(slot_end_hour, 0, 0).unwrap();

            // Find meetings that overlap with this slot
            let mut slot_meetings = Vec::new();
            let mut slot_consumption = 0.0_f64;

            for meeting in meetings {
                let meeting_date = meeting.start_time.date_naive();
                // Only consider meetings on this specific day
                if meeting_date != slot_date {
                    continue;
                }

                let meeting_start = meeting.start_time.time();
                let meeting_end = meeting.end_time.time();

                // Calculate overlap between meeting time and slot time
                let overlap_start = meeting_start.max(slot_start_time);
                let overlap_end = meeting_end.min(slot_end_time);

                if overlap_start < overlap_end {
                    let overlap_minutes =
                        (overlap_end - overlap_start).num_minutes() as f64;
                    let overlap_hours = overlap_minutes / 60.0;
                    slot_consumption += overlap_hours / HALF_DAY_HOURS;
                    total_meeting_hours += overlap_hours;
                    slot_meetings.push(meeting.clone());
                }
            }

            // Find tasks with planned dates on this day
            let slot_tasks: Vec<Task> = tasks
                .iter()
                .filter(|t| {
                    // Tasks with deadline on this date
                    if let Some(deadline) = t.deadline {
                        if deadline == slot_date {
                            return true;
                        }
                    }
                    // Tasks with planned_start on this date
                    if let Some(planned_start) = t.planned_start {
                        let planned_date = planned_start.date_naive();
                        if planned_date == slot_date {
                            return true;
                        }
                    }
                    false
                })
                .cloned()
                .collect();

            let is_free = slot_consumption < 0.5;

            half_days.push(HalfDaySlotData {
                date: slot_date,
                half_day,
                meetings: slot_meetings,
                tasks: slot_tasks,
                consumption: slot_consumption,
                is_free,
            });
        }
    }

    // total_planned = sum of estimated_hours from tasks with planned dates in the week
    let total_planned: f64 = tasks
        .iter()
        .filter_map(|t| t.estimated_hours.map(|h| h as f64))
        .sum();

    let capacity_hours = capacity as f64 * HALF_DAY_HOURS;
    let overload = domain::rules::workload::detect_overload(
        total_planned,
        total_meeting_hours,
        capacity_hours,
    );

    WeeklyWorkload {
        week_start,
        capacity,
        half_days,
        total_planned,
        total_meetings: total_meeting_hours,
        overload,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    // ─── week_start_for ───

    #[test]
    fn week_start_for_monday_returns_same_date() {
        let monday = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap(); // Monday
        assert_eq!(week_start_for(monday), monday);
    }

    #[test]
    fn week_start_for_wednesday_returns_monday() {
        let wednesday = NaiveDate::from_ymd_opt(2026, 3, 11).unwrap(); // Wednesday
        let expected_monday = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        assert_eq!(week_start_for(wednesday), expected_monday);
    }

    #[test]
    fn week_start_for_friday_returns_monday() {
        let friday = NaiveDate::from_ymd_opt(2026, 3, 13).unwrap(); // Friday
        let expected_monday = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        assert_eq!(week_start_for(friday), expected_monday);
    }

    #[test]
    fn week_start_for_sunday_returns_monday() {
        let sunday = NaiveDate::from_ymd_opt(2026, 3, 15).unwrap(); // Sunday
        let expected_monday = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        assert_eq!(week_start_for(sunday), expected_monday);
    }

    // ─── compute_weekly_workload ───

    #[test]
    fn empty_week_has_zero_workload() {
        let monday = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        let result = compute_weekly_workload(monday, &[], &[]);

        assert_eq!(result.week_start, monday);
        assert_eq!(result.capacity, 10);
        assert_eq!(result.half_days.len(), 10);
        assert!((result.total_planned - 0.0).abs() < f64::EPSILON);
        assert!((result.total_meetings - 0.0).abs() < f64::EPSILON);
        assert!(result.overload.is_none());

        // All slots should be free
        for slot in &result.half_days {
            assert!(slot.is_free);
            assert!((slot.consumption - 0.0).abs() < f64::EPSILON);
            assert!(slot.meetings.is_empty());
        }
    }

    #[test]
    fn single_morning_meeting_computes_consumption() {
        let monday = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        let user_id = Uuid::new_v4();

        // 2-hour meeting Monday morning 09:00-11:00
        let meeting = Meeting {
            id: Uuid::new_v4(),
            user_id,
            title: "Standup".to_string(),
            start_time: Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap(),
            end_time: Utc.with_ymd_and_hms(2026, 3, 9, 11, 0, 0).unwrap(),
            location: None,
            participants: vec![],
            project_id: None,
            outlook_id: "outlook-1".to_string(),
            created_at: Utc::now(),
        };

        let result = compute_weekly_workload(monday, &[meeting], &[]);

        // Monday morning slot should have consumption = 2.0 / 4.0 = 0.5
        let monday_morning = &result.half_days[0];
        assert_eq!(monday_morning.date, monday);
        assert_eq!(monday_morning.half_day, HalfDay::Morning);
        assert!((monday_morning.consumption - 0.5).abs() < f64::EPSILON);
        assert!(!monday_morning.is_free); // consumption >= 0.5

        // Total meeting hours = 2.0
        assert!((result.total_meetings - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn full_morning_meeting_consumption_is_one() {
        let monday = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        let user_id = Uuid::new_v4();

        // 4-hour meeting Monday morning 08:00-12:00
        let meeting = Meeting {
            id: Uuid::new_v4(),
            user_id,
            title: "All morning".to_string(),
            start_time: Utc.with_ymd_and_hms(2026, 3, 9, 8, 0, 0).unwrap(),
            end_time: Utc.with_ymd_and_hms(2026, 3, 9, 12, 0, 0).unwrap(),
            location: None,
            participants: vec![],
            project_id: None,
            outlook_id: "outlook-2".to_string(),
            created_at: Utc::now(),
        };

        let result = compute_weekly_workload(monday, &[meeting], &[]);

        let monday_morning = &result.half_days[0];
        assert!((monday_morning.consumption - 1.0).abs() < f64::EPSILON);
        assert!(!monday_morning.is_free);

        // Monday afternoon should be free
        let monday_afternoon = &result.half_days[1];
        assert!(monday_afternoon.is_free);
        assert!((monday_afternoon.consumption - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn meeting_spanning_lunch_only_counts_overlap() {
        let monday = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        let user_id = Uuid::new_v4();

        // Meeting 11:00-14:00 (spans lunch break)
        let meeting = Meeting {
            id: Uuid::new_v4(),
            user_id,
            title: "Lunch meeting".to_string(),
            start_time: Utc.with_ymd_and_hms(2026, 3, 9, 11, 0, 0).unwrap(),
            end_time: Utc.with_ymd_and_hms(2026, 3, 9, 14, 0, 0).unwrap(),
            location: None,
            participants: vec![],
            project_id: None,
            outlook_id: "outlook-3".to_string(),
            created_at: Utc::now(),
        };

        let result = compute_weekly_workload(monday, &[meeting], &[]);

        // Morning overlap: 11:00-12:00 = 1 hour => consumption = 0.25
        let morning = &result.half_days[0];
        assert!((morning.consumption - 0.25).abs() < f64::EPSILON);
        assert!(morning.is_free); // < 0.5

        // Afternoon overlap: 13:00-14:00 = 1 hour => consumption = 0.25
        let afternoon = &result.half_days[1];
        assert!((afternoon.consumption - 0.25).abs() < f64::EPSILON);
        assert!(afternoon.is_free);

        // Total meeting hours should count both overlaps
        assert!((result.total_meetings - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn tasks_with_estimated_hours_contribute_to_total_planned() {
        let monday = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        let user_id = Uuid::new_v4();

        let task1 = Task {
            id: Uuid::new_v4(),
            user_id,
            title: "Task 1".to_string(),
            description: None,
            source: Source::Personal,
            source_id: None,
            jira_status: None,
            status: TaskStatus::InProgress,
            project_id: None,
            assignee: None,
            deadline: Some(monday),
            planned_start: None,
            planned_end: None,
            estimated_hours: Some(8.0),
            urgency: UrgencyLevel::Medium,
            urgency_manual: false,
            impact: ImpactLevel::Medium,
            tags: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let task2 = Task {
            id: Uuid::new_v4(),
            user_id,
            title: "Task 2".to_string(),
            description: None,
            source: Source::Personal,
            source_id: None,
            jira_status: None,
            status: TaskStatus::Todo,
            project_id: None,
            assignee: None,
            deadline: Some(monday + Duration::days(2)),
            planned_start: None,
            planned_end: None,
            estimated_hours: Some(4.0),
            urgency: UrgencyLevel::Low,
            urgency_manual: false,
            impact: ImpactLevel::Low,
            tags: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let result = compute_weekly_workload(monday, &[], &[task1, task2]);

        assert!((result.total_planned - 12.0).abs() < f64::EPSILON);
        assert!(result.overload.is_none()); // 12.0 + 0.0 <= 40.0
    }

    #[test]
    fn overload_detected_when_exceeding_capacity() {
        let monday = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        let user_id = Uuid::new_v4();

        // Create many meetings to fill 30 hours of the week
        let mut meetings = Vec::new();
        for day_offset in 0..5 {
            // Full morning meetings (4 hours each = 20 hours)
            meetings.push(Meeting {
                id: Uuid::new_v4(),
                user_id,
                title: format!("Morning meeting day {}", day_offset),
                start_time: Utc
                    .with_ymd_and_hms(2026, 3, 9 + day_offset, 8, 0, 0)
                    .unwrap(),
                end_time: Utc
                    .with_ymd_and_hms(2026, 3, 9 + day_offset, 12, 0, 0)
                    .unwrap(),
                location: None,
                participants: vec![],
                project_id: None,
                outlook_id: format!("outlook-m-{}", day_offset),
                created_at: Utc::now(),
            });
            // Full afternoon meetings (4 hours each = 20 hours)
            meetings.push(Meeting {
                id: Uuid::new_v4(),
                user_id,
                title: format!("Afternoon meeting day {}", day_offset),
                start_time: Utc
                    .with_ymd_and_hms(2026, 3, 9 + day_offset, 13, 0, 0)
                    .unwrap(),
                end_time: Utc
                    .with_ymd_and_hms(2026, 3, 9 + day_offset, 17, 0, 0)
                    .unwrap(),
                location: None,
                participants: vec![],
                project_id: None,
                outlook_id: format!("outlook-a-{}", day_offset),
                created_at: Utc::now(),
            });
        }

        // Task with 5 estimated hours
        let task = Task {
            id: Uuid::new_v4(),
            user_id,
            title: "Big task".to_string(),
            description: None,
            source: Source::Personal,
            source_id: None,
            jira_status: None,
            status: TaskStatus::InProgress,
            project_id: None,
            assignee: None,
            deadline: Some(monday),
            planned_start: None,
            planned_end: None,
            estimated_hours: Some(5.0),
            urgency: UrgencyLevel::Critical,
            urgency_manual: false,
            impact: ImpactLevel::High,
            tags: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let result = compute_weekly_workload(monday, &meetings, &[task]);

        // total_meetings = 40.0 hours (all slots full)
        assert!((result.total_meetings - 40.0).abs() < f64::EPSILON);
        // total_planned = 5.0
        assert!((result.total_planned - 5.0).abs() < f64::EPSILON);
        // overload = 40.0 + 5.0 - 40.0 = 5.0
        assert!(result.overload.is_some());
        assert!((result.overload.unwrap() - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn slot_structure_covers_full_week() {
        let monday = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        let result = compute_weekly_workload(monday, &[], &[]);

        assert_eq!(result.half_days.len(), 10);

        // Verify slot order: Mon AM, Mon PM, Tue AM, Tue PM, ...
        for (i, slot) in result.half_days.iter().enumerate() {
            let expected_day_offset = i / 2;
            let expected_half_day = if i % 2 == 0 {
                HalfDay::Morning
            } else {
                HalfDay::Afternoon
            };

            assert_eq!(
                slot.date,
                monday + Duration::days(expected_day_offset as i64)
            );
            assert_eq!(slot.half_day, expected_half_day);
        }
    }

    #[test]
    fn task_without_estimated_hours_contributes_zero() {
        let monday = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        let user_id = Uuid::new_v4();

        let task = Task {
            id: Uuid::new_v4(),
            user_id,
            title: "No estimate".to_string(),
            description: None,
            source: Source::Personal,
            source_id: None,
            jira_status: None,
            status: TaskStatus::Todo,
            project_id: None,
            assignee: None,
            deadline: Some(monday),
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            urgency: UrgencyLevel::Low,
            urgency_manual: false,
            impact: ImpactLevel::Low,
            tags: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let result = compute_weekly_workload(monday, &[], &[task]);
        assert!((result.total_planned - 0.0).abs() < f64::EPSILON);
    }
}
