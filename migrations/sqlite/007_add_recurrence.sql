-- 007_add_recurrence.sql
-- Adds recurring-task template table and relaxes tasks.status CHECK to include 'cancelled'.

-- Recurrence templates
CREATE TABLE task_recurrences (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    title TEXT NOT NULL,
    description TEXT,
    notes TEXT,
    project_id TEXT REFERENCES projects(id),
    urgency INTEGER NOT NULL,
    urgency_manual INTEGER NOT NULL DEFAULT 0,
    impact INTEGER NOT NULL,
    estimated_hours REAL,
    rule_json TEXT NOT NULL,                 -- serialized RecurrenceRule
    starts_on TEXT NOT NULL,                 -- ISO date
    ends_on TEXT,
    max_occurrences INTEGER,
    last_generated_through TEXT,
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_recurrences_user_active ON task_recurrences(user_id, active);

CREATE TABLE task_recurrence_tags (
    template_id TEXT NOT NULL REFERENCES task_recurrences(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (template_id, tag_id)
);

-- Rebuild tasks table to add 'cancelled' to the status CHECK constraint.
-- SQLite does not support ALTER TABLE ... ALTER COLUMN CHECK, so we use table-rebuild.
PRAGMA foreign_keys = OFF;

CREATE TABLE tasks_new (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    description TEXT,
    source TEXT NOT NULL CHECK (source IN ('jira', 'excel', 'obsidian', 'personal', 'outlook')),
    source_id TEXT,
    jira_status TEXT,
    status TEXT NOT NULL DEFAULT 'todo'
        CHECK (status IN ('todo', 'in_progress', 'done', 'blocked', 'cancelled')),
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
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    tracking_state TEXT NOT NULL DEFAULT 'inbox'
        CHECK (tracking_state IN ('inbox', 'followed', 'dismissed')),
    jira_remaining_seconds INTEGER,
    jira_original_estimate_seconds INTEGER,
    jira_time_spent_seconds INTEGER,
    remaining_hours_override REAL,
    estimated_hours_override REAL,
    notes TEXT
);

INSERT INTO tasks_new (
    id, user_id, title, description, source, source_id, jira_status,
    status, project_id, assignee, deadline, planned_start, planned_end,
    estimated_hours, urgency, urgency_manual, impact, created_at, updated_at,
    tracking_state, jira_remaining_seconds, jira_original_estimate_seconds,
    jira_time_spent_seconds, remaining_hours_override, estimated_hours_override,
    notes
)
SELECT
    id, user_id, title, description, source, source_id, jira_status,
    status, project_id, assignee, deadline, planned_start, planned_end,
    estimated_hours, urgency, urgency_manual, impact, created_at, updated_at,
    tracking_state, jira_remaining_seconds, jira_original_estimate_seconds,
    jira_time_spent_seconds, remaining_hours_override, estimated_hours_override,
    notes
FROM tasks;

DROP TABLE tasks;
ALTER TABLE tasks_new RENAME TO tasks;

PRAGMA foreign_keys = ON;

-- Add recurrence columns to the (now-rebuilt) tasks table
ALTER TABLE tasks ADD COLUMN recurrence_id TEXT REFERENCES task_recurrences(id);
ALTER TABLE tasks ADD COLUMN occurrence_date TEXT;   -- ISO date; the slot this instance fills

-- Unique partial index: one task per (template, occurrence date)
CREATE UNIQUE INDEX idx_tasks_recurrence_slot
    ON tasks(recurrence_id, occurrence_date)
    WHERE recurrence_id IS NOT NULL;

CREATE INDEX idx_tasks_recurrence ON tasks(recurrence_id);

-- Restore indexes dropped with the old tasks table
CREATE INDEX idx_tasks_user ON tasks(user_id);
CREATE INDEX idx_tasks_source ON tasks(user_id, source, source_id);
CREATE INDEX idx_tasks_deadline ON tasks(user_id, deadline);
CREATE INDEX idx_tasks_project ON tasks(project_id);
CREATE INDEX idx_tasks_status ON tasks(user_id, status);
