use domain::types::*;

// --- Source ---

pub fn source_to_str(s: Source) -> &'static str {
    match s {
        Source::Jira => "jira",
        Source::Excel => "excel",
        Source::Obsidian => "obsidian",
        Source::Personal => "personal",
    }
}

pub fn source_from_str(s: &str) -> Source {
    match s {
        "jira" => Source::Jira,
        "excel" => Source::Excel,
        "obsidian" => Source::Obsidian,
        "personal" => Source::Personal,
        _ => Source::Personal,
    }
}

// --- TaskStatus ---

pub fn task_status_to_str(s: TaskStatus) -> &'static str {
    match s {
        TaskStatus::Todo => "todo",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Done => "done",
        TaskStatus::Blocked => "blocked",
    }
}

pub fn task_status_from_str(s: &str) -> TaskStatus {
    match s {
        "todo" => TaskStatus::Todo,
        "in_progress" => TaskStatus::InProgress,
        "done" => TaskStatus::Done,
        "blocked" => TaskStatus::Blocked,
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
    }
}

pub fn alert_type_from_str(s: &str) -> AlertType {
    match s {
        "deadline" => AlertType::Deadline,
        "overload" => AlertType::Overload,
        "conflict" => AlertType::Conflict,
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
    }
}

pub fn sync_status_from_str(s: &str) -> SyncSourceStatus {
    match s {
        "idle" => SyncSourceStatus::Idle,
        "syncing" => SyncSourceStatus::Syncing,
        "success" => SyncSourceStatus::Success,
        "error" => SyncSourceStatus::Error,
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
