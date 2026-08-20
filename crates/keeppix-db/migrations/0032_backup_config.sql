-- Backup destinations and run history (Fase 6 Tasks 3–4).
-- Spec §2: destinations are local / S3-compatible / WebDAV / SFTP;
-- credentials live encrypted in `config` jsonb.

CREATE TABLE backup_destinations (
    id            uuid PRIMARY KEY,
    kind          text NOT NULL CHECK (kind IN ('local', 's3', 'webdav', 'sftp')),
    label         text NOT NULL,
    config        jsonb NOT NULL,
    enabled       boolean NOT NULL DEFAULT true,
    created_at    timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE backup_runs (
    id              uuid PRIMARY KEY,
    destination_id  uuid REFERENCES backup_destinations (id) ON DELETE SET NULL,
    started_at      timestamptz NOT NULL DEFAULT now(),
    completed_at    timestamptz,
    size_bytes      bigint,
    verified_at     timestamptz,
    status          text NOT NULL CHECK (status IN ('running', 'ok', 'failed')),
    error           text,
    path            text,
    keeppix_version text
);

CREATE INDEX backup_runs_started_idx ON backup_runs (started_at DESC);
