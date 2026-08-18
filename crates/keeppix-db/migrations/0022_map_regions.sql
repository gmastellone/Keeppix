CREATE TABLE map_regions (
    id              text PRIMARY KEY
                    CHECK (id ~ '^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$'),
    label           text NOT NULL,
    file_path       text NOT NULL,
    size_bytes      bigint NOT NULL CHECK (size_bytes > 0),
    version         text NOT NULL,
    downloaded_at   timestamptz,
    status          text NOT NULL
                    CHECK (status IN ('available', 'downloading', 'error')),
    source_url      text NOT NULL,
    checksum_sha256 text NOT NULL
                    CHECK (checksum_sha256 ~ '^[0-9a-fA-F]{64}$'),
    downloaded_bytes bigint NOT NULL DEFAULT 0
                    CHECK (downloaded_bytes >= 0),
    last_error      text
);
