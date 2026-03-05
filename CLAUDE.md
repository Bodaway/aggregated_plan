# CLAUDE.md

Guide for AI assistants working on the Aggregated Plan codebase.

## Project Overview

A planning and staffing management application for software development teams. Features include portfolio management, developer staffing with half-day granularity (morning/afternoon), conflict detection, availability tracking, and milestone management. Currently in MVP phase with in-memory data storage.

## Repository Structure

```
aggregated_plan/                  # pnpm monorepo
├── frontend/                     # React 18 + Vite (port 3000)
├── backend/                      # Hono API server (port 3001)
├── packages/
│   ├── shared-types/             # Shared TypeScript type definitions
│   └── shared-utils/             # Pure utility functions (date handling)
├── SPEC_FONCTIONNELLE.md         # Functional specification (French)
├── SPEC_TECHNIQUE.md             # Technical specification (French)
├── .cursorrules                  # IDE coding rules
├── pnpm-workspace.yaml           # Workspace config
└── tsconfig.json                 # Root TypeScript config (strict)
```

Each package follows DDD layer separation:

```
src/
├── domain/          # Pure business logic, NO external dependencies
├── application/     # Use cases, repository interfaces, orchestration
├── infrastructure/  # Concrete implementations (in-memory stores, HTTP clients)
└── presentation/    # UI components (frontend only)
```

## Quick Reference Commands

```bash
pnpm install                        # Install all dependencies
pnpm dev                            # Start frontend + backend in parallel
pnpm test                           # Run all tests (recursive across workspaces)
pnpm test:watch                     # Watch mode for tests
pnpm lint                           # Lint all packages
pnpm type-check                     # TypeScript type checking (no emit)
pnpm build                          # Build all packages
pnpm clean                          # Clean build artifacts

# Per-package commands
pnpm --filter frontend test         # Frontend tests only
pnpm --filter backend test          # Backend tests only
pnpm --filter frontend test:coverage  # Frontend coverage (threshold: 80%)
pnpm --filter backend test:coverage   # Backend coverage (threshold: 80%)
```

## Tech Stack

| Component | Technology |
|-----------|-----------|
| Language | TypeScript 5.3 (all strict flags enabled) |
| Package manager | pnpm 8+ with workspaces |
| Runtime | Node.js >= 18 |
| Frontend | React 18, Vite 5 |
| Backend | Hono 4, Zod (validation) |
| Testing | Jest 29, React Testing Library, ts-jest |
| Linting | ESLint 8 + @typescript-eslint |
| Formatting | Prettier (single quotes, trailing commas, 100 char width, 2-space indent) |

## Mandatory Coding Conventions

These are non-negotiable rules enforced by linting and project standards.

### No `any` Types

ESLint enforces `@typescript-eslint/no-explicit-any: error`. Use `unknown` with type guards if the type is genuinely unknown.

### No Classes

Use `type` and `interface` only. No `class`, no `new`, no inheritance. Factories are plain functions returning typed objects:

```typescript
// Correct
const createUser = (params: CreateUserParams): User => ({
  id: crypto.randomUUID(),
  name: params.name,
});

// Wrong - do NOT use classes
class User { ... }
```

### Functional Paradigm

- Pure functions preferred (no side effects)
- Immutability enforced: `readonly` properties, no direct mutation
- `const` over `let`, never `var`
- `map`/`filter`/`reduce` over imperative loops
- Function composition over inheritance
- Avoid mutations: use `Object.freeze()` or immutability libraries if necessary

### Result Type for Error Handling

Domain functions return `Result<T, DomainError>` instead of throwing exceptions:

```typescript
type Result<T, E> =
  | { readonly ok: true; readonly value: T }
  | { readonly ok: false; readonly error: E };
```

Use the `ok()` and `err()` helpers from `backend/src/domain/result.ts`.

### DDD Layer Rules

