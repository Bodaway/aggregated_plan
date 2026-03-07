use async_graphql::Enum;
use domain::types;

/// GraphQL enum for task source.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum SourceGql {
    Jira,
    Excel,
    Obsidian,
    Personal,
}

impl From<types::Source> for SourceGql {
    fn from(s: types::Source) -> Self {
        match s {
            types::Source::Jira => SourceGql::Jira,
            types::Source::Excel => SourceGql::Excel,
            types::Source::Obsidian => SourceGql::Obsidian,
            types::Source::Personal => SourceGql::Personal,
        }
    }
}

impl From<SourceGql> for types::Source {
    fn from(s: SourceGql) -> Self {
        match s {
            SourceGql::Jira => types::Source::Jira,
            SourceGql::Excel => types::Source::Excel,
            SourceGql::Obsidian => types::Source::Obsidian,
            SourceGql::Personal => types::Source::Personal,
        }
    }
}

/// GraphQL enum for task status.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum TaskStatusGql {
    Todo,
    InProgress,
    Done,
    Blocked,
}

impl From<types::TaskStatus> for TaskStatusGql {
    fn from(s: types::TaskStatus) -> Self {
        match s {
            types::TaskStatus::Todo => TaskStatusGql::Todo,
            types::TaskStatus::InProgress => TaskStatusGql::InProgress,
            types::TaskStatus::Done => TaskStatusGql::Done,
            types::TaskStatus::Blocked => TaskStatusGql::Blocked,
        }
    }
}

impl From<TaskStatusGql> for types::TaskStatus {
    fn from(s: TaskStatusGql) -> Self {
        match s {
            TaskStatusGql::Todo => types::TaskStatus::Todo,
            TaskStatusGql::InProgress => types::TaskStatus::InProgress,
            TaskStatusGql::Done => types::TaskStatus::Done,
            TaskStatusGql::Blocked => types::TaskStatus::Blocked,
        }
    }
}

/// GraphQL enum for urgency level.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum UrgencyLevelGql {
    Low,
    Medium,
    High,
    Critical,
}

impl From<types::UrgencyLevel> for UrgencyLevelGql {
    fn from(u: types::UrgencyLevel) -> Self {
        match u {
            types::UrgencyLevel::Low => UrgencyLevelGql::Low,
            types::UrgencyLevel::Medium => UrgencyLevelGql::Medium,
            types::UrgencyLevel::High => UrgencyLevelGql::High,
            types::UrgencyLevel::Critical => UrgencyLevelGql::Critical,
        }
    }
}

impl From<UrgencyLevelGql> for types::UrgencyLevel {
    fn from(u: UrgencyLevelGql) -> Self {
        match u {
            UrgencyLevelGql::Low => types::UrgencyLevel::Low,
            UrgencyLevelGql::Medium => types::UrgencyLevel::Medium,
            UrgencyLevelGql::High => types::UrgencyLevel::High,
            UrgencyLevelGql::Critical => types::UrgencyLevel::Critical,
        }
    }
}

/// GraphQL enum for impact level.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum ImpactLevelGql {
    Low,
    Medium,
    High,
    Critical,
}

impl From<types::ImpactLevel> for ImpactLevelGql {
    fn from(i: types::ImpactLevel) -> Self {
        match i {
            types::ImpactLevel::Low => ImpactLevelGql::Low,
            types::ImpactLevel::Medium => ImpactLevelGql::Medium,
            types::ImpactLevel::High => ImpactLevelGql::High,
            types::ImpactLevel::Critical => ImpactLevelGql::Critical,
        }
    }
}

impl From<ImpactLevelGql> for types::ImpactLevel {
    fn from(i: ImpactLevelGql) -> Self {
        match i {
            ImpactLevelGql::Low => types::ImpactLevel::Low,
            ImpactLevelGql::Medium => types::ImpactLevel::Medium,
            ImpactLevelGql::High => types::ImpactLevel::High,
            ImpactLevelGql::Critical => types::ImpactLevel::Critical,
        }
    }
}

/// GraphQL enum for Eisenhower quadrant.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum QuadrantGql {
    UrgentImportant,
    Important,
    Urgent,
    Neither,
}

