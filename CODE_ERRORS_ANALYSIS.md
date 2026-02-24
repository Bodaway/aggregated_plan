# Code Error Analysis Report

## Summary

This report identifies **13 errors** across the codebase, categorized by severity:
- **Critical (build-breaking)**: 5
- **High (runtime/logic errors)**: 4
- **Medium (configuration/tooling)**: 4

---

## Critical Errors (Build-Breaking)

### 1. Duplicated jest.config.ts in shared-utils

**File:** `packages/shared-utils/jest.config.ts`
**Lines:** 1-50

The entire jest config is duplicated — the file contains the config block twice with two `export default config` statements. This causes a TypeScript compilation error (`TS2528: A module cannot have multiple default exports`) and prevents `pnpm test` from running for this package.

**Error output:**
```
jest.config.ts(1,15): error TS2300: Duplicate identifier 'Config'.
jest.config.ts(3,7): error TS2451: Cannot redeclare block-scoped variable 'config'.
jest.config.ts(27,16): error TS2528: A module cannot have multiple default exports.
```

**Fix:** Remove lines 28-50 (the duplicate block).

---

### 2. Missing shared-types build artifacts break shared-utils type-check

**File:** `packages/shared-utils/tsconfig.json` / `packages/shared-types/package.json`

`shared-utils` depends on `@aggregated-plan/shared-types`, but `shared-types` exports point to `./dist/index.d.ts` which doesn't exist until built. The `shared-utils` tsconfig has no path mapping or project reference to resolve the source directly. Running `pnpm type-check` fails with:

```
src/date-utils.ts(1,45): error TS2307: Cannot find module '@aggregated-plan/shared-types'
```

**Fix:** Either add a `paths` mapping in `shared-utils/tsconfig.json` pointing to the source, or add a project reference, or ensure `shared-types` is built before `shared-utils` type-checks.

---

### 3. Frontend build fails — missing module resolution for @aggregated-plan/shared-types

**File:** `frontend/tsconfig.json`

The frontend's TypeScript config doesn't include a path mapping for `@aggregated-plan/shared-types`. While Vite resolves it at runtime through the workspace, `tsc` cannot. The build command (`tsc && vite build`) fails:

```
src/domain/index.ts(12,8): error TS2307: Cannot find module '@aggregated-plan/shared-types'
src/infrastructure/api-client.ts(14,8): error TS2307: Cannot find module '@aggregated-plan/shared-types'
```

**Fix:** Add `"@aggregated-plan/shared-types": ["../packages/shared-types/src/index.ts"]` to `frontend/tsconfig.json` paths.

---

### 4. Frontend TypeScript strict mode: implicit `any` in timeline.tsx

**File:** `frontend/src/presentation/timeline.tsx:231`

The `date` parameter in a `.some()` callback has an implicit `any` type, which violates the `noImplicitAny` compiler setting:

```
src/presentation/timeline.tsx(231,33): error TS7006: Parameter 'date' implicitly has an 'any' type.
```

The line in question:
```typescript
return conflict.dates.some((date) => isDateInRange(toDate(date), rangeStart, rangeEnd));
```

This fails because `Conflict.dates` is `readonly IsoDateString[]` but TypeScript can't resolve the `IsoDateString` type due to error #3 above.

**Fix:** Resolving error #3 should fix this. Alternatively, add an explicit type annotation: `(date: string)`.

---

### 5. Frontend import.meta.env not recognized by TypeScript

**File:** `frontend/src/infrastructure/api-client.ts:17`

```
src/infrastructure/api-client.ts(17,15): error TS2339: Property 'env' does not exist on type 'ImportMeta'.
```

The frontend tsconfig doesn't include a reference to `vite/client` types, so `import.meta.env` isn't typed.

**Fix:** Add `"types": ["vite/client"]` to `frontend/tsconfig.json` compilerOptions, or add a `/// <reference types="vite/client" />` triple-slash directive, or add a `vite-env.d.ts` file.

---

## High Severity (Runtime/Logic Errors)

### 6. Frontend tests fail — import.meta not supported in Jest

**File:** `frontend/src/infrastructure/api-client.ts:4` + `frontend/jest.config.ts`

The frontend test suite cannot run because `import.meta.env` is a Vite-specific feature that Jest's default transformer (`ts-jest`) doesn't support:

```
SyntaxError: Cannot use 'import.meta' outside a module
```

**Fix:** Configure Jest to mock or transform `import.meta.env`. Options:
- Add a `globals` config in jest.config.ts to define `import.meta`
- Use a custom transformer or the `babel-plugin-transform-import-meta` plugin
- Mock the `api-client` module in tests

---

### 7. updateProject silently clears optional fields when updating

**File:** `backend/src/domain/project-domain.ts:110-121`

