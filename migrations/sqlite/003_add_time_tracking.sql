-- 003_add_time_tracking.sql
-- Add Jira time tracking fields and local override fields to tasks table.
ALTER TABLE tasks ADD COLUMN jira_remaining_seconds INTEGER;
ALTER TABLE tasks ADD COLUMN jira_original_estimate_seconds INTEGER;
ALTER TABLE tasks ADD COLUMN jira_time_spent_seconds INTEGER;
ALTER TABLE tasks ADD COLUMN remaining_hours_override REAL;
ALTER TABLE tasks ADD COLUMN estimated_hours_override REAL;
