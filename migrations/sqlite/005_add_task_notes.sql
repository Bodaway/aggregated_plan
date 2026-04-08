-- 005_add_task_notes.sql
-- User-owned markdown notes on tasks. Never overwritten by Jira sync.
ALTER TABLE tasks ADD COLUMN notes TEXT;
