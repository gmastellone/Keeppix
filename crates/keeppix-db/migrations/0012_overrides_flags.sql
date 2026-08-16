-- Metadati originali immutabili, modifiche accanto (spec fase-2-raw-culling
-- §3). Il valore mostrato è COALESCE(override, exif).
CREATE TABLE asset_overrides (
    asset_id       uuid PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
    title          text,
    description    text,
    taken_at       timestamptz,
    location       geography(Point, 4326),
    place_id       bigint,
    orientation    smallint,
    updated_by     uuid REFERENCES users(id),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    -- NULL = mai scritto su file. Il job dei sidecar seleziona
    -- WHERE updated_at > COALESCE(xmp_written_at, '-infinity').
    xmp_written_at timestamptz
);

CREATE INDEX asset_overrides_pending_idx ON asset_overrides (updated_at)
    WHERE xmp_written_at IS NULL OR xmp_written_at < updated_at;

-- Culling per utente (spec §4.1): il 5 stelle di uno non è il 5 stelle di
-- un altro.
CREATE TABLE asset_flags (
    asset_id    uuid NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    user_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    rating      smallint CHECK (rating BETWEEN 0 AND 5),
    pick        text CHECK (pick IN ('none','pick','reject')),
    color_label text,
    updated_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (asset_id, user_id)
);

-- Il culling filtra per "gli scarti di questo utente": indice su (user_id, pick).
CREATE INDEX asset_flags_user_pick_idx ON asset_flags (user_id, pick)
    WHERE pick <> 'none';

-- Registro delle operazioni batch, per l'annullamento.
CREATE TABLE metadata_batches (
    id          uuid PRIMARY KEY,
    actor_id    uuid NOT NULL REFERENCES users(id),
    applied_at  timestamptz NOT NULL DEFAULT now(),
    undone_at   timestamptz,
    -- Valori precedenti, per asset. Serve solo all'annullamento.
    previous    jsonb NOT NULL
);
