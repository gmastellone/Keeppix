-- `ltree` rende "tutto ciò che sta sotto questa cartella" una singola
-- condizione indicizzata (`path <@ prefisso`) invece di una ricorsione.
-- È un'estensione trusted: non richiede privilegi di superuser.
CREATE EXTENSION IF NOT EXISTS ltree;

CREATE TABLE libraries (
    id               uuid        PRIMARY KEY,
    name             text        NOT NULL,
    owner_id         uuid        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    root_path        text        NOT NULL,
    -- Contatore per-libreria delle etichette ltree (`path`).
    next_folder_seq  bigint      NOT NULL DEFAULT 1,
    scan_enabled     boolean     NOT NULL DEFAULT true,
    exclude_patterns text[]      NOT NULL DEFAULT '{}',
    -- 'active' | 'offline' : offline significa "path non raggiungibile",
    -- stato in cui la scansione si ferma e non viene cancellato nulla.
    status           text        NOT NULL DEFAULT 'active'
                                 CHECK (status IN ('active', 'offline')),
    last_scan_at     timestamptz,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now()
);

-- Due librerie che indicizzano lo stesso albero produrrebbero asset duplicati
-- con cancellazioni ambigue.
CREATE UNIQUE INDEX libraries_root_path_key ON libraries (root_path);
CREATE INDEX libraries_owner_idx ON libraries (owner_id);

CREATE TABLE folders (
    id         uuid        PRIMARY KEY,
    library_id uuid        NOT NULL REFERENCES libraries (id) ON DELETE CASCADE,
    parent_id  uuid        REFERENCES folders (id) ON DELETE CASCADE,
    -- Nome così come appare sul filesystem: spazi, accenti, qualsiasi cosa.
    -- La radice della libreria ha nome vuoto.
    name       text        NOT NULL,
    -- Percorso materializzato. Le etichette sono numeri progressivi per
    -- libreria, non nomi: `ltree` ammette solo [A-Za-z0-9_-] e "Matrimonio
    -- Rossi 2024" non è un'etichetta valida.
    path       ltree       NOT NULL,
    depth      int         NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

-- La condizione che serve a ogni query di sottoalbero.
CREATE INDEX folders_path_gist ON folders USING gist (path);
CREATE INDEX folders_library_idx ON folders (library_id);
CREATE INDEX folders_parent_idx ON folders (parent_id);

-- Un percorso identifica una cartella dentro la sua libreria.
CREATE UNIQUE INDEX folders_library_path_key ON folders (library_id, path);

-- Due sorelle non possono avere lo stesso nome. `parent_id` è NULL per la
-- radice, e in Postgres NULL non è uguale a NULL: serve un indice separato
-- che imponga una sola radice per libreria.
CREATE UNIQUE INDEX folders_sibling_name_key
    ON folders (parent_id, name) WHERE parent_id IS NOT NULL;
CREATE UNIQUE INDEX folders_single_root_key
    ON folders (library_id) WHERE parent_id IS NULL;

-- Contatori per la scrollbar della timeline, mantenuti da trigger in 1c.
-- La tabella nasce qui perché `assets` la referenzia concettualmente e
-- crearla dopo significherebbe ricalcolarla su tutta la libreria.
CREATE TABLE folder_month_counts (
    folder_id   uuid  NOT NULL REFERENCES folders (id) ON DELETE CASCADE,
    month       date  NOT NULL,
    asset_count int   NOT NULL DEFAULT 0,
    PRIMARY KEY (folder_id, month)
);
