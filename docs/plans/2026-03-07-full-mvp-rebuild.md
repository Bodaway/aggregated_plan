# Aggregated Plan — Full MVP Rebuild Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Rebuild the Aggregated Plan application from scratch as specified in SPEC_TECHNIQUE.md and SPEC_FONCTIONNELLE.md — a personal cockpit for a Tech Lead aggregating Jira, Outlook, and Excel data with prioritization, activity tracking, and alerting.

**Architecture:** Rust backend (Axum + async-graphql + SQLite via sqlx) with a React/TypeScript frontend (urql + shadcn/ui + Tailwind). The backend uses 4 crates in a Cargo workspace (domain, application, infrastructure, api) following DDD with strict layer separation. The frontend communicates exclusively via GraphQL (queries, mutations, SSE subscriptions).

**Tech Stack:**
- Backend: Rust (stable), Axum 0.7, async-graphql 7, sqlx 0.8 (SQLite), tokio, reqwest, chrono, uuid, serde, thiserror, tracing
- Frontend: TypeScript 5.3+, React 18, Vite 5, urql 4, graphql-sse, shadcn/ui, Tailwind CSS 3, Recharts 2, @dnd-kit, react-router-dom 6, date-fns 3, vitest, Playwright
- Database: SQLite (MVP), PostgreSQL (future Teams deployment)

**Key specs to reference:**
- `SPEC_FONCTIONNELLE.md` — business rules (R01-R26), user stories (US-001 to US-064), entity definitions
- `SPEC_TECHNIQUE.md` — full technical specification, code examples, database schema, GraphQL schema, API details

---

## Phase 1: Foundation

### Task 1: Clean workspace and set up root project structure

**Files:**
- Delete: all existing `backend/`, `frontend/`, `packages/` source files
- Keep: `SPEC_FONCTIONNELLE.md`, `SPEC_TECHNIQUE.md`, `CLAUDE.md`, `.git/`, `docs/`
- Create: `backend/Cargo.toml` (workspace root)
- Create: `backend/.env.example`
- Create: `backend/crates/domain/Cargo.toml`
- Create: `backend/crates/domain/src/lib.rs`
- Create: `backend/crates/application/Cargo.toml`
- Create: `backend/crates/application/src/lib.rs`
- Create: `backend/crates/infrastructure/Cargo.toml`
- Create: `backend/crates/infrastructure/src/lib.rs`
- Create: `backend/crates/api/Cargo.toml`
- Create: `backend/crates/api/src/main.rs`
- Create: `migrations/sqlite/001_initial.sql`
- Modify: root `package.json` (remove old scripts, keep for frontend only)

**Step 1: Remove old source code**

```bash
# Remove old TypeScript backend/frontend/packages source (keep config files we'll replace)
rm -rf backend/src backend/jest.config.ts backend/tsconfig.json backend/package.json
rm -rf frontend/src frontend/jest.config.ts frontend/tsconfig.json frontend/package.json frontend/vite.config.ts
rm -rf packages/
rm -rf node_modules/
rm -f pnpm-workspace.yaml pnpm-lock.yaml
```

**Step 2: Create Cargo workspace**

Create `backend/Cargo.toml`:

```toml
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.dependencies]
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
thiserror = "1"
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
```

Create `backend/.env.example`:

```bash
DATABASE_URL=sqlite://data/aggregated-plan.db
SERVER_PORT=3001
RUST_LOG=info
```

**Step 3: Create domain crate**

Create `backend/crates/domain/Cargo.toml`:

```toml
[package]
name = "domain"
version = "0.1.0"
edition = "2021"

[dependencies]
chrono = { workspace = true }
serde = { workspace = true }
uuid = { workspace = true }
```

Create `backend/crates/domain/src/lib.rs`:

```rust
pub mod types;
pub mod rules;
pub mod errors;

pub type DomainResult<T> = Result<T, errors::DomainError>;
```

**Step 4: Create application crate**

Create `backend/crates/application/Cargo.toml`:

```toml
[package]
name = "application"
version = "0.1.0"
edition = "2021"

[dependencies]
domain = { path = "../domain" }
async-trait = "0.1"
thiserror = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }
tokio = { workspace = true }
```

Create `backend/crates/application/src/lib.rs`:

```rust
pub mod repositories;
pub mod services;
pub mod use_cases;
pub mod dto;
pub mod errors;
```

**Step 5: Create infrastructure crate**

Create `backend/crates/infrastructure/Cargo.toml`:

```toml
[package]
name = "infrastructure"
version = "0.1.0"
edition = "2021"

[dependencies]
domain = { path = "../domain" }
application = { path = "../application" }
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "chrono", "uuid"] }
reqwest = { version = "0.12", features = ["json"] }
tokio = { workspace = true }
tokio-cron-scheduler = "0.11"
serde = { workspace = true }
serde_json = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
async-trait = "0.1"
```

Create `backend/crates/infrastructure/src/lib.rs`:

```rust
pub mod database;
pub mod connectors;
pub mod sync;
pub mod dedup;
```

**Step 6: Create api crate**

Create `backend/crates/api/Cargo.toml`:

```toml
[package]
name = "api"
version = "0.1.0"
edition = "2021"

[dependencies]
domain = { path = "../domain" }
application = { path = "../application" }
infrastructure = { path = "../infrastructure" }
axum = "0.7"
async-graphql = { version = "7", features = ["chrono", "uuid"] }
async-graphql-axum = "7"
tokio = { workspace = true }
tower = "0.5"
tower-http = { version = "0.5", features = ["cors", "trace"] }
tracing = { workspace = true }
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
serde = { workspace = true }
serde_json = { workspace = true }
uuid = { workspace = true }
dotenvy = "0.15"
```

Create `backend/crates/api/src/main.rs`:

```rust
fn main() {
    println!("Aggregated Plan API — placeholder");
}
```

**Step 7: Verify workspace compiles**

Run: `cd backend && cargo check`
Expected: compiles successfully (warnings about unused modules are OK)

**Step 8: Create SQLite migration**

Create `migrations/sqlite/001_initial.sql` with the full schema from SPEC_TECHNIQUE.md section 7.1:

```sql
-- Users
CREATE TABLE users (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Projects
CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('jira', 'excel', 'obsidian', 'personal')),
    source_id TEXT,
    status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'paused', 'completed')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Tasks
CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    description TEXT,
    source TEXT NOT NULL CHECK (source IN ('jira', 'excel', 'obsidian', 'personal')),
    source_id TEXT,
    jira_status TEXT,
    status TEXT NOT NULL DEFAULT 'todo'
        CHECK (status IN ('todo', 'in_progress', 'done', 'blocked')),
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
    assignee TEXT,
    deadline TEXT,
    planned_start TEXT,
    planned_end TEXT,
    estimated_hours REAL,
    urgency INTEGER NOT NULL DEFAULT 1 CHECK (urgency BETWEEN 1 AND 4),
    urgency_manual INTEGER NOT NULL DEFAULT 0,
    impact INTEGER NOT NULL DEFAULT 2 CHECK (impact BETWEEN 1 AND 4),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Task deduplication links
CREATE TABLE task_links (
    id TEXT PRIMARY KEY,
    task_id_primary TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    task_id_secondary TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    link_type TEXT NOT NULL
        CHECK (link_type IN ('auto_merged', 'manual_merged', 'rejected')),
    confidence_score REAL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(task_id_primary, task_id_secondary)
);

-- Meetings
CREATE TABLE meetings (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    start_time TEXT NOT NULL,
    end_time TEXT NOT NULL,
    location TEXT,
    participants TEXT,
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
    outlook_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(user_id, outlook_id)
);

-- Activity slots
CREATE TABLE activity_slots (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    start_time TEXT NOT NULL,
    end_time TEXT,
    half_day TEXT NOT NULL CHECK (half_day IN ('morning', 'afternoon')),
    date TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Alerts
CREATE TABLE alerts (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    alert_type TEXT NOT NULL
        CHECK (alert_type IN ('deadline', 'overload', 'conflict')),
    severity TEXT NOT NULL
        CHECK (severity IN ('critical', 'warning', 'information')),
    message TEXT NOT NULL,
    related_items TEXT NOT NULL DEFAULT '[]',
    date TEXT NOT NULL,
    resolved INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Tags
CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    color TEXT,
    UNIQUE(user_id, name)
);

-- Task-Tag junction
CREATE TABLE task_tags (
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, tag_id)
);

-- Sync status
CREATE TABLE sync_status (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source TEXT NOT NULL
        CHECK (source IN ('jira', 'outlook', 'excel', 'obsidian')),
    last_sync_at TEXT,
    status TEXT NOT NULL DEFAULT 'idle'
        CHECK (status IN ('idle', 'syncing', 'success', 'error')),
    error_message TEXT,
    UNIQUE(user_id, source)
);

-- Configuration (key-value per user)
CREATE TABLE configuration (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    UNIQUE(user_id, key)
);

-- Indexes
CREATE INDEX idx_tasks_user ON tasks(user_id);
CREATE INDEX idx_tasks_source ON tasks(user_id, source, source_id);
CREATE INDEX idx_tasks_deadline ON tasks(user_id, deadline);
CREATE INDEX idx_tasks_project ON tasks(project_id);
CREATE INDEX idx_tasks_status ON tasks(user_id, status);
CREATE INDEX idx_meetings_user_time ON meetings(user_id, start_time);
CREATE INDEX idx_meetings_project ON meetings(project_id);
CREATE INDEX idx_activity_user_date ON activity_slots(user_id, date);
CREATE INDEX idx_alerts_user_resolved ON alerts(user_id, resolved);
CREATE INDEX idx_projects_user ON projects(user_id);
```

