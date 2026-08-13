-- Estensioni richieste dallo schema completo. PostGIS arriva in Fase 4 ma
-- l'immagine è già postgis/postgis, quindi la si abilita subito per evitare
-- una migrazione che richieda privilegi elevati più avanti.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE users (
    id            uuid        PRIMARY KEY,
    username      text        NOT NULL,
    email         text,
    display_name  text        NOT NULL,
    password_hash text        NOT NULL,
    role          text        NOT NULL CHECK (role IN ('admin', 'user')),
    locale        text,
    totp_secret_enc bytea,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    disabled_at   timestamptz
);

-- Unicità case-insensitive: gli username sono già normalizzati in minuscolo
-- dal dominio, questo indice impedisce che un bug futuro crei duplicati.
CREATE UNIQUE INDEX users_username_key ON users (lower(username));
CREATE UNIQUE INDEX users_email_key ON users (lower(email)) WHERE email IS NOT NULL;

CREATE TABLE groups (
    id         uuid        PRIMARY KEY,
    name       text        NOT NULL,
    created_by uuid        REFERENCES users (id) ON DELETE SET NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX groups_name_key ON groups (lower(name));

CREATE TABLE group_members (
    group_id uuid        NOT NULL REFERENCES groups (id) ON DELETE CASCADE,
    user_id  uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    added_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (group_id, user_id)
);

CREATE INDEX group_members_user_idx ON group_members (user_id);
