-- Performance indexes for Task 12 (Fase 6).
--
-- Trigram indexes let ILIKE '%...%' on camera/lens fields use the index
-- instead of a sequential scan.  The pg_trgm extension is already enabled
-- since migration 0001.
--
-- The two FK indexes cover join paths that the query planner would otherwise
-- resolve with seqscans on stacks and album_assets.

CREATE INDEX IF NOT EXISTS asset_exif_camera_trgm
    ON asset_exif USING gin (camera_model gin_trgm_ops);

CREATE INDEX IF NOT EXISTS asset_exif_lens_trgm
    ON asset_exif USING gin (lens gin_trgm_ops);

CREATE INDEX IF NOT EXISTS stacks_primary_asset_idx
    ON stacks (primary_asset_id);

CREATE INDEX IF NOT EXISTS album_assets_added_by_idx
    ON album_assets (added_by);
