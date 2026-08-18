-- Album virtuali: nessuno storage, una foto può stare in N album.
-- Ordinamento manuale tramite `position` (gap di 1000 per facilitare il riordino).

CREATE TABLE albums (
    id              uuid        PRIMARY KEY,
    name            text        NOT NULL,
    description     text        NOT NULL DEFAULT '',
    owner_id        uuid        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    cover_asset_id  uuid        REFERENCES assets(id) ON DELETE SET NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX albums_owner_idx ON albums (owner_id);

CREATE TABLE album_assets (
    album_id    uuid        NOT NULL REFERENCES albums(id) ON DELETE CASCADE,
    asset_id    uuid        NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    position    bigint      NOT NULL DEFAULT 0,
    added_by    uuid        NOT NULL REFERENCES users(id),
    added_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (album_id, asset_id)
);

CREATE INDEX album_assets_album_pos_idx ON album_assets (album_id, position);
CREATE INDEX album_assets_asset_idx     ON album_assets (asset_id);
