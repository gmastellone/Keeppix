-- Una "famiglia" è la catena di refresh token nata da un singolo login.
-- Il riuso di un token già consumato indica furto: si revoca l'intera famiglia.
CREATE TABLE sessions (
    id                uuid        PRIMARY KEY,
    family_id         uuid        NOT NULL,
    user_id           uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    refresh_token_hash bytea      NOT NULL,
    parent_id         uuid        REFERENCES sessions (id) ON DELETE SET NULL,
    user_agent        text,
    ip                inet,
    created_at        timestamptz NOT NULL DEFAULT now(),
    expires_at        timestamptz NOT NULL,
    consumed_at       timestamptz,
    revoked_at        timestamptz
);

CREATE UNIQUE INDEX sessions_refresh_hash_key ON sessions (refresh_token_hash);
CREATE INDEX sessions_family_idx ON sessions (family_id);
CREATE INDEX sessions_user_idx ON sessions (user_id);
CREATE INDEX sessions_expiry_idx ON sessions (expires_at) WHERE revoked_at IS NULL;
