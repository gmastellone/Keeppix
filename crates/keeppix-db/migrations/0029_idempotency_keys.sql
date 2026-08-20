CREATE TABLE idempotency_keys (
    key             text PRIMARY KEY,
    user_id         uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    response_status smallint NOT NULL,
    response_body   jsonb,
    created_at      timestamptz NOT NULL DEFAULT now()
);
