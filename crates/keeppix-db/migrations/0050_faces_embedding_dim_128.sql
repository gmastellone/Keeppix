-- Task A (piano modelli IA, docs/superpowers/plans/2026-08-22-keeppix-
-- modelli-ai.md): SCRFD+ArcFace (mai scaricati, pesi research-only)
-- sostituiti da YuNet+SFace (MIT/Apache 2.0). `SFace` produce un'impronta
-- a 128 dimensioni, non 512 come `ArcFace` — verificato caricando il grafo
-- ONNX reale (`fc1: [1, 128]`) e sul commento ufficiale OpenCV
-- `samples/dnn/js_face_recognition.html`, "Get 128 floating points feature
-- vector". `0046_faces.sql` dichiarava `vector(512)`: nessuna riga reale a
-- cui pensare (i pesi precedenti non sono mai stati eseguiti, in nessun
-- ambiente — sandbox di sviluppo compresa, per lo stesso motivo di licenza
-- — quindi `embedding`/`centroid` sono sempre NULL ovunque questa
-- migrazione possa già essere stata applicata), ma una colonna già
-- spedita non si riscrive in place: nuova migrazione, stesso contratto
-- no-op di 0046 se pgvector manca.
--
-- pgvector non supporta un ALTER TYPE diretto vector(512) -> vector(128)
-- quando un indice ivfflat dipende dalla colonna (l'indice è costruito per
-- la dimensione dichiarata): si droppa l'indice, si cambia il tipo — USING
-- NULL perché ogni valore esistente è NULL per costruzione, un cast reale
-- non avrebbe senso per dati a 512 dimensioni che non esistono — poi si
-- ricrea l'indice sulla nuova dimensione, stessi parametri di 0046.
DO $faces_dim$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_available_extensions WHERE name = 'vector'
    ) THEN
        RAISE NOTICE
            'keeppix: pgvector package missing; skipping faces dim migration';
        RETURN;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables WHERE table_name = 'faces'
    ) THEN
        -- pgvector è disponibile ma 0046 non ha mai girato (ordine di
        -- migrazione anomalo, o pgvector installato dopo): niente da fare,
        -- 0046 stessa creerà già la colonna alla dimensione giusta se
        -- questo file venisse eventualmente rinumerato prima di lei — non
        -- il nostro caso qui, ma no-op sicuro comunque.
        RETURN;
    END IF;

    DROP INDEX IF EXISTS faces_embedding_ivfflat_idx;

    ALTER TABLE faces
        ALTER COLUMN embedding TYPE vector(128) USING NULL;
    ALTER TABLE persons
        ALTER COLUMN centroid TYPE vector(128) USING NULL;

    CREATE INDEX IF NOT EXISTS faces_embedding_ivfflat_idx
        ON faces USING ivfflat (embedding vector_cosine_ops) WITH (lists = 200);
END
$faces_dim$;