**Step 9: Commit**

```bash
git add -A
git commit -m "chore: clean workspace, set up Rust backend Cargo workspace with 4 crates + SQLite migration"
```

---

### Task 2: Domain types — common enums and type aliases

**Files:**
- Create: `backend/crates/domain/src/types/mod.rs`
- Create: `backend/crates/domain/src/types/common.rs`

**Step 1: Create types module**

Create `backend/crates/domain/src/types/mod.rs`:

```rust
pub mod common;
pub mod task;
pub mod meeting;
pub mod project;
pub mod activity;
pub mod alert;
pub mod tag;
pub mod user;

pub use common::*;
pub use task::*;
pub use meeting::*;
pub use project::*;
pub use activity::*;
pub use alert::*;
pub use tag::*;
pub use user::*;
```

**Step 2: Create common types**

Create `backend/crates/domain/src/types/common.rs` — copy from SPEC_TECHNIQUE.md section 5.1.1:

```rust
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type UserId = Uuid;
pub type TaskId = Uuid;
pub type MeetingId = Uuid;
pub type ProjectId = Uuid;
pub type ActivitySlotId = Uuid;
pub type AlertId = Uuid;
pub type TagId = Uuid;
pub type TaskLinkId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Source {
    Jira,
    Excel,
    Obsidian,
    Personal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Todo,
    InProgress,
    Done,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum UrgencyLevel {
    Low = 1,
    Medium = 2,
    High = 3,
    Critical = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum ImpactLevel {
    Low = 1,
    Medium = 2,
    High = 3,
    Critical = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HalfDay {
    Morning,
    Afternoon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertType {
    Deadline,
    Overload,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AlertSeverity {
    Information,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectStatus {
    Active,
    Paused,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncSourceStatus {
    Idle,
    Syncing,
    Success,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Quadrant {
    UrgentImportant,
    Important,
    Urgent,
    Neither,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskLinkType {
    AutoMerged,
    ManualMerged,
    Rejected,
}
```

**Step 3: Verify compiles**

Run: `cd backend && cargo check -p domain`
Expected: success

**Step 4: Commit**

```bash
git add backend/crates/domain/src/types/
git commit -m "feat(domain): add common enums and type aliases"
```

---

### Task 3: Domain types — entity structs

**Files:**
- Create: `backend/crates/domain/src/types/task.rs`
- Create: `backend/crates/domain/src/types/meeting.rs`
- Create: `backend/crates/domain/src/types/project.rs`
- Create: `backend/crates/domain/src/types/activity.rs`
- Create: `backend/crates/domain/src/types/alert.rs`
- Create: `backend/crates/domain/src/types/tag.rs`
- Create: `backend/crates/domain/src/types/user.rs`

**Step 1: Create all entity structs**

Copy all struct definitions from SPEC_TECHNIQUE.md section 5.1.1 (types/task.rs through types/user.rs). Each file follows the exact pattern from the spec. Key struct:

`backend/crates/domain/src/types/task.rs`:

```rust
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use super::common::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub user_id: UserId,
    pub title: String,
    pub description: Option<String>,
    pub source: Source,
    pub source_id: Option<String>,
    pub jira_status: Option<String>,
    pub status: TaskStatus,
    pub project_id: Option<ProjectId>,
    pub assignee: Option<String>,
    pub deadline: Option<NaiveDate>,
    pub planned_start: Option<DateTime<Utc>>,
    pub planned_end: Option<DateTime<Utc>>,
    pub estimated_hours: Option<f32>,
    pub urgency: UrgencyLevel,
    pub urgency_manual: bool,
    pub impact: ImpactLevel,
    pub tags: Vec<TagId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

`backend/crates/domain/src/types/meeting.rs`:

```rust
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use super::common::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meeting {
    pub id: MeetingId,
    pub user_id: UserId,
    pub title: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub location: Option<String>,
    pub participants: Vec<String>,
    pub project_id: Option<ProjectId>,
    pub outlook_id: String,
    pub created_at: DateTime<Utc>,
}
```

`backend/crates/domain/src/types/project.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use super::common::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub user_id: UserId,
    pub name: String,
    pub source: Source,
    pub source_id: Option<String>,
    pub status: ProjectStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

`backend/crates/domain/src/types/activity.rs`:

```rust
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use super::common::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivitySlot {
    pub id: ActivitySlotId,
    pub user_id: UserId,
    pub task_id: Option<TaskId>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub half_day: HalfDay,
    pub date: NaiveDate,
    pub created_at: DateTime<Utc>,
}
```

`backend/crates/domain/src/types/alert.rs`:

```rust
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use super::common::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: AlertId,
    pub user_id: UserId,
    pub alert_type: AlertType,
    pub severity: AlertSeverity,
    pub message: String,
    pub related_items: Vec<RelatedItem>,
    pub date: NaiveDate,
    pub resolved: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RelatedItem {
    Task(TaskId),
    Meeting(MeetingId),
}
```

`backend/crates/domain/src/types/tag.rs`:

```rust
use serde::{Deserialize, Serialize};
use super::common::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: TagId,
    pub user_id: UserId,
    pub name: String,
    pub color: Option<String>,
}
```

`backend/crates/domain/src/types/user.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use super::common::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub name: String,
    pub email: String,
    pub created_at: DateTime<Utc>,
}
```

Also add the `TaskLink` struct. Create `backend/crates/domain/src/types/task_link.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use super::common::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLink {
    pub id: TaskLinkId,
    pub task_id_primary: TaskId,
    pub task_id_secondary: TaskId,
    pub link_type: TaskLinkType,
    pub confidence_score: Option<f64>,
    pub created_at: DateTime<Utc>,
}
```

Add `pub mod task_link;` and `pub use task_link::*;` to `types/mod.rs`.

**Step 2: Verify compiles**

Run: `cd backend && cargo check -p domain`
Expected: success

**Step 3: Commit**

```bash
git add backend/crates/domain/src/types/
git commit -m "feat(domain): add all entity structs (Task, Meeting, Project, ActivitySlot, Alert, Tag, User, TaskLink)"
```

---

### Task 4: Domain errors and Result type

**Files:**
- Create: `backend/crates/domain/src/errors.rs`

**Step 1: Create domain errors**

Copy from SPEC_TECHNIQUE.md section 5.1.3:

```rust
use crate::types::*;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    #[error("Task not found: {0}")]
    TaskNotFound(TaskId),
    #[error("Project not found: {0}")]
    ProjectNotFound(ProjectId),
    #[error("Invalid urgency value: {0}. Must be 1-4.")]
    InvalidUrgency(u8),
    #[error("Invalid impact value: {0}. Must be 1-4.")]
    InvalidImpact(u8),
    #[error("Activity slot overlap: existing slot covers this time range")]
    ActivitySlotOverlap,
    #[error("Invalid date range: start {start} is after end {end}")]
    InvalidDateRange { start: String, end: String },
}
```

Add `thiserror` dependency to domain Cargo.toml:

```toml
[dependencies]
chrono = { workspace = true }
serde = { workspace = true }
uuid = { workspace = true }
thiserror = { workspace = true }
```

**Step 2: Verify compiles**

Run: `cd backend && cargo check -p domain`
Expected: success

**Step 3: Commit**

```bash
git add backend/crates/domain/
git commit -m "feat(domain): add DomainError enum and DomainResult type alias"
```

---

### Task 5: Business rules — urgency calculation (TDD)

**Files:**
- Create: `backend/crates/domain/src/rules/mod.rs`
- Create: `backend/crates/domain/src/rules/urgency.rs`

**Step 1: Write failing tests for urgency calculation**

Create `backend/crates/domain/src/rules/mod.rs`:

