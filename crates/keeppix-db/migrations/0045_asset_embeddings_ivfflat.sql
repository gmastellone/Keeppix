-- Fase 7 Task 11: indice vettoriale IVFFlat su asset_embeddings.
-- Scelto al posto di HNSW: build e RAM più leggeri su Pi 8 GB.
-- lists ≈ N/1000 per ~200k impronte (regola pgvector).
--
-- No-op se pgvector non è installato (stesso contratto di 0043).

DO $ai_idx$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_available_extensions WHERE name = 'vector'
    ) THEN
        RAISE NOTICE
            'keeppix: pgvector package missing; skipping AI vector index';
        RETURN;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_extension WHERE extname = 'vector'
    ) THEN
        RAISE NOTICE
            'keeppix: vector extension not enabled; skipping AI vector index';
        RETURN;
    END IF;

    IF to_regclass('public.asset_embeddings') IS NULL THEN
        RAISE NOTICE
            'keeppix: asset_embeddings missing; skipping AI vector index';
        RETURN;
    END IF;

    CREATE INDEX IF NOT EXISTS asset_embeddings_ivfflat_idx
        ON asset_embeddings
        USING ivfflat (embedding vector_cosine_ops)
        WITH (lists = 200);
END
$ai_idx$;
