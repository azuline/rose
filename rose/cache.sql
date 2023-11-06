CREATE TABLE locks (
    name TEXT,
    -- Unix epoch.
    valid_until REAL NOT NULL,
    PRIMARY KEY (name, valid_until)
);

CREATE TABLE releasetype_enum (value TEXT PRIMARY KEY);
INSERT INTO releasetype_enum (value) VALUES
    ('album'),
    ('single'),
    ('ep'),
    ('compilation'),
    ('anthology'),
    ('soundtrack'),
    ('live'),
    ('remix'),
    ('djmix'),
    ('mixtape'),
    ('other'),
    ('unknown');

CREATE TABLE releases (
    id TEXT PRIMARY KEY,
    source_path TEXT NOT NULL UNIQUE,
    cover_image_path TEXT,
    -- ISO8601 timestamp.
    added_at TEXT NOT NULL,
    datafile_mtime TEXT NOT NULL,
    title TEXT NOT NULL,
    releasetype TEXT NOT NULL REFERENCES releasetype_enum(value),
    year INTEGER,
    multidisc BOOLEAN NOT NULL,
    new BOOLEAN NOT NULL DEFAULT true
);
CREATE INDEX releases_source_path ON releases(source_path);
CREATE INDEX releases_year ON releases(year);
CREATE INDEX releases_title ON releases(title);
CREATE INDEX releases_type ON releases(releasetype);

CREATE TABLE releases_genres (
    release_id TEXT REFERENCES releases(id) ON DELETE CASCADE,
    genre TEXT,
    genre_sanitized TEXT NOT NULL,
    PRIMARY KEY (release_id, genre)
);
CREATE INDEX releases_genres_release_id ON releases_genres(release_id);
CREATE INDEX releases_genres_genre ON releases_genres(genre);
CREATE INDEX releases_genres_genre_sanitized ON releases_genres(genre_sanitized);

CREATE TABLE releases_labels (
    release_id TEXT REFERENCES releases(id) ON DELETE CASCADE,
    label TEXT,
    label_sanitized TEXT NOT NULL,
    PRIMARY KEY (release_id, label)
);
CREATE INDEX releases_labels_release_id ON releases_labels(release_id);
CREATE INDEX releases_labels_label ON releases_labels(label);
CREATE INDEX releases_labels_label_sanitized ON releases_labels(label_sanitized);

CREATE TABLE tracks (
    id TEXT PRIMARY KEY,
    source_path TEXT NOT NULL UNIQUE,
    source_mtime TEXT NOT NULL,
    title TEXT NOT NULL,
    release_id TEXT NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
    tracknumber TEXT NOT NULL,
    discnumber TEXT NOT NULL,
    duration_seconds INTEGER NOT NULL
);
CREATE INDEX tracks_source_path ON tracks(source_path);
CREATE INDEX tracks_release_id ON tracks(release_id);
CREATE INDEX tracks_ordering ON tracks(release_id, discnumber, tracknumber);
CREATE INDEX tracks_title ON tracks(title);
CREATE INDEX tracks_tracknumber ON tracks(tracknumber);
CREATE INDEX tracks_discnumber ON tracks(discnumber);

CREATE TABLE artist_role_enum (value TEXT PRIMARY KEY);
INSERT INTO artist_role_enum (value) VALUES
    ('main'),
    ('guest'),
    ('remixer'),
    ('producer'),
    ('composer'),
    ('djmixer');

CREATE TABLE releases_artists (
    release_id TEXT REFERENCES releases(id) ON DELETE CASCADE,
    artist TEXT,
    artist_sanitized TEXT NOT NULL,
    role TEXT REFERENCES artist_role_enum(value) NOT NULL,
    PRIMARY KEY (release_id, artist, role)
);
CREATE INDEX releases_artists_release_id ON releases_artists(release_id);
CREATE INDEX releases_artists_artist ON releases_artists(artist);
CREATE INDEX releases_artists_artist_sanitized ON releases_artists(artist_sanitized);

CREATE TABLE tracks_artists (
    track_id TEXT REFERENCES tracks(id) ON DELETE CASCADE,
    artist TEXT,
    artist_sanitized TEXT NOT NULL,
    role TEXT REFERENCES artist_role_enum(value) NOT NULL,
    PRIMARY KEY (track_id, artist, role)
);
CREATE INDEX tracks_artists_track_id ON tracks_artists(track_id);
CREATE INDEX tracks_artists_artist ON tracks_artists(artist);
CREATE INDEX tracks_artists_artist_sanitized ON tracks_artists(artist_sanitized);

CREATE TABLE collages (
    name TEXT PRIMARY KEY,
    source_mtime TEXT NOT NULL
);

