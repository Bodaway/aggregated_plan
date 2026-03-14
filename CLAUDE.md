# CLAUDE.md

Guide for AI assistants working on the Aggregated Plan codebase.

## Project Overview

A personal Tech Lead cockpit that aggregates Jira tasks, Outlook meetings, and Excel/SharePoint data into a unified planning view. Features include priority matrix (Eisenhower), workload visualization, activity tracking with half-day granularity, automatic deduplication, and real-time alerts. Currently in MVP phase with SQLite storage.

## Repository Structure

```
aggregated_plan/
├── backend/                      # Rust workspace (Cargo)
│   ├── Cargo.toml                # Workspace root
│   ├── crates/
│   │   ├── domain/               # Pure business logic, zero I/O
│   │   ├── application/          # Use cases, repository traits, service traits
│   │   ├── infrastructure/       # SQLite repos, HTTP connectors, sync engine
│   │   └── api/                  # Axum server + async-graphql resolvers
│   └── .env.example
├── frontend/                     # React 18 + Vite (port 3000)
├── migrations/
│   └── sqlite/                   # SQLite migration files
│       └── 001_initial.sql
├── infra/                        # Azure IaC (Bicep + scripts)
│   ├── main.bicep                # Main orchestrator
│   ├── modules/                  # Bicep modules (ACR, Container Apps, SWA, Storage)
│   ├── parameters/               # Environment parameters
│   ├── Dockerfile.backend        # Multi-stage Rust build
│   ├── deploy.sh                 # Full deployment script
│   └── build-and-push.sh         # Docker build & push to ACR
├── docs/
│   └── plans/                    # Implementation plans
├── SPEC_FONCTIONNELLE.md         # Functional specification (French)
├── SPEC_TECHNIQUE.md             # Technical specification
└── CLAUDE.md                     # This file
```

### Backend DDD Layer Separation

```
backend/crates/
├── domain/          # Pure types, business rules. NO external deps (except chrono/serde/uuid/thiserror)
├── application/     # Repository traits, service traits, use case functions. Depends on domain only.
├── infrastructure/  # SQLite repos (sqlx), HTTP connectors (reqwest), sync/dedup engines
└── api/             # Axum routes, async-graphql schema, middleware. Depends on all layers.
```

## Quick Reference Commands

```bash
# Backend (Rust)
cd backend && cargo build                    # Build all crates
cd backend && cargo test                     # Run all backend tests
cd backend && cargo test -p domain           # Domain tests only (52 tests)
cd backend && cargo test -p infrastructure   # Infrastructure tests only (50 tests)
cd backend && cargo check                    # Type-check without building
cd backend && cargo run -p api               # Start API server (port 3001)
cd backend && cargo clippy                   # Lint

# Frontend (TypeScript/React) — not yet set up
cd frontend && pnpm install                  # Install dependencies
cd frontend && pnpm dev                      # Start dev server (port 3000)
cd frontend && pnpm test                     # Run tests
cd frontend && pnpm build                    # Production build

# Azure Deployment (IaC)
./infra/deploy.sh dev                        # Deploy full stack to Azure (dev)
./infra/deploy.sh prod                       # Deploy full stack to Azure (prod)
./infra/build-and-push.sh <acr-server> v1.0  # Build & push Docker image only
```

## Tech Stack

| Component | Technology |
|-----------|-----------|
| Backend language | Rust (stable) |
| HTTP framework | Axum 0.7 |
| GraphQL | async-graphql 7 (queries, mutations, SSE subscriptions) |
| Database | SQLite via sqlx 0.8 (compile-time unchecked, runtime queries) |
| Async runtime | Tokio 1 |
| HTTP client | reqwest 0.12 |
| Frontend language | TypeScript 5.3+ (strict) |
| Frontend framework | React 18, Vite 5 |
| GraphQL client | urql 4, graphql-sse |
| UI components | shadcn/ui, Tailwind CSS 3 |
| Charts | Recharts 2 |
| Drag & drop | @dnd-kit |
| Testing (backend) | Rust built-in `#[test]` + tokio::test |
| Testing (frontend) | Vitest, React Testing Library, Playwright (E2E) |

## Mandatory Coding Conventions

### DDD Layer Rules (strict)

