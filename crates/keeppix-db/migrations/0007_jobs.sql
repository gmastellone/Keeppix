-- Coda dei job di ingestione. Claim con SKIP LOCKED (spec 1b §2.3).
-- Le righe done/failed restano per diagnostica; la pulizia è manutenzione.

CREATE TABLE jobs (
    id           bigserial   PRIMARY KEY,
    kind         text        NOT NULL,
    payload      jsonb       NOT NULL,
    priority     smallint    NOT NULL DEFAULT 3
                             CHECK (priority BETWEEN 0 AND 3),
    status       text        NOT NULL DEFAULT 'pending'
                             CHECK (status IN ('pending','running','done','failed')),
    attempts     int         NOT NULL DEFAULT 0,
    max_attempts int         NOT NULL DEFAULT 3,
    last_error   text,
    run_after    timestamptz NOT NULL DEFAULT now(),
    locked_by    uuid,
    locked_at    timestamptz,
    created_at   timestamptz NOT NULL DEFAULT now(),
    dedup_key    text
);

CREATE INDEX jobs_claim_idx ON jobs (priority, run_after, id)
    WHERE status = 'pending';

CREATE UNIQUE INDEX jobs_dedup_key ON jobs (dedup_key)
    WHERE dedup_key IS NOT NULL AND status IN ('pending', 'running');

CREATE INDEX jobs_stale_idx ON jobs (locked_at) WHERE status = 'running';