CREATE TABLE collages_releases (
    collage_name TEXT REFERENCES collages(name) ON DELETE CASCADE,
    -- We used to have a foreign key here with ON DELETE CASCADE, but we now do
    -- not, because not having one makes Rose resilient to temporary track
    -- deletion and reinsertion.
    release_id TEXT,
    position INTEGER NOT NULL,
    missing BOOL NOT NULL
);
CREATE INDEX collages_releases_collage_name ON collages_releases(collage_name);
CREATE INDEX collages_releases_release_id ON collages_releases(release_id);
CREATE INDEX collages_releases_access ON collages_releases(collage_name, missing, release_id);

CREATE TABLE playlists (
    name TEXT PRIMARY KEY,
    source_mtime TEXT NOT NULL,
    cover_path TEXT
);

CREATE TABLE playlists_tracks (
    playlist_name TEXT REFERENCES playlists(name) ON DELETE CASCADE,
    -- We used to have a foreign key here with ON DELETE CASCADE, but we now do
    -- not, because not having one makes Rose resilient to temporary track
    -- deletion and reinsertion, which happens during the cache update process
    -- if a release is renamed.
    track_id TEXT,
    position INTEGER NOT NULL,
    missing BOOL NOT NULL
);
CREATE INDEX playlists_tracks_playlist_name ON playlists_tracks(playlist_name);
CREATE INDEX playlists_tracks_track_id ON playlists_tracks(track_id);
CREATE INDEX playlists_tracks_access ON playlists_tracks(playlist_name, missing, track_id);

-- A full text search setup for rules engine performance. The point of this table is to enable
-- performant substring search, without requiring us to do a bunch of custom shit (just yet!). So
-- our shitty working hack is to create a shitload of single-character tokens.
--
-- We sync the virtual table with the source data by hand at the end of the
-- cache update sequence. We don't use automatic triggers in order to avoid
-- write amplification potentially affecting cache update performance.
CREATE VIRTUAL TABLE rules_engine_fts USING fts5 (
    tracktitle
  , tracknumber
  , discnumber
  , albumtitle
  , year
  , releasetype
  , genre
  , label
  , albumartist
  , trackartist
  -- Use standard unicode tokenizer; do not remove diacritics; treat everything we know as token.
  -- Except for the ¬, which is our "separator." We use that separator to produce single-character
  -- tokens.
  , tokenize="unicode61 remove_diacritics 0 categories 'L* M* N* P* S* Z* C*' separators '¬'"
);

-- These are views that we use when fetching entities. They aggregate associated relations into a
-- single view. We use ` ¬ ` as a delimiter for joined values, hoping that there are no conflicts.

CREATE VIEW releases_view AS
    WITH genres AS (
        SELECT
            release_id
          , GROUP_CONCAT(genre, ' ¬ ') AS genres
        FROM (SELECT * FROM releases_genres ORDER BY genre)
        GROUP BY release_id
    ), labels AS (
        SELECT
            release_id
          , GROUP_CONCAT(label, ' ¬ ') AS labels
        FROM (SELECT * FROM releases_labels ORDER BY label)
        GROUP BY release_id
    ), artists AS (
        SELECT
            release_id
          , GROUP_CONCAT(artist, ' ¬ ') AS names
          , GROUP_CONCAT(role, ' ¬ ') AS roles
        FROM (SELECT * FROM releases_artists ORDER BY artist, role)
        GROUP BY release_id
    )
    SELECT
        r.id
      , r.source_path
      , r.cover_image_path
      , r.added_at
      , r.datafile_mtime
      , r.title
      , r.releasetype
      , r.year
      , r.multidisc
      , r.new
      , COALESCE(g.genres, '') AS genres
      , COALESCE(l.labels, '') AS labels
      , COALESCE(a.names, '') AS artist_names
      , COALESCE(a.roles, '') AS artist_roles
    FROM releases r
    LEFT JOIN genres g ON g.release_id = r.id
    LEFT JOIN labels l ON l.release_id = r.id
    LEFT JOIN artists a ON a.release_id = r.id;

CREATE VIEW tracks_view AS
    WITH artists AS (
        SELECT
            track_id
          , GROUP_CONCAT(artist, ' ¬ ') AS names
          , GROUP_CONCAT(role, ' ¬ ') AS roles
        FROM (SELECT * FROM tracks_artists ORDER BY artist, role)
        GROUP BY track_id
    )
    SELECT
        t.id
      , t.source_path
      , t.source_mtime
      , t.title
      , t.release_id
      , t.tracknumber
      , t.discnumber
      , t.duration_seconds
      , r.multidisc
      , COALESCE(a.names, '') AS artist_names
      , COALESCE(a.roles, '') AS artist_roles
    FROM tracks t
    JOIN releases r ON r.id = t.release_id
    LEFT JOIN artists a ON a.track_id = t.id;
