-- Asymptote diagrams, rendered to SVG by myapps-challenges-prep and loaded
-- from the bundle. Keyed by the SHA-256 of the block's source (src/diagram.rs),
-- so a problem's text names its diagrams, identical ones are stored once, and
-- reloading a bundle never changes what a hash means.
CREATE TABLE challenges_diagrams (
    hash  TEXT PRIMARY KEY,
    svg   TEXT NOT NULL
);
