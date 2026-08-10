use async_graphql::Enum;
use application::use_cases::timesheet::DayOffScope;
use domain::rules::reconstruction::BlockKind;
use domain::types;

/// GraphQL enum for task source.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum SourceGql {
    Jira,
    Excel,
    Obsidian,
    Personal,
    Outlook,
    Gryzzly,
}

impl From<types::Source> for SourceGql {
    fn from(s: types::Source) -> Self {
        match s {
            types::Source::Jira => SourceGql::Jira,
            types::Source::Excel => SourceGql::Excel,
            types::Source::Obsidian => SourceGql::Obsidian,
            types::Source::Personal => SourceGql::Personal,
            types::Source::Outlook => SourceGql::Outlook,
            types::Source::Gryzzly => SourceGql::Gryzzly,
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
            SourceGql::Outlook => types::Source::Outlook,
            SourceGql::Gryzzly => types::Source::Gryzzly,
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
    /// Marks a skipped occurrence of a recurring task.
    Cancelled,
}

impl From<types::TaskStatus> for TaskStatusGql {
    fn from(s: types::TaskStatus) -> Self {
        match s {
            types::TaskStatus::Todo => TaskStatusGql::Todo,
            types::TaskStatus::InProgress => TaskStatusGql::InProgress,
            types::TaskStatus::Done => TaskStatusGql::Done,
            types::TaskStatus::Blocked => TaskStatusGql::Blocked,
            types::TaskStatus::Cancelled => TaskStatusGql::Cancelled,
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
            TaskStatusGql::Cancelled => types::TaskStatus::Cancelled,
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
    TimesheetReady,
}

impl From<types::AlertType> for AlertTypeGql {
    fn from(a: types::AlertType) -> Self {
        match a {
            types::AlertType::Deadline => AlertTypeGql::Deadline,
            types::AlertType::Overload => AlertTypeGql::Overload,
            types::AlertType::Conflict => AlertTypeGql::Conflict,
            types::AlertType::TimesheetReady => AlertTypeGql::TimesheetReady,
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
    /// No usable credentials, so no sync was attempted. Not a failure.
    NotConfigured,
}

impl From<types::SyncSourceStatus> for SyncSourceStatusGql {
    fn from(s: types::SyncSourceStatus) -> Self {
        match s {
            types::SyncSourceStatus::Idle => SyncSourceStatusGql::Idle,
            types::SyncSourceStatus::Syncing => SyncSourceStatusGql::Syncing,
            types::SyncSourceStatus::Success => SyncSourceStatusGql::Success,
            types::SyncSourceStatus::Error => SyncSourceStatusGql::Error,
            types::SyncSourceStatus::NotConfigured => SyncSourceStatusGql::NotConfigured,
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

/// GraphQL enum for task tracking state (triage workflow).
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum TrackingStateGql {
    Inbox,
    Followed,
    Dismissed,
}

impl From<types::TrackingState> for TrackingStateGql {
    fn from(t: types::TrackingState) -> Self {
        match t {
            types::TrackingState::Inbox => TrackingStateGql::Inbox,
            types::TrackingState::Followed => TrackingStateGql::Followed,
            types::TrackingState::Dismissed => TrackingStateGql::Dismissed,
        }
    }
}

impl From<TrackingStateGql> for types::TrackingState {
    fn from(t: TrackingStateGql) -> Self {
        match t {
            TrackingStateGql::Inbox => types::TrackingState::Inbox,
            TrackingStateGql::Followed => types::TrackingState::Followed,
            TrackingStateGql::Dismissed => types::TrackingState::Dismissed,
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum ConfidenceGql {
    High,
    Medium,
    Low,
}
impl From<types::Confidence> for ConfidenceGql {
    fn from(c: types::Confidence) -> Self {
        match c {
            types::Confidence::High => ConfidenceGql::High,
            types::Confidence::Medium => ConfidenceGql::Medium,
            types::Confidence::Low => ConfidenceGql::Low,
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum TimesheetStatusGql {
    Draft,
    Validated,
    Submitted,
    DayOff,
}
impl From<types::TimesheetStatus> for TimesheetStatusGql {
    fn from(s: types::TimesheetStatus) -> Self {
        match s {
            types::TimesheetStatus::Draft => TimesheetStatusGql::Draft,
            types::TimesheetStatus::Validated => TimesheetStatusGql::Validated,
            types::TimesheetStatus::Submitted => TimesheetStatusGql::Submitted,
            types::TimesheetStatus::DayOff => TimesheetStatusGql::DayOff,
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum BlockKindGql {
    Meeting,
    Work,
    OutOfOffice,
}
impl From<BlockKind> for BlockKindGql {
    fn from(b: BlockKind) -> Self {
        match b {
            BlockKind::Meeting => BlockKindGql::Meeting,
            BlockKind::Work => BlockKindGql::Work,
            BlockKind::OutOfOffice => BlockKindGql::OutOfOffice,
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum MappingKindGql {
    RepoPath,
    Branch,
    MeetingSubject,
    MeetingOrganizer,
    InternalProject,
}
impl From<types::MappingKind> for MappingKindGql {
    fn from(k: types::MappingKind) -> Self {
        match k {
            types::MappingKind::RepoPath => MappingKindGql::RepoPath,
            types::MappingKind::Branch => MappingKindGql::Branch,
            types::MappingKind::MeetingSubject => MappingKindGql::MeetingSubject,
            types::MappingKind::MeetingOrganizer => MappingKindGql::MeetingOrganizer,
            types::MappingKind::InternalProject => MappingKindGql::InternalProject,
        }
    }
}
impl From<MappingKindGql> for types::MappingKind {
    fn from(k: MappingKindGql) -> Self {
        match k {
            MappingKindGql::RepoPath => types::MappingKind::RepoPath,
            MappingKindGql::Branch => types::MappingKind::Branch,
            MappingKindGql::MeetingSubject => types::MappingKind::MeetingSubject,
            MappingKindGql::MeetingOrganizer => types::MappingKind::MeetingOrganizer,
            MappingKindGql::InternalProject => types::MappingKind::InternalProject,
        }
    }
}

/// What kind of thing a memory records.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum MemoryKindGql {
    Decision,
    Commitment,
    Fact,
    Preference,
}
impl From<types::MemoryKind> for MemoryKindGql {
    fn from(k: types::MemoryKind) -> Self {
        match k {
            types::MemoryKind::Decision => MemoryKindGql::Decision,
            types::MemoryKind::Commitment => MemoryKindGql::Commitment,
            types::MemoryKind::Fact => MemoryKindGql::Fact,
            types::MemoryKind::Preference => MemoryKindGql::Preference,
        }
    }
}
impl From<MemoryKindGql> for types::MemoryKind {
    fn from(k: MemoryKindGql) -> Self {
        match k {
            MemoryKindGql::Decision => types::MemoryKind::Decision,
            MemoryKindGql::Commitment => types::MemoryKind::Commitment,
            MemoryKindGql::Fact => types::MemoryKind::Fact,
            MemoryKindGql::Preference => types::MemoryKind::Preference,
        }
    }
}

/// Where a memory came from.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum MemorySourceGql {
    ClaudeSession,
    Manual,
    Dreaming,
}
impl From<types::MemorySource> for MemorySourceGql {
    fn from(s: types::MemorySource) -> Self {
        match s {
            types::MemorySource::ClaudeSession => MemorySourceGql::ClaudeSession,
            types::MemorySource::Manual => MemorySourceGql::Manual,
            types::MemorySource::Dreaming => MemorySourceGql::Dreaming,
        }
    }
}
impl From<MemorySourceGql> for types::MemorySource {
    fn from(s: MemorySourceGql) -> Self {
        match s {
            MemorySourceGql::ClaudeSession => types::MemorySource::ClaudeSession,
            MemorySourceGql::Manual => types::MemorySource::Manual,
            MemorySourceGql::Dreaming => types::MemorySource::Dreaming,
        }
    }
}

/// Validation-queue status of a memory. Distinct from the truth lifecycle
/// carried by `invalidatedAt` / `supersededBy`.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum MemoryStatusGql {
    Pending,
    Active,
    Rejected,
}
impl From<types::MemoryStatus> for MemoryStatusGql {
    fn from(s: types::MemoryStatus) -> Self {
        match s {
            types::MemoryStatus::Pending => MemoryStatusGql::Pending,
            types::MemoryStatus::Active => MemoryStatusGql::Active,
            types::MemoryStatus::Rejected => MemoryStatusGql::Rejected,
        }
    }
}
impl From<MemoryStatusGql> for types::MemoryStatus {
    fn from(s: MemoryStatusGql) -> Self {
        match s {
            MemoryStatusGql::Pending => types::MemoryStatus::Pending,
            MemoryStatusGql::Active => types::MemoryStatus::Active,
            MemoryStatusGql::Rejected => types::MemoryStatus::Rejected,
        }
    }
}

/// What a session was told to do with its worklog.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum SessionModeGql {
    Tracking,
    Off,
}

impl From<types::SessionMode> for SessionModeGql {
    fn from(m: types::SessionMode) -> Self {
        match m {
            types::SessionMode::Tracking => SessionModeGql::Tracking,
            types::SessionMode::Off => SessionModeGql::Off,
        }
    }
}

impl From<SessionModeGql> for types::SessionMode {
    fn from(m: SessionModeGql) -> Self {
        match m {
            SessionModeGql::Tracking => types::SessionMode::Tracking,
            SessionModeGql::Off => types::SessionMode::Off,
        }
    }
}

/// Which projection owns an activity slot.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum SlotSourceGql {
    Worklog,
    Manual,
}

impl From<types::SlotSource> for SlotSourceGql {
    fn from(s: types::SlotSource) -> Self {
        match s {
            types::SlotSource::Worklog => SlotSourceGql::Worklog,
            types::SlotSource::Manual => SlotSourceGql::Manual,
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum DayOffScopeGql {
    Full,
    Morning,
    Afternoon,
}
impl From<DayOffScopeGql> for DayOffScope {
    fn from(s: DayOffScopeGql) -> Self {
        match s {
            DayOffScopeGql::Full => DayOffScope::Full,
            DayOffScopeGql::Morning => DayOffScope::Morning,
            DayOffScopeGql::Afternoon => DayOffScope::Afternoon,
        }
    }
}

#[cfg(test)]
mod timesheet_enum_tests {
    use super::*;

    #[test]
    fn confidence_maps() {
        assert_eq!(ConfidenceGql::from(types::Confidence::Low), ConfidenceGql::Low);
    }
    #[test]
    fn mapping_kind_roundtrips() {
        for k in [
            types::MappingKind::RepoPath,
            types::MappingKind::Branch,
            types::MappingKind::MeetingSubject,
            types::MappingKind::MeetingOrganizer,
            types::MappingKind::InternalProject,
        ] {
            let g: MappingKindGql = k.into();
            let back: types::MappingKind = g.into();
            assert_eq!(back, k);
        }
    }
    #[test]
    fn day_off_scope_maps() {
        assert!(matches!(DayOffScope::from(DayOffScopeGql::Morning), DayOffScope::Morning));
    }
}

#[cfg(test)]
mod memory_enum_tests {
    use super::*;

    #[test]
    fn memory_kind_roundtrips() {
        for k in [
            types::MemoryKind::Decision,
            types::MemoryKind::Commitment,
            types::MemoryKind::Fact,
            types::MemoryKind::Preference,
        ] {
            let g: MemoryKindGql = k.into();
            let back: types::MemoryKind = g.into();
            assert_eq!(back, k);
        }
    }

    #[test]
    fn memory_source_roundtrips() {
        for s in [
            types::MemorySource::ClaudeSession,
            types::MemorySource::Manual,
            types::MemorySource::Dreaming,
        ] {
            let g: MemorySourceGql = s.into();
            let back: types::MemorySource = g.into();
            assert_eq!(back, s);
        }
    }

    #[test]
    fn memory_status_roundtrips() {
        for s in [
            types::MemoryStatus::Pending,
            types::MemoryStatus::Active,
            types::MemoryStatus::Rejected,
        ] {
            let g: MemoryStatusGql = s.into();
            let back: types::MemoryStatus = g.into();
            assert_eq!(back, s);
        }
    }
}
