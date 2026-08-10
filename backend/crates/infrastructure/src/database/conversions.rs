use domain::types::*;

/// Escape the LIKE metacharacters so a prefix is matched literally.
///
/// Shared by every repository that resolves an id prefix (memories, worklog
/// entries): `_` matches any single character in LIKE, so an unescaped token would
/// turn a mistyped reference into a match on an arbitrary row.
pub fn escape_like(raw: &str) -> String {
    raw.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

// --- Source ---

pub fn source_to_str(s: Source) -> &'static str {
    match s {
        Source::Jira => "jira",
        Source::Excel => "excel",
        Source::Obsidian => "obsidian",
        Source::Personal => "personal",
        Source::Outlook => "outlook",
        Source::Gryzzly => "gryzzly",
    }
}

pub fn source_from_str(s: &str) -> Source {
    match s {
        "jira" => Source::Jira,
        "excel" => Source::Excel,
        "obsidian" => Source::Obsidian,
        "personal" => Source::Personal,
        "outlook" => Source::Outlook,
        "gryzzly" => Source::Gryzzly,
        _ => Source::Personal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::types::Source;

    #[test]
    fn source_round_trips_every_variant() {
        for s in [Source::Jira, Source::Excel, Source::Obsidian, Source::Personal, Source::Outlook, Source::Gryzzly] {
            assert_eq!(source_from_str(source_to_str(s)), s, "round-trip failed for {s:?}");
        }
    }

    #[test]
    fn gryzzly_maps_to_its_own_string() {
        assert_eq!(source_to_str(Source::Gryzzly), "gryzzly");
        assert_eq!(source_from_str("gryzzly"), Source::Gryzzly);
    }

    #[test]
    fn alert_type_timesheet_ready_roundtrips() {
        assert_eq!(alert_type_to_str(AlertType::TimesheetReady), "timesheet_ready");
        assert_eq!(alert_type_from_str("timesheet_ready"), AlertType::TimesheetReady);
    }

    #[test]
    fn session_mode_round_trips() {
        for mode in [SessionMode::Tracking, SessionMode::Off] {
            assert_eq!(session_mode_from_str(session_mode_to_str(mode)), mode);
        }
    }

    #[test]
    fn not_configured_status_round_trips() {
        assert_eq!(sync_status_to_str(SyncSourceStatus::NotConfigured), "not_configured");
        assert_eq!(sync_status_from_str("not_configured"), SyncSourceStatus::NotConfigured);
    }

    #[test]
    fn an_unreadable_session_mode_falls_back_to_off() {
        // A row we cannot interpret must not be able to log. Reading it as
        // `tracking` would make a corrupt row write to a task nobody chose.
        assert_eq!(session_mode_from_str("garbage"), SessionMode::Off);
    }

    #[test]
    fn slot_source_round_trips() {
        for source in [SlotSource::Worklog, SlotSource::Manual] {
            assert_eq!(
                slot_source_from_str(Some(slot_source_to_str(source))),
                source
            );
        }
    }

    #[test]
    fn an_unclassified_slot_reads_as_manual() {
        // Migration 014 leaves historical rows NULL until the classification pass
        // runs. A NULL read as `worklog` would let a rebuild delete a slot whose
        // provenance nobody has established yet.
        assert_eq!(slot_source_from_str(None), SlotSource::Manual);
        assert_eq!(slot_source_from_str(Some("nonsense")), SlotSource::Manual);
    }
}

// --- TaskStatus ---

pub fn task_status_to_str(s: TaskStatus) -> &'static str {
    match s {
        TaskStatus::Todo => "todo",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Done => "done",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Cancelled => "cancelled",
    }
}

pub fn task_status_from_str(s: &str) -> TaskStatus {
    match s {
        "todo" => TaskStatus::Todo,
        "in_progress" => TaskStatus::InProgress,
        "done" => TaskStatus::Done,
        "blocked" => TaskStatus::Blocked,
        "cancelled" => TaskStatus::Cancelled,
        _ => TaskStatus::Todo,
    }
}

// --- UrgencyLevel ---

pub fn urgency_to_i32(u: UrgencyLevel) -> i32 {
    u as i32
}

pub fn urgency_from_i32(v: i32) -> UrgencyLevel {
    match v {
        1 => UrgencyLevel::Low,
        2 => UrgencyLevel::Medium,
        3 => UrgencyLevel::High,
        4 => UrgencyLevel::Critical,
        _ => UrgencyLevel::Medium,
    }
}

// --- ImpactLevel ---

pub fn impact_to_i32(i: ImpactLevel) -> i32 {
    i as i32
}

