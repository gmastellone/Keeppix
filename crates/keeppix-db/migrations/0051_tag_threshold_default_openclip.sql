-- Task B (piano modelli IA): ricalibra il default di tags.threshold sui
-- punteggi reali di OpenCLIP XLM-R IT/EN. 0.75 assumeva implicitamente una
-- scala "confidenza" 0-1; la cosine similarity testo-immagine vera in
-- questo spazio di embedding sta a 0,10-0,20 anche per abbinamenti
-- corretti (banco CI reale, run bcf9b4a) — con 0.75 nessuna proposta
-- sarebbe mai stata generata. Solo il default di colonna: le righe
-- esistenti (e ogni threshold passato esplicitamente) non cambiano.
--
-- No-op se pgvector non è installato o `tags` non esiste (stesso contratto
-- di 0043/0044/0045).

DO $tag_threshold$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_available_extensions WHERE name = 'vector'
    ) THEN
        RAISE NOTICE
            'keeppix: pgvector package missing; skipping tags.threshold default';
        RETURN;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_extension WHERE extname = 'vector'
    ) THEN
        RAISE NOTICE
            'keeppix: vector extension not enabled; skipping tags.threshold default';
        RETURN;
    END IF;

    IF to_regclass('public.tags') IS NULL THEN
        RAISE NOTICE
            'keeppix: tags missing; skipping tags.threshold default';
        RETURN;
    END IF;

    ALTER TABLE tags
        ALTER COLUMN threshold SET DEFAULT 0.20;
END
$tag_threshold$;
