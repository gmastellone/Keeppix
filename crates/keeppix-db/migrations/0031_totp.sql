-- TOTP 2FA (Fase 6 Task 5). Spec §3 / plan Task 5.
-- `users.totp_secret_enc` from 0001 is unused; these tables are the source of
-- truth. `last_used_step` blocks reuse of the same code in the same 30s interval.

CREATE TABLE totp_secrets (
    user_id         uuid PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    secret_enc      bytea NOT NULL,
    enabled_at      timestamptz,
    last_used_step  bigint,
    created_at      timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE totp_recovery_codes (
    id         uuid PRIMARY KEY,
    user_id    uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    code_hash  text NOT NULL,
    used_at    timestamptz
);

CREATE INDEX totp_recovery_codes_user_idx
    ON totp_recovery_codes (user_id)
    WHERE used_at IS NULL;
