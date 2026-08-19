-- Lock `WebDAV` Class 2 (Task 8, Fase 5): richiesti da Finder e Windows
-- Explorer prima di scrivere un file — senza risposta a `LOCK`/`UNLOCK` i
-- client nativi si rifiutano di salvare, non solo "vanno più lento".
--
-- `resource_path` è il percorso opaco `/dav/...` usato dal client per
-- indirizzare la risorsa (mai un percorso filesystem, stesso principio del
-- resto di `dav::mod`) — non è unico: nulla impedisce due lock su percorsi
-- diversi, ma anche una riga scaduta rimasta per errore non blocca un nuovo
-- lock sullo stesso percorso (i confronti sono sempre `timeout_at > now()`).
CREATE TABLE dav_locks (
    token         text PRIMARY KEY,
    resource_path text NOT NULL,
    owner         text,
    depth         text NOT NULL,
    timeout_at    timestamptz NOT NULL,
    created_at    timestamptz NOT NULL DEFAULT now()
);

-- `LOCK` senza header `If:` deve poter verificare in fretta se il percorso
-- ha già un lock attivo, senza conoscerne il token.
CREATE INDEX dav_locks_resource_path_idx ON dav_locks (resource_path);
