use crate::types::{ImpactLevel, Quadrant, Task, UrgencyLevel};

/// R01-R04: Determine Eisenhower quadrant from urgency and impact levels.
/// Urgent = level >= 3 (High, Critical). Important = level >= 3 (High, Critical).
pub fn determine_quadrant(urgency: UrgencyLevel, impact: ImpactLevel) -> Quadrant {
    let is_urgent = (urgency as u8) >= 3;
    let is_important = (impact as u8) >= 3;
    match (is_urgent, is_important) {
        (true, true) => Quadrant::UrgentImportant,
        (false, true) => Quadrant::Important,
        (true, false) => Quadrant::Urgent,
        (false, false) => Quadrant::Neither,
    }
}

/// R05-R07: Sort tasks by priority (quadrant first, then deadline).
/// Tasks in higher-priority quadrants come first. Within the same quadrant,
/// tasks with earlier deadlines come first. Tasks without deadlines come last.
pub fn sort_tasks_by_priority(tasks: &mut [Task]) {
    tasks.sort_by(|a, b| {
        let qa = determine_quadrant(a.urgency, a.impact);
        let qb = determine_quadrant(b.urgency, b.impact);
        qa.cmp(&qb).then_with(|| match (&a.deadline, &b.deadline) {
            (Some(da), Some(db)) => da.cmp(db),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use chrono::{NaiveDate, Utc};
    use uuid::Uuid;

    fn make_task(
        title: &str,
        urgency: UrgencyLevel,
        impact: ImpactLevel,
        deadline: Option<NaiveDate>,
    ) -> Task {
        Task {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            title: title.to_string(),
            description: None,
            source: Source::Personal,
            source_id: None,
            jira_status: None,
            status: TaskStatus::Todo,
            project_id: None,
            assignee: None,
            deadline,
            planned_start: None,
            planned_end: None,
            estimated_hours: None,
            urgency,
            urgency_manual: false,
            impact,
            tags: vec![],
            tracking_state: TrackingState::Inbox,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    // ─── determine_quadrant ───

    #[test]
    fn quadrant_urgent_important() {
        assert_eq!(
            determine_quadrant(UrgencyLevel::Critical, ImpactLevel::Critical),
            Quadrant::UrgentImportant
        );
        assert_eq!(
            determine_quadrant(UrgencyLevel::High, ImpactLevel::High),
            Quadrant::UrgentImportant
        );
    }

    #[test]
    fn quadrant_important_not_urgent() {
        assert_eq!(
            determine_quadrant(UrgencyLevel::Low, ImpactLevel::High),
            Quadrant::Important
        );
        assert_eq!(
            determine_quadrant(UrgencyLevel::Medium, ImpactLevel::Critical),
            Quadrant::Important
        );
    }

    #[test]
    fn quadrant_urgent_not_important() {
        assert_eq!(
            determine_quadrant(UrgencyLevel::High, ImpactLevel::Low),
            Quadrant::Urgent
        );
        assert_eq!(
            determine_quadrant(UrgencyLevel::Critical, ImpactLevel::Medium),
            Quadrant::Urgent
        );
    }

    #[test]
    fn quadrant_neither() {
        assert_eq!(
            determine_quadrant(UrgencyLevel::Low, ImpactLevel::Low),
            Quadrant::Neither
        );
        assert_eq!(
            determine_quadrant(UrgencyLevel::Medium, ImpactLevel::Medium),
            Quadrant::Neither
        );
    }

    // ─── sort_tasks_by_priority ───

    #[test]
    fn sort_by_quadrant_then_deadline() {
        let mut tasks = vec![
            make_task("Neither", UrgencyLevel::Low, ImpactLevel::Low, Some(date(2026, 3, 10))),
            make_task(
                "UrgentImportant",
                UrgencyLevel::Critical,
                ImpactLevel::Critical,
                Some(date(2026, 3, 15)),
            ),
            make_task("Important", UrgencyLevel::Low, ImpactLevel::High, Some(date(2026, 3, 12))),
            make_task("Urgent", UrgencyLevel::High, ImpactLevel::Low, Some(date(2026, 3, 11))),
        ];
        sort_tasks_by_priority(&mut tasks);
        assert_eq!(tasks[0].title, "UrgentImportant");
        assert_eq!(tasks[1].title, "Important");
        assert_eq!(tasks[2].title, "Urgent");
        assert_eq!(tasks[3].title, "Neither");
    }

    #[test]
    fn sort_same_quadrant_by_deadline() {
        let mut tasks = vec![
            make_task("Later", UrgencyLevel::Critical, ImpactLevel::High, Some(date(2026, 4, 1))),
            make_task(
                "Sooner",
                UrgencyLevel::Critical,
                ImpactLevel::High,
                Some(date(2026, 3, 10)),
            ),
            make_task("No deadline", UrgencyLevel::Critical, ImpactLevel::High, None),
        ];
        sort_tasks_by_priority(&mut tasks);
        assert_eq!(tasks[0].title, "Sooner");
        assert_eq!(tasks[1].title, "Later");
        assert_eq!(tasks[2].title, "No deadline");
    }
}
