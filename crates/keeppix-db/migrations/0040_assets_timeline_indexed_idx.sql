-- Task 12 (Fase 10): partial index aligned with TimelineRepo::page filters.
CREATE INDEX assets_timeline_indexed_idx ON assets (taken_at_utc DESC, id DESC)
    WHERE status = 'indexed' AND kind <> 'unknown';
