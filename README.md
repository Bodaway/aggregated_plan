# Aggregated Plan

A personal Tech Lead cockpit that aggregates Jira tasks, Outlook meetings, and Excel/SharePoint data into a unified planning view. Features include an Eisenhower priority matrix, workload visualization, activity tracking with half-day granularity, automatic task deduplication, and real-time alerts.

## Architecture

Rust backend (DDD with 4 crates) + React frontend communicating via GraphQL.

```
aggregated_plan/
├── backend/                      # Rust workspace (Cargo)
│   ├── crates/
│   │   ├── domain/               # Pure business logic, zero I/O
│   │   ├── application/          # Use cases, repository traits, service traits
│   │   ├── infrastructure/       # SQLite repos, HTTP connectors, sync engine
│   │   └── api/                  # Axum server + async-graphql resolvers
│   └── .env.example
├── frontend/                     # React 18 + Vite (port 3000)
│   ├── src/
│   │   ├── components/           # Reusable UI components
│   │   ├── pages/                # Page components (7 pages)
│   │   ├── hooks/                # Custom React hooks
│   │   ├── graphql/              # GraphQL queries and mutations
│   │   └── lib/                  # Utilities, urql client, constants
│   └── e2e/                      # Playwright E2E tests
├── migrations/sqlite/            # Database migrations
├── docs/plans/                   # Implementation plans
├── SPEC_FONCTIONNELLE.md         # Functional specification (French)
└── SPEC_TECHNIQUE.md             # Technical specification
```

## Prerequisites

- **Rust** (stable toolchain) - [rustup.rs](https://rustup.rs)
- **Node.js** >= 18
- **pnpm** >= 8

## Quick Start

### Backend

```bash
cd backend

# Copy environment config
cp .env.example .env
# Edit .env with your database path and connector credentials

# Build and run
cargo build
cargo run -p api
# Server starts on http://localhost:3001
# GraphQL playground at http://localhost:3001/graphql/playground
```

### Frontend

```bash
cd frontend
pnpm install
pnpm dev
# Dev server starts on http://localhost:3000
```

### Run Tests

```bash
# Backend (178 tests)
cd backend && cargo test

# Frontend type check
cd frontend && pnpm type-check

# Frontend build
cd frontend && pnpm build

# E2E tests (requires both servers running)
cd frontend && pnpm test:e2e
```

## Tech Stack

| Component | Technology |
|-----------|-----------|
| Backend | Rust, Axum 0.8, async-graphql 7, sqlx 0.8 (SQLite), Tokio |
| Frontend | TypeScript 5, React 18, Vite 5, urql 4, Tailwind CSS 3 |
| UI | shadcn/ui (New York), Recharts 2, @dnd-kit |
| Database | SQLite (MVP), PostgreSQL (future) |
| Testing | Rust #[test], Vitest, Playwright |

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `sqlite:aggregated_plan.db?mode=rwc` | SQLite database path |
| `RUST_LOG` | `info` | Log level (trace, debug, info, warn, error) |
| `VITE_API_URL` | `http://localhost:3001` | Backend API URL (frontend) |

Connector credentials are managed via the Settings page and stored in the `configuration` table.

## Key Features

- **Dashboard**: Daily view with tasks, meetings, workload chart, and alerts
- **Priority Matrix**: Eisenhower quadrant with drag-and-drop
- **Workload View**: Half-day capacity visualization for the week
- **Activity Journal**: Time tracking with start/stop timer
- **Deduplication**: Automatic detection of duplicate tasks across sources
- **Alerts**: Deadline warnings, schedule conflicts, overload detection
- **External Sync**: Read-only integration with Jira, Outlook, Excel/SharePoint
- **Settings**: Connector configuration and preferences

## GraphQL API

The backend exposes a single GraphQL endpoint:
- `POST /graphql` - queries and mutations
- `GET /graphql/playground` - GraphiQL IDE

15 queries, 20 mutations, 3 subscriptions (SSE).

## Documentation

- [Functional Specification](./SPEC_FONCTIONNELLE.md) (French)
- [Technical Specification](./SPEC_TECHNIQUE.md)
- [Implementation Plan](./docs/plans/2026-03-07-full-mvp-rebuild.md)

## Development Principles

- **DDD**: Domain layer has zero I/O dependencies
- **TDD**: Tests written before production code
- **Business rules**: R01-R26 implemented in `domain/src/rules/`
- **Repository pattern**: Traits in application, implementations in infrastructure
- **Error handling**: `thiserror` enums, `Result<T, E>` everywhere, no `.unwrap()` in production

## License

[To be defined]
