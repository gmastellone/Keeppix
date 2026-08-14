-- Estensioni richieste dallo schema completo. PostGIS serve solo dalla Fase 4,
-- ma la si abilita subito, con `pg_trgm`, per una ragione di privilegi:
-- `pg_trgm` è *trusted* da PG13 e la può creare il proprietario del database,
-- `postgis` non lo è e richiede il superuser. Farlo ora, su un database vuoto
-- creato dall'amministratore, evita che la Fase 4 chieda un `CREATE EXTENSION`
-- privilegiato su un'istanza gestita già piena di dati. L'immagine di
-- riferimento è `postgis/postgis:17-3.5` (compose e harness di test), e
-- `docs/DEPLOY.md` chiede PostGIS disponibile anche sui Postgres esterni.
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS postgis;

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
