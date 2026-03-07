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
