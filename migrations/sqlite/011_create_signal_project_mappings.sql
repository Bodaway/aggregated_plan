-- Learned rules mapping a raw signal (git repo/branch, meeting subject/organizer,
-- internal project) to a Gryzzly project. User-scoped; upsert-once, disable never delete.
CREATE TABLE signal_project_mappings (
    id                   TEXT PRIMARY KEY,
    user_id              TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind                 TEXT NOT NULL,   -- repo_path | branch | meeting_subject | meeting_organizer | internal_project
    pattern              TEXT NOT NULL,
    branch_pattern       TEXT,
    gryzzly_project_id   TEXT NOT NULL,
    gryzzly_project_name TEXT,
    is_enabled           INTEGER NOT NULL DEFAULT 1,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    UNIQUE(user_id, kind, pattern)
);
CREATE INDEX idx_spm_user_kind ON signal_project_mappings(user_id, kind, is_enabled);
