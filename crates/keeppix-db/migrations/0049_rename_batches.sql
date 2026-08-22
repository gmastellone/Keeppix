-- Registro delle rinomine di massa, per l'annullamento (Fase 9 Task 8-9).
-- Stessa forma di metadata_batches (migrazione 0012): il piano lo chiede
-- esplicitamente ("si riusa il concetto di raggruppamento e controllo di
-- metadata_batches, stesso batch_id, stesso audit"). Tabella separata, non
-- la stessa: metadata_batches.previous è per colonne di asset_overrides,
-- qui previous è (folder_id, filename) per asset — forme diverse, stesso
-- schema di registro non forzato in un'unica tabella innaturale.
CREATE TABLE rename_batches (
    id          uuid PRIMARY KEY,
    actor_id    uuid NOT NULL REFERENCES users(id),
    applied_at  timestamptz NOT NULL DEFAULT now(),
    undone_at   timestamptz,
    -- { "<asset_id>": { "folder_id": "...", "filename": "..." } } — solo
    -- gli asset rinominati con successo: un fallimento parziale non
    -- registra nulla per gli asset che sono rimasti com'erano.
    previous    jsonb NOT NULL
);
