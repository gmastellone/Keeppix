-- Per-user presentation preferences (Fase 10 Task 9): one jsonb document per user.
ALTER TABLE users
    ADD COLUMN preferences jsonb NOT NULL DEFAULT '{}'::jsonb;
