CREATE TABLE locks (
    name TEXT,
    -- Unix epoch.
    valid_until REAL NOT NULL,
    PRIMARY KEY (name, valid_until)
);

CREATE TABLE releases (
    id TEXT PRIMARY KEY,
    source_path TEXT NOT NULL UNIQUE,
    cover_image_path TEXT,
    -- ISO8601 timestamp.
    added_at TEXT NOT NULL,
    datafile_mtime TEXT NOT NULL,
    title TEXT NOT NULL,
    releasetype TEXT NOT NULL,
    releaseyear INTEGER,
    compositionyear INTEGER,
    catalognumber TEXT,
    disctotal INTEGER NOT NULL,
    -- A sha256() of the release object, which can be used as a performant cache
    -- key.
    metahash TEXT NOT NULL UNIQUE,
    new BOOLEAN NOT NULL DEFAULT true
);
CREATE INDEX releases_source_path ON releases(source_path);
CREATE INDEX releases_new ON releases(new);

CREATE TABLE releases_genres (
    release_id TEXT REFERENCES releases(id) ON DELETE CASCADE,
    genre TEXT,
    position INTEGER NOT NULL,
    PRIMARY KEY (release_id, genre),
    UNIQUE (release_id, position)
);
CREATE INDEX releases_genres_release_id_position ON releases_genres(release_id, position);
CREATE INDEX releases_genres_genre ON releases_genres(genre);

CREATE TABLE releases_labels (
    release_id TEXT REFERENCES releases(id) ON DELETE CASCADE,
    label TEXT,
    position INTEGER NOT NULL,
    PRIMARY KEY (release_id, label),
    UNIQUE (release_id, position)
);
CREATE INDEX releases_labels_release_id_position ON releases_labels(release_id, position);
CREATE INDEX releases_labels_label ON releases_labels(label);

CREATE TABLE tracks (
    id TEXT PRIMARY KEY,
    source_path TEXT NOT NULL UNIQUE,
    source_mtime TEXT NOT NULL,
    title TEXT NOT NULL,
    release_id TEXT NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
    tracknumber TEXT NOT NULL,
    -- Per-disc track total.
    tracktotal INTEGER NOT NULL,
    discnumber TEXT NOT NULL,
    duration_seconds INTEGER NOT NULL,
    -- A sha256 of the release object, which can be used as a performant cache
    -- key.
    metahash TEXT NOT NULL UNIQUE
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
    ('conductor'),
    ('djmixer');

CREATE TABLE releases_artists (
    release_id TEXT REFERENCES releases(id) ON DELETE CASCADE,
    artist TEXT,
    role TEXT REFERENCES artist_role_enum(value) NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY (release_id, artist, role)
    UNIQUE (release_id, position)
);
CREATE INDEX releases_artists_release_id_position ON releases_artists(release_id, position);
CREATE INDEX releases_artists_artist ON releases_artists(artist);

CREATE TABLE tracks_artists (
    track_id TEXT REFERENCES tracks(id) ON DELETE CASCADE,
    artist TEXT,
    role TEXT REFERENCES artist_role_enum(value) NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY (track_id, artist, role),
    UNIQUE (track_id, position)
);
CREATE INDEX tracks_artists_track_id_position ON tracks_artists(track_id, position);
CREATE INDEX tracks_artists_artist ON tracks_artists(artist);

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
  , tracktotal
  , discnumber
  , disctotal
  , releasetitle
  , releasetype
  , releaseyear
  , compositionyear
  , catalognumber
  , genre
  , label
  , releaseartist
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
        FROM (SELECT * FROM releases_genres ORDER BY position)
        GROUP BY release_id
    ), labels AS (
        SELECT
            release_id
          , GROUP_CONCAT(label, ' ¬ ') AS labels
        FROM (SELECT * FROM releases_labels ORDER BY position)
        GROUP BY release_id
    ), artists AS (
        SELECT
            release_id
          , GROUP_CONCAT(artist, ' ¬ ') AS names
          , GROUP_CONCAT(role, ' ¬ ') AS roles
        FROM (SELECT * FROM releases_artists ORDER BY release_id, position)
        GROUP BY release_id
    )
    SELECT
        r.id
      , r.source_path
      , r.cover_image_path
      , r.added_at
      , r.datafile_mtime
      , r.title AS releasetitle
      , r.releasetype
      , r.releaseyear
      , r.compositionyear
      , r.catalognumber
      , r.disctotal
      , r.new
      , r.metahash
      , COALESCE(g.genres, '') AS genres
      , COALESCE(l.labels, '') AS labels
      , COALESCE(a.names, '') AS releaseartist_names
      , COALESCE(a.roles, '') AS releaseartist_roles
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
        FROM (SELECT * FROM tracks_artists ORDER BY track_id, position)
        GROUP BY track_id
    )
    SELECT
        t.id
      , t.source_path
      , t.source_mtime
      , t.title AS tracktitle
      , t.release_id
      , t.tracknumber
      , t.tracktotal
      , t.discnumber
      , t.duration_seconds
      , t.metahash
      , COALESCE(a.names, '') AS trackartist_names
      , COALESCE(a.roles, '') AS trackartist_roles
    FROM tracks t
    LEFT JOIN artists a ON a.track_id = t.id;
