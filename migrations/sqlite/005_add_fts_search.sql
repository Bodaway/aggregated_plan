-- Full-text search for tasks using FTS5
-- Denormalizes searchable fields into a single virtual table for fast text matching.

CREATE VIRTUAL TABLE tasks_fts USING fts5(
    task_id UNINDEXED,
    user_id UNINDEXED,
    title,
    description,
    assignee,
    source_id,
    jira_status,
    status,
    source,
    urgency_text,
    impact_text,
    project_name,
    tag_names,
    tokenize='unicode61 remove_diacritics 2'
);

-- Helper view: builds the denormalized fields for a given task row.
-- Used by triggers to keep FTS in sync.
CREATE VIEW task_fts_data AS
SELECT
    t.id AS task_id,
    t.user_id,
    t.title,
    COALESCE(t.description, '') AS description,
    COALESCE(t.assignee, '') AS assignee,
    COALESCE(t.source_id, '') AS source_id,
    COALESCE(t.jira_status, '') AS jira_status,
    t.status,
    t.source,
    CASE t.urgency
        WHEN 1 THEN 'low'
        WHEN 2 THEN 'medium'
        WHEN 3 THEN 'high'
        WHEN 4 THEN 'critical'
    END AS urgency_text,
    CASE t.impact
        WHEN 1 THEN 'low'
        WHEN 2 THEN 'medium'
        WHEN 3 THEN 'high'
        WHEN 4 THEN 'critical'
    END AS impact_text,
    COALESCE(p.name, '') AS project_name,
    COALESCE(
        (SELECT GROUP_CONCAT(tg.name, ' ')
         FROM task_tags tt
         JOIN tags tg ON tg.id = tt.tag_id
         WHERE tt.task_id = t.id),
        ''
    ) AS tag_names
FROM tasks t
LEFT JOIN projects p ON p.id = t.project_id;

-- Trigger: AFTER INSERT on tasks
CREATE TRIGGER tasks_fts_ai AFTER INSERT ON tasks
BEGIN
    INSERT INTO tasks_fts(task_id, user_id, title, description, assignee, source_id, jira_status, status, source, urgency_text, impact_text, project_name, tag_names)
    SELECT task_id, user_id, title, description, assignee, source_id, jira_status, status, source, urgency_text, impact_text, project_name, tag_names
    FROM task_fts_data WHERE task_id = NEW.id;
END;

-- Trigger: AFTER UPDATE on tasks — delete old, insert new
CREATE TRIGGER tasks_fts_au AFTER UPDATE ON tasks
BEGIN
    DELETE FROM tasks_fts WHERE task_id = OLD.id;
    INSERT INTO tasks_fts(task_id, user_id, title, description, assignee, source_id, jira_status, status, source, urgency_text, impact_text, project_name, tag_names)
    SELECT task_id, user_id, title, description, assignee, source_id, jira_status, status, source, urgency_text, impact_text, project_name, tag_names
    FROM task_fts_data WHERE task_id = NEW.id;
END;

-- Trigger: AFTER DELETE on tasks
CREATE TRIGGER tasks_fts_ad AFTER DELETE ON tasks
BEGIN
    DELETE FROM tasks_fts WHERE task_id = OLD.id;
END;

-- Trigger: AFTER INSERT on task_tags — rebuild FTS row for affected task
CREATE TRIGGER task_tags_fts_ai AFTER INSERT ON task_tags
BEGIN
    DELETE FROM tasks_fts WHERE task_id = NEW.task_id;
    INSERT INTO tasks_fts(task_id, user_id, title, description, assignee, source_id, jira_status, status, source, urgency_text, impact_text, project_name, tag_names)
    SELECT task_id, user_id, title, description, assignee, source_id, jira_status, status, source, urgency_text, impact_text, project_name, tag_names
    FROM task_fts_data WHERE task_id = NEW.task_id;
END;

-- Trigger: AFTER DELETE on task_tags — rebuild FTS row for affected task
CREATE TRIGGER task_tags_fts_ad AFTER DELETE ON task_tags
BEGIN
    DELETE FROM tasks_fts WHERE task_id = OLD.task_id;
    INSERT INTO tasks_fts(task_id, user_id, title, description, assignee, source_id, jira_status, status, source, urgency_text, impact_text, project_name, tag_names)
    SELECT task_id, user_id, title, description, assignee, source_id, jira_status, status, source, urgency_text, impact_text, project_name, tag_names
    FROM task_fts_data WHERE task_id = OLD.task_id;
END;

-- Trigger: AFTER UPDATE on projects — rebuild FTS rows for all tasks in that project
CREATE TRIGGER projects_fts_au AFTER UPDATE OF name ON projects
BEGIN
    DELETE FROM tasks_fts WHERE task_id IN (SELECT id FROM tasks WHERE project_id = NEW.id);
    INSERT INTO tasks_fts(task_id, user_id, title, description, assignee, source_id, jira_status, status, source, urgency_text, impact_text, project_name, tag_names)
    SELECT task_id, user_id, title, description, assignee, source_id, jira_status, status, source, urgency_text, impact_text, project_name, tag_names
    FROM task_fts_data WHERE task_id IN (SELECT id FROM tasks WHERE project_id = NEW.id);
END;

-- Trigger: AFTER UPDATE on tags — rebuild FTS rows for all tasks with that tag
CREATE TRIGGER tags_fts_au AFTER UPDATE OF name ON tags
BEGIN
    DELETE FROM tasks_fts WHERE task_id IN (SELECT task_id FROM task_tags WHERE tag_id = NEW.id);
    INSERT INTO tasks_fts(task_id, user_id, title, description, assignee, source_id, jira_status, status, source, urgency_text, impact_text, project_name, tag_names)
    SELECT task_id, user_id, title, description, assignee, source_id, jira_status, status, source, urgency_text, impact_text, project_name, tag_names
    FROM task_fts_data WHERE task_id IN (SELECT task_id FROM task_tags WHERE tag_id = NEW.id);
END;

-- Backfill: insert all existing tasks into FTS
INSERT INTO tasks_fts(task_id, user_id, title, description, assignee, source_id, jira_status, status, source, urgency_text, impact_text, project_name, tag_names)
SELECT task_id, user_id, title, description, assignee, source_id, jira_status, status, source, urgency_text, impact_text, project_name, tag_names
FROM task_fts_data;