```rust
pub mod urgency;
pub mod priority;
pub mod workload;
pub mod alerts;
pub mod dedup;
```

Create `backend/crates/domain/src/rules/urgency.rs`:

```rust
use chrono::NaiveDate;
use crate::types::UrgencyLevel;

/// Count business days between two dates (excluding weekends).
/// Positive if `to` is after `from`, negative if `to` is before `from`.
pub fn count_business_days(from: NaiveDate, to: NaiveDate) -> i64 {
    todo!()
}

/// R10-R14: Calculate urgency from deadline relative to today.
pub fn calculate_urgency(deadline: Option<NaiveDate>, today: NaiveDate) -> UrgencyLevel {
    todo!()
}

/// R15: Resolve urgency — manual override takes precedence.
pub fn resolve_urgency(
    manual_urgency: Option<UrgencyLevel>,
    deadline: Option<NaiveDate>,
    today: NaiveDate,
) -> (UrgencyLevel, bool) {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    // --- count_business_days ---

    #[test]
    fn count_business_days_same_day_is_zero() {
        assert_eq!(count_business_days(date(2026, 3, 9), date(2026, 3, 9)), 0);
    }

    #[test]
    fn count_business_days_one_weekday() {
        // Monday to Tuesday
        assert_eq!(count_business_days(date(2026, 3, 9), date(2026, 3, 10)), 1);
    }

    #[test]
    fn count_business_days_skips_weekend() {
        // Friday to Monday = 1 business day (skips Sat+Sun)
        assert_eq!(count_business_days(date(2026, 3, 6), date(2026, 3, 9)), 1);
    }

    #[test]
    fn count_business_days_full_week() {
        // Monday to next Monday = 5 business days
        assert_eq!(count_business_days(date(2026, 3, 9), date(2026, 3, 16)), 5);
    }

    #[test]
    fn count_business_days_negative_when_past() {
        // Tuesday to previous Monday
        assert_eq!(count_business_days(date(2026, 3, 10), date(2026, 3, 9)), -1);
    }

    // --- calculate_urgency (R10-R14) ---

    #[test]
    fn r10_no_deadline_returns_low() {
        assert_eq!(calculate_urgency(None, date(2026, 3, 9)), UrgencyLevel::Low);
    }

    #[test]
    fn r11_deadline_more_than_5_business_days_returns_low() {
        // 9 March (Mon) + 6 business days = 17 March (Tue)
        assert_eq!(
            calculate_urgency(Some(date(2026, 3, 17)), date(2026, 3, 9)),
            UrgencyLevel::Low
        );
    }

    #[test]
    fn r12_deadline_in_2_to_5_business_days_returns_medium() {
        // 9 March (Mon) + 3 business days = 12 March (Thu)
        assert_eq!(
            calculate_urgency(Some(date(2026, 3, 12)), date(2026, 3, 9)),
            UrgencyLevel::Medium
        );
    }

    #[test]
    fn r13_deadline_in_1_business_day_returns_high() {
        // 9 March (Mon) + 1 business day = 10 March (Tue)
        assert_eq!(
            calculate_urgency(Some(date(2026, 3, 10)), date(2026, 3, 9)),
            UrgencyLevel::High
        );
    }

    #[test]
    fn r13_deadline_today_returns_high() {
        assert_eq!(
            calculate_urgency(Some(date(2026, 3, 9)), date(2026, 3, 9)),
            UrgencyLevel::High
        );
    }

    #[test]
    fn r14_deadline_overdue_returns_critical() {
        assert_eq!(
            calculate_urgency(Some(date(2026, 3, 6)), date(2026, 3, 9)),
            UrgencyLevel::Critical
        );
    }

    // --- resolve_urgency (R15) ---

    #[test]
    fn r15_manual_urgency_takes_precedence() {
        let (level, manual) = resolve_urgency(
            Some(UrgencyLevel::Critical),
            Some(date(2026, 3, 20)),
            date(2026, 3, 9),
        );
        assert_eq!(level, UrgencyLevel::Critical);
        assert!(manual);
    }

    #[test]
    fn r15_auto_urgency_when_no_manual() {
        let (level, manual) = resolve_urgency(
            None,
            Some(date(2026, 3, 10)),
            date(2026, 3, 9),
        );
        assert_eq!(level, UrgencyLevel::High);
        assert!(!manual);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cd backend && cargo test -p domain -- rules::urgency`
Expected: FAIL (tests panic with `todo!()`)

**Step 3: Implement urgency functions**

Replace the `todo!()` bodies with real implementations:

```rust
use chrono::{Datelike, NaiveDate, Duration};
use crate::types::UrgencyLevel;

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
        let wd = current.weekday().num_days_from_monday(); // 0=Mon, 6=Sun
        if wd < 5 {
            count += 1;
        }
    }

    if forward { count } else { -count }
}

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
```

**Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test -p domain -- rules::urgency`
Expected: all tests PASS

**Step 5: Commit**

```bash
git add backend/crates/domain/src/rules/
git commit -m "feat(domain): implement urgency calculation rules R10-R15 with tests"
```

---

### Task 6: Business rules — priority and quadrant classification (TDD)

**Files:**
- Create: `backend/crates/domain/src/rules/priority.rs`

**Step 1: Write failing tests**

```rust
use crate::types::{ImpactLevel, Quadrant, Task, UrgencyLevel};

/// Classify a task into a priority quadrant based on urgency and impact.
pub fn determine_quadrant(urgency: UrgencyLevel, impact: ImpactLevel) -> Quadrant {
    todo!()
}

/// Sort tasks by priority: UrgentImportant first, then Important, Urgent, Neither.
/// Within same quadrant, sort by closest deadline first (None deadlines go last).
pub fn sort_tasks_by_priority(tasks: &mut [Task]) {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quadrant_high_urgency_high_impact() {
        assert_eq!(
            determine_quadrant(UrgencyLevel::High, ImpactLevel::High),
            Quadrant::UrgentImportant
        );
    }

    #[test]
    fn quadrant_low_urgency_high_impact() {
        assert_eq!(
            determine_quadrant(UrgencyLevel::Low, ImpactLevel::High),
            Quadrant::Important
        );
    }

    #[test]
    fn quadrant_high_urgency_low_impact() {
        assert_eq!(
            determine_quadrant(UrgencyLevel::High, ImpactLevel::Low),
            Quadrant::Urgent
        );
    }

    #[test]
    fn quadrant_low_urgency_low_impact() {
        assert_eq!(
            determine_quadrant(UrgencyLevel::Low, ImpactLevel::Low),
            Quadrant::Neither
        );
    }

    #[test]
    fn quadrant_medium_urgency_medium_impact_is_neither() {
        // Medium (2) < 3, so not urgent and not important
        assert_eq!(
            determine_quadrant(UrgencyLevel::Medium, ImpactLevel::Medium),
            Quadrant::Neither
        );
    }

    #[test]
    fn quadrant_critical_urgency_critical_impact() {
        assert_eq!(
            determine_quadrant(UrgencyLevel::Critical, ImpactLevel::Critical),
            Quadrant::UrgentImportant
        );
    }
}
```

**Step 2: Run tests to verify failure**

Run: `cd backend && cargo test -p domain -- rules::priority`
Expected: FAIL

**Step 3: Implement**

```rust
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

