-- 008_add_delegated_to.sql
-- Person a task is delegated to (free text). User-owned: never overwritten by sync.
ALTER TABLE tasks ADD COLUMN delegated_to TEXT;
