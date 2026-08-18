-- Colonna che traccia gli upload da ospite (via link pubblico con
-- allow_upload). I file in coda di revisione hanno questo flag = true
-- finché il proprietario approva o scarta.
ALTER TABLE assets ADD COLUMN uploaded_by_guest boolean NOT NULL DEFAULT false;

-- Coda di revisione: associa un asset a upload caricato tramite un link.
-- `approved_at`/`rejected_at` NULL = in attesa. Un asset approvato rimane
-- nella libreria; uno rifiutato viene rimosso fisicamente (gestito
-- dall'applicazione, non da un trigger, per tenere la logica nel codice).
CREATE TABLE guest_upload_queue (
    id              uuid PRIMARY KEY,
    asset_id        uuid NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    share_link_id   uuid NOT NULL REFERENCES share_links(id) ON DELETE CASCADE,
    filename        text NOT NULL,
    size_bytes      bigint NOT NULL,
    uploaded_at     timestamptz NOT NULL DEFAULT now(),
    approved_at     timestamptz,
    rejected_at     timestamptz,
    reviewed_by     uuid REFERENCES users(id) ON DELETE SET NULL,
    CONSTRAINT guest_upload_queue_single_outcome
        CHECK (
            (approved_at IS NULL) OR (rejected_at IS NULL)
        )
);

CREATE INDEX guest_upload_queue_link_idx
    ON guest_upload_queue (share_link_id, uploaded_at DESC);

CREATE INDEX guest_upload_queue_pending_idx
    ON guest_upload_queue (share_link_id)
    WHERE approved_at IS NULL AND rejected_at IS NULL;
