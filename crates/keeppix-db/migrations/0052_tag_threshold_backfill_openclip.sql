-- Segue 0051 (Task B, piano modelli IA): quella ha corretto solo il
-- DEFAULT di colonna per i tag NUOVI. Un tag già creato prima del fix —
-- con l'editor che mandava sempre threshold=0.75, o chiunque abbia
-- impostato a mano un valore pensando a una confidenza 0-100 — resta
-- fermo su un numero che il cosine score reale di OpenCLIP XLM-R IT/EN
-- (0,10-0,20 anche per abbinamenti corretti) non raggiunge mai: quel tag
-- non si assegna mai in automatico né arriva in coda di revisione, morto
-- alla nascita, silenziosamente.
--
-- 0.5 come soglia della UPDATE, non 0.75 esatto: qualunque valore sopra
-- mezzo è chiaramente pensato per la vecchia scala 0-1, mai per la scala
-- reale (il tetto osservato dei falsi positivi più confondibili è 0,177).
-- Un tag già corretto a mano dopo il fix di oggi (nel range realistico
-- 0,05-0,40) non viene toccato.
--
-- No-op se pgvector non è installato o `tags` non esiste (stesso
-- contratto di 0043/0044/0045/0051).

DO $tag_threshold_backfill$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_available_extensions WHERE name = 'vector'
    ) THEN
        RAISE NOTICE
            'keeppix: pgvector package missing; skipping tags.threshold backfill';
        RETURN;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_extension WHERE extname = 'vector'
    ) THEN
        RAISE NOTICE
            'keeppix: vector extension not enabled; skipping tags.threshold backfill';
        RETURN;
    END IF;

    IF to_regclass('public.tags') IS NULL THEN
        RAISE NOTICE
            'keeppix: tags missing; skipping tags.threshold backfill';
        RETURN;
    END IF;

    UPDATE tags SET threshold = 0.20 WHERE threshold >= 0.5;
END
$tag_threshold_backfill$;
