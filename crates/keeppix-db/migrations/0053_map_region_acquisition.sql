ALTER TABLE map_regions
    ADD COLUMN acquisition text NOT NULL DEFAULT 'download'
        CHECK (acquisition IN ('download', 'extract'));
