-- Ricerche salvate (album intelligenti). pg_trgm è già in 0001.
CREATE TABLE saved_searches (
    id          uuid PRIMARY KEY,
    owner_id    uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name        text NOT NULL,
    query_text  text NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX saved_searches_owner_idx ON saved_searches (owner_id, created_at DESC);

CREATE INDEX assets_filename_trgm ON assets USING gin (filename gin_trgm_ops);
