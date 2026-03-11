ALTER TABLE tasks ADD COLUMN tracking_state TEXT NOT NULL DEFAULT 'inbox'
    CHECK (tracking_state IN ('inbox', 'followed', 'dismissed'));

UPDATE tasks SET tracking_state = 'followed' WHERE source = 'personal';
