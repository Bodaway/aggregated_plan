# Aggregated Plan

TypeScript project with DDD (Domain Driven Design) architecture, TDD (Test Driven Development) and functional paradigm.

## 🏗️ Architecture

This project uses a monorepo architecture with pnpm workspace, separating frontend and backend into two distinct applications.

### Project Structure

```
aggregated_plan/
├── frontend/          # React application with Vite
├── backend/           # Hono API (functional programming)
├── packages/          # Shared packages
│   ├── shared-types/  # Shared TypeScript types
│   └── shared-utils/  # Functional utilities
├── .cursorrules       # Cursor development rules
├── pnpm-workspace.yaml
├── package.json
├── tsconfig.json
├── README.md
├── SPEC_FONCTIONNELLE.md
└── SPEC_TECHNIQUE.md
```

## 🚀 Quick Start

### Prerequisites

- Node.js >= 18.0.0
- pnpm >= 8.0.0

### Installation

```bash
pnpm install
```

### Development

```bash
# Start frontend and backend in parallel
pnpm dev

# Or separately
pnpm --filter frontend dev
pnpm --filter backend dev
```

### Database (PostgreSQL)

Local dev uses PostgreSQL with a selectable persistence driver.

```bash
# Start Postgres (Docker or Podman)
docker compose --env-file config/env/local.env -f infra/compose/compose.yaml up -d db

# Run migrations
pnpm --filter backend db:migrate
```

### Test Environment (full stack)

```bash
# Single command (Docker/Podman)
./scripts/start-test.sh
```

Environment templates live in `config/env/`:

- `config/env/example.env`
- `config/env/local.env`
- `config/env/dev.env`
- `config/env/test.env`

Set `PERSISTENCE_KIND=in-memory` to fall back to the in-memory repositories.

### Build

```bash
pnpm build
```

### Tests

```bash
# All tests
pnpm test

# Watch mode
pnpm test:watch

# With coverage
pnpm --filter frontend test:coverage
pnpm --filter backend test:coverage
```

### Linting

```bash
pnpm lint
```

### Type Checking

```bash
pnpm type-check
```

## 📋 Development Principles

### Strict Typing

- **NEVER** use `any`. Always type explicitly.
- Use `unknown` if the type is truly unknown, then validate with type guards.
- All TypeScript strict flags are enabled.

### Functional Paradigm

- Prefer pure functions (no side effects).
- Use immutability: never mutate objects/arrays directly.
- Prefer `const` over `let`, avoid `var`.
- Use function composition rather than inheritance.
- Use `map`, `filter`, `reduce` rather than imperative loops.

### Types Only, No Classes

- Use `type` and `interface` only.
- No classes, no `new`, no inheritance.
- For factories, use functions that return typed objects.

### Test Driven Development (TDD)

- **ALWAYS** write tests BEFORE production code.
- Structure: Red → Green → Refactor.
- Minimum coverage: 80%.

### Domain Driven Design (DDD)

- **Domain**: pure business logic, no external dependencies.
- **Application**: orchestration, use cases.
- **Infrastructure**: concrete implementations (DB, HTTP, etc.).
- **Presentation**: UI, API routes.

## 📚 Documentation

- [Functional specification](./SPEC_FONCTIONNELLE.md)
- [Technical specification](./SPEC_TECHNIQUE.md)

## 🛠️ Technologies

### Frontend

- React 18
- Vite
- TypeScript
- Jest + React Testing Library
- ESLint + Prettier

### Backend

- Hono (fast functional framework)
- TypeScript
- Zod (functional validation)
- Jest
- ESLint + Prettier

### Tools

- pnpm (package manager)
- TypeScript (language)
- ESLint (linting)
- Prettier (formatting)
- Jest (testing)

## 📝 Available Scripts

### Workspace root

- `pnpm dev`: Start frontend and backend in parallel
- `pnpm build`: Build all packages
- `pnpm test`: Run all tests
- `pnpm lint`: Lint all packages
- `pnpm type-check`: Check types everywhere

### Frontend

- `pnpm --filter frontend dev`: Development server
- `pnpm --filter frontend build`: Production build
- `pnpm --filter frontend test`: Tests
- `pnpm --filter frontend lint`: Linting

### Backend

- `pnpm --filter backend dev`: Development server
- `pnpm --filter backend build`: Production build
- `pnpm --filter backend start`: Production server
- `pnpm --filter backend test`: Tests
- `pnpm --filter backend lint`: Linting
- `pnpm --filter backend db:generate`: Generate migrations (Drizzle)
- `pnpm --filter backend db:migrate`: Apply SQL migrations
- `pnpm --filter backend db:push`: Push schema (dev only)
- `pnpm --filter backend db:studio`: Open Drizzle Studio

## 🤝 Contributing

1. Write tests first (TDD)
2. Respect DDD architecture
3. Use types only, no classes
4. Prefer functional paradigm
5. Keep documentation up to date

## 📄 License

[To be defined]
