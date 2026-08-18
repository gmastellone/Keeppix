CREATE TABLE places (
    id           bigint PRIMARY KEY,
    name         text NOT NULL,
    ascii_name   text NOT NULL,
    country_code char(2),
    admin1       text,
    admin2       text,
    location     geography(Point, 4326) NOT NULL,
    population   int NOT NULL DEFAULT 0
);

CREATE INDEX places_location_gist ON places USING gist (location);
CREATE INDEX places_ascii_trgm ON places USING gin (ascii_name gin_trgm_ops);
CREATE INDEX places_population_idx ON places (population DESC);