pub fn sort_tasks_by_priority(tasks: &mut [Task]) {
    tasks.sort_by(|a, b| {
        let qa = determine_quadrant(a.urgency, a.impact);
        let qb = determine_quadrant(b.urgency, b.impact);
        qa.cmp(&qb).then_with(|| {
            match (&a.deadline, &b.deadline) {
                (Some(da), Some(db)) => da.cmp(db),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        })
    });
}
```

**Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test -p domain -- rules::priority`
Expected: PASS

**Step 5: Commit**

```bash
git add backend/crates/domain/src/rules/priority.rs
git commit -m "feat(domain): implement priority quadrant classification and task sorting"
```

---

### Task 7: Business rules — workload calculation (TDD)

**Files:**
- Create: `backend/crates/domain/src/rules/workload.rs`

**Step 1: Write failing tests**

```rust
use chrono::{DateTime, Utc};
use crate::types::HalfDay;

/// Calculate hours consumed by a meeting.
pub fn meeting_hours(start: DateTime<Utc>, end: DateTime<Utc>) -> f64 {
    todo!()
}

/// R16: Detect overload for a week.
/// Returns Some(excess) if total > capacity, None otherwise.
pub fn detect_overload(
    planned_task_hours: f64,
    meeting_hours: f64,
    weekly_capacity_hours: f64,
) -> Option<f64> {
    todo!()
}

/// Determine which half-day a given hour falls into.
pub fn half_day_of(hour: u32) -> HalfDay {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn meeting_hours_one_hour() {
        let start = Utc.with_ymd_and_hms(2026, 3, 9, 9, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 3, 9, 10, 0, 0).unwrap();
        assert!((meeting_hours(start, end) - 1.0).abs() < 0.001);
    }

    #[test]
    fn meeting_hours_90_minutes() {
        let start = Utc.with_ymd_and_hms(2026, 3, 9, 14, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 3, 9, 15, 30, 0).unwrap();
        assert!((meeting_hours(start, end) - 1.5).abs() < 0.001);
    }

    #[test]
    fn detect_overload_under_capacity() {
        assert_eq!(detect_overload(20.0, 10.0, 40.0), None);
    }

    #[test]
    fn detect_overload_over_capacity() {
        assert_eq!(detect_overload(30.0, 15.0, 40.0), Some(5.0));
    }

    #[test]
    fn detect_overload_exactly_at_capacity() {
        assert_eq!(detect_overload(20.0, 20.0, 40.0), None);
    }

    #[test]
    fn half_day_morning() {
        assert_eq!(half_day_of(8), HalfDay::Morning);
        assert_eq!(half_day_of(12), HalfDay::Morning);
    }

    #[test]
    fn half_day_afternoon() {
        assert_eq!(half_day_of(13), HalfDay::Afternoon);
        assert_eq!(half_day_of(17), HalfDay::Afternoon);
    }
}
```

**Step 2: Run tests, verify failure**

Run: `cd backend && cargo test -p domain -- rules::workload`

**Step 3: Implement**

```rust
pub fn meeting_hours(start: DateTime<Utc>, end: DateTime<Utc>) -> f64 {
    (end - start).num_minutes() as f64 / 60.0
}

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

pub fn half_day_of(hour: u32) -> HalfDay {
    if hour < 13 { HalfDay::Morning } else { HalfDay::Afternoon }
}
```

**Step 4: Run tests, verify pass**

Run: `cd backend && cargo test -p domain -- rules::workload`

**Step 5: Commit**

```bash
git add backend/crates/domain/src/rules/workload.rs
git commit -m "feat(domain): implement workload calculation rules R01-R03 with tests"
```

---

### Task 8: Business rules — alert detection (TDD)

**Files:**
- Create: `backend/crates/domain/src/rules/alerts.rs`

**Step 1: Write failing tests and stub functions**

Implement `AlertData`, `ScheduledItem`, `check_deadline_alerts`, `check_conflict_alerts`, and `check_overload_alerts` as described in SPEC_TECHNIQUE.md section 5.1.2 (rules/alerts.rs).

Key tests:
- `check_deadline_alerts` with overdue task returns Critical alert
- `check_deadline_alerts` with deadline within threshold returns Warning
- `check_deadline_alerts` with deadline far away returns empty
- `check_overload_alerts` returns alert when over capacity
- `check_conflict_alerts` detects overlapping time ranges (start_a < end_b AND start_b < end_a)
- `check_conflict_alerts` returns empty for non-overlapping items

**Step 2: Run tests, verify failure**

Run: `cd backend && cargo test -p domain -- rules::alerts`

**Step 3: Implement all alert functions**

Follow SPEC_TECHNIQUE.md section 5.1.2 exactly. The conflict detection uses the overlap formula: `start_a < end_b AND start_b < end_a`.

**Step 4: Run tests, verify pass**

Run: `cd backend && cargo test -p domain -- rules::alerts`

**Step 5: Commit**

```bash
git add backend/crates/domain/src/rules/alerts.rs
git commit -m "feat(domain): implement alert detection rules R16-R19 with tests"
```

---

### Task 9: Business rules — deduplication scoring (TDD)

**Files:**
- Create: `backend/crates/domain/src/rules/dedup.rs`

**Step 1: Write failing tests and stubs**

Implement `SimilarityScore`, `find_jira_key_in_text`, `calculate_similarity`, and `normalized_levenshtein` as per SPEC_TECHNIQUE.md section 5.1.2 (rules/dedup.rs).

Key tests:
- `find_jira_key_in_text("PROJ-123", "Row about PROJ-123 task")` → true
- `find_jira_key_in_text("PROJ-123", "No match here")` → false
- `normalized_levenshtein("hello", "hello")` → 1.0
- `normalized_levenshtein("hello", "world")` → low score
- `calculate_similarity` with matching assignee/project boosts overall score
- `DEDUP_CONFIDENCE_THRESHOLD` is 0.7

**Step 2: Run tests, verify failure**

Run: `cd backend && cargo test -p domain -- rules::dedup`

**Step 3: Implement**

The Levenshtein distance implementation is standard. Normalize by dividing by max string length.

**Step 4: Run tests, verify pass**

Run: `cd backend && cargo test -p domain -- rules::dedup`

**Step 5: Commit**

```bash
git add backend/crates/domain/src/rules/dedup.rs
git commit -m "feat(domain): implement deduplication scoring R08-R09 with tests"
```

---

### Task 10: Application layer — repository traits

**Files:**
- Create: `backend/crates/application/src/repositories/mod.rs`
- Create: `backend/crates/application/src/repositories/task_repository.rs`
- Create: `backend/crates/application/src/repositories/meeting_repository.rs`
- Create: `backend/crates/application/src/repositories/project_repository.rs`
- Create: `backend/crates/application/src/repositories/activity_repository.rs`
- Create: `backend/crates/application/src/repositories/alert_repository.rs`
- Create: `backend/crates/application/src/repositories/tag_repository.rs`
- Create: `backend/crates/application/src/repositories/task_link_repository.rs`
- Create: `backend/crates/application/src/repositories/sync_status_repository.rs`
- Create: `backend/crates/application/src/repositories/config_repository.rs`

**Step 1: Create all repository traits**

Copy trait definitions from SPEC_TECHNIQUE.md section 5.2.1. Each trait is an `#[async_trait]` trait with CRUD methods.

The `mod.rs` re-exports everything:

```rust
pub mod task_repository;
pub mod meeting_repository;
pub mod project_repository;
pub mod activity_repository;
pub mod alert_repository;
pub mod tag_repository;
pub mod task_link_repository;
pub mod sync_status_repository;
pub mod config_repository;

pub use task_repository::*;
pub use meeting_repository::*;
pub use project_repository::*;
pub use activity_repository::*;
pub use alert_repository::*;
pub use tag_repository::*;
pub use task_link_repository::*;
pub use sync_status_repository::*;
pub use config_repository::*;
```

All traits depend on types from the `domain` crate and `RepositoryError` from the application errors module.

**Step 2: Verify compiles**

Run: `cd backend && cargo check -p application`

**Step 3: Commit**

```bash
git add backend/crates/application/src/repositories/
git commit -m "feat(application): add repository trait definitions for all entities"
```

---

### Task 11: Application layer — service traits and errors

**Files:**
- Create: `backend/crates/application/src/services/mod.rs`
- Create: `backend/crates/application/src/services/jira_client.rs`
- Create: `backend/crates/application/src/services/outlook_client.rs`
- Create: `backend/crates/application/src/services/excel_client.rs`
- Create: `backend/crates/application/src/errors.rs`
- Create: `backend/crates/application/src/dto.rs`

**Step 1: Create service traits**

Copy from SPEC_TECHNIQUE.md section 5.2.2. Each trait defines the methods the infrastructure connectors must implement.

**Step 2: Create application errors**

Copy `AppError`, `RepositoryError`, `ConnectorError` from SPEC_TECHNIQUE.md section 5.2.4.

**Step 3: Create empty dto.rs**

```rust
// Data transfer objects for use cases — populated as use cases are implemented
```

**Step 4: Verify compiles**

Run: `cd backend && cargo check -p application`

**Step 5: Commit**

```bash
git add backend/crates/application/src/
git commit -m "feat(application): add service traits (Jira, Outlook, Excel), error types, and DTOs"
```

---

### Task 12: Application layer — use cases (stubs)

**Files:**
- Create: `backend/crates/application/src/use_cases/mod.rs`
- Create: `backend/crates/application/src/use_cases/dashboard.rs`
- Create: `backend/crates/application/src/use_cases/task_management.rs`
- Create: `backend/crates/application/src/use_cases/priority.rs`
- Create: `backend/crates/application/src/use_cases/activity_tracking.rs`
- Create: `backend/crates/application/src/use_cases/sync.rs`
- Create: `backend/crates/application/src/use_cases/deduplication.rs`
- Create: `backend/crates/application/src/use_cases/alerts.rs`
- Create: `backend/crates/application/src/use_cases/configuration.rs`

**Step 1: Create use case function signatures**

Create all use case functions with proper signatures matching SPEC_TECHNIQUE.md section 5.2.3. Body can be `todo!()` for now — they'll be implemented as each phase needs them.

Key use cases to stub with full signatures:
- `get_daily_dashboard` (dashboard.rs)
- `create_personal_task`, `update_task`, `delete_task`, `complete_task` (task_management.rs)
- `update_priority`, `reset_urgency` (priority.rs)
- `start_activity`, `stop_activity`, `update_activity_slot`, `get_activity_journal` (activity_tracking.rs)
- `sync_jira`, `sync_outlook`, `sync_excel`, `sync_all` (sync.rs)
- `run_deduplication`, `confirm_deduplication` (deduplication.rs)
- `run_alert_engine`, `resolve_alert` (alerts.rs)
- `get_configuration`, `update_configuration` (configuration.rs)

**Step 2: Verify compiles**

Run: `cd backend && cargo check -p application`

**Step 3: Commit**

```bash
git add backend/crates/application/src/use_cases/
git commit -m "feat(application): add use case function stubs for all MVP features"
```

---

### Task 13: Infrastructure — database connection and repository implementations

**Files:**
- Create: `backend/crates/infrastructure/src/database/mod.rs`
- Create: `backend/crates/infrastructure/src/database/connection.rs`
- Create: `backend/crates/infrastructure/src/database/task_repo.rs`
- Create: `backend/crates/infrastructure/src/database/meeting_repo.rs`
- Create: `backend/crates/infrastructure/src/database/project_repo.rs`
- Create: `backend/crates/infrastructure/src/database/activity_repo.rs`
- Create: `backend/crates/infrastructure/src/database/alert_repo.rs`
- Create: `backend/crates/infrastructure/src/database/tag_repo.rs`
- Create: `backend/crates/infrastructure/src/database/sync_status_repo.rs`
- Create: `backend/crates/infrastructure/src/database/config_repo.rs`

**Step 1: Create database connection module**

```rust
// database/connection.rs
use sqlx::sqlite::SqlitePool;

pub async fn create_sqlite_pool(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let pool = SqlitePool::connect(database_url).await?;
    sqlx::migrate!("../../../migrations/sqlite").run(&pool).await?;
    Ok(pool)
}
```

**Step 2: Implement repository structs**

Follow the pattern from SPEC_TECHNIQUE.md section 5.3.2. Each repo wraps a `SqlitePool` and implements the corresponding trait.

Start with `task_repo.rs` as the pattern:

```rust
use sqlx::SqlitePool;
use async_trait::async_trait;
use domain::types::*;
use application::repositories::*;
use application::errors::RepositoryError;

pub struct SqliteTaskRepository {
    pool: SqlitePool,
}

pub fn new_sqlite_task_repository(pool: SqlitePool) -> SqliteTaskRepository {
    SqliteTaskRepository { pool }
}

#[async_trait]
impl TaskRepository for SqliteTaskRepository {
    // Implement all methods using sqlx queries
    // Pattern: query -> map row to domain type -> return Result
}
```

Repeat for all repositories.

**Step 3: Write integration tests**

For each repository, write integration tests using an in-memory SQLite database (`sqlite::memory:`). Test CRUD operations, filtering, and edge cases.

**Step 4: Run tests**

Run: `cd backend && cargo test -p infrastructure`

**Step 5: Commit**

```bash
git add backend/crates/infrastructure/src/database/
git commit -m "feat(infrastructure): implement SQLite repository implementations with integration tests"
```

---

### Task 14: Infrastructure — connector stubs

**Files:**
- Create: `backend/crates/infrastructure/src/connectors/mod.rs`
- Create: `backend/crates/infrastructure/src/connectors/jira/mod.rs`
- Create: `backend/crates/infrastructure/src/connectors/jira/client.rs`
- Create: `backend/crates/infrastructure/src/connectors/jira/types.rs`
- Create: `backend/crates/infrastructure/src/connectors/jira/mapper.rs`
- Create: `backend/crates/infrastructure/src/connectors/outlook/mod.rs`
- Create: `backend/crates/infrastructure/src/connectors/outlook/client.rs`
- Create: `backend/crates/infrastructure/src/connectors/outlook/types.rs`
- Create: `backend/crates/infrastructure/src/connectors/outlook/mapper.rs`
- Create: `backend/crates/infrastructure/src/connectors/excel/mod.rs`
- Create: `backend/crates/infrastructure/src/connectors/excel/client.rs`
- Create: `backend/crates/infrastructure/src/connectors/excel/types.rs`
- Create: `backend/crates/infrastructure/src/connectors/excel/mapper.rs`

**Step 1: Create connector module structure with stubs**

Each connector has a `client.rs` (implements the service trait), `types.rs` (API response types), and `mapper.rs` (external -> domain mapping). Bodies are `todo!()` for now — implemented in Phase 4.

**Step 2: Create empty sync/dedup modules**

```rust
// sync/mod.rs
pub mod engine;
pub mod scheduler;

// dedup/mod.rs
pub mod engine;
```

**Step 3: Verify compiles**

Run: `cd backend && cargo check -p infrastructure`

**Step 4: Commit**

```bash
git add backend/crates/infrastructure/src/connectors/ backend/crates/infrastructure/src/sync/ backend/crates/infrastructure/src/dedup/
git commit -m "feat(infrastructure): add connector, sync, and dedup module stubs"
```

---

### Task 15: Update CLAUDE.md for new tech stack

**Files:**
- Modify: `CLAUDE.md`

**Step 1: Update CLAUDE.md**

Update the following sections:
- **Repository Structure**: Replace pnpm monorepo with Rust workspace + React frontend
- **Quick Reference Commands**: `cargo` commands for backend, `pnpm` for frontend only
- **Tech Stack table**: Add Rust, Axum, async-graphql, sqlx, SQLite; update frontend to urql, shadcn/ui, Tailwind
- **API Endpoints**: Replace REST with GraphQL
- **Path Aliases**: Remove TypeScript-specific aliases, add Rust crate references
- Keep all coding conventions that still apply (functional paradigm, immutability, no classes, DDD layers)

**Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md for Rust/Axum backend + GraphQL + urql frontend tech stack"
```

---

## Phase 2: Core API

### Task 16: GraphQL schema setup (Axum + async-graphql)

**Files:**
- Create: `backend/crates/api/src/graphql/mod.rs`
- Create: `backend/crates/api/src/graphql/schema.rs`
- Create: `backend/crates/api/src/graphql/query.rs`
- Create: `backend/crates/api/src/graphql/mutation.rs`
- Create: `backend/crates/api/src/graphql/subscription.rs`
- Create: `backend/crates/api/src/graphql/types/mod.rs`
- Create: `backend/crates/api/src/middleware/mod.rs`
- Create: `backend/crates/api/src/middleware/auth.rs`
- Create: `backend/crates/api/src/context.rs`
- Create: `backend/crates/api/src/state.rs`
- Modify: `backend/crates/api/src/main.rs`

**Step 1: Set up Axum server with GraphQL endpoint**

Implement `main.rs` following SPEC_TECHNIQUE.md section 5.4.1:
- Create SQLite pool
- Build repository instances
- Build GraphQL schema
- Set up Axum router with `/graphql` (POST) and `/graphql/sse` (GET) routes
- CORS permissive for local dev
- Auth middleware injects default UserId

**Step 2: Implement schema builder**

Follow SPEC_TECHNIQUE.md section 5.4.2 — register all repositories as context data.

**Step 3: Add basic query/mutation/subscription roots (empty)**

```rust
pub struct QueryRoot;
#[Object]
impl QueryRoot {
    async fn health(&self) -> bool { true }
}

pub struct MutationRoot;
#[Object]
impl MutationRoot {
    async fn noop(&self) -> bool { true }
}

pub struct SubscriptionRoot;
#[Subscription]
impl SubscriptionRoot {}
```

**Step 4: Verify server starts**

Run: `cd backend && cargo run -p api`
Expected: Server starts on port 3001, GraphQL playground accessible

**Step 5: Commit**

```bash
git add backend/crates/api/src/
git commit -m "feat(api): set up Axum server with async-graphql endpoint and auth middleware"
```

---

### Task 17: GraphQL types (async-graphql output types)

**Files:**
- Create: `backend/crates/api/src/graphql/types/task.rs`
- Create: `backend/crates/api/src/graphql/types/meeting.rs`
- Create: `backend/crates/api/src/graphql/types/project.rs`
- Create: `backend/crates/api/src/graphql/types/dashboard.rs`
- Create: `backend/crates/api/src/graphql/types/activity.rs`
- Create: `backend/crates/api/src/graphql/types/alert.rs`
- Create: `backend/crates/api/src/graphql/types/workload.rs`
- Create: `backend/crates/api/src/graphql/types/priority.rs`
- Create: `backend/crates/api/src/graphql/types/sync.rs`

**Step 1: Create GQL type wrappers**

Each domain type gets a GraphQL wrapper (e.g., `TaskGql`) with `#[Object]` impl that maps domain fields to GraphQL fields. Follow SPEC_TECHNIQUE.md section 8.1 for field names and computed fields like `quadrant`, `durationHours`, `halfDayConsumption`.

**Step 2: Create input types**

`CreateTaskInput`, `UpdateTaskInput`, `UpdateActivitySlotInput`, `TaskFilter` as `#[InputObject]` structs.

**Step 3: Verify compiles**

Run: `cd backend && cargo check -p api`

**Step 4: Commit**

```bash
git add backend/crates/api/src/graphql/types/
git commit -m "feat(api): add GraphQL type definitions and input types"
```

---

### Task 18: Task CRUD resolvers (TDD)

**Files:**
- Modify: `backend/crates/api/src/graphql/query.rs`
- Modify: `backend/crates/api/src/graphql/mutation.rs`
- Modify: `backend/crates/application/src/use_cases/task_management.rs`

**Step 1: Implement `create_personal_task` use case**

Follow SPEC_TECHNIQUE.md section 5.2.3 (use_cases/task_management.rs). Write integration tests that use mock repositories (or in-memory repos).

**Step 2: Add task query resolvers**

```rust
// In QueryRoot
async fn tasks(&self, ctx: &Context<'_>, filter: Option<TaskFilterInput>, first: Option<i32>, after: Option<String>) -> Result<TaskConnection> { ... }
async fn task(&self, ctx: &Context<'_>, id: ID) -> Result<Option<TaskGql>> { ... }
```

**Step 3: Add task mutation resolvers**

```rust
// In MutationRoot
async fn create_task(&self, ctx: &Context<'_>, input: CreateTaskInput) -> Result<TaskGql> { ... }
async fn update_task(&self, ctx: &Context<'_>, id: ID, input: UpdateTaskInput) -> Result<TaskGql> { ... }
async fn delete_task(&self, ctx: &Context<'_>, id: ID) -> Result<bool> { ... }
```

**Step 4: Add project and tag query resolvers**

```rust
async fn projects(&self, ctx: &Context<'_>) -> Result<Vec<ProjectGql>> { ... }
async fn tags(&self, ctx: &Context<'_>) -> Result<Vec<TagGql>> { ... }
```

**Step 5: Write integration tests**

Test full GraphQL queries against the running schema with in-memory SQLite.

**Step 6: Commit**

```bash
git add backend/
git commit -m "feat(api): implement task CRUD resolvers with use cases and tests"
```

---

### Task 19: Priority resolvers

**Files:**
- Modify: `backend/crates/api/src/graphql/query.rs`
- Modify: `backend/crates/api/src/graphql/mutation.rs`
- Implement: `backend/crates/application/src/use_cases/priority.rs`

**Step 1: Implement `priorityMatrix` query**

Groups all user tasks by quadrant using `determine_quadrant`.

**Step 2: Implement `updatePriority` and `resetUrgency` mutations**

**Step 3: Write tests and verify**

**Step 4: Commit**

```bash
git commit -m "feat(api): implement priority matrix query and updatePriority/resetUrgency mutations"
```

---

### Task 20: Frontend project setup

**Files:**
- Create: `frontend/package.json`
- Create: `frontend/tsconfig.json`
- Create: `frontend/vite.config.ts`
- Create: `frontend/tailwind.config.ts`
- Create: `frontend/postcss.config.js`
- Create: `frontend/index.html`
- Create: `frontend/codegen.ts`
- Create: `frontend/src/main.tsx`
- Create: `frontend/src/App.tsx`
- Create: `frontend/src/index.css` (Tailwind imports)
- Create: `frontend/src/lib/urql-client.ts`
- Create: `frontend/src/lib/date-utils.ts`
- Create: `frontend/src/lib/constants.ts`

**Step 1: Initialize React project**

```bash
cd frontend
pnpm init
pnpm add react react-dom react-router-dom urql graphql graphql-sse @urql/exchange-graphcache date-fns recharts @dnd-kit/core @dnd-kit/sortable
pnpm add -D typescript @types/react @types/react-dom vite @vitejs/plugin-react tailwindcss postcss autoprefixer vitest @testing-library/react @testing-library/jest-dom jsdom @graphql-codegen/cli @graphql-codegen/typescript @graphql-codegen/typescript-operations @graphql-codegen/typescript-urql
npx tailwindcss init -p
```

**Step 2: Set up shadcn/ui**

```bash
npx shadcn@latest init
```

Configure with Tailwind, New York style, default settings.

**Step 3: Create urql client**

Follow SPEC_TECHNIQUE.md section 6.1 — set up urql with SSE subscriptions.

**Step 4: Create App.tsx with router**

Follow SPEC_TECHNIQUE.md section 6.3 — set up routes for all pages (placeholder components initially).

**Step 5: Create codegen.ts**

Follow SPEC_TECHNIQUE.md section 6.2.

**Step 6: Verify dev server starts**

Run: `cd frontend && pnpm dev`
Expected: Vite dev server on port 3000 with blank React app

**Step 7: Commit**

```bash
git add frontend/
git commit -m "feat(frontend): set up React project with Vite, urql, Tailwind, shadcn/ui, and routing"
```

---

### Task 21: Frontend — basic task components

**Files:**
- Create: `frontend/src/components/layout/PageLayout.tsx`
- Create: `frontend/src/components/layout/Sidebar.tsx`
- Create: `frontend/src/components/layout/Header.tsx`
- Create: `frontend/src/components/task/TaskCard.tsx`
- Create: `frontend/src/components/task/TaskList.tsx`
- Create: `frontend/src/components/task/TaskForm.tsx`
- Create: `frontend/src/components/task/TaskQuickAdd.tsx`
- Create: `frontend/src/graphql/queries/tasks.graphql`
- Create: `frontend/src/graphql/mutations/task.graphql`

**Step 1: Create layout components**

PageLayout with Sidebar navigation and Header. Use shadcn/ui components for structure.

**Step 2: Define GraphQL operations**

```graphql
# queries/tasks.graphql
query Tasks($filter: TaskFilter, $first: Int, $after: String) {
  tasks(filter: $filter, first: $first, after: $after) {
    edges {
      node {
        id
        title
        description
        source
        status
        deadline
        urgency
        urgencyManual
        impact
        quadrant
        project { id name }
        tags { id name color }
      }
    }
    pageInfo { hasNextPage endCursor }
    totalCount
  }
}
```

```graphql
# mutations/task.graphql
mutation CreateTask($input: CreateTaskInput!) {
  createTask(input: $input) {
    id title status urgency impact quadrant
  }
}
```

**Step 3: Run codegen**

Run: `cd frontend && pnpm graphql-codegen`
(Requires backend running. If not, create types manually for now.)

**Step 4: Build TaskCard component**

Display source badge, title, priority indicator, deadline, assignee, project, tags. Use shadcn/ui Card component.

**Step 5: Build TaskList and TaskForm**

**Step 6: Write component tests**

Test rendering, props handling with vitest + @testing-library/react.

**Step 7: Commit**

```bash
git add frontend/src/
git commit -m "feat(frontend): add layout and task components with GraphQL operations"
```

---

## Phase 3: Dashboard

### Task 22: Dashboard query resolver

**Files:**
- Implement: `backend/crates/application/src/use_cases/dashboard.rs`
- Modify: `backend/crates/api/src/graphql/query.rs`

**Step 1: Implement `get_daily_dashboard` use case**

Follow SPEC_TECHNIQUE.md section 5.2.3 exactly. Fetches tasks, meetings, alerts, sync statuses for a given date. Computes weekly workload.

**Step 2: Add `dailyDashboard` query resolver**

**Step 3: Implement `weeklyWorkload` query resolver**

Computes half-day slots for the week, calculates consumption per slot, detects overload.

**Step 4: Write integration tests**

**Step 5: Commit**

```bash
git commit -m "feat(api): implement dailyDashboard and weeklyWorkload query resolvers"
```

---

### Task 23: DashboardPage

**Files:**
- Create: `frontend/src/pages/DashboardPage.tsx`
- Create: `frontend/src/components/meeting/MeetingCard.tsx`
- Create: `frontend/src/components/meeting/MeetingList.tsx`
- Create: `frontend/src/components/workload/WorkloadChart.tsx`
- Create: `frontend/src/components/alert/AlertPanel.tsx`
- Create: `frontend/src/components/alert/AlertBadge.tsx`
- Create: `frontend/src/components/sync/SyncStatusBar.tsx`
- Create: `frontend/src/hooks/use-dashboard.ts`
- Create: `frontend/src/graphql/queries/dashboard.graphql`

**Step 1: Define dashboard GraphQL query**

```graphql
query DailyDashboard($date: Date!) {
  dailyDashboard(date: $date) {
    date
    tasks { id title status urgency impact quadrant deadline project { name } }
    meetings { id title startTime endTime location durationHours }
    alerts { id alertType severity message resolved }
    weeklyWorkload { weekStart capacity totalPlanned totalMeetings overload
      halfDays { date halfDay consumption isFree meetings { title } tasks { title } }
    }
    syncStatuses { source status lastSyncAt }
  }
}
```

**Step 2: Build DashboardPage with 4 zones**

Following SPEC_TECHNIQUE.md section 6.4:
- Tasks of the day (TaskList sorted by priority)
- Meetings (MeetingList sorted by time)
- Weekly workload (WorkloadChart — Recharts stacked bar)
- Alerts (AlertPanel grouped by severity)
- SyncStatusBar at top
- Date navigation

**Step 3: Write tests**

**Step 4: Commit**

```bash
git commit -m "feat(frontend): implement DashboardPage with 4 zones, meetings, workload chart, alerts"
```

---

### Task 24: PriorityMatrixPage with drag-and-drop

**Files:**
- Create: `frontend/src/pages/PriorityMatrixPage.tsx`
- Create: `frontend/src/components/priority/PriorityGrid.tsx`
- Create: `frontend/src/components/priority/QuadrantColumn.tsx`
- Create: `frontend/src/hooks/use-priority-matrix.ts`
- Create: `frontend/src/graphql/queries/priority-matrix.graphql`
- Create: `frontend/src/graphql/mutations/priority.graphql`

**Step 1: Define GraphQL operations**

```graphql
query PriorityMatrix {
  priorityMatrix {
    urgentImportant { id title urgency impact deadline }
    important { id title urgency impact deadline }
    urgent { id title urgency impact deadline }
    neither { id title urgency impact deadline }
  }
}

mutation UpdatePriority($taskId: ID!, $urgency: Int, $impact: Int) {
  updatePriority(taskId: $taskId, urgency: $urgency, impact: $impact) {
    id urgency impact quadrant
  }
}
```

**Step 2: Build PriorityGrid with @dnd-kit**

2x2 grid of QuadrantColumn components. Dragging a task from one quadrant to another fires `UpdatePriority` mutation with appropriate urgency/impact values.

**Step 3: Write tests**

**Step 4: Commit**

```bash
git commit -m "feat(frontend): implement PriorityMatrixPage with drag-and-drop between quadrants"
```

---

### Task 25: WorkloadPage

**Files:**
- Create: `frontend/src/pages/WorkloadPage.tsx`
- Create: `frontend/src/components/workload/HalfDayGrid.tsx`
- Create: `frontend/src/components/workload/WeekNavigator.tsx`
- Create: `frontend/src/hooks/use-workload.ts`
- Create: `frontend/src/graphql/queries/workload.graphql`

**Step 1: Build WorkloadPage**

Following SPEC_TECHNIQUE.md section 6.4:
- WeekNavigator (prev/next week, today button)
- HalfDayGrid: 5 columns (Mon-Fri) x 2 rows (Morning/Afternoon)
- Color coding: green (free), yellow (partial), red (full/overloaded)
- WorkloadChart: Recharts bar chart showing capacity vs load

**Step 2: Write tests**

**Step 3: Commit**

```bash
git commit -m "feat(frontend): implement WorkloadPage with half-day grid and capacity chart"
```

---

## Phase 4: External Integrations

### Task 26: Jira connector implementation

**Files:**
- Implement: `backend/crates/infrastructure/src/connectors/jira/client.rs`
- Implement: `backend/crates/infrastructure/src/connectors/jira/types.rs`
- Implement: `backend/crates/infrastructure/src/connectors/jira/mapper.rs`
- Test: `backend/crates/infrastructure/tests/jira_client_test.rs`

**Step 1: Implement Jira API types**

Define Rust structs matching the Jira REST API v3 response format (issues search endpoint).

**Step 2: Implement mapper functions**

Follow SPEC_TECHNIQUE.md section 9.1 field mapping table. Map Jira status category to domain TaskStatus: "new"->Todo, "indeterminate"->InProgress, "done"->Done. Store raw `fields.status.name` as `jira_status`.

**Step 3: Implement JiraHttpClient**

Build JQL queries with configured project keys and assignees. Paginate with `maxResults=100`.

**Step 4: Write integration tests with wiremock**

Mock Jira API responses, test request building and response parsing.

**Step 5: Commit**

```bash
git commit -m "feat(infrastructure): implement Jira REST API connector with tests"
```

---

### Task 27: Microsoft Graph connector — Outlook

**Files:**
- Implement: `backend/crates/infrastructure/src/connectors/outlook/client.rs`
- Implement: `backend/crates/infrastructure/src/connectors/outlook/types.rs`
- Implement: `backend/crates/infrastructure/src/connectors/outlook/mapper.rs`

**Step 1: Implement Graph API types for calendar events**

**Step 2: Implement mapper**

Follow SPEC_TECHNIQUE.md section 9.2 calendar event mapping. Auto-detect project from meeting title (case-insensitive substring match against known project names).

**Step 3: Implement GraphOutlookClient**

Fetch `/me/calendarView?startDateTime=...&endDateTime=...`.

**Step 4: Write tests with wiremock**

**Step 5: Commit**

```bash
git commit -m "feat(infrastructure): implement Outlook calendar connector via Microsoft Graph API"
```

---

### Task 28: Microsoft Graph connector — Excel/SharePoint

**Files:**
- Implement: `backend/crates/infrastructure/src/connectors/excel/client.rs`
- Implement: `backend/crates/infrastructure/src/connectors/excel/types.rs`
- Implement: `backend/crates/infrastructure/src/connectors/excel/mapper.rs`

**Step 1: Implement Excel reading via Graph API**

Fetch `/sites/{site-id}/drive/items/{item-id}/workbook/worksheets/{sheet}/usedRange`. Parse using `ExcelMappingConfig` for column mapping.

**Step 2: Write tests**

**Step 3: Commit**

```bash
git commit -m "feat(infrastructure): implement Excel/SharePoint connector via Microsoft Graph API"
```

---

### Task 29: Sync engine

**Files:**
- Implement: `backend/crates/infrastructure/src/sync/engine.rs`
- Implement: `backend/crates/infrastructure/src/sync/scheduler.rs`
- Implement: `backend/crates/application/src/use_cases/sync.rs`

**Step 1: Implement sync flow**

Follow SPEC_TECHNIQUE.md section 10.2:
1. Update sync_status -> syncing
2. Fetch from external API
3. Transform -> domain types
4. Reconcile (insert/update/delete, preserving local overrides per section 10.5)
5. Run dedup engine
6. Run alert engine
7. Update sync_status -> success
8. Emit SSE events

**Step 2: Implement scheduler**

Use `tokio-cron-scheduler` for periodic sync (configurable, default 15 min).

**Step 3: Add `forceSync` mutation and `syncProgress` subscription**

**Step 4: Write tests**

**Step 5: Commit**

```bash
git commit -m "feat: implement sync engine with scheduler, forceSync mutation, and syncProgress subscription"
```

---

### Task 30: Settings page

**Files:**
- Create: `frontend/src/pages/SettingsPage.tsx`
- Create: `frontend/src/hooks/use-config.ts`
- Create: `frontend/src/graphql/queries/settings.graphql`
- Create: `frontend/src/graphql/mutations/config.graphql`
- Implement: `backend/crates/application/src/use_cases/configuration.rs`

**Step 1: Implement configuration use cases**

`get_configuration` and `update_configuration` backed by the config repository.

**Step 2: Add `configuration` query and `updateConfiguration` mutation resolvers**

**Step 3: Build SettingsPage**

Sections: Jira connection, Microsoft Graph, Excel mapping, sync frequency, weekly capacity, activity reminders, deadline alert threshold.

**Step 4: Build SyncStatusBar component**

Shows last sync time per source, manual sync button.

**Step 5: Commit**

```bash
git commit -m "feat: implement settings page with configuration management and sync status"
```

---

## Phase 5: Deduplication

### Task 31: Deduplication engine

**Files:**
- Implement: `backend/crates/infrastructure/src/dedup/engine.rs`
- Implement: `backend/crates/application/src/use_cases/deduplication.rs`

**Step 1: Implement dedup engine**

Follow SPEC_TECHNIQUE.md section 11.1:
1. Fetch Jira and Excel tasks
2. R08: Auto-merge when Jira key found in Excel row
3. R09: Calculate similarity for unlinked pairs, suggest above threshold
4. Create/update task_links

**Step 2: Write tests for auto-merge and similarity matching**

**Step 3: Commit**

```bash
git commit -m "feat: implement deduplication engine with auto-merge and similarity matching"
```

---

### Task 32: Deduplication resolvers and UI

**Files:**
- Modify: `backend/crates/api/src/graphql/query.rs` (add `deduplicationSuggestions`)
- Modify: `backend/crates/api/src/graphql/mutation.rs` (add `confirmDeduplication`, `linkTasks`, `unlinkTasks`)
- Create: `frontend/src/components/dedup/DeduplicationPanel.tsx`
- Create: `frontend/src/graphql/queries/dedup.graphql`
- Create: `frontend/src/graphql/mutations/dedup.graphql`

**Step 1: Add resolvers**

**Step 2: Build DeduplicationPanel**

Side-by-side comparison, confidence score, Accept/Reject buttons per suggestion.

**Step 3: Commit**

```bash
git commit -m "feat: implement deduplication UI with suggestions panel and merge/reject actions"
```

---

## Phase 6: Alerts

### Task 33: Alert engine

**Files:**
- Implement: `backend/crates/application/src/use_cases/alerts.rs`

**Step 1: Implement alert engine**

Follow SPEC_TECHNIQUE.md section 12.1:
1. Collect tasks, meetings, config
2. Run domain alert functions (deadline, overload, conflict)
3. Diff against existing alerts (insert new, auto-resolve stale)
4. Emit `alertsUpdated` subscription

**Step 2: Write tests**

**Step 3: Commit**

```bash
git commit -m "feat: implement alert engine with deadline, overload, and conflict detection"
```

---

### Task 34: Alert resolvers and UI

**Files:**
- Modify: `backend/crates/api/src/graphql/query.rs` (add `alerts` with pagination)
- Modify: `backend/crates/api/src/graphql/mutation.rs` (add `resolveAlert`)
- Modify: `backend/crates/api/src/graphql/subscription.rs` (add `alertsUpdated`)
- Modify: `frontend/src/components/alert/AlertPanel.tsx` (full implementation)

**Step 1: Add resolvers with pagination**

**Step 2: Add `alertsUpdated` subscription**

**Step 3: Update AlertPanel with resolve action**

**Step 4: Commit**

```bash
git commit -m "feat: implement alert resolvers, subscription, and AlertPanel UI"
```

---

## Phase 7: Activity Tracking

### Task 35: Activity tracking use cases and resolvers

**Files:**
- Implement: `backend/crates/application/src/use_cases/activity_tracking.rs`
- Modify: `backend/crates/api/src/graphql/query.rs` (add `activityJournal`, `currentActivity`)
- Modify: `backend/crates/api/src/graphql/mutation.rs` (add `startActivity`, `stopActivity`, `updateActivitySlot`, `deleteActivitySlot`)

**Step 1: Implement `start_activity` use case**

Follow SPEC_TECHNIQUE.md section 5.2.3:
- Close active slot (R21)
- Create new slot with half_day from hour
- Persist

**Step 2: Implement `stop_activity`, `update_activity_slot`, `get_activity_journal`**

**Step 3: Add all query and mutation resolvers**

**Step 4: Write tests**

**Step 5: Commit**

```bash
git commit -m "feat: implement activity tracking use cases and GraphQL resolvers"
```

---

### Task 36: Activity reminder subscription

**Files:**
- Modify: `backend/crates/api/src/graphql/subscription.rs` (add `activityReminder`)
- Implement: background task for post-meeting and periodic reminders

**Step 1: Implement reminder logic**

Follow SPEC_TECHNIQUE.md section 13:
- Post-meeting: check current time against meeting end times, emit reminder
- Periodic: configurable interval (default 2h), emit reminder
- Suppression: no reminders on weekends, outside working hours, or if recently switched

**Step 2: Wire up broadcast channel for `activityReminder` subscription**

**Step 3: Commit**

```bash
git commit -m "feat: implement activity reminder subscription with post-meeting and periodic triggers"
```

---

### Task 37: ActivityJournalPage

**Files:**
- Create: `frontend/src/pages/ActivityJournalPage.tsx`
- Create: `frontend/src/components/activity/ActivityTimeline.tsx`
- Create: `frontend/src/components/activity/ActivitySwitcher.tsx`
- Create: `frontend/src/components/activity/SlotEditor.tsx`
- Create: `frontend/src/hooks/use-activity.ts`
- Create: `frontend/src/graphql/queries/activity.graphql`
- Create: `frontend/src/graphql/mutations/activity.graphql`
- Create: `frontend/src/graphql/subscriptions/activity-reminder.graphql`

**Step 1: Build ActivityTimeline**

Vertical timeline with colored blocks per task. Gray blocks for untracked time. Day navigation.

**Step 2: Build SlotEditor**

Click slot to edit start/end time or change task. Add missing slot button.

**Step 3: Build ActivitySwitcher**

Lightweight popup triggered by subscription events (post-meeting, periodic) or manual button. List of in-progress tasks, "no task/break" option.

**Step 4: Wire up subscription for reminders**

**Step 5: Write tests**

**Step 6: Commit**

```bash
git commit -m "feat(frontend): implement ActivityJournalPage with timeline, switcher, and slot editor"
```

---

### Task 38: Meeting-project association

**Files:**
- Modify: `backend/crates/api/src/graphql/mutation.rs` (add `updateMeetingProject`)

**Step 1: Add `updateMeetingProject` mutation**

Allows user to manually associate a meeting with a project (overriding auto-detection).

**Step 2: Commit**

```bash
git commit -m "feat: add updateMeetingProject mutation for manual project association"
```

---

### Task 39: Tag management

**Files:**
- Modify: `backend/crates/api/src/graphql/mutation.rs` (add `createTag`, `updateTag`, `deleteTag`)

**Step 1: Implement tag CRUD mutations**

**Step 2: Commit**

```bash
git commit -m "feat: implement tag CRUD mutations"
```

---

## Phase 8: Final Integration and Polish

### Task 40: End-to-end smoke test

**Files:**
- Create: `e2e/smoke.spec.ts` (Playwright)

**Step 1: Set up Playwright**

```bash
cd frontend
pnpm add -D @playwright/test
npx playwright install
```

**Step 2: Write smoke test**

Test critical path: open dashboard, create task, drag in priority matrix, log activity.

**Step 3: Run test**

Run: `cd frontend && npx playwright test`

**Step 4: Commit**

```bash
git commit -m "test: add Playwright smoke test for critical user path"
```

---

### Task 41: Coverage check and cleanup

**Step 1: Run backend coverage**

Run: `cd backend && cargo install cargo-tarpaulin && cargo tarpaulin --workspace`
Target: 80% lines, branches, functions

**Step 2: Run frontend coverage**

Run: `cd frontend && pnpm vitest --coverage`
Target: 80%

**Step 3: Fill coverage gaps**

Add tests for under-covered areas.

**Step 4: Commit**

```bash
git commit -m "test: improve coverage to meet 80% threshold across backend and frontend"
```

---

### Task 42: Update root documentation

**Files:**
- Modify: `README.md`
- Verify: `CLAUDE.md` is up to date

**Step 1: Update README**

Document:
- New architecture (Rust backend + React frontend)
- Prerequisites (Rust toolchain, Node.js, pnpm)
- Setup and run instructions
- Environment variables
- Project structure overview

**Step 2: Commit**

```bash
git commit -m "docs: update README and CLAUDE.md for new Rust/React architecture"
```

---

## Summary

| Phase | Tasks | Key Deliverables |
|-------|-------|-----------------|
| 1. Foundation | 1-15 | Cargo workspace, domain types, business rules (TDD), repository traits, SQLite repos, migrations |
| 2. Core API | 16-21 | GraphQL server, task CRUD, priority resolvers, frontend setup with urql + shadcn/ui |
| 3. Dashboard | 22-25 | DashboardPage (4 zones), PriorityMatrixPage (drag-drop), WorkloadPage (chart + grid) |
| 4. External Integrations | 26-30 | Jira/Outlook/Excel connectors, sync engine, settings page |
| 5. Deduplication | 31-32 | Dedup engine, suggestion UI |
| 6. Alerts | 33-34 | Alert engine, alert panel with subscriptions |
| 7. Activity Tracking | 35-39 | Activity CRUD, reminders, ActivityJournalPage, tag management |
| 8. Polish | 40-42 | E2E tests, coverage, documentation |

**Total: 42 tasks across 8 phases.**
