-- Link pubblici. Il token in chiaro esiste solo nel client; in database
-- solo l'hash SHA-256 (bytea, non text: evita dipendenze dall'encoding).
CREATE TABLE share_links (
    id              uuid PRIMARY KEY,
    token_hash      bytea NOT NULL,
    object_type     text NOT NULL CHECK (object_type IN ('asset','folder','album')),
    object_id       uuid NOT NULL,
    created_by      uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    password_hash   text,
    expires_at      timestamptz,
    max_views       int,
    view_count      int NOT NULL DEFAULT 0,
    allow_download  boolean NOT NULL DEFAULT true,
    allow_original  boolean NOT NULL DEFAULT false,
    allow_upload    boolean NOT NULL DEFAULT false,
    allow_cdn_cache boolean NOT NULL DEFAULT false,
    hide_metadata   boolean NOT NULL DEFAULT true,
    upload_quota_bytes bigint,
    revoked_at      timestamptz,
    last_accessed_at timestamptz,
    created_at      timestamptz NOT NULL DEFAULT now()
);

-- Lookup a tempo costante per token (il percorso caldo).
CREATE UNIQUE INDEX share_links_token_hash_key ON share_links (token_hash);

-- Pannello «Condivisioni»: tutti i link creati da un utente.
CREATE INDEX share_links_creator_idx ON share_links (created_by, created_at DESC);
