CREATE TABLE user_home_locations (
    user_id  uuid PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    location geography(Point, 4326) NOT NULL,
    radius_m integer NOT NULL DEFAULT 200 CHECK (radius_m > 0)
);
