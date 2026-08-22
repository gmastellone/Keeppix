-- Fase 8 Task 4: traccia quali asset sono già stati passati al rilevatore.
--
-- `faces` da sola non basta: un asset senza volti produce zero righe, e zero
-- righe è indistinguibile da "non ancora analizzato" senza un marcatore
-- esplicito. Stesso ruolo di `asset_embeddings` per Fase 7 (un embedding per
-- asset è sempre prodotto), ma qui il numero di righe in `faces` per asset è
-- 0..N, quindi serve una tabella dedicata invece di riusare `faces` come
-- marcatore.
--
-- Stesso contratto no-op di 0043/0045/0046 se pgvector non è installato: la
-- pipeline volti non gira comunque senza lo schema `faces`.

DO $face_scans$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_available_extensions WHERE name = 'vector'
    ) THEN
        RAISE NOTICE
            'keeppix: pgvector package missing; skipping face scan tracking';
        RETURN;
    END IF;

    IF to_regclass('public.faces') IS NULL THEN
        RAISE NOTICE
            'keeppix: faces table missing; skipping face scan tracking';
        RETURN;
    END IF;

    CREATE TABLE IF NOT EXISTS asset_face_scans (
        asset_id      uuid PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
        model_version text NOT NULL,
        scanned_at    timestamptz NOT NULL DEFAULT now()
    );

    CREATE INDEX IF NOT EXISTS asset_face_scans_model_idx
        ON asset_face_scans (model_version);
END
$face_scans$;
