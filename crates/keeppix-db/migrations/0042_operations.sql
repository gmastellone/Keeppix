-- Operazioni lunghe con avanzamento e annullamento (Fase 10 Task 16).
-- `owner_id` è chi ha avviato l'operazione: solo lui (o un admin) la vede o
-- la annulla. `succeeded_asset_ids` è l'involucro di riuscita parziale
-- (BulkOutcome) accumulato mentre l'operazione procede — annullare a metà
-- non lo svuota, lo congela.
CREATE TABLE operations (
    id                   uuid PRIMARY KEY,
    kind                 text NOT NULL,
    owner_id             uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status               text NOT NULL DEFAULT 'running',
    done                 bigint NOT NULL DEFAULT 0,
    total                bigint,
    phase                text NOT NULL DEFAULT '',
    cancel_requested     boolean NOT NULL DEFAULT false,
    succeeded_asset_ids  uuid[] NOT NULL DEFAULT '{}',
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX operations_owner_status_idx ON operations (owner_id, status);
