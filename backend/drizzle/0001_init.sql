CREATE TABLE projects (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  description TEXT,
  start_date TEXT NOT NULL,
  end_date TEXT NOT NULL,
  status TEXT NOT NULL,
  team_ids TEXT[] NOT NULL DEFAULT '{}',
  client TEXT,
  priority TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  created_by TEXT NOT NULL
);

CREATE UNIQUE INDEX projects_name_unique ON projects (name);

CREATE TABLE developers (
  id TEXT PRIMARY KEY,
  display_name TEXT NOT NULL,
  email TEXT NOT NULL,
  capacity_half_days_per_week INTEGER NOT NULL
);

CREATE TABLE assignments (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  developer_id TEXT NOT NULL,
  date TEXT NOT NULL,
  half_day TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE allocations (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  developer_id TEXT NOT NULL,
  start_date TEXT NOT NULL,
  end_date TEXT NOT NULL,
  half_days_per_week INTEGER NOT NULL,
  preferred_weekdays TEXT[],
  created_at TEXT NOT NULL
);

CREATE TABLE availabilities (
  id TEXT PRIMARY KEY,
  developer_id TEXT NOT NULL,
  start_date TEXT NOT NULL,
  end_date TEXT NOT NULL,
  type TEXT NOT NULL,
  description TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE milestones (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL,
  name TEXT NOT NULL,
  date TEXT NOT NULL,
  type TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
