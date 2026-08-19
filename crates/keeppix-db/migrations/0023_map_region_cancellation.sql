ALTER TABLE map_regions
    ADD COLUMN cancel_requested boolean NOT NULL DEFAULT false;
