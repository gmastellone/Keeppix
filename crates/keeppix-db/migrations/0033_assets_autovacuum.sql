-- Aggressive autovacuum on assets: index-only scans need a fresh visibility map.
-- Default scale factor 0.2 waits for ~20% dead tuples; on a large library that
-- leaves geometry/timeline plans degrading to heap fetches without error.
ALTER TABLE assets SET (autovacuum_vacuum_scale_factor = 0.05);
