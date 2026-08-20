-- Il filtro di primario di pila (Task 3, fase-10) aggiunge `a.stack_id` alla
-- WHERE di /timeline/geometry (via LEFT JOIN stacks + confronto col primario)
-- e `a.kind <> 'unknown'` per restare coerente con /timeline e con i nuovi
-- /timeline/buckets che non leggono più da folder_month_counts. Senza
-- ricreare l'indice, quelle due colonne forzerebbero una lettura dalla heap
-- riga per riga (niente più index-only scan) su `assets_geometry_idx`
-- (migrazione 0034): le si aggiunge all'INCLUDE.
DROP INDEX assets_geometry_idx;
CREATE INDEX assets_geometry_idx ON assets (folder_id, taken_at_utc DESC, id DESC)
    INCLUDE (width, height, stack_id, kind) WHERE status = 'indexed';