- **Domain** (`src/domain/`): Pure business logic. Zero imports from application/infrastructure/presentation layers. No external library dependencies.
- **Application** (`src/application/`): Use cases and repository interfaces. Depends only on domain.
- **Infrastructure** (`src/infrastructure/`): Concrete implementations. Depends on domain and application.
- **Presentation** (`src/presentation/`, frontend only): React components. Can depend on all layers.

### Design Patterns

- **Factory**: for creating complex objects (plain functions returning typed objects)
- **Repository**: for persistence abstraction (interfaces in application, implementations in infrastructure)
- **Strategy**: for interchangeable algorithms
- **Adapter**: for adapting external interfaces
- Use composition rather than inheritance

### Test-Driven Development

Write tests BEFORE production code. Follow Red -> Green -> Refactor cycle. Minimum coverage: 80% for branches, functions, lines, and statements.

Tests are colocated with source in `__tests__/` subdirectories or use `.test.ts(x)` suffix.

## Naming Conventions

| Entity | Convention | Example |
|--------|-----------|---------|
| Types/Interfaces | PascalCase | `User`, `ProjectRepository` |
| Functions | camelCase | `getUserById`, `createProject` |
| Constants | UPPER_SNAKE_CASE | `MAX_RETRY_COUNT` |
| Files | kebab-case | `user-repository.ts` |

## Code Structure

### File Organization

- One file = one main responsibility
- Name files in kebab-case: `user-repository.ts`
- Export types/interfaces with domain prefix: `User`, `UserRepository`, etc.

### Documentation

- Document public functions with JSDoc
- Keep `SPEC_FONCTIONNELLE.md` and `SPEC_TECHNIQUE.md` up to date
- Update README.md for major changes

## Path Aliases

**Backend:** `@domain/*`, `@application/*`, `@infrastructure/*`
**Frontend:** `@domain/*`, `@application/*`, `@infrastructure/*`, `@presentation/*`
**Shared packages:** Referenced via `@aggregated-plan/shared-types` and `@aggregated-plan/shared-utils` (workspace protocol)

## API Endpoints (Backend)

```
GET    /projects                     # List all projects
POST   /projects                     # Create project
GET    /projects/:id                 # Get project by ID
PUT    /projects/:id                 # Update project
DELETE /projects/:id                 # Delete project
GET    /milestones                   # List all milestones
GET    /projects/:id/milestones      # List milestones for project
POST   /projects/:id/milestones      # Create milestone for project
GET    /developers                   # List developers
POST   /developers                   # Create developer
PUT    /developers/:id               # Update developer
GET    /assignments                  # List assignments
POST   /assignments                  # Create assignment
POST   /allocations                  # Create weekly allocation
GET    /conflicts                    # List detected conflicts
GET    /availabilities               # List availabilities
POST   /availabilities               # Create availability
```

All request bodies are validated with Zod schemas. Dates use `YYYY-MM-DD` format (`IsoDateString` type).

## Key Domain Concepts

- **Half-day granularity**: All scheduling uses morning/afternoon slots, not full days
- **Weekly capacity**: Developers have 1-10 half-days per week capacity
- **Assignments**: Concrete developer-to-project mapping on a specific date + half-day
- **Weekly allocations**: Recurring assignments expanded from a date range and preferred weekdays
- **Conflicts**: Automatically detected - capacity overloads and double bookings
- **Availability**: Leave, training, unavailability periods that constrain assignments

## Prettier Config

```json
{
  "semi": true,
  "trailingComma": "es5",
  "singleQuote": true,
  "printWidth": 100,
  "tabWidth": 2,
  "useTabs": false,
  "arrowParens": "avoid"
}
```

## Common Gotchas

- The project uses `"type": "module"` in both frontend and backend `package.json` files
- Backend dev server uses `tsx watch` (not `ts-node`)
- Frontend uses Vite's `import.meta.env` for environment variables (prefix: `VITE_`)
- The backend API base URL defaults to `http://localhost:3001` in the frontend API client
- All TypeScript strict flags are individually enabled in root `tsconfig.json` -- do not relax them
- Specifications are written in French; code and comments should be in English
