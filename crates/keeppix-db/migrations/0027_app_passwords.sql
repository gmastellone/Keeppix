-- App-password: credenziali dedicate per client non interattivi (WebDAV,
-- Fase 5 Task 5+). Il segreto in chiaro non è mai persistito: solo l'hash
-- Argon2id in secret_hash, prodotto con lo stesso schema delle password di
-- login (spec fase-5-webdav-upload.md §2.1).
CREATE TABLE app_passwords (
    id           uuid PRIMARY KEY,
    user_id      uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    label        text NOT NULL,
    secret_hash  text NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    last_used_at timestamptz,
    revoked_at   timestamptz
);

CREATE INDEX app_passwords_user_idx ON app_passwords (user_id) WHERE revoked_at IS NULL;