pub fn impact_from_i32(v: i32) -> ImpactLevel {
    match v {
        1 => ImpactLevel::Low,
        2 => ImpactLevel::Medium,
        3 => ImpactLevel::High,
        4 => ImpactLevel::Critical,
        _ => ImpactLevel::Medium,
    }
}

// --- HalfDay ---

pub fn half_day_to_str(h: HalfDay) -> &'static str {
    match h {
        HalfDay::Morning => "morning",
        HalfDay::Afternoon => "afternoon",
    }
}

pub fn half_day_from_str(s: &str) -> HalfDay {
    match s {
        "morning" => HalfDay::Morning,
        "afternoon" => HalfDay::Afternoon,
        _ => HalfDay::Morning,
    }
}

// --- AlertType ---

pub fn alert_type_to_str(a: AlertType) -> &'static str {
    match a {
        AlertType::Deadline => "deadline",
        AlertType::Overload => "overload",
        AlertType::Conflict => "conflict",
        AlertType::TimesheetReady => "timesheet_ready",
    }
}

pub fn alert_type_from_str(s: &str) -> AlertType {
    match s {
        "deadline" => AlertType::Deadline,
        "overload" => AlertType::Overload,
        "conflict" => AlertType::Conflict,
        "timesheet_ready" => AlertType::TimesheetReady,
        _ => AlertType::Conflict,
    }
}

// --- AlertSeverity ---

pub fn alert_severity_to_str(s: AlertSeverity) -> &'static str {
    match s {
        AlertSeverity::Critical => "critical",
        AlertSeverity::Warning => "warning",
        AlertSeverity::Information => "information",
    }
}

pub fn alert_severity_from_str(s: &str) -> AlertSeverity {
    match s {
        "critical" => AlertSeverity::Critical,
        "warning" => AlertSeverity::Warning,
        "information" => AlertSeverity::Information,
        _ => AlertSeverity::Information,
    }
}

// --- ProjectStatus ---

pub fn project_status_to_str(s: ProjectStatus) -> &'static str {
    match s {
        ProjectStatus::Active => "active",
        ProjectStatus::Paused => "paused",
        ProjectStatus::Completed => "completed",
    }
}

pub fn project_status_from_str(s: &str) -> ProjectStatus {
    match s {
        "active" => ProjectStatus::Active,
        "paused" => ProjectStatus::Paused,
        "completed" => ProjectStatus::Completed,
        _ => ProjectStatus::Active,
    }
}

// --- SyncSourceStatus ---

pub fn sync_status_to_str(s: SyncSourceStatus) -> &'static str {
    match s {
        SyncSourceStatus::Idle => "idle",
        SyncSourceStatus::Syncing => "syncing",
        SyncSourceStatus::Success => "success",
        SyncSourceStatus::Error => "error",
        SyncSourceStatus::NotConfigured => "not_configured",
    }
}

pub fn sync_status_from_str(s: &str) -> SyncSourceStatus {
    match s {
        "idle" => SyncSourceStatus::Idle,
        "syncing" => SyncSourceStatus::Syncing,
        "success" => SyncSourceStatus::Success,
        "error" => SyncSourceStatus::Error,
        "not_configured" => SyncSourceStatus::NotConfigured,
        _ => SyncSourceStatus::Idle,
    }
}

// --- TaskLinkType ---

pub fn task_link_type_to_str(t: TaskLinkType) -> &'static str {
    match t {
        TaskLinkType::AutoMerged => "auto_merged",
        TaskLinkType::ManualMerged => "manual_merged",
        TaskLinkType::Rejected => "rejected",
    }
}

pub fn task_link_type_from_str(s: &str) -> TaskLinkType {
    match s {
        "auto_merged" => TaskLinkType::AutoMerged,
        "manual_merged" => TaskLinkType::ManualMerged,
        "rejected" => TaskLinkType::Rejected,
        _ => TaskLinkType::AutoMerged,
    }
}

// --- SessionMode ---

pub fn session_mode_to_str(m: SessionMode) -> &'static str {
    m.as_str()
}

/// Anything unreadable is `Off`: a row we cannot interpret must not be able to log.
pub fn session_mode_from_str(s: &str) -> SessionMode {
    SessionMode::parse(s).unwrap_or(SessionMode::Off)
}

// --- SlotSource ---

pub fn slot_source_to_str(s: SlotSource) -> &'static str {
    match s {
        SlotSource::Worklog => "worklog",
        SlotSource::Manual => "manual",
    }
}

/// NULL and anything unrecognised are `Manual` — the value nothing rebuilds.
/// Migration 014 leaves pre-existing rows NULL on purpose, and the safe reading of
/// "provenance unknown" is "do not touch it".
pub fn slot_source_from_str(s: Option<&str>) -> SlotSource {
    match s {
        Some("worklog") => SlotSource::Worklog,
        _ => SlotSource::Manual,
    }
}
