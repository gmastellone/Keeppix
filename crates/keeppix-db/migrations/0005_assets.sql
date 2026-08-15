-- Identità = (folder_id, filename). content_hash è indicizzato, non unico:
-- la stessa foto in due cartelle sono due asset, con cancellazioni indipendenti.
CREATE TABLE assets (
    id          uuid        PRIMARY KEY,
    folder_id   uuid        NOT NULL REFERENCES folders (id) ON DELETE CASCADE,
    filename    text        NOT NULL,

    -- blake3, NULL finché la fase di hash non è passata.
    content_hash bytea,

    size_bytes  bigint      NOT NULL,
    mtime       timestamptz NOT NULL,
    inode       bigint,

    kind        text        NOT NULL DEFAULT 'unknown'
                            CHECK (kind IN ('image', 'raw_image', 'video', 'unknown')),
    status      text        NOT NULL DEFAULT 'discovered'
                            CHECK (status IN ('discovered','indexed','offline','error','trashed')),
    error_detail text,

    -- Normalizzata in UTC dal fuso ricavato dal GPS quando c'è; altrimenti
    -- dall'ora locale del file. È la colonna su cui ordina la timeline.
    taken_at_utc timestamptz,
    tz_offset_minutes int,

    width       int,
    height      int,
    duration_ms int,

    -- Predisposte per le fasi successive: aggiungere colonne a una tabella
    -- con 200.000 righe costa, prevederle no.
    location    geography(Point, 4326),
    place_id    bigint,
    location_source text CHECK (location_source IN ('exif','user','map_pin','copied','gpx')),
    stack_id    uuid,

    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);

-- L'identità: un file in una cartella.
CREATE UNIQUE INDEX assets_folder_filename_key ON assets (folder_id, filename);

-- Duplicati e rilevamento degli spostamenti.
CREATE INDEX assets_content_hash_idx ON assets (content_hash) WHERE content_hash IS NOT NULL;

-- L'ordinamento della timeline: (data, id) come chiave di paginazione
-- keyset, che non degrada come OFFSET.
CREATE INDEX assets_timeline_idx ON assets (taken_at_utc DESC, id DESC)
    WHERE status = 'indexed';

CREATE INDEX assets_folder_idx ON assets (folder_id);
CREATE INDEX assets_status_idx ON assets (status) WHERE status IN ('discovered', 'error');
CREATE INDEX assets_location_gist ON assets USING gist (location) WHERE location IS NOT NULL;

-- EXIF grezzi, mai riscritti. Le modifiche dell'utente vivranno in
-- asset_overrides (Fase 2), e il valore mostrato sarà COALESCE(override, exif).
-- I campi fotocamera stanno qui, non su assets: si filtra su di essi e lo
-- spec §2.4 li mette accanto al jsonb.
CREATE TABLE asset_exif (
    asset_id  uuid  PRIMARY KEY REFERENCES assets (id) ON DELETE CASCADE,
    raw       jsonb NOT NULL,
    camera_make  text,
    camera_model text,
    lens         text,
    iso          int,
    f_number     real,
    exposure     text,
    focal_length real,
    parsed_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX asset_exif_camera_idx ON asset_exif (camera_model) WHERE camera_model IS NOT NULL;
