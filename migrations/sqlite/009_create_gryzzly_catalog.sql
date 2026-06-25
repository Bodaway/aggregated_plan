-- Gryzzly read-only catalog cache (active projects + their tasks).
-- Refreshed by sync (Source::Gryzzly). Denormalized: the project is "just for info",
-- so project/customer names are copied onto each task row. gryzzly_project_id is kept
-- because a future hours-upload phase needs it to build declarations.
CREATE TABLE gryzzly_tasks (
    id                 TEXT PRIMARY KEY,
    user_id            TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    gryzzly_task_id    TEXT NOT NULL,
    name               TEXT NOT NULL,
    gryzzly_project_id TEXT NOT NULL,
    project_name       TEXT NOT NULL,
    customer_name      TEXT,
    is_active          INTEGER NOT NULL DEFAULT 1,
    last_synced_at     TEXT NOT NULL,
    UNIQUE(user_id, gryzzly_task_id)
);

CREATE INDEX idx_gryzzly_tasks_user_active_project
    ON gryzzly_tasks(user_id, is_active, project_name);

-- Assignment of an aplan task to a Gryzzly task. Both nullable, user-owned, never
-- overwritten by Jira/Excel sync. gryzzly_project_id is snapshotted at assign time so
-- a future declaration push never depends on a live catalog row.
ALTER TABLE tasks ADD COLUMN gryzzly_task_id TEXT;
ALTER TABLE tasks ADD COLUMN gryzzly_project_id TEXT;