- **Domain** (`crates/domain/`): Pure business logic. Zero I/O. Only depends on chrono, serde, uuid, thiserror.
- **Application** (`crates/application/`): Defines repository and service traits. Use case functions. Depends only on domain.
- **Infrastructure** (`crates/infrastructure/`): Implements traits with real I/O (SQLite, HTTP). Depends on domain + application.
- **API** (`crates/api/`): Axum server, GraphQL resolvers, middleware. Depends on all layers.

### Rust Conventions

- Use `struct` with `impl` blocks. No OOP inheritance.
- Factory pattern: `StructName::new(...)` associated functions
- Repository pattern: traits in application, implementations in infrastructure
- Error handling: `thiserror` for error enums, `Result<T, E>` everywhere, no `.unwrap()` in production
- Domain functions return `DomainResult<T>` (alias for `Result<T, DomainError>`)
- Use `async_trait` for async trait definitions
- Map `sqlx::Error` → `RepositoryError::Database(e.to_string())`

### TypeScript/Frontend Conventions

- Strict TypeScript (all strict flags enabled)
- Functional components with hooks
- urql for GraphQL queries/mutations
- shadcn/ui components (New York variant)
- Tailwind CSS for styling
- `const` over `let`, never `var`

### Spec Maintenance

Whenever code changes affect documented behaviour (API endpoints, domain rules, UI features, data
model, config keys), update **SPEC_FONCTIONNELLE.md** and/or **SPEC_TECHNIQUE.md** in the same
commit. Specifications are written in French.

### Test-Driven Development

Write tests BEFORE production code. Follow Red → Green → Refactor cycle.

Backend tests are inline with `#[cfg(test)] mod tests`. Integration tests use in-memory SQLite (`sqlite::memory:`).

## Naming Conventions

| Entity | Backend (Rust) | Frontend (TypeScript) |
|--------|---------------|----------------------|
| Types/Structs | PascalCase | PascalCase |
| Functions | snake_case | camelCase |
| Constants | UPPER_SNAKE_CASE | UPPER_SNAKE_CASE |
| Files | snake_case (`task_repo.rs`) | kebab-case (`task-list.tsx`) |
| Modules | snake_case | kebab-case |

## GraphQL API

The backend exposes a single GraphQL endpoint:
- `POST /graphql` — queries and mutations
- `GET /graphql/sse` — SSE subscriptions

Key queries: `tasks`, `task`, `projects`, `dashboard`, `priorityMatrix`, `workload`, `alerts`
Key mutations: `createTask`, `updateTask`, `deleteTask`, `updatePriority`, `startActivity`, `stopActivity`, `triggerSync`

## Database

SQLite with migrations at `migrations/sqlite/`. All IDs are UUID strings (`TEXT`). Dates stored as ISO 8601 `TEXT`. Enums as lowercase `TEXT`. Booleans as `INTEGER` (0/1).

11 tables: users, projects, tasks, task_tags, task_links, meetings, activity_slots, alerts, tags, sync_status, configuration.

## Key Domain Concepts

- **Half-day granularity**: Activity tracking uses morning (08:00-12:00) and afternoon (13:00-17:00) slots
- **Priority matrix**: Eisenhower quadrant based on urgency (1-4) × impact (1-4)
- **Urgency calculation**: Auto-computed from deadline proximity (R10-R14), manual override possible (R15)
- **Workload detection**: Overload alerts when planned hours + meeting hours > capacity (R16)
- **Deduplication**: Jira key matching (R08) + similarity scoring (R09) with 0.7 threshold
- **External integrations**: Read-only sync from Jira REST API, Microsoft Graph (Outlook + Excel/SharePoint)
- **Multi-user ready**: All tables include `user_id`, auth middleware injects default user locally

## Common Gotchas

- `sqlx::migrate!` macro path is relative to the crate's `Cargo.toml`, not the workspace root
- Infrastructure repos use runtime queries (`sqlx::query`), not compile-time checked (`sqlx::query!`)
- The `participants` field in meetings and `related_items` in alerts are JSON-serialized `TEXT` columns
- Task tags live in a junction table `task_tags`, not as a column on the tasks table
- Specifications are written in French; code and comments should be in English
- Backend serves on port 3001, frontend on port 3000
