-- Sessioni di upload riprendibili in stile tus (spec §1.5). Il temporaneo
-- vive dentro la libreria (`.keeppix-tmp/`), così il rename() finale è sullo
-- stesso filesystem e resta atomico anche per un file da 2 GB.
CREATE TABLE upload_sessions (
    id                uuid PRIMARY KEY,
    user_id           uuid REFERENCES users(id) ON DELETE CASCADE,
    share_link_id     uuid REFERENCES share_links(id) ON DELETE CASCADE,
    target_folder_id  uuid NOT NULL REFERENCES folders(id),
    filename          text NOT NULL,
    expected_size     bigint NOT NULL,
    expected_hash     bytea,
    received_bytes    bigint NOT NULL DEFAULT 0,
    temp_path         text NOT NULL,
    client_mtime      timestamptz,
    expires_at        timestamptz NOT NULL,
    created_at        timestamptz NOT NULL DEFAULT now(),
    -- Esattamente uno dei due: un upload appartiene a un utente autenticato
    -- OPPURE a un link condiviso con allow_upload, mai a entrambi o a nessuno.
    CONSTRAINT upload_sessions_one_actor CHECK (
        (user_id IS NOT NULL) <> (share_link_id IS NOT NULL)
    )
);

CREATE INDEX upload_sessions_expires_idx ON upload_sessions (expires_at);
