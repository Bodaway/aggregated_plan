# Task Delegation Field — Design

**Date:** 2026-06-04
**Status:** Approved

## Summary

Add a `delegated_to` field to tasks: a free-text name recording who a task was handed off to.
Purely informational — no effect on workload, alerts, or the priority matrix. Name suggestions
are auto-learned from previously used values (no settings UI, no people table).

## Requirements

- A task can carry an optional delegate name, independent of the Jira-synced `assignee`.
- The field is user-owned: Jira/Excel sync must never overwrite it (same contract as `notes`).
- When editing a task, the user gets suggestions of every name previously used across their
  tasks, and can type any new name (combobox behavior, no strict list).
- Setting/clearing the delegate has no behavioral side effects anywhere in the app.

## Decisions made during brainstorming

| Question | Decision |
|---|---|
| Relation to existing `assignee` | Separate field; `assignee` stays a read-only Jira mirror |
| Behavioral impact | None — purely informational |
| Name list management | Auto-learned from prior values; free text allowed; no settings UI |
| Suggestion source | Backend `delegates` GraphQL query (`SELECT DISTINCT`), not config-persisted, not frontend-derived |

## Design

### 1. Data model

- Migration `migrations/sqlite/008_add_delegated_to.sql`:
  `ALTER TABLE tasks ADD COLUMN delegated_to TEXT;` (nullable — same pattern as migrations 002–006).
- Domain `Task` (backend/crates/domain/src/types/task.rs) gains
  `pub delegated_to: Option<String>` with a doc comment marking it user-owned and
  never overwritten by sync, mirroring the `notes` field's contract.

### 2. Sync safety

Jira and Excel sync update paths (backend/crates/application/src/use_cases/sync.rs) preserve
`delegated_to` on existing tasks, exactly as they preserve `notes`. A test pins this:
sync over a task with a delegate set → the delegate survives.

### 3. Application layer

- `delegated_to` added to the application-layer `UpdateTaskInput`, following the same
  set/clear convention used by `notes`.
- New task repository trait method: `list_delegates(user_id) -> Vec<String>`.
- SQLite implementation:
  `SELECT DISTINCT delegated_to FROM tasks WHERE user_id = ? AND delegated_to IS NOT NULL ORDER BY delegated_to`.

### 4. GraphQL API

- Task type exposes `delegatedTo: String`.
- `UpdateTaskInput` accepts `delegatedTo` (explicit null clears the field, per the
  existing `notes` convention).
- New query `delegates: [String!]!` returning the distinct learned names for the
  current user.

### 5. Frontend

- **TaskEditSheet** (frontend/src/components/task/TaskEditSheet.tsx): a "Delegated to"
  text input backed by a `<datalist>` populated from the `delegates` query. Typing shows
  prior names as suggestions; any new name is accepted; an empty input clears the field.
  Plain HTML controls, consistent with the sheet's existing `<select>`/`<input>` style.
- **TaskCard** (frontend/src/components/task/TaskCard.tsx): when `delegatedTo` is set,
  render the name (e.g. `→ Marie`) in the bottom meta row, next to the existing assignee
  display.
- No changes to workload, alerts, dashboard, or priority-matrix components.

### 6. Testing (TDD — tests first)

- **Infrastructure:** task repo roundtrip persists `delegated_to`; `list_delegates`
  returns distinct, sorted, non-null names scoped to the user.
- **Application:** sync-preservation test (delegate survives a Jira sync update).
- **API:** GraphQL tests — `updateTask` sets and clears `delegatedTo`; `delegates`
  query returns learned names.
- **Frontend:** TaskEditSheet renders the input with datalist suggestions and submits
  the value; TaskCard shows the delegate when present.

### 7. Specification updates (same commit as implementation)

- **SPEC_FONCTIONNELLE.md:** nouvelle section décrivant la délégation — champ libre,
  suggestions auto-apprises, purement informatif.
- **SPEC_TECHNIQUE.md:** colonne `delegated_to`, champ GraphQL `delegatedTo`,
  requête `delegates`.

## Out of scope (YAGNI)

- Settings UI for managing the names list.
- A people/contacts table or entity.
- Filtering/grouping the task list by delegate.
- Excluding delegated tasks from workload or alerts.
- Any "delegated" task status.