impl From<types::Quadrant> for QuadrantGql {
    fn from(q: types::Quadrant) -> Self {
        match q {
            types::Quadrant::UrgentImportant => QuadrantGql::UrgentImportant,
            types::Quadrant::Important => QuadrantGql::Important,
            types::Quadrant::Urgent => QuadrantGql::Urgent,
            types::Quadrant::Neither => QuadrantGql::Neither,
        }
    }
}

impl From<QuadrantGql> for types::Quadrant {
    fn from(q: QuadrantGql) -> Self {
        match q {
            QuadrantGql::UrgentImportant => types::Quadrant::UrgentImportant,
            QuadrantGql::Important => types::Quadrant::Important,
            QuadrantGql::Urgent => types::Quadrant::Urgent,
            QuadrantGql::Neither => types::Quadrant::Neither,
        }
    }
}

/// GraphQL enum for half-day.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum HalfDayGql {
    Morning,
    Afternoon,
}

impl From<types::HalfDay> for HalfDayGql {
    fn from(h: types::HalfDay) -> Self {
        match h {
            types::HalfDay::Morning => HalfDayGql::Morning,
            types::HalfDay::Afternoon => HalfDayGql::Afternoon,
        }
    }
}

impl From<HalfDayGql> for types::HalfDay {
    fn from(h: HalfDayGql) -> Self {
        match h {
            HalfDayGql::Morning => types::HalfDay::Morning,
            HalfDayGql::Afternoon => types::HalfDay::Afternoon,
        }
    }
}

/// GraphQL enum for alert type.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum AlertTypeGql {
    Deadline,
    Overload,
    Conflict,
}

impl From<types::AlertType> for AlertTypeGql {
    fn from(a: types::AlertType) -> Self {
        match a {
            types::AlertType::Deadline => AlertTypeGql::Deadline,
            types::AlertType::Overload => AlertTypeGql::Overload,
            types::AlertType::Conflict => AlertTypeGql::Conflict,
        }
    }
}

/// GraphQL enum for alert severity.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum AlertSeverityGql {
    Information,
    Warning,
    Critical,
}

impl From<types::AlertSeverity> for AlertSeverityGql {
    fn from(a: types::AlertSeverity) -> Self {
        match a {
            types::AlertSeverity::Information => AlertSeverityGql::Information,
            types::AlertSeverity::Warning => AlertSeverityGql::Warning,
            types::AlertSeverity::Critical => AlertSeverityGql::Critical,
        }
    }
}

/// GraphQL enum for project status.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum ProjectStatusGql {
    Active,
    Paused,
    Completed,
}

impl From<types::ProjectStatus> for ProjectStatusGql {
    fn from(p: types::ProjectStatus) -> Self {
        match p {
            types::ProjectStatus::Active => ProjectStatusGql::Active,
            types::ProjectStatus::Paused => ProjectStatusGql::Paused,
            types::ProjectStatus::Completed => ProjectStatusGql::Completed,
        }
    }
}

impl From<ProjectStatusGql> for types::ProjectStatus {
    fn from(p: ProjectStatusGql) -> Self {
        match p {
            ProjectStatusGql::Active => types::ProjectStatus::Active,
            ProjectStatusGql::Paused => types::ProjectStatus::Paused,
            ProjectStatusGql::Completed => types::ProjectStatus::Completed,
        }
    }
}

/// GraphQL enum for sync source status.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum SyncSourceStatusGql {
    Idle,
    Syncing,
    Success,
    Error,
}

impl From<types::SyncSourceStatus> for SyncSourceStatusGql {
    fn from(s: types::SyncSourceStatus) -> Self {
        match s {
            types::SyncSourceStatus::Idle => SyncSourceStatusGql::Idle,
            types::SyncSourceStatus::Syncing => SyncSourceStatusGql::Syncing,
            types::SyncSourceStatus::Success => SyncSourceStatusGql::Success,
            types::SyncSourceStatus::Error => SyncSourceStatusGql::Error,
        }
    }
}

/// GraphQL enum for task link type (deduplication).
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum TaskLinkTypeGql {
    AutoMerged,
    ManualMerged,
    Rejected,
}

impl From<types::TaskLinkType> for TaskLinkTypeGql {
    fn from(t: types::TaskLinkType) -> Self {
        match t {
            types::TaskLinkType::AutoMerged => TaskLinkTypeGql::AutoMerged,
            types::TaskLinkType::ManualMerged => TaskLinkTypeGql::ManualMerged,
            types::TaskLinkType::Rejected => TaskLinkTypeGql::Rejected,
        }
    }
}
