CREATE TABLE tz_boundaries (
    tz_name  text PRIMARY KEY,
    boundary geography(MultiPolygon, 4326) NOT NULL
);

CREATE INDEX tz_boundaries_gist ON tz_boundaries USING gist (boundary);