The `updateProject` function uses `??` (nullish coalescing) for all fields:
```typescript
description: updates.description ?? project.description,
client: updates.client ?? project.client,
priority: updates.priority ?? project.priority,
```

This means there's no way to intentionally clear an optional field (set it to `undefined`). If a user sends `{ description: undefined }` or omits the field, the old value is preserved. However, if a user wants to **remove** the description, they cannot do so because `undefined ?? project.description` returns the old value.

This is a design limitation rather than a crash bug, but it makes it impossible to clear optional fields via the API.

**Fix:** Use a sentinel value or explicitly check for `key in updates` rather than relying on `??`.

---

### 8. No duplicate assignment check — same developer can be double-booked

**File:** `backend/src/application/staffing-use-cases.ts:84-98`

The `createAssignmentHandler` saves an assignment without checking whether the same developer already has an assignment for the same date + halfDay. While conflicts are *detected* after the fact via `listConflicts`, the assignment is still persisted. There's no way to prevent or undo overlapping assignments.

**Fix:** Check for existing assignments for the same developer/date/halfDay before saving, and return an `assignment-conflict` error if found.

---

### 9. ESLint rule `no-mutating-assignments` doesn't exist

**File:** `frontend/.eslintrc.json:36`

```json
"no-mutating-assignments": "error"
```

This is not a valid ESLint rule (neither core nor from any plugin in the dependencies). It's likely a typo or confused with another rule. Currently, this causes an ESLint error when running, and since the frontend ESLint also fails due to error #10, this is masked.

**Fix:** Remove the rule or replace with the intended rule (e.g., `no-param-reassign`).

---

## Medium Severity (Configuration/Tooling)

### 10. Missing eslint-plugin-react-refresh config

**File:** `frontend/.eslintrc.json:13`

The ESLint config extends `plugin:react-refresh/essential`, but this config name doesn't exist in the installed version of `eslint-plugin-react-refresh`. The package is installed (v0.4.5) but that version uses `plugin:react-refresh/recommended` rather than `plugin:react-refresh/essential`.

```
ESLint couldn't find the config "plugin:react-refresh/essential" to extend from.
```

**Fix:** Change to `"plugin:react-refresh/recommended"` or remove the extends entry and keep only the manual rule configuration already present in the `rules` section.

---

### 11. Backend API test is a no-op placeholder

**File:** `backend/src/index.test.ts`

The test file contains only `expect(true).toBe(true)` — it doesn't test any actual API behavior. This gives a false sense of coverage.

```typescript
describe('Backend API', () => {
  it('should be configured correctly', () => {
    expect(true).toBe(true);
  });
});
```

**Fix:** Replace with actual integration tests that start the Hono app and test the API routes, or remove the file to avoid misleading test counts.

---

### 12. Developer email validation uses wrong error code

**File:** `backend/src/application/developer-use-cases.ts:39-41`

When email validation fails, the error code is `'invalid-name'` instead of something like `'invalid-email'`:

```typescript
if (!email.includes('@')) {
  return err(createDomainError('invalid-name', 'Developer email must be valid.'));
}
```

While `'invalid-name'` is a valid `DomainErrorCode`, it's semantically incorrect. The `DomainErrorCode` union doesn't include an email-specific code.

**Fix:** Add `'invalid-email'` to the `DomainErrorCode` type and use it here, or use a more generic code.

---

### 13. Conflict detection: capacity conflict type mismatch with shared Conflict type

**File:** `backend/src/domain/conflict-domain.ts:127-137`

The `buildCapacityConflicts` function returns objects with extra properties (`weekStart`, `assignedHalfDays`, `capacityHalfDays`) that go beyond the base `Conflict` type definition. These properties are optional in the shared type, so TypeScript doesn't complain, but `projectIds` is not set for capacity conflicts. In the frontend (`app.tsx:218`), `conflict.projectIds?.map(...)` safely handles this with optional chaining, but the `hasConflictForProjectAndRange` function in `timeline.tsx:228` checks:

```typescript
if (!conflict.projectIds || !conflict.projectIds.includes(projectId)) {
  return false;
}
```

This means capacity conflicts are **never shown** in the Timeline view's conflict indicators, since they lack `projectIds`.

**Fix:** Include the relevant `projectIds` in capacity conflict objects, or adjust the timeline to handle capacity conflicts separately.

---

## Additional Notes

### Test Coverage Gaps
- Backend has 17 passing tests but no integration tests for the HTTP API layer
- Frontend has 0 passing tests (blocked by error #6)
- shared-utils has 0 passing tests (blocked by error #1)

### Working Correctly
- Backend domain logic (project, staffing, availability, milestone, conflict detection) — all 17 unit tests pass
- Backend infrastructure (in-memory repositories, server config)
- Shared types package (type-check passes)
- Backend API route definitions and Zod validation schemas
