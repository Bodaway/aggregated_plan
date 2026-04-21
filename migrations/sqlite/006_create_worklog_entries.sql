-- 006_create_worklog_entries.sql
-- Timestamped, task-scoped journal entries. Parallel to tasks.notes (unchanged).
CREATE TABLE worklog_entries (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id),
    task_id    TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    body       TEXT NOT NULL,
    logged_at  TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_worklog_entries_user_logged_at
    ON worklog_entries(user_id, logged_at DESC);
CREATE INDEX idx_worklog_entries_task_logged_at
    ON worklog_entries(task_id, logged_at DESC);
