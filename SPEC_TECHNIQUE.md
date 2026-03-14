# Technical Specification — Aggregated Plan

## Table of Contents

1. [Overview](#1-overview)
2. [Architecture Overview](#2-architecture-overview)
3. [Tech Stack](#3-tech-stack)
4. [Project Structure](#4-project-structure)
5. [Backend Architecture](#5-backend-architecture)
6. [Frontend Architecture](#6-frontend-architecture)
7. [Database Schema](#7-database-schema)
8. [GraphQL API](#8-graphql-api)
9. [External Integrations](#9-external-integrations)
10. [Synchronization Engine](#10-synchronization-engine)
11. [Deduplication Engine](#11-deduplication-engine)
12. [Alert Engine](#12-alert-engine)
13. [Activity Tracking](#13-activity-tracking)
14. [Authentication & Security](#14-authentication--security)
15. [Configuration](#15-configuration)
16. [Testing Strategy](#16-testing-strategy)
17. [Deployment](#17-deployment)
18. [MVP Scope](#18-mvp-scope)
19. [Coding Conventions](#19-coding-conventions)

---

## 1. Overview

### 1.1 Purpose

This document is the complete technical specification for **Aggregated Plan**, a personal cockpit for a Tech Lead managing 4-8 software projects with 5-15 people. It aggregates data from Jira, Microsoft Outlook, Excel (SharePoint), and Obsidian into a single dashboard with prioritization, activity tracking, and alerting capabilities.

This specification is self-contained. An implementation agent should be able to build the entire application from this document alone, using the functional specification (`SPEC_FONCTIONNELLE.md`) as the source of business requirements.

### 1.2 Key Constraints

| Constraint | Description |
|-----------|-------------|
| **Functional paradigm** | Pure functions, immutability, algebraic data types, `Result` types — no classes, no inheritance |
| **Multi-user ready** | `user_id` on all tables, auth middleware (no-op locally, Azure AD for Teams) |
| **Teams migration path** | Architecture must support future deployment as a Microsoft Teams Tab application |
| **Read-only integration** | The application never writes back to external sources (Jira, Outlook, Excel) |
| **Offline resilience** | The application remains functional with cached data when sources are unavailable |

### 1.3 Definitions

| Term | Definition |
|------|-----------|
| **Half-day** | Scheduling unit. Morning: 08:00-12:00, Afternoon: 13:00-17:00 |
| **Capacity** | Available half-days per week (default: 10) |
| **Workload** | Half-days consumed by planned tasks + meetings |
| **Source** | External system: Jira, Outlook, Excel, Obsidian |
| **Aggregated task** | A task in the application, possibly merged from multiple sources |
| **Week** | Monday to Friday (5 business days). Monday is the first day of the week. `week_start_of(date)` returns the Monday of the given date's week. |

---

## 2. Architecture Overview

### 2.1 High-Level Architecture

```
+------------------------------------------------------+
|                     Frontend                         |
|          React + TypeScript + urql + Shadcn/ui       |
|                                                      |
|   +----------+ +----------+ +----------+            |
|   |Dashboard | | Priority | | Activity |  ...       |
|   |  Page    | |  Matrix  | | Journal  |            |
|   +----+-----+ +----+-----+ +----+-----+            |
|        |             |            |                  |
|        +-------------+------------+                  |
|                      |                               |
|              urql GraphQL Client                     |
|         (Queries/Mutations + SSE Subscriptions)      |
+----------------------+-------------------------------+
                       | HTTP / SSE
+----------------------+-------------------------------+
|                     Backend                          |
|              Rust + Axum + async-graphql             |
|                                                      |
|  +---------------------------------------------+    |
|  |              API Layer (crate: api)          |    |
|  |     GraphQL Resolvers + Subscriptions        |    |
|  |     Axum HTTP Server + SSE Transport         |    |
|  +----------------------+-----------------------+    |
|  +----------------------+-----------------------+    |
|  |        Application Layer (crate: app)       |    |
|  |     Use Cases + Repository Traits           |    |
|  |     Service Traits (connectors)             |    |
|  +----------------------+-----------------------+    |
|  +----------------------+-----------------------+    |
|  |        Domain Layer (crate: domain)         |    |
|  |     Pure Types + Business Rules             |    |
|  |     Zero external dependencies              |    |
|  +---------------------------------------------+    |
|  +---------------------------------------------+    |
|  |     Infrastructure Layer (crate: infra)     |    |
|  |     SQLite/Postgres Repos + API Clients     |    |
|  |     Sync Engine + Dedup Engine              |    |
|  +----------------------+-----------------------+    |
+----------------------+-------------------------------+
                       |
          +------------+------------+
          |            |            |
     +----+----+ +-----+-----+ +---+------+
     |  Jira   | | Microsoft | |  SQLite  |
     |  REST   | | Graph API | | Database |
     |  API    | | (Outlook  | |          |
     |         | | +SharePt) | |          |
     +---------+ +-----------+ +----------+
```

### 2.2 Communication Patterns

| Pattern | Transport | Direction | Use Case |
|---------|-----------|-----------|----------|
| GraphQL Query | HTTP POST | Client -> Server | Fetch dashboard, tasks, workload |
| GraphQL Mutation | HTTP POST | Client -> Server | Create task, log activity, change priority |
| GraphQL Subscription | SSE | Server -> Client | Sync progress, activity reminders, alert updates |

### 2.3 Data Flow

```
External Sources --sync--> Infrastructure --transform--> Domain Types
                                                              |
                                                        --persist--> SQLite
                                                              |
                                                        --deduplicate--> Merged Tasks
                                                              |
                                                        --alert check--> Alerts
                                                              |
GraphQL Resolvers <--read--  Application Use Cases  <--query--+
       |
       +--> Frontend (urql cache + React state)
```

### 2.4 Layer Dependency Rules

These rules are enforced at compile time via Cargo workspace crate boundaries:

```
domain       ->  (no internal dependencies)
application  ->  domain
infrastructure -> domain, application
api          ->  domain, application, infrastructure
```

The **domain** crate has zero dependencies on other internal crates and zero external I/O dependencies. It contains only pure types and pure functions.

---

## 3. Tech Stack

### 3.1 Backend

| Component | Technology | Version | Purpose |
|-----------|-----------|---------|---------|
| Language | Rust | stable (latest) | Type safety, performance, functional paradigm |
| HTTP Framework | Axum | 0.7+ | Async web framework by Tokio team |
| GraphQL | async-graphql | 7+ | GraphQL server with SSE subscription support |
| Database Driver | sqlx | 0.8+ | Compile-time checked SQL, SQLite + Postgres support |
| HTTP Client | reqwest | 0.12+ | Jira and Microsoft Graph API calls |
| Async Runtime | tokio | 1.x | Async runtime for Axum and background tasks |
| Serialization | serde + serde_json | 1.x | JSON serialization/deserialization |
| Date/Time | chrono | 0.4+ | Date and time handling |
| UUID | uuid | 1.x | Unique identifier generation |
| Error Handling | thiserror | 1.x | Derive macro for error types |
| Logging | tracing + tracing-subscriber | 0.1+ | Structured logging |
| Environment | dotenvy | latest | .env file loading |
| CORS | tower-http | 0.5+ | CORS middleware for local dev |
| Task Scheduling | tokio-cron-scheduler | latest | Periodic sync scheduling |

### 3.2 Frontend

| Component | Technology | Version | Purpose |
|-----------|-----------|---------|---------|
| Language | TypeScript | 5.3+ | Strict mode, all strict flags enabled |
| UI Framework | React | 18+ | Component-based UI |
| Build Tool | Vite | 5+ | Fast dev server and build |
| GraphQL Client | urql | 4+ | Lightweight GraphQL client with SSE subscriptions |
| Subscriptions | graphql-sse | latest | SSE transport for GraphQL subscriptions |
| UI Components | shadcn/ui | latest | Accessible, customizable component library (Radix-based) |
| Styling | Tailwind CSS | 3+ | Utility-first CSS framework |
| Charts | Recharts | 2+ | Workload charts, retrospective visualizations |
| Drag and Drop | @dnd-kit/core + @dnd-kit/sortable | 6+ | Priority matrix drag-and-drop |
| Routing | react-router-dom | 6+ | Client-side routing |
| Date Utilities | date-fns | 3+ | Date formatting and calculations |
| Type Generation | @graphql-codegen/cli | latest | Generate TypeScript types from GraphQL schema |
| Testing | vitest + @testing-library/react | latest | Unit and component tests |
| E2E Testing | Playwright | latest | End-to-end browser tests |

### 3.3 Database

| Phase | Technology | Reason |
|-------|-----------|--------|
| Local (MVP) | SQLite | Zero setup, file-based, perfect for single-user local |
| Teams deployment | PostgreSQL | Multi-user, concurrent access, server deployment |

The transition is handled by **sqlx** which supports both SQLite and PostgreSQL via feature flags. SQL queries use the common subset of both dialects, with migration files per database engine where needed.

---

## 4. Project Structure

```
aggregated-plan/
|
+-- backend/                          # Rust workspace root
|   +-- Cargo.toml                    # Workspace definition
|   +-- .env.example                  # Environment variable template
|   |
|   +-- crates/
|       +-- domain/                   # Pure business logic
|       |   +-- Cargo.toml
|       |   +-- src/
|       |       +-- lib.rs
|       |       +-- types/            # Algebraic data types
|       |       |   +-- mod.rs
|       |       |   +-- task.rs
|       |       |   +-- meeting.rs
|       |       |   +-- project.rs
|       |       |   +-- activity.rs
|       |       |   +-- alert.rs
|       |       |   +-- tag.rs
|       |       |   +-- user.rs
|       |       |   +-- common.rs     # Source, HalfDay, etc.
|       |       +-- rules/            # Business rules as pure functions
|       |       |   +-- mod.rs
|       |       |   +-- urgency.rs    # R10-R15: urgency calculation
|       |       |   +-- priority.rs   # Quadrant classification, sorting
|       |       |   +-- workload.rs   # R01-R03: capacity, half-day consumption
|       |       |   +-- alerts.rs     # R16-R19: alert detection
|       |       |   +-- dedup.rs      # R08-R09: similarity scoring
|       |       +-- errors.rs         # Domain error types
|       |
|       +-- application/              # Use cases and trait definitions
|       |   +-- Cargo.toml
|       |   +-- src/
|       |       +-- lib.rs
|       |       +-- repositories/     # Repository trait definitions
|       |       |   +-- mod.rs
|       |       |   +-- task_repository.rs
|       |       |   +-- meeting_repository.rs
|       |       |   +-- project_repository.rs
|       |       |   +-- activity_repository.rs
|       |       |   +-- alert_repository.rs
|       |       |   +-- tag_repository.rs
|       |       |   +-- sync_status_repository.rs
|       |       |   +-- config_repository.rs
|       |       +-- services/         # External service trait definitions
|       |       |   +-- mod.rs
|       |       |   +-- jira_client.rs
|       |       |   +-- outlook_client.rs
|       |       |   +-- excel_client.rs
|       |       +-- use_cases/        # Application use case functions
|       |       |   +-- mod.rs
|       |       |   +-- dashboard.rs
|       |       |   +-- task_management.rs
|       |       |   +-- priority.rs
|       |       |   +-- activity_tracking.rs
|       |       |   +-- sync.rs
|       |       |   +-- deduplication.rs
|       |       |   +-- alerts.rs
|       |       |   +-- configuration.rs
|       |       +-- dto.rs            # Data transfer objects for use cases
|       |       +-- errors.rs         # Application error types
|       |
|       +-- infrastructure/           # Concrete implementations
|       |   +-- Cargo.toml
|       |   +-- src/
|       |       +-- lib.rs
|       |       +-- database/         # SQLite/Postgres repository implementations
|       |       |   +-- mod.rs
|       |       |   +-- connection.rs # Connection pool setup
|       |       |   +-- task_repo.rs
|       |       |   +-- meeting_repo.rs
|       |       |   +-- project_repo.rs
|       |       |   +-- activity_repo.rs
|       |       |   +-- alert_repo.rs
|       |       |   +-- tag_repo.rs
|       |       |   +-- sync_status_repo.rs
|       |       |   +-- config_repo.rs
|       |       +-- connectors/       # External API clients
|       |       |   +-- mod.rs
|       |       |   +-- jira/
|       |       |   |   +-- mod.rs
|       |       |   |   +-- client.rs
|       |       |   |   +-- types.rs  # Jira API response types
|       |       |   |   +-- mapper.rs # Jira -> domain type mapping
|       |       |   +-- outlook/
|       |       |   |   +-- mod.rs
|       |       |   |   +-- client.rs
|       |       |   |   +-- types.rs
|       |       |   |   +-- mapper.rs
|       |       |   +-- excel/
|       |       |       +-- mod.rs
|       |       |       +-- client.rs
|       |       |       +-- types.rs
|       |       |       +-- mapper.rs
|       |       +-- sync/             # Synchronization engine
|       |       |   +-- mod.rs
|       |       |   +-- engine.rs
|       |       |   +-- scheduler.rs
|       |       +-- dedup/            # Deduplication engine
|       |           +-- mod.rs
|       |           +-- engine.rs
|       |
|       +-- api/                      # HTTP + GraphQL server
|           +-- Cargo.toml
|           +-- src/
|               +-- main.rs           # Entry point: Axum server setup
|               +-- graphql/
|               |   +-- mod.rs
|               |   +-- schema.rs     # Schema construction
|               |   +-- query.rs      # Root query resolvers
|               |   +-- mutation.rs   # Root mutation resolvers
|               |   +-- subscription.rs # Root subscription resolvers
|               |   +-- types/        # GraphQL type definitions
|               |       +-- mod.rs
|               |       +-- task.rs
|               |       +-- meeting.rs
|               |       +-- project.rs
|               |       +-- dashboard.rs
|               |       +-- activity.rs
|               |       +-- alert.rs
|               |       +-- workload.rs
|               |       +-- priority.rs
|               |       +-- sync.rs
|               +-- middleware/
|               |   +-- mod.rs
|               |   +-- auth.rs       # Auth middleware (no-op locally)
|               +-- context.rs        # Request context (user_id extraction)
|               +-- state.rs          # Application state (repos, services)
|
+-- frontend/                         # React application
|   +-- package.json
|   +-- tsconfig.json
|   +-- vite.config.ts
|   +-- tailwind.config.ts
|   +-- codegen.ts                    # GraphQL codegen configuration
|   +-- index.html
|   |
|   +-- src/
|       +-- main.tsx                  # Entry point
|       +-- App.tsx                   # Router setup
|       |
|       +-- lib/                      # Utilities and setup
|       |   +-- urql-client.ts        # urql client configuration
|       |   +-- date-utils.ts         # Date formatting helpers
|       |   +-- constants.ts          # Application constants
|       |
|       +-- generated/                # Auto-generated (graphql-codegen)
|       |   +-- graphql.ts            # TypeScript types + operation hooks
|       |
|       +-- graphql/                  # GraphQL operation definitions
|       |   +-- queries/
|       |   |   +-- dashboard.graphql
|       |   |   +-- tasks.graphql
|       |   |   +-- priority-matrix.graphql
|       |   |   +-- workload.graphql
|       |   |   +-- activity.graphql
|       |   |   +-- alerts.graphql
|       |   |   +-- projects.graphql
|       |   +-- mutations/
|       |   |   +-- task.graphql
|       |   |   +-- priority.graphql
|       |   |   +-- activity.graphql
|       |   |   +-- alert.graphql
|       |   |   +-- dedup.graphql
|       |   |   +-- sync.graphql
|       |   |   +-- config.graphql
|       |   +-- subscriptions/
|       |       +-- sync-progress.graphql
|       |       +-- activity-reminder.graphql
|       |       +-- alerts-updated.graphql
|       |
|       +-- hooks/                    # Custom React hooks
|       |   +-- use-dashboard.ts
|       |   +-- use-tasks.ts
|       |   +-- use-priority-matrix.ts
|       |   +-- use-workload.ts
|       |   +-- use-activity.ts
|       |   +-- use-alerts.ts
|       |   +-- use-sync.ts
|       |   +-- use-config.ts
|       |
|       +-- pages/                    # Page-level components
|       |   +-- DashboardPage.tsx
|       |   +-- PriorityMatrixPage.tsx
|       |   +-- WorkloadPage.tsx
|       |   +-- ActivityJournalPage.tsx
|       |   +-- SettingsPage.tsx
|       |   +-- TeamPage.tsx          # v2
|       |   +-- ProjectPage.tsx       # v2
|       |   +-- RetrospectivePage.tsx  # v2
|       |
|       +-- components/               # Reusable UI components
|           +-- layout/
|           |   +-- Sidebar.tsx
|           |   +-- Header.tsx
|           |   +-- PageLayout.tsx
|           +-- task/
|           |   +-- TaskCard.tsx
|           |   +-- TaskList.tsx
|           |   +-- TaskForm.tsx
|           |   +-- TaskQuickAdd.tsx
|           +-- meeting/
|           |   +-- MeetingCard.tsx
|           |   +-- MeetingList.tsx
|           +-- priority/
|           |   +-- PriorityGrid.tsx
|           |   +-- QuadrantColumn.tsx
|           +-- workload/
|           |   +-- WorkloadChart.tsx
|           |   +-- HalfDayGrid.tsx
|           |   +-- WeekNavigator.tsx
|           +-- activity/
|           |   +-- ActivityTimeline.tsx
|           |   +-- ActivitySwitcher.tsx
|           |   +-- SlotEditor.tsx
|           +-- alert/
|           |   +-- AlertBadge.tsx
|           |   +-- AlertPanel.tsx
|           +-- sync/
|           |   +-- SyncStatusBar.tsx
|           +-- dedup/
|               +-- DeduplicationPanel.tsx
|
+-- migrations/                       # Database migrations (sqlx)
|   +-- sqlite/
|   |   +-- 001_initial.sql
|   +-- postgres/
|       +-- 001_initial.sql
|
+-- docs/
|   +-- plans/                        # Design documents
|
+-- SPEC_FONCTIONNELLE.md             # Functional specification (French)
+-- SPEC_TECHNIQUE.md                 # This file
+-- README.md
```

### 4.1 Cargo Workspace Configuration

```toml
# backend/Cargo.toml
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

```toml
# backend/crates/domain/Cargo.toml
[package]
name = "domain"
version = "0.1.0"
edition = "2021"

[dependencies]
chrono = { workspace = true }
serde = { workspace = true }
uuid = { workspace = true }
# NO other crate dependencies -- this is enforced
```

```toml
# backend/crates/application/Cargo.toml
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

```toml
# backend/crates/infrastructure/Cargo.toml
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
```

```toml
# backend/crates/api/Cargo.toml
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

---

## 5. Backend Architecture

### 5.1 Domain Layer (`crates/domain`)

The domain layer contains **only** pure types and pure functions. It has zero I/O, zero async, and zero dependencies on other internal crates.

#### 5.1.1 Core Types (Algebraic Data Types)

All types are **immutable structs** and **enums**. No methods attached to types -- all logic is in free functions.

```rust
// types/common.rs

use chrono::{NaiveDate, DateTime, Utc};
use serde::{Serialize, Deserialize};
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
pub enum TrackingState {
    Inbox,      // Newly synced, not yet triaged
    Followed,   // User chose to track this task
    Dismissed,  // User chose to ignore this task
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskLinkType {
    AutoMerged,
    ManualMerged,
    Rejected,
}
```

```rust
// types/task.rs

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
    pub tracking_state: TrackingState,
    pub jira_remaining_seconds: Option<i32>,        // From Jira timeestimate
    pub jira_original_estimate_seconds: Option<i32>, // From Jira timeoriginalestimate
    pub jira_time_spent_seconds: Option<i32>,       // From Jira timespent
    pub remaining_hours_override: Option<f32>,       // Local override for remaining time
    pub estimated_hours_override: Option<f32>,       // Local override for estimated time
    pub tags: Vec<TagId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Task {
    /// Effective remaining hours: local override > Jira remaining > None
    pub fn effective_remaining_hours(&self) -> Option<f32> {
        self.remaining_hours_override
            .or(self.jira_remaining_seconds.map(|s| s as f32 / 3600.0))
    }

    /// Effective estimated hours: local override > Jira estimate > estimated_hours
    pub fn effective_estimated_hours(&self) -> Option<f32> {
        self.estimated_hours_override
            .or(self.jira_original_estimate_seconds.map(|s| s as f32 / 3600.0))
            .or(self.estimated_hours)
    }
}
```

```rust
// types/meeting.rs

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

```rust
// types/project.rs

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

```rust
// types/activity.rs

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

```rust
// types/alert.rs

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

```rust
// types/tag.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: TagId,
    pub user_id: UserId,
    pub name: String,
    pub color: Option<String>,
}
```

```rust
// types/user.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub name: String,
    pub email: String,
    pub created_at: DateTime<Utc>,
}
```

#### 5.1.2 Business Rules (Pure Functions)

All business rules from the functional spec (R01-R26) are implemented as **pure functions** -- no I/O, no side effects, fully testable with simple input/output assertions.

**Urgency calculation (R10-R15):**

```rust
// rules/urgency.rs

/// R10-R14: Calculate urgency from deadline relative to today.
/// Pure function: deadline x today -> UrgencyLevel
pub fn calculate_urgency(deadline: Option<NaiveDate>, today: NaiveDate) -> UrgencyLevel {
    match deadline {
        None => UrgencyLevel::Low,                                // R10
        Some(d) => {
            let business_days = count_business_days(today, d);
            match business_days {
                n if n < 0 => UrgencyLevel::Critical,             // R14: overdue
                0..=1 => UrgencyLevel::High,                      // R13
                2..=5 => UrgencyLevel::Medium,                    // R12
                _ => UrgencyLevel::Low,                           // R11
            }
        }
    }
}

/// R15: Resolve urgency -- manual override takes precedence.
pub fn resolve_urgency(
    manual_urgency: Option<UrgencyLevel>,
    deadline: Option<NaiveDate>,
    today: NaiveDate,
) -> (UrgencyLevel, bool) {
    match manual_urgency {
        Some(u) => (u, true),                                     // R15: manual prevails
        None => (calculate_urgency(deadline, today), false),
    }
}

/// Count business days between two dates (excluding weekends).
/// Negative if target is in the past.
pub fn count_business_days(from: NaiveDate, to: NaiveDate) -> i64 {
    // Implementation: iterate days, skip Saturday/Sunday
    // Return positive if to > from, negative if to < from
}
```

**Priority and quadrant (sorting rules):**

```rust
// rules/priority.rs

/// Classify a task into a priority quadrant based on urgency and impact.
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

/// Sort tasks by priority: UrgentImportant > Important > Urgent > Neither.
/// Within the same quadrant, sort by closest deadline first.
pub fn sort_tasks_by_priority(tasks: &mut [Task]) {
    tasks.sort_by(|a, b| {
        let qa = determine_quadrant(a.urgency, a.impact);
        let qb = determine_quadrant(b.urgency, b.impact);
        qa.cmp(&qb).then_with(|| a.deadline.cmp(&b.deadline))
    });
}
```

**Workload calculation (R01-R03):**

```rust
// rules/workload.rs

/// Calculate hours consumed by a meeting.
pub fn meeting_hours(start: DateTime<Utc>, end: DateTime<Utc>) -> f64 {
    (end - start).num_minutes() as f64 / 60.0
}

/// R16: Detect overload for a week.
/// Returns Some(excess_hours) if total load exceeds capacity, None otherwise.
pub fn detect_overload(
    planned_task_hours: f64,
    meeting_hours: f64,
    weekly_capacity_hours: f64,
) -> Option<f64> {
    let total = planned_task_hours + meeting_hours;
    if total > weekly_capacity_hours { Some(total - weekly_capacity_hours) } else { None }
}

/// Determine which half-day a datetime falls into.
pub fn half_day_of(hour: u32) -> HalfDay {
    if hour < 13 { HalfDay::Morning } else { HalfDay::Afternoon }
}
```

**Alert detection (R16-R19):**

```rust
// rules/alerts.rs

/// Data needed to generate an alert (not yet persisted).
pub struct AlertData {
    pub alert_type: AlertType,
    pub severity: AlertSeverity,
    pub message: String,
    pub related_items: Vec<RelatedItem>,
    pub date: NaiveDate,
}

/// R17: Check all tasks for approaching or past deadlines.
pub fn check_deadline_alerts(
    tasks: &[Task],
    today: NaiveDate,
    threshold_days: i64,
) -> Vec<AlertData> {
    tasks.iter().filter_map(|task| {
        let deadline = task.deadline?;
        let days_remaining = count_business_days(today, deadline);
        if days_remaining < 0 {
            Some(AlertData {
                alert_type: AlertType::Deadline,
                severity: AlertSeverity::Critical,
                message: format!("Task '{}' is overdue by {} day(s)", task.title, -days_remaining),
                related_items: vec![RelatedItem::Task(task.id)],
                date: today,
            })
        } else if days_remaining <= threshold_days {
            Some(AlertData {
                alert_type: AlertType::Deadline,
                severity: AlertSeverity::Warning,
                message: format!("Task '{}' is due in {} day(s)", task.title, days_remaining),
                related_items: vec![RelatedItem::Task(task.id)],
                date: today,
            })
        } else {
            None
        }
    }).collect()
}

/// R18: Check for scheduling conflicts on a given date.
/// A conflict occurs when two items have overlapping time ranges.
/// Overlap condition: start_a < end_b AND start_b < end_a
pub fn check_conflict_alerts(
    scheduled_items: &[ScheduledItem],
    date: NaiveDate,
) -> Vec<AlertData> {
    // For each pair of items, check if [start_a, end_a) overlaps [start_b, end_b)
}

pub enum ScheduledItem {
    Task { id: TaskId, title: String, start: DateTime<Utc>, end: DateTime<Utc> },
    Meeting { id: MeetingId, title: String, start: DateTime<Utc>, end: DateTime<Utc> },
}

/// R16: Check overload for the week.
pub fn check_overload_alerts(
    planned_half_days: f64,
    meeting_half_days: f64,
    weekly_capacity: u32,
    week_start: NaiveDate,
) -> Option<AlertData> {
    detect_overload(planned_half_days, meeting_half_days, weekly_capacity).map(|excess| {
        let severity = if excess > 2.0 { AlertSeverity::Critical } else { AlertSeverity::Warning };
        AlertData {
            alert_type: AlertType::Overload,
            severity,
            message: format!("Overloaded by {:.1} half-day(s) this week", excess),
            related_items: vec![],
            date: week_start,
        }
    })
}
```

**Deduplication scoring (R08-R09):**

```rust
// rules/dedup.rs

pub struct SimilarityScore {
    pub title_score: f64,       // 0.0 to 1.0 (normalized Levenshtein)
    pub assignee_match: bool,
    pub project_match: bool,
    pub overall: f64,           // Weighted composite: 0.0 to 1.0
}

/// R08: Check if a Jira ticket key appears in an arbitrary string (Excel row data).
pub fn find_jira_key_in_text(jira_key: &str, text: &str) -> bool {
    text.contains(jira_key)
}

/// R09: Calculate similarity between two tasks for potential deduplication.
/// Weights: title similarity (60%), assignee match (20%), project match (20%).
pub fn calculate_similarity(
    title_a: &str,
    title_b: &str,
    assignee_a: Option<&str>,
    assignee_b: Option<&str>,
    project_a: Option<&str>,
    project_b: Option<&str>,
) -> SimilarityScore {
    let title_score = normalized_levenshtein(title_a, title_b);
    let assignee_match = match (assignee_a, assignee_b) {
        (Some(a), Some(b)) => a.to_lowercase() == b.to_lowercase(),
        _ => false,
    };
    let project_match = match (project_a, project_b) {
        (Some(a), Some(b)) => a.to_lowercase() == b.to_lowercase(),
        _ => false,
    };
    let overall = title_score * 0.6
        + if assignee_match { 0.2 } else { 0.0 }
        + if project_match { 0.2 } else { 0.0 };

    SimilarityScore { title_score, assignee_match, project_match, overall }
}

/// Normalized Levenshtein distance: 1.0 = identical, 0.0 = completely different.
pub fn normalized_levenshtein(a: &str, b: &str) -> f64 {
    // Implementation: standard Levenshtein, normalized by max(len(a), len(b))
}

/// Dedup confidence threshold: suggestions above this score are shown to the user.
pub const DEDUP_CONFIDENCE_THRESHOLD: f64 = 0.7;
```

#### 5.1.3 Domain Errors

```rust
// errors.rs

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

#### 5.1.4 Result Type

```rust
// lib.rs

pub type DomainResult<T> = Result<T, DomainError>;
```

### 5.2 Application Layer (`crates/application`)

The application layer defines **repository traits** (interfaces) and **use case functions**. It depends only on the domain layer.

#### 5.2.1 Repository Traits

Each repository trait is defined as an async trait. Implementations live in the infrastructure layer.

```rust
// repositories/task_repository.rs

use async_trait::async_trait;
use domain::types::*;

pub struct TaskFilter {
    pub status: Option<Vec<TaskStatus>>,
    pub source: Option<Vec<Source>>,
    pub project_id: Option<ProjectId>,
    pub assignee: Option<String>,
    pub deadline_before: Option<NaiveDate>,
    pub deadline_after: Option<NaiveDate>,
    pub tag_ids: Option<Vec<TagId>>,
}

#[async_trait]
pub trait TaskRepository: Send + Sync {
    async fn find_by_id(&self, id: TaskId) -> Result<Option<Task>, RepositoryError>;
    async fn find_by_user(
        &self, user_id: UserId, filter: &TaskFilter,
    ) -> Result<Vec<Task>, RepositoryError>;
    async fn find_by_source(
        &self, user_id: UserId, source: Source, source_id: &str,
    ) -> Result<Option<Task>, RepositoryError>;
    async fn find_by_date_range(
        &self, user_id: UserId, start: NaiveDate, end: NaiveDate,
    ) -> Result<Vec<Task>, RepositoryError>;
    async fn save(&self, task: &Task) -> Result<(), RepositoryError>;
    async fn save_batch(&self, tasks: &[Task]) -> Result<(), RepositoryError>;
    async fn delete(&self, id: TaskId) -> Result<(), RepositoryError>;
}
```

```rust
// repositories/meeting_repository.rs

#[async_trait]
pub trait MeetingRepository: Send + Sync {
    async fn find_by_user_and_date(
        &self, user_id: UserId, date: NaiveDate,
    ) -> Result<Vec<Meeting>, RepositoryError>;
    async fn find_by_user_and_range(
        &self, user_id: UserId, start: NaiveDate, end: NaiveDate,
    ) -> Result<Vec<Meeting>, RepositoryError>;
    async fn upsert_batch(&self, meetings: &[Meeting]) -> Result<(), RepositoryError>;
    async fn delete_stale(
        &self, user_id: UserId, current_outlook_ids: &[String],
    ) -> Result<u64, RepositoryError>;
    async fn find_by_project(
        &self, user_id: UserId, project_id: ProjectId,
    ) -> Result<Vec<Meeting>, RepositoryError>;
}
```

```rust
// repositories/activity_repository.rs

#[async_trait]
pub trait ActivitySlotRepository: Send + Sync {
    async fn find_by_user_and_date(
        &self, user_id: UserId, date: NaiveDate,
    ) -> Result<Vec<ActivitySlot>, RepositoryError>;
    async fn find_active(
        &self, user_id: UserId,
    ) -> Result<Option<ActivitySlot>, RepositoryError>;
    async fn save(&self, slot: &ActivitySlot) -> Result<(), RepositoryError>;
    async fn update(&self, slot: &ActivitySlot) -> Result<(), RepositoryError>;
    async fn delete(&self, id: ActivitySlotId) -> Result<(), RepositoryError>;
}
```

```rust
// repositories/task_link_repository.rs

#[async_trait]
pub trait TaskLinkRepository: Send + Sync {
    async fn find_by_user(&self, user_id: UserId) -> Result<Vec<TaskLink>, RepositoryError>;
    async fn find_rejected_pairs(
        &self, user_id: UserId,
    ) -> Result<Vec<(TaskId, TaskId)>, RepositoryError>;
    async fn save(&self, link: &TaskLink) -> Result<(), RepositoryError>;
    async fn delete(&self, id: TaskLinkId) -> Result<(), RepositoryError>;
}
```

Similar traits for: `ProjectRepository`, `AlertRepository`, `TagRepository`, `SyncStatusRepository`, `ConfigRepository`.

#### 5.2.2 External Service Traits

```rust
// services/jira_client.rs

pub struct JiraTask {
    pub key: String,           // e.g., "PROJ-123"
    pub title: String,
    pub description: Option<String>,
    pub status: String,        // Raw Jira status name
    pub assignee: Option<String>,
    pub deadline: Option<NaiveDate>,
    pub priority: Option<String>,
    pub project_key: String,
    pub project_name: String,
}

#[async_trait]
pub trait JiraClient: Send + Sync {
    async fn fetch_tasks(
        &self, project_keys: &[String], assignees: Option<&[String]>,
    ) -> Result<Vec<JiraTask>, ConnectorError>;
}
```

```rust
// services/outlook_client.rs

pub struct OutlookEvent {
    pub outlook_id: String,
    pub title: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub location: Option<String>,
    pub participants: Vec<String>,
    pub is_cancelled: bool,
}

#[async_trait]
pub trait OutlookClient: Send + Sync {
    async fn fetch_calendar(
        &self, start: NaiveDate, end: NaiveDate,
    ) -> Result<Vec<OutlookEvent>, ConnectorError>;
}
```

```rust
// services/excel_client.rs

pub struct ExcelRow {
    pub row_index: usize,
    pub columns: HashMap<String, String>,   // column_name -> cell_value
}

pub struct ExcelMappingConfig {
    pub sharepoint_path: String,
    pub sheet_name: Option<String>,
    pub title_column: String,
    pub assignee_column: Option<String>,
    pub project_column: Option<String>,
    pub date_column: Option<String>,
    pub jira_key_column: Option<String>,
    pub status_column: Option<String>,
}

#[async_trait]
pub trait ExcelClient: Send + Sync {
    async fn fetch_rows(
        &self, config: &ExcelMappingConfig,
    ) -> Result<Vec<ExcelRow>, ConnectorError>;
}
```

#### 5.2.3 Use Cases

Use cases are **async functions** that compose repository calls with domain logic. They receive trait references (dependency injection via function arguments).

```rust
// use_cases/dashboard.rs

pub struct DailyDashboard {
    pub date: NaiveDate,
    pub tasks: Vec<Task>,
    pub meetings: Vec<Meeting>,
    pub alerts: Vec<Alert>,
    pub weekly_workload: WeeklyWorkload,
    pub sync_statuses: Vec<SyncStatus>,
}

pub async fn get_daily_dashboard(
    task_repo: &dyn TaskRepository,
    meeting_repo: &dyn MeetingRepository,
    alert_repo: &dyn AlertRepository,
    sync_repo: &dyn SyncStatusRepository,
    user_id: UserId,
    date: NaiveDate,
) -> Result<DailyDashboard, AppError> {
    let tasks = task_repo.find_by_user(user_id, &TaskFilter::for_date(date)).await?;
    let mut sorted_tasks = tasks;
    sort_tasks_by_priority(&mut sorted_tasks);

    let meetings = meeting_repo.find_by_user_and_date(user_id, date).await?;
    let alerts = alert_repo.find_unresolved(user_id).await?;
    let sync_statuses = sync_repo.find_by_user(user_id).await?;

    let week_start = week_start_of(date);
    let weekly_workload = compute_weekly_workload(
        task_repo, meeting_repo, user_id, week_start,
    ).await?;

    Ok(DailyDashboard {
        date, tasks: sorted_tasks, meetings, alerts, weekly_workload, sync_statuses,
    })
}
```

```rust
// use_cases/task_management.rs

pub struct CreateTaskInput {
    pub title: String,
    pub description: Option<String>,
    pub project_id: Option<ProjectId>,
    pub deadline: Option<NaiveDate>,
    pub planned_start: Option<DateTime<Utc>>,
    pub planned_end: Option<DateTime<Utc>>,
    pub estimated_hours: Option<f32>,
    pub impact: Option<ImpactLevel>,
    pub urgency: Option<UrgencyLevel>,
    pub tags: Vec<TagId>,
}

pub async fn create_personal_task(
    task_repo: &dyn TaskRepository,
    user_id: UserId,
    input: CreateTaskInput,
    today: NaiveDate,
) -> Result<Task, AppError> {
    let (urgency, urgency_manual) = resolve_urgency(
        input.urgency,
        input.deadline,
        today,
    );

    let task = Task {
        id: Uuid::new_v4(),
        user_id,
        title: input.title,
        description: input.description,
        source: Source::Personal,
        source_id: None,
        status: TaskStatus::Todo,
        project_id: input.project_id,
        assignee: None,
        deadline: input.deadline,
        planned_start: input.planned_start,
        planned_end: input.planned_end,
        estimated_hours: input.estimated_hours,
        urgency,
        urgency_manual,
        impact: input.impact.unwrap_or(ImpactLevel::Medium),
        tags: input.tags,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    task_repo.save(&task).await?;
    Ok(task)
}

pub async fn update_task(/* ... */) -> Result<Task, AppError> { /* ... */ }
pub async fn delete_task(/* ... */) -> Result<(), AppError> { /* ... */ }
pub async fn complete_task(/* ... */) -> Result<Task, AppError> { /* ... */ }
```

```rust
// use_cases/activity_tracking.rs

/// Start tracking a new activity. Closes the currently active slot (if any).
pub async fn start_activity(
    activity_repo: &dyn ActivitySlotRepository,
    user_id: UserId,
    task_id: Option<TaskId>,
    now: DateTime<Utc>,
) -> Result<ActivitySlot, AppError> {
    // Close active slot (R21)
    if let Some(mut active) = activity_repo.find_active(user_id).await? {
        active.end_time = Some(now);
        activity_repo.update(&active).await?;
    }

    let date = now.date_naive();
    let half_day = half_day_of(now.hour());

    let slot = ActivitySlot {
        id: Uuid::new_v4(),
        user_id,
        task_id,
        start_time: now,
        end_time: None,
        half_day,
        date,
        created_at: now,
    };

    activity_repo.save(&slot).await?;
    Ok(slot)
}

pub async fn stop_activity(/* ... */) -> Result<Option<ActivitySlot>, AppError> { /* ... */ }
pub async fn update_activity_slot(/* ... */) -> Result<ActivitySlot, AppError> { /* ... */ }
pub async fn get_activity_journal(/* ... */) -> Result<Vec<ActivitySlot>, AppError> { /* ... */ }
```

```rust
// use_cases/sync.rs

pub struct SyncResult {
    pub source: Source,
    pub tasks_created: usize,
    pub tasks_updated: usize,
    pub tasks_removed: usize,
    pub meetings_synced: usize,
    pub errors: Vec<String>,
}

pub async fn sync_jira(
    jira_client: &dyn JiraClient,
    task_repo: &dyn TaskRepository,
    project_repo: &dyn ProjectRepository,
    sync_repo: &dyn SyncStatusRepository,
    user_id: UserId,
    config: &JiraConfig,
) -> Result<SyncResult, AppError> { /* ... */ }

pub async fn sync_outlook(
    outlook_client: &dyn OutlookClient,
    meeting_repo: &dyn MeetingRepository,
    sync_repo: &dyn SyncStatusRepository,
    user_id: UserId,
    date_range: (NaiveDate, NaiveDate),
) -> Result<SyncResult, AppError> { /* ... */ }

pub async fn sync_excel(
    excel_client: &dyn ExcelClient,
    task_repo: &dyn TaskRepository,
    project_repo: &dyn ProjectRepository,
    sync_repo: &dyn SyncStatusRepository,
    user_id: UserId,
    config: &ExcelMappingConfig,
) -> Result<SyncResult, AppError> { /* ... */ }

pub async fn sync_all(/* ... */) -> Result<Vec<SyncResult>, AppError> { /* ... */ }
```

#### 5.2.4 Application Errors

```rust
// errors.rs

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Domain error: {0}")]
    Domain(#[from] DomainError),

    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),

    #[error("Connector error: {source} -- {message}")]
    Connector { source: Source, message: String },

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Not found: {0}")]
    NotFound(String),
}

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("HTTP error: {status} -- {message}")]
    Http { status: u16, message: String },

    #[error("Authentication failed for {source}")]
    AuthFailed { source: String },

    #[error("Network unreachable: {0}")]
    NetworkError(String),

    #[error("Parsing error: {0}")]
    ParseError(String),
}
```

### 5.3 Infrastructure Layer (`crates/infrastructure`)

#### 5.3.1 Database Connection

```rust
// database/connection.rs

use sqlx::sqlite::SqlitePool;

pub async fn create_sqlite_pool(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let pool = SqlitePool::connect(database_url).await?;
    sqlx::migrate!("../../migrations/sqlite").run(&pool).await?;
    Ok(pool)
}
```

#### 5.3.2 Repository Implementations

Each repository implementation wraps a `SqlitePool` (or `PgPool`) and implements the corresponding trait from the application layer. Queries use `sqlx::query!` or `sqlx::query_as!` macros for compile-time verification.

Example pattern for all repositories:

```rust
// database/task_repo.rs

pub struct SqliteTaskRepository {
    pool: SqlitePool,
}

pub fn new_sqlite_task_repository(pool: SqlitePool) -> SqliteTaskRepository {
    SqliteTaskRepository { pool }
}

#[async_trait]
impl TaskRepository for SqliteTaskRepository {
    async fn find_by_id(&self, id: TaskId) -> Result<Option<Task>, RepositoryError> {
        let id_str = id.to_string();
        let row = sqlx::query_as!(
            TaskRow,
            "SELECT * FROM tasks WHERE id = ?",
            id_str
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepositoryError::Database(e.to_string()))?;

        Ok(row.map(task_row_to_domain))
    }

    // ... other methods follow the same pattern
}

/// Map a database row to a domain Task. Pure function.
fn task_row_to_domain(row: TaskRow) -> Task { /* ... */ }

/// Map a domain Task to database values. Pure function.
fn task_to_row(task: &Task) -> TaskRow { /* ... */ }
```

#### 5.3.3 External API Clients

**Jira Client:**

```rust
// connectors/jira/client.rs

pub struct JiraHttpClient {
    http: reqwest::Client,
    base_url: String,
    auth_token: String,
}

pub fn new_jira_client(base_url: String, auth_token: String) -> JiraHttpClient {
    JiraHttpClient {
        http: reqwest::Client::new(),
        base_url,
        auth_token,
    }
}

#[async_trait]
impl JiraClient for JiraHttpClient {
    async fn fetch_tasks(
        &self, project_keys: &[String], assignees: Option<&[String]>,
    ) -> Result<Vec<JiraTask>, ConnectorError> {
        // Build JQL query
        // GET /rest/api/3/search?jql=...
        // Map response to Vec<JiraTask>
    }
}
```

**Microsoft Graph Client (Outlook + Excel):**

```rust
// connectors/outlook/client.rs

pub struct GraphOutlookClient {
    http: reqwest::Client,
    access_token: String,
}

#[async_trait]
impl OutlookClient for GraphOutlookClient {
    async fn fetch_calendar(
        &self, start: NaiveDate, end: NaiveDate,
    ) -> Result<Vec<OutlookEvent>, ConnectorError> {
        // GET /me/calendarView?startDateTime=...&endDateTime=...
        // Map response to Vec<OutlookEvent>
    }
}
```

```rust
// connectors/excel/client.rs

pub struct GraphExcelClient {
    http: reqwest::Client,
    access_token: String,
}

#[async_trait]
impl ExcelClient for GraphExcelClient {
    async fn fetch_rows(
        &self, config: &ExcelMappingConfig,
    ) -> Result<Vec<ExcelRow>, ConnectorError> {
        // GET /sites/{site-id}/drive/items/{item-id}/workbook/worksheets/{sheet}/usedRange
        // Parse table structure using config mapping
        // Map rows to Vec<ExcelRow>
    }
}
```

### 5.4 API Layer (`crates/api`)

#### 5.4.1 Axum Server Setup

```rust
// main.rs

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::init();

    let db_pool = create_sqlite_pool(&env("DATABASE_URL")).await.unwrap();

    // Build repository instances
    let task_repo = Arc::new(new_sqlite_task_repository(db_pool.clone()));
    let meeting_repo = Arc::new(new_sqlite_meeting_repository(db_pool.clone()));
    // ... other repos

    // Build external clients
    let jira_client = Arc::new(new_jira_client(/* ... */));
    let outlook_client = Arc::new(new_graph_outlook_client(/* ... */));
    let excel_client = Arc::new(new_graph_excel_client(/* ... */));

    // Build broadcast channels for subscriptions
    let (sync_tx, _) = broadcast::channel::<SyncEvent>(100);
    let (reminder_tx, _) = broadcast::channel::<ActivityReminder>(100);
    let (alerts_tx, _) = broadcast::channel::<Vec<Alert>>(100);

    // Build GraphQL schema
    let schema = build_schema(task_repo, meeting_repo, /* ... */);

    // Build Axum router
    let app = Router::new()
        .route("/graphql", post(graphql_handler).get(graphql_playground))
        .route("/graphql/sse", get(graphql_sse_handler))
        .layer(CorsLayer::permissive())
        .layer(auth_middleware())
        .with_state(AppState { schema });

    let addr = SocketAddr::from(([0, 0, 0, 0], 3001));
    tracing::info!("Server running on {}", addr);
    axum::serve(TcpListener::bind(addr).await.unwrap(), app).await.unwrap();
}
```

#### 5.4.2 GraphQL Schema Construction

```rust
// graphql/schema.rs

use async_graphql::Schema;

pub type AppSchema = Schema<QueryRoot, MutationRoot, SubscriptionRoot>;

pub fn build_schema(
    task_repo: Arc<dyn TaskRepository>,
    meeting_repo: Arc<dyn MeetingRepository>,
    // ... all repositories and services
) -> AppSchema {
    Schema::build(QueryRoot, MutationRoot, SubscriptionRoot)
        .data(task_repo)
        .data(meeting_repo)
        // ... register all dependencies
        .finish()
}
```

#### 5.4.3 Resolver Pattern

All resolvers follow the same pattern: extract `user_id` from context, call the use case function, return the result.

```rust
// graphql/query.rs

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn daily_dashboard(
        &self,
        ctx: &Context<'_>,
        date: NaiveDate,
    ) -> Result<DailyDashboardGql> {
        let user_id = ctx.data::<UserId>()?;
        let task_repo = ctx.data::<Arc<dyn TaskRepository>>()?;
        let meeting_repo = ctx.data::<Arc<dyn MeetingRepository>>()?;
        let alert_repo = ctx.data::<Arc<dyn AlertRepository>>()?;
        let sync_repo = ctx.data::<Arc<dyn SyncStatusRepository>>()?;

        use_cases::get_daily_dashboard(
            task_repo.as_ref(),
            meeting_repo.as_ref(),
            alert_repo.as_ref(),
            sync_repo.as_ref(),
            *user_id,
            date,
        ).await.map(Into::into).map_err(Into::into)
    }

    // ... other queries follow the same pattern
}
```

#### 5.4.4 Subscription Implementation

Subscriptions use `async-graphql`'s `Stream` type with `tokio::sync::broadcast` channels.

```rust
// graphql/subscription.rs

pub struct SubscriptionRoot;

#[Subscription]
impl SubscriptionRoot {
    async fn sync_progress(
        &self,
        ctx: &Context<'_>,
    ) -> impl Stream<Item = SyncEventGql> {
        let rx = ctx.data::<broadcast::Sender<SyncEvent>>()
            .unwrap()
            .subscribe();
        BroadcastStream::new(rx).filter_map(|r| r.ok().map(Into::into))
    }

    async fn activity_reminder(
        &self,
        ctx: &Context<'_>,
    ) -> impl Stream<Item = ActivityReminderGql> {
        let rx = ctx.data::<broadcast::Sender<ActivityReminder>>()
            .unwrap()
            .subscribe();
        BroadcastStream::new(rx).filter_map(|r| r.ok().map(Into::into))
    }

    async fn alerts_updated(
        &self,
        ctx: &Context<'_>,
    ) -> impl Stream<Item = Vec<AlertGql>> {
        let rx = ctx.data::<broadcast::Sender<Vec<Alert>>>()
            .unwrap()
            .subscribe();
        BroadcastStream::new(rx)
            .filter_map(|r| r.ok().map(|alerts| alerts.into_iter().map(Into::into).collect()))
    }
}
```

The SSE transport is handled by `async-graphql-axum`:

```rust
// In main.rs route setup
.route("/graphql/sse", get(async_graphql_axum::GraphQLSubscription::new(schema)))
```

#### 5.4.5 Auth Middleware

```rust
// middleware/auth.rs

/// In local mode: always injects a default user_id.
/// In Teams mode: validates Azure AD JWT and extracts user_id from claims.
pub fn auth_middleware() -> impl Layer {
    // Local mode: inject default UserId from env or create one
    // Teams mode: validate Bearer token, extract oid claim as UserId
}
```

---

## 6. Frontend Architecture

### 6.1 urql Client Setup

```typescript
// lib/urql-client.ts

import { Client, cacheExchange, fetchExchange, subscriptionExchange } from 'urql'
import { createClient as createSSEClient } from 'graphql-sse'

const sseClient = createSSEClient({
  url: 'http://localhost:3001/graphql/sse',
})

export const urqlClient = new Client({
  url: 'http://localhost:3001/graphql',
  exchanges: [
    cacheExchange,
    fetchExchange,
    subscriptionExchange({
      forwardSubscription: (operation) => ({
        subscribe: (sink) => ({
          unsubscribe: sseClient.subscribe(operation, sink),
        }),
      }),
    }),
  ],
})
```

### 6.2 GraphQL Codegen

```typescript
// codegen.ts

import type { CodegenConfig } from '@graphql-codegen/cli'

const config: CodegenConfig = {
  schema: 'http://localhost:3001/graphql',
  documents: 'src/graphql/**/*.graphql',
  generates: {
    'src/generated/graphql.ts': {
      plugins: [
        'typescript',
        'typescript-operations',
        'typescript-urql',
      ],
    },
  },
}

export default config
```

This generates:
- TypeScript types for all GraphQL types (Task, Meeting, Alert, etc.)
- Typed hooks for all queries, mutations, and subscriptions (`useDailyDashboardQuery`, `useCreateTaskMutation`, etc.)

### 6.3 Pages and Routing

```typescript
// App.tsx

const router = createBrowserRouter([
  {
    path: '/',
    element: <PageLayout />,
    children: [
      { index: true, element: <DashboardPage /> },
      { path: 'triage', element: <TriagePage /> },
      { path: 'priority', element: <PriorityMatrixPage /> },
      { path: 'workload', element: <WorkloadPage /> },
      { path: 'activity', element: <ActivityJournalPage /> },
      { path: 'settings', element: <SettingsPage /> },
      // v2
      { path: 'team', element: <TeamPage /> },
      { path: 'project/:id', element: <ProjectPage /> },
      { path: 'retrospective', element: <RetrospectivePage /> },
    ],
  },
])
```

### 6.4 Page Specifications

#### DashboardPage (`/`)

The default view. Displays 4 zones as described in US-010:

| Zone | Component | Data Source |
|------|-----------|-------------|
| Followed tasks | `<TaskList>` | `dailyDashboard.tasks` (filtered to `trackingState: FOLLOWED`, sorted by priority) |
| Meetings | `<MeetingList>` | `dailyDashboard.meetings` (sorted by time) |
| Weekly workload | `<WorkloadChart>` | `dailyDashboard.weeklyWorkload` (Recharts bar chart) |
| Alerts | `<AlertPanel>` | `dailyDashboard.alerts` (grouped by severity) |

Additional elements:
- `<SyncStatusBar>` -- Last sync time per source, manual sync button
- `<DateNavigator>` -- Navigate between days (US-011)
- `<ActivitySwitcher>` -- Quick task selection for activity tracking (floating or sidebar)
- `<TaskQuickAdd>` -- Inline task creation

#### TriagePage (`/triage`)

Two-column drag-and-drop interface for task triage (US-042):
- **Inbox column** (amber accent): Tasks with `trackingState: INBOX`, sorted by status then date
- **Following column** (green accent): Tasks with `trackingState: FOLLOWED`
- `@dnd-kit/core` for drag-and-drop between columns (`DndContext`, `useDraggable`, `useDroppable`, `DragOverlay`)
- Each task card shows: Jira key (`sourceId`), title, status badge, assignee, deadline
- Dismiss button (×) on each inbox card calls `setTrackingState(taskId, DISMISSED)`
- "Follow All" button calls `setTrackingStateBatch` for all inbox tasks
- Dashboard only shows tasks with `trackingState: FOLLOWED`

#### PriorityMatrixPage (`/priority`)

2x2 grid with drag-and-drop (US-020, US-021):
- Four `<QuadrantColumn>` components arranged in a grid
- Each quadrant contains `<TaskCard>` components
- `@dnd-kit` for drag-and-drop between quadrants
- Dropping a task into a different quadrant updates its urgency/impact via mutation
- Tasks within a quadrant are sorted by deadline

#### WorkloadPage (`/workload`)

Week view of capacity consumption (US-051):
- `<WeekNavigator>` -- Previous/next week, "Today" button
- `<HalfDayGrid>` -- 5 columns (Mon-Fri) x 2 rows (Morning/Afternoon)
- Each cell shows meetings and tasks assigned to that half-day
- Color coding: green (free), yellow (partially used), red (full/overloaded)
- `<WorkloadChart>` -- Recharts stacked bar chart showing capacity vs. load

#### ActivityJournalPage (`/activity`)

Timeline of the day's activity (US-032):
- `<ActivityTimeline>` -- Vertical timeline with colored blocks per task
- `<SlotEditor>` -- Click a slot to edit start/end time or change task
- Add missing slot button
- Day navigation
- Summary: time tracked vs. untracked

#### SettingsPage (`/settings`)

Configuration interface (section 15):
- Jira connection: URL, API token, project keys
- Microsoft Graph: access token / app registration details
- Excel mapping: SharePoint path, column mapping
- Sync frequency
- Weekly capacity
- Activity reminder settings
- Deadline alert threshold

### 6.5 Key Components

#### TaskCard

Displays a single task across all views.

Props: `task: Task`

Content:
- Source badge (Jira icon, Excel icon, Personal icon)
- Title
- Priority indicator (colored dot or border based on quadrant)
- Deadline (with color: red if overdue, orange if close)
- Assignee (if present)
- Project name (if present)
- Tags (colored chips)

#### ActivitySwitcher

Lightweight popup for quick activity tracking (US-030).

Triggered by:
- Post-meeting subscription event
- Periodic reminder subscription event
- Manual "Change task" button

Content:
- List of in-progress tasks (filterable)
- "No task / break" option
- One-click selection fires `startActivity` mutation

#### DeduplicationPanel

Shown when deduplication suggestions exist (US-004).

Content:
- List of suggested matches with confidence score
- Side-by-side comparison of the two tasks
- Accept / Reject buttons per suggestion
- "Don't suggest again" option (saves rejection)

---

## 7. Database Schema

### 7.1 Migration: `001_initial.sql`

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
    tracking_state TEXT NOT NULL DEFAULT 'inbox'
        CHECK (tracking_state IN ('inbox', 'followed', 'dismissed')),
    jira_remaining_seconds INTEGER,           -- Jira timeestimate (seconds)
    jira_original_estimate_seconds INTEGER,   -- Jira timeoriginalestimate (seconds)
    jira_time_spent_seconds INTEGER,          -- Jira timespent (seconds)
    remaining_hours_override REAL,            -- Local override for remaining time (hours)
    estimated_hours_override REAL,            -- Local override for estimated time (hours)
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

### 7.2 Notes

- All IDs are UUIDs stored as TEXT (both SQLite and Postgres support this).
- All datetimes are stored as ISO 8601 TEXT in SQLite. For the PostgreSQL migration, these become `TIMESTAMPTZ` columns.
- `participants` in meetings and `related_items` in alerts are stored as JSON TEXT arrays.
- Boolean fields use INTEGER (0/1) in SQLite, becoming `BOOLEAN` in Postgres.
- `sqlx` handles the dialect differences transparently via its `Any` pool or feature-flagged query macros.

---

## 8. GraphQL API

### 8.1 Full Schema

```graphql
scalar DateTime
scalar Date
scalar JSON

# --- Enums ---

enum Source {
  JIRA
  EXCEL
  OBSIDIAN
  PERSONAL
}

enum TaskStatus {
  TODO
  IN_PROGRESS
  DONE
  BLOCKED
}

enum HalfDay {
  MORNING
  AFTERNOON
}

enum AlertType {
  DEADLINE
  OVERLOAD
  CONFLICT
}

enum AlertSeverity {
  CRITICAL
  WARNING
  INFORMATION
}

enum ProjectStatus {
  ACTIVE
  PAUSED
  COMPLETED
}

enum SyncSourceStatus {
  IDLE
  SYNCING
  SUCCESS
  ERROR
}

enum TrackingState {
  INBOX
  FOLLOWED
  DISMISSED
}

# --- Core Types ---

type Task {
  id: ID!
  title: String!
  description: String
  source: Source!
  sourceId: String
  jiraStatus: String
  status: TaskStatus!
  project: Project
  assignee: String
  deadline: Date
  plannedStart: DateTime
  plannedEnd: DateTime
  estimatedHours: Float
  urgency: Int!
  urgencyManual: Boolean!
  impact: Int!
  trackingState: TrackingState!
  jiraRemainingSeconds: Int
  jiraOriginalEstimateSeconds: Int
  jiraTimeSpentSeconds: Int
  remainingHoursOverride: Float
  estimatedHoursOverride: Float
  effectiveRemainingHours: Float       # Computed: override > Jira remaining > None
  effectiveEstimatedHours: Float       # Computed: override > Jira estimate > estimatedHours
  tags: [Tag!]!
  quadrant: String!
  createdAt: DateTime!
  updatedAt: DateTime!
}

type Meeting {
  id: ID!
  title: String!
  startTime: DateTime!
  endTime: DateTime!
  location: String
  participants: [String!]!
  project: Project
  durationHours: Float!
  halfDayConsumption: Float!
}

type Project {
  id: ID!
  name: String!
  source: Source!
  sourceId: String
  status: ProjectStatus!
  taskCount: Int!
  openTaskCount: Int!
}

type ActivitySlot {
  id: ID!
  task: Task
  startTime: DateTime!
  endTime: DateTime
  halfDay: HalfDay!
  date: Date!
  durationMinutes: Int
}

type Alert {
  id: ID!
  alertType: AlertType!
  severity: AlertSeverity!
  message: String!
  relatedTasks: [Task!]!
  relatedMeetings: [Meeting!]!
  date: Date!
  resolved: Boolean!
  createdAt: DateTime!
}

type Tag {
  id: ID!
  name: String!
  color: String
}

type SyncStatus {
  source: Source!
  lastSyncAt: DateTime
  status: SyncSourceStatus!
  errorMessage: String
}

# --- Composite Types ---

type DailyDashboard {
  date: Date!
  tasks: [Task!]!
  meetings: [Meeting!]!
  alerts: [Alert!]!
  weeklyWorkload: WeeklyWorkload!
  syncStatuses: [SyncStatus!]!
}

type WeeklyWorkload {
  weekStart: Date!
  capacity: Int!
  halfDays: [HalfDaySlot!]!
  totalPlanned: Float!
  totalMeetings: Float!
  overload: Float
}

# HalfDaySlot is used for the project assignment view (developer-to-project allocation).
# Individual tasks and meetings use hour-based time slots (plannedStart/plannedEnd).
type HalfDaySlot {
  date: Date!
  halfDay: HalfDay!
  meetings: [Meeting!]!
  tasks: [Task!]!
  consumption: Float!
  isFree: Boolean!
}

type PriorityMatrix {
  urgentImportant: [Task!]!
  important: [Task!]!
  urgent: [Task!]!
  neither: [Task!]!
}

type DeduplicationSuggestion {
  id: ID!
  taskA: Task!
  taskB: Task!
  confidenceScore: Float!
  titleSimilarity: Float!
  assigneeMatch: Boolean!
  projectMatch: Boolean!
}

# --- v2 Types ---

type TeamMemberView {
  name: String!
  projects: [ProjectAllocation!]!
  totalLoad: Float!
  isOverloaded: Boolean!
}

type ProjectAllocation {
  project: Project!
  taskCount: Int!
  estimatedLoad: Float!
}

type WeeklyRetrospective {
  weekStart: Date!
  timeByProject: [ProjectTime!]!
  timeByTag: [TagTime!]!
  completedTasks: Int!
  remainingTasks: Int!
  dailyBreakdown: [DailyBreakdown!]!
}

type ProjectTime {
  project: Project!
  halfDays: Float!
  percentage: Float!
}

type TagTime {
  tag: Tag!
  halfDays: Float!
  percentage: Float!
}

type DailyBreakdown {
  date: Date!
  slots: [ActivitySlot!]!
  totalTrackedMinutes: Int!
}

# --- Input Types ---

input TaskFilter {
  status: [TaskStatus!]
  source: [Source!]
  trackingState: [TrackingState!]
  projectId: ID
  assignee: String
  deadlineBefore: Date
  deadlineAfter: Date
  tagIds: [ID!]
}

input CreateTaskInput {
  title: String!
  description: String
  projectId: ID
  deadline: Date
  plannedStart: DateTime
  plannedEnd: DateTime
  estimatedHours: Float
  impact: Int
  urgency: Int
  tagIds: [ID!]
}

input UpdateTaskInput {
  title: String
  description: String
  projectId: ID
  deadline: Date
  plannedStart: DateTime
  plannedEnd: DateTime
  estimatedHours: Float
  status: TaskStatus
  impact: Int
  urgency: Int
  tagIds: [ID!]
  remainingHoursOverride: Float    # null = clear override, absent = don't change
  estimatedHoursOverride: Float    # null = clear override, absent = don't change
}

input UpdateActivitySlotInput {
  taskId: ID
  startTime: DateTime
  endTime: DateTime
}

input TeamFilter {
  projectId: ID
  assignee: String
  weekStart: Date
}

# --- Pagination ---

type PageInfo {
  hasNextPage: Boolean!
  endCursor: String
}

type TaskEdge {
  node: Task!
  cursor: String!
}

type TaskConnection {
  edges: [TaskEdge!]!
  pageInfo: PageInfo!
  totalCount: Int!
}

type AlertEdge {
  node: Alert!
  cursor: String!
}

type AlertConnection {
  edges: [AlertEdge!]!
  pageInfo: PageInfo!
  totalCount: Int!
}

# --- Queries ---

type Query {
  dailyDashboard(date: Date!): DailyDashboard!
  tasks(filter: TaskFilter, first: Int = 50, after: String): TaskConnection!
  task(id: ID!): Task
  priorityMatrix: PriorityMatrix!
  weeklyWorkload(weekStart: Date!): WeeklyWorkload!
  activityJournal(date: Date!): [ActivitySlot!]!
  currentActivity: ActivitySlot
  alerts(resolved: Boolean, first: Int = 50, after: String): AlertConnection!
  projects: [Project!]!
  project(id: ID!): Project
  tags: [Tag!]!
  syncStatuses: [SyncStatus!]!
  deduplicationSuggestions: [DeduplicationSuggestion!]!
  configuration: JSON!
  # v2
  teamView(filter: TeamFilter): [TeamMemberView!]!
  weeklyRetrospective(weekStart: Date!): WeeklyRetrospective!
}

# --- Mutations ---

type Mutation {
  # Task management
  createTask(input: CreateTaskInput!): Task!
  updateTask(id: ID!, input: UpdateTaskInput!): Task!
  deleteTask(id: ID!): Boolean!

  # Triage / Tracking state
  setTrackingState(taskId: ID!, state: TrackingState!): Task!
  setTrackingStateBatch(taskIds: [ID!]!, state: TrackingState!): [Task!]!

  # Priority
  updatePriority(taskId: ID!, urgency: Int, impact: Int): Task!
  resetUrgency(taskId: ID!): Task!

  # Activity tracking
  startActivity(taskId: ID): ActivitySlot!
  stopActivity: ActivitySlot
  updateActivitySlot(id: ID!, input: UpdateActivitySlotInput!): ActivitySlot!
  deleteActivitySlot(id: ID!): Boolean!

  # Alerts
  resolveAlert(id: ID!): Alert!

  # Deduplication
  linkTasks(taskIdPrimary: ID!, taskIdSecondary: ID!): Boolean!
  unlinkTasks(taskIdPrimary: ID!, taskIdSecondary: ID!): Boolean!
  confirmDeduplication(suggestionId: ID!, accept: Boolean!): Boolean!

  # Tags
  createTag(name: String!, color: String): Tag!
  updateTag(id: ID!, name: String, color: String): Tag!
  deleteTag(id: ID!): Boolean!

  # Sync
  forceSync(source: Source): [SyncStatus!]!

  # Meeting-Project association
  updateMeetingProject(meetingId: ID!, projectId: ID): Meeting!

  # Configuration
  updateConfiguration(key: String!, value: JSON!): Boolean!
}

# --- Subscriptions ---

type SyncEvent {
  source: Source!
  status: SyncSourceStatus!
  progress: Float
  message: String
}

type ActivityReminder {
  reminderType: String!
  message: String!
  suggestedTasks: [Task!]!
  endedMeeting: Meeting
}

type Subscription {
  syncProgress: SyncEvent!
  activityReminder: ActivityReminder!
  alertsUpdated: [Alert!]!
}
```

---

## 9. External Integrations

### 9.1 Jira REST API

**Authentication:** API token (Basic Auth with `email:token` base64-encoded) or OAuth 2.0 (for Jira Cloud).

**Endpoints used:**

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/rest/api/3/search` | GET | Search issues with JQL |
| `/rest/api/3/project` | GET | List all accessible projects |

**JQL Queries:**
```
project IN ({configured_keys})
  AND (assignee = currentUser() OR assignee IN ({team_members}))
  ORDER BY updated DESC
```

**Response mapping (Jira -> Domain):**

| Jira Field | Domain Field | Transformation |
|-----------|-------------|---------------|
| `key` | `source_id` | Direct |
| `fields.summary` | `title` | Direct |
| `fields.description` | `description` | ADF -> plain text |
| `fields.status.name` | `jira_status` | Direct — raw Jira status string stored as-is |
| `fields.status.statusCategory.key` | `status` | Map: "new"->Todo, "indeterminate"->InProgress, "done"->Done. Uses Jira status category (3 universal values) rather than custom status names. |
| `fields.assignee.displayName` | `assignee` | Direct |
| `fields.duedate` | `deadline` | Parse ISO date |
| `fields.project.key` | project `source_id` | Direct |
| `fields.project.name` | project `name` | Direct |

**Rate limiting:** Jira Cloud allows ~100 requests/minute. The sync should paginate with `maxResults=100` and respect rate limit headers.

### 9.2 Microsoft Graph API

**Authentication:** OAuth 2.0 with Azure AD app registration. Scopes needed:
- `Calendars.Read` -- Read user's calendar
- `Files.Read.All` -- Read SharePoint files (for Excel)
- `User.Read` -- Read user profile

**Token management:** The backend stores refresh tokens (encrypted) and automatically refreshes access tokens. For local mode, the initial auth flow is:
1. Backend starts an OAuth code flow
2. User authenticates in browser
3. Backend receives auth code, exchanges for tokens
4. Tokens stored encrypted in database

**Endpoints used:**

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/me/calendarView?startDateTime=...&endDateTime=...` | GET | Calendar events in range |
| `/sites/{site-id}/drive/root:/{path}` | GET | Locate Excel file on SharePoint |
| `/sites/{site-id}/drive/items/{item-id}/workbook/worksheets/{sheet}/usedRange` | GET | Read Excel data |

**Calendar event mapping (Graph -> Domain):**

| Graph Field | Domain Field | Transformation |
|------------|-------------|---------------|
| `id` | `outlook_id` | Direct |
| `subject` | `title` | Direct |
| `start.dateTime` | `start_time` | Parse ISO datetime |
| `end.dateTime` | `end_time` | Parse ISO datetime |
| `location.displayName` | `location` | Direct |
| `attendees[].emailAddress.name` | `participants` | Extract names |
| `isCancelled` | -- | If true, skip/delete |

**Project auto-detection:** After mapping, the sync engine scans the meeting `title` for known project names (case-insensitive substring match). If found, `project_id` is set automatically. The user can override this association via the `updateMeetingProject` mutation.

**Excel data mapping:**
The Excel file structure is fully configurable (R24-R26). The `ExcelMappingConfig` defines which columns map to which fields. The raw cell values are read from the `usedRange` response and mapped to `ExcelRow` structs using the configured column names.

### 9.3 Obsidian Integration (v2)

**Method:** Direct filesystem access. The backend reads `.md` files from the configured vault path.

**Parsing rules:**
- Scan files matching configured glob patterns (e.g., `**/*.md`)
- Extract tasks matching Markdown checkbox syntax: `- [ ] Task text` and `- [x] Completed task`
- Identify tasks tagged with configured tags (e.g., `#task`, `#todo`)
- Extract metadata: file path (as source reference), tags, completion status

**Note:** This is local file I/O only, unsuitable for Teams deployment. In Teams mode, Obsidian integration would be disabled or replaced with a file upload mechanism.

---

## 10. Synchronization Engine

### 10.1 Architecture

The sync engine runs as a background task in the Axum server process, using `tokio-cron-scheduler` for periodic execution.

```
+------------------------------------------+
|           Sync Scheduler                 |
|   (tokio-cron-scheduler, configurable)   |
+------------------------------------------+
|                                          |
|  +-----------+ +----------+ +--------+  |
|  | Jira Sync | | Outlook  | | Excel  |  |
|  |  Worker   | |  Sync    | |  Sync  |  |
|  +-----+-----+ +----+-----+ +---+----+  |
|        |             |           |       |
|        +-------------+-----------+       |
|                      |                   |
|              Sync Coordinator            |
|        (sequences sync steps,            |
|         emits SSE events)                |
+------------------------------------------+
```

### 10.2 Sync Flow (per source)

```
1. Update sync_status -> "syncing"
2. Emit SyncEvent subscription (status: SYNCING)
3. Fetch data from external API
   |-- Success: proceed to step 4
   +-- Failure: update sync_status -> "error", emit SyncEvent, stop
4. Transform external data -> domain types (mapper functions)
5. Reconcile with existing data:
   |-- New items -> INSERT
   |-- Changed items -> UPDATE (preserve local overrides: manual urgency, tags)
   +-- Deleted items -> mark for removal (notify user if local data exists)
6. Run deduplication engine (Jira <-> Excel)
7. Run alert engine (deadline, overload, conflict checks)
8. Update sync_status -> "success" + timestamp
9. Emit SyncEvent subscription (status: SUCCESS)
10. Emit alertsUpdated subscription (if alerts changed)
```

### 10.3 Sync Rules

| Rule | Implementation |
|------|---------------|
| **R04** | Sync on app open: triggered when first GraphQL query is received (or explicit `forceSync` mutation) |
| **R05** | Background sync interval: `tokio-cron-scheduler` job, configurable (default: 15 min) |
| **R06** | Cache: all synced data is in SQLite. If API call fails, existing data remains |
| **R07** | Local data (personal tasks, activity journal, priorities) never depends on sync |

### 10.4 Idempotency

Sync operations are idempotent. Running the same sync twice with unchanged external data produces no database changes. This is achieved by:
- Matching on `(user_id, source, source_id)` to detect existing records
- Comparing field values before UPDATE (only write if changed)
- Using `UPSERT` (INSERT ON CONFLICT UPDATE) for meetings (matched on `outlook_id`)

### 10.5 Preserving Local Overrides

When a synced task is updated from the source, the following local fields are **never overwritten**:
- `urgency` + `urgency_manual` (if `urgency_manual = true`)
- `impact`
- `tags`

These fields belong to the user's local enrichment and persist across syncs.

---

## 11. Deduplication Engine

### 11.1 Process

The deduplication engine runs after each sync that involves both Jira and Excel data.

```
1. Fetch all Jira-sourced tasks for the user
2. Fetch all Excel-sourced tasks for the user
3. Fetch existing task_links (both merged and rejected)
4. For each Jira task:
   a. Check R08: search for Jira key in all Excel rows
      |-- Found -> auto-merge (create task_link with type "auto_merged")
      +-- Not found -> proceed to step b
   b. Check R09: calculate similarity with each unlinked Excel task
      |-- Score >= DEDUP_CONFIDENCE_THRESHOLD and pair not rejected
      |   -> create DeduplicationSuggestion for user review
      +-- Score < threshold -> no action
5. For auto-merged tasks:
   - Jira task becomes the primary (source of truth for common fields)
   - Excel data enriches: fields present in Excel but not in Jira are added
   - A `task_link` record (type: auto_merged) links the Excel task to the Jira task
   - The Excel task is hidden from normal views (only the merged Jira task is shown)
```

### 11.2 Merge Rules

When two tasks are merged (R08 auto-merge or R09 user-confirmed merge):

| Field | Source of Truth | Fallback |
|-------|---------------|----------|
| `title` | Jira | Excel |
| `status` | Jira | Excel |
| `assignee` | Jira | Excel |
| `deadline` | Jira | Excel |
| `description` | Jira | Excel |
| `project_id` | Jira | Excel |
| Planning dates (from Excel) | Excel | -- |
| Tags/categories | Merge both | -- |

### 11.3 User Interactions

- **Accept suggestion**: Creates a `task_link` with type `manual_merged`. The secondary task is hidden.
- **Reject suggestion**: Creates a `task_link` with type `rejected`. The pair is never suggested again.
- **Manual link**: User can manually link any two tasks via `linkTasks` mutation.
- **Manual unlink**: User can break a link via `unlinkTasks` mutation.

---

## 12. Alert Engine

### 12.1 Process

The alert engine runs:
- After each sync completes
- After task priority/deadline changes
- On demand (when dashboard is loaded)

```
1. Collect current state:
   - All active tasks for the user
   - All meetings for the relevant period
   - Current configuration (capacity, threshold)
2. Run pure domain functions:
   - check_deadline_alerts(tasks, today, threshold)
   - check_overload_alerts(tasks, meetings, capacity, week_start)
   - check_conflict_alerts(scheduled_items, dates) — using time-range overlap
3. Diff new alerts against existing alerts:
   - New alerts -> INSERT
   - Existing alerts still valid -> keep
   - Existing alerts no longer valid -> auto-resolve
4. Emit alertsUpdated subscription if changes occurred
```

### 12.2 Alert Severity (R19)

| Severity | Conditions |
|----------|-----------|
| **Critical** | Deadline overdue (R14), capacity exceeded by > 2 half-days |
| **Warning** | Deadline within threshold (R17), capacity exceeded by <= 2 half-days |
| **Information** | Scheduling conflict (R18), minor capacity warning |

### 12.3 Alert Lifecycle

1. **Created**: Alert generated by engine
2. **Active**: Displayed in dashboard alert zone
3. **Resolved**: User marks as resolved (via `resolveAlert` mutation) or condition no longer applies (auto-resolved)

Resolved alerts are kept in history but hidden from the active alert panel.

---

## 13. Activity Tracking

### 13.1 Interaction Model

Activity tracking uses three trigger types (US-031):

| Trigger | Mechanism | Implementation |
|---------|-----------|---------------|
| **Post-meeting** | After a meeting ends | Background task checks meetings against current time. When `end_time` passes, emits `activityReminder` subscription with `reminderType: "post_meeting"` |
| **Periodic** | Configurable interval | `tokio-cron-scheduler` job emits `activityReminder` subscription with `reminderType: "periodic"` at configured interval (default: 2h) |
| **Manual** | User clicks button | Frontend sends `startActivity` mutation directly |

### 13.2 Activity Slot Rules

| Rule | Implementation |
|------|---------------|
| **R20** | `ActivitySlot` struct: task_id, start_time, end_time, half_day, date |
| **R21** | `start_activity` use case: closes active slot (sets `end_time = now`), opens new slot |
| **R22** | Gaps between slots are "untracked". The frontend displays them as gray blocks on the timeline. |
| **R23** | `update_activity_slot` and `delete_activity_slot` mutations allow corrections |

### 13.3 Reminder Suppression

- No reminders on weekends or outside configured working hours (default: 08:00-17:00, configurable via `working_hours_start` / `working_hours_end`)
- Post-meeting reminders: only for meetings the user attended (not declined)
- If the user already changed activity within the last 15 minutes, skip the periodic reminder

---

## 14. Authentication & Security

### 14.1 Local Mode (MVP)

| Aspect | Implementation |
|--------|---------------|
| User auth | None. A default user is created at first startup. `user_id` is injected by middleware automatically. |
| API tokens (Jira) | Stored encrypted in `configuration` table. Encryption key derived from a local secret (generated at first startup, stored in a `.secret` file). |
| Graph tokens | OAuth2 tokens (access + refresh) stored encrypted in `configuration` table. Backend handles token refresh automatically. |
| CORS | Permissive (`*`) for localhost development |

### 14.2 Teams Mode (Future)

| Aspect | Implementation |
|--------|---------------|
| User auth | Azure AD / Microsoft Entra ID. JWT validation middleware extracts `oid` claim as `UserId`. |
| API tokens | Per-user, stored encrypted. Graph tokens obtained via Teams SSO (on-behalf-of flow). |
| CORS | Restricted to Teams origin |
| HTTPS | Required (Azure deployment) |

### 14.3 Token Encryption

Sensitive values (API tokens, refresh tokens) are encrypted at rest using AES-256-GCM:
- **Local mode**: Encryption key stored in `backend/.secret` (auto-generated, git-ignored)
- **Teams mode**: Encryption key from Azure Key Vault or environment variable

---

## 15. Configuration

### 15.1 Configuration Parameters

All parameters from the functional spec (section 8.2) are stored in the `configuration` table as key-value pairs with JSON values.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `weekly_capacity` | integer | `10` | Half-days per week |
| `sync_frequency_minutes` | integer | `15` | Background sync interval |
| `activity_reminder_minutes` | integer | `120` | Activity reminder interval |
| `deadline_alert_threshold_days` | integer | `2` | Days before deadline to trigger alert |
| `post_meeting_reminder_enabled` | boolean | `true` | Enable post-meeting activity prompt |
| `periodic_reminder_enabled` | boolean | `true` | Enable periodic activity prompt |
| `working_hours_start` | string | `"08:00"` | Start of working day (HH:MM format) |
| `working_hours_end` | string | `"17:00"` | End of working day (HH:MM format) |
| `jira_base_url` | string | `""` | Jira instance URL |
| `jira_api_token` | string (encrypted) | `""` | Jira API token |
| `jira_email` | string | `""` | Jira user email |
| `jira_project_keys` | string[] | `[]` | Jira project keys to sync |
| `jira_team_members` | string[] | `[]` | Team member usernames for Jira |
| `graph_client_id` | string | `""` | Azure AD app client ID |
| `graph_tenant_id` | string | `""` | Azure AD tenant ID |
| `graph_access_token` | string (encrypted) | `""` | Microsoft Graph access token |
| `graph_refresh_token` | string (encrypted) | `""` | Microsoft Graph refresh token |
| `excel_sharepoint_path` | string | `""` | SharePoint path to Excel file |
| `excel_sheet_name` | string | `""` | Sheet name in Excel |
| `excel_mapping` | object | `{}` | Column name -> field mapping |
| `obsidian_vault_path` | string | `""` | Path to Obsidian vault (v2) |
| `obsidian_task_tags` | string[] | `["#task"]` | Tags identifying tasks in Obsidian (v2) |

### 15.2 Environment Variables

Sensitive bootstrapping values can be set via environment variables (`.env` file):

```bash
DATABASE_URL=sqlite://data/aggregated-plan.db
SERVER_PORT=3001
RUST_LOG=info
# Optional: override DB-stored values
JIRA_BASE_URL=https://mycompany.atlassian.net
JIRA_API_TOKEN=...
GRAPH_CLIENT_ID=...
GRAPH_TENANT_ID=...
```

Environment variables take precedence over database-stored configuration for the same key.

---

## 16. Testing Strategy

### 16.1 Backend Testing

| Layer | Test Type | Tool | What to Test |
|-------|----------|------|-------------|
| **Domain** | Unit tests | `cargo test` | All business rules (urgency, priority, workload, alerts, dedup). Pure functions = simple input/output assertions. |
| **Application** | Unit tests with mocks | `cargo test` + `mockall` | Use cases with mock repository implementations. Verify correct orchestration. |
| **Infrastructure** | Integration tests | `cargo test` + SQLite in-memory | Repository implementations against real SQLite. Test CRUD, queries, edge cases. |
| **Infrastructure** | Integration tests | `cargo test` + `wiremock` | External API clients against mocked HTTP responses. Test request building, response parsing, error handling. |
| **API** | Integration tests | `cargo test` + `axum::test` | Full GraphQL queries/mutations against test server with in-memory DB. |

**Coverage target:** 80% lines, branches, functions (measured with `cargo-tarpaulin`).

### 16.2 Frontend Testing

| Scope | Test Type | Tool | What to Test |
|-------|----------|------|-------------|
| **Components** | Unit/render tests | vitest + @testing-library/react | Component rendering, user interactions, props handling |
| **Hooks** | Unit tests | vitest + renderHook | Custom hook behavior, state transitions |
| **GraphQL** | Integration | vitest + MSW | Mock GraphQL responses, test data flow |
| **Pages** | Integration | vitest + @testing-library/react + MSW | Full page rendering with mocked API |

**Coverage target:** 80% lines, branches, functions (measured with `vitest --coverage`).

### 16.3 End-to-End Testing

| Scope | Tool | What to Test |
|-------|------|-------------|
| Critical paths | Playwright | Dashboard load, create task, drag priority, log activity, settings save |

E2E tests run against the full stack (backend + frontend) with a test SQLite database and mocked external APIs (wiremock for Jira/Graph).

### 16.4 Test File Organization

Tests are colocated with source code:
- Rust: `#[cfg(test)] mod tests { ... }` in the same file, or `tests/` directory at crate root for integration tests
- Frontend: `__tests__/` directories next to source, or `.test.tsx` suffix

---

## 17. Deployment

### 17.1 Local Development

```bash
# Prerequisites: Rust toolchain, Node.js >= 18, pnpm

# Backend
cd backend
cp .env.example .env    # Configure API tokens
cargo run               # Starts on port 3001

# Frontend
cd frontend
pnpm install
pnpm dev                # Starts on port 3000 (Vite dev server)
```

The frontend proxies `/graphql` requests to `http://localhost:3001` via Vite's proxy configuration.

### 17.2 Production Build (Local)

```bash
# Backend: compile release binary
cd backend
cargo build --release
# Binary at: target/release/api

# Frontend: build static assets
cd frontend
pnpm build
# Output at: dist/

# The Axum server can serve the frontend dist/ as static files
```

### 17.3 Azure Deployment (MVP — Coût minimum)

Infrastructure as Code avec Bicep + Azure CLI. Coût estimé : ~5 €/mois.

```
+─────────────────────────────────────────────+
│              Azure Cloud                     │
│                                              │
│  +─────────────────+  +──────────────────+  │
│  │ Container Apps   │  │ Azure Container  │  │
│  │ (Consumption)    │  │ Registry (Basic) │  │
│  │ Backend Rust API │◄─│ ~5€/mois         │  │
│  │ scale-to-zero    │  +──────────────────+  │
│  +────────┬─────────+                        │
│           │ mount                             │
│  +────────▼─────────+  +──────────────────+  │
│  │ Azure File Share │  │ Static Web App   │  │
│  │ SQLite DB        │  │ (Free tier)      │  │
│  │ ~0.01€/mois      │  │ Frontend React   │  │
│  +──────────────────+  +──────────────────+  │
+──────────────────────────────────────────────+
```

| Ressource | Service Azure | SKU | Coût estimé |
|-----------|--------------|-----|-------------|
| Backend API | Container Apps | Consumption (scale-to-0) | ~0 € au repos |
| Frontend | Static Web Apps | Free | 0 € |
| Container Registry | ACR | Basic | ~5 €/mois |
| Base de données | Azure File Share + SQLite | Standard_LRS | ~0.01 €/mois |

#### Structure IaC

```
infra/
├── main.bicep                  # Orchestrateur principal
├── modules/
│   ├── container-registry.bicep   # ACR Basic
│   ├── container-apps.bicep       # Container Apps Environment + backend app
│   ├── static-web-app.bicep       # SWA Free tier
│   └── storage.bicep              # Storage Account + File Share
├── parameters/
│   └── dev.bicepparam             # Paramètres environnement dev
├── Dockerfile.backend             # Multi-stage build Rust
├── .dockerignore
├── deploy.sh                      # Script déploiement complet
└── build-and-push.sh              # Build & push image Docker
```

#### Déploiement

```bash
# Prérequis : az cli connecté (az login), Docker, jq
./infra/deploy.sh dev          # Déploie l'environnement dev
./infra/deploy.sh prod         # Déploie l'environnement prod
IMAGE_TAG=v1.0.0 ./infra/deploy.sh dev  # Tag spécifique
```

#### Points clés

- **Scale-to-zero** : le backend ne consomme rien quand il n'y a pas de requêtes
- **SQLite persisté** : la base est montée via Azure File Share dans `/data`
- **maxReplicas: 1** : SQLite ne supporte pas les écritures concurrentes multi-instances
- **ACR admin auth** : authentification simplifiée pour MVP (migrer vers managed identity en prod)

### 17.4 Teams Deployment (Future)

```
+---------------------------------------+
|           Azure Cloud                 |
|                                       |
|  +-------------+  +---------------+  |
|  | Azure App   |  |  PostgreSQL   |  |
|  | Service     |  |  (Azure DB)   |  |
|  | (Rust API)  |--|               |  |
|  +------+------+  +---------------+  |
|         |                             |
|  +------+------+  +---------------+  |
|  | Static Web  |  |  Azure Key    |  |
|  | App (React) |  |  Vault        |  |
|  +-------------+  +---------------+  |
|                                       |
|  +-------------+                     |
|  | Azure AD    |                     |
|  | App Reg     |                     |
|  +-------------+                     |
+---------------------------------------+
         |
         | Teams Tab (iframe)
         v
+-----------------+
| Microsoft Teams  |
| (Tab App)        |
+-----------------+
```

Migration steps:
1. Switch `sqlx` feature from `sqlite` to `postgres`
2. Run migrations against PostgreSQL
3. Enable Azure AD JWT validation in auth middleware
4. Configure Teams Tab manifest to point to the frontend URL
5. Implement Teams SSO for Microsoft Graph token acquisition (on-behalf-of flow)
6. Deploy backend to Azure App Service, frontend to Azure Static Web Apps

---

## 18. MVP Scope

### 18.1 MVP v1 -- Implementation Order

The MVP should be built in this order, with each phase being independently testable:

**Phase 1: Foundation**
- Backend project setup (Cargo workspace, 4 crates)
- Database schema + migrations (SQLite)
- Domain types (all structs and enums)
- Domain business rules (urgency, priority, workload, alerts, dedup)
- Domain unit tests
- Repository traits (application layer)
- SQLite repository implementations (infrastructure layer)
- Repository integration tests
- Update `CLAUDE.md` to reflect new tech stack (Rust/Axum backend, GraphQL API, SQLite, urql frontend)

**Phase 2: Core API**
- GraphQL schema setup (async-graphql + Axum)
- Query resolvers: `tasks`, `task`, `projects`, `tags`
- Mutation resolvers: `createTask`, `updateTask`, `deleteTask`, `updatePriority`
- Personal task management (full CRUD)
- Frontend project setup (Vite + React + urql + Tailwind + shadcn/ui)
- GraphQL codegen pipeline
- Basic TaskCard, TaskList, TaskForm components

**Phase 3: Dashboard**
- `dailyDashboard` query resolver
- `weeklyWorkload` query resolver
- `priorityMatrix` query resolver
- DashboardPage with 4 zones
- PriorityMatrixPage with drag-and-drop
- WorkloadPage with chart and half-day grid
- Date navigation

**Phase 4: External Integrations**
- Jira connector (infrastructure)
- Microsoft Graph connector -- Outlook calendar (infrastructure)
- Microsoft Graph connector -- Excel/SharePoint (infrastructure)
- Sync engine (scheduler + coordinator)
- `forceSync` mutation + `syncProgress` subscription
- SyncStatusBar component
- Settings page (connection configuration)

**Phase 5: Deduplication**
- Deduplication engine
- `deduplicationSuggestions` query
- `confirmDeduplication`, `linkTasks`, `unlinkTasks` mutations
- DeduplicationPanel component

**Phase 6: Alerts**
- Alert engine (runs post-sync and on-demand)
- `alerts` query + `resolveAlert` mutation + `alertsUpdated` subscription
- AlertPanel and AlertBadge components

**Phase 7: Activity Tracking**
- Activity slot CRUD (use cases + resolvers + repos)
- `startActivity` / `stopActivity` mutations
- `activityReminder` subscription (post-meeting + periodic)
- ActivityJournalPage with timeline
- ActivitySwitcher component

### 18.2 v2 Features (Post-MVP)

- Team view (US-060)
- Project consolidated view (US-061)
- Weekly retrospective (US-062)
- Project workload dashboard (US-063)
- Tags and categories (US-064)
- Obsidian integration (US-005)

---

## 19. Coding Conventions

### 19.1 Rust Conventions

| Rule | Description |
|------|-------------|
| **No classes** | Rust has no classes. Use structs + free functions. No `impl` blocks with methods on domain types -- all logic in free functions. |
| **Immutability** | All struct fields are immutable by default. Use owned values, not mutable references, for transformations. Return new values instead of mutating. |
| **Result everywhere** | All fallible functions return `Result<T, E>`. No `.unwrap()` or `.expect()` in production code (only in tests). |
| **No panic** | No `panic!`, `todo!`, or `unimplemented!` in production code. |
| **Pattern matching** | Use `match` exhaustively. No wildcard `_` catch-all unless intentional and commented. |
| **Iterator combinators** | Prefer `.map()`, `.filter()`, `.fold()`, `.flat_map()` over imperative loops. |
| **Type aliases** | Use type aliases for IDs: `type TaskId = Uuid`. |
| **Error types** | Use `thiserror` derive macro for error enums. |
| **Naming** | Types: `PascalCase`. Functions: `snake_case`. Constants: `UPPER_SNAKE_CASE`. Files: `snake_case.rs`. |
| **Module structure** | One module = one responsibility. Use `mod.rs` for module declarations. |

### 19.2 TypeScript/React Conventions

| Rule | Description |
|------|-------------|
| **No classes** | Function components only. No `class` keyword anywhere. |
| **Immutability** | `const` over `let`. Never `var`. Immutable state updates (`...spread`). |
| **No `any`** | Use `unknown` with type guards if type is genuinely unknown. |
| **Functional** | `map`/`filter`/`reduce` over loops. Pure utility functions. Function composition. |
| **Result pattern** | For complex operations, use discriminated unions: `{ ok: true; value: T } | { ok: false; error: E }` |
| **Components** | Arrow function components. Props as destructured typed objects. |
| **Hooks** | Custom hooks for all non-trivial state logic. |
| **Naming** | Components: `PascalCase`. Hooks: `useCamelCase`. Functions: `camelCase`. Files: `kebab-case.tsx`. |
| **Formatting** | Prettier: single quotes, trailing commas, 100 char width, 2-space indent. |

### 19.3 General Principles

- **YAGNI**: Do not build features not specified. Do not add configurability beyond what is listed.
- **Single responsibility**: One file = one main export/concept.
- **Composition over inheritance**: Always. No inheritance anywhere.
- **Explicit over implicit**: Prefer verbose clarity over clever brevity.
- **Tests first**: Write tests before implementation. Red -> Green -> Refactor.
- **Domain purity**: Domain logic must be testable without any I/O, database, or HTTP setup.
