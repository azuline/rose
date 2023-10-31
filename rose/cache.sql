CREATE TABLE locks (
    name TEXT,
    -- Unix epoch.
    valid_until REAL NOT NULL,
    PRIMARY KEY (name, valid_until)
);

CREATE TABLE release_type_enum (value TEXT PRIMARY KEY);
INSERT INTO release_type_enum (value) VALUES
    ('album'),
    ('single'),
    ('ep'),
    ('compilation'),
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
    virtual_dirname TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    release_type TEXT NOT NULL REFERENCES release_type_enum(value),
    release_year INTEGER,
    multidisc BOOLEAN NOT NULL,
    new BOOLEAN NOT NULL DEFAULT true,
    -- This is its own state because ordering matters--we preserve the ordering in the tags.
    -- However, the one-to-many table does not have ordering.
    formatted_artists TEXT NOT NULL
);
CREATE INDEX releases_source_path ON releases(source_path);
CREATE INDEX releases_release_year ON releases(release_year);
CREATE INDEX releases_title ON releases(release_title);
CREATE INDEX releases_type ON releases(release_type);

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
    virtual_filename TEXT NOT NULL,
    title TEXT NOT NULL,
    release_id TEXT NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
    track_number TEXT NOT NULL,
    disc_number TEXT NOT NULL,
    -- Formatted disc_number/track_number combination that prefixes the virtual_filename in the
    -- release view. This can be derived on-the-fly, but doesn't hurt to compute it once and pull it
    -- from the cache after.
    formatted_release_position TEXT NOT NULL,
    duration_seconds INTEGER NOT NULL,
    -- This is its own state because ordering matters--we preserve the ordering in the tags.
    -- However, the one-to-many table does not have ordering.
    formatted_artists TEXT NOT NULL,
    UNIQUE (release_id, virtual_filename)
);
CREATE INDEX tracks_source_path ON tracks(source_path);
CREATE INDEX tracks_release_id ON tracks(release_id);
CREATE INDEX tracks_ordering ON tracks(release_id, disc_number, track_number);
CREATE INDEX tracks_title ON tracks(title);
CREATE INDEX tracks_track_number ON tracks(track_number);
CREATE INDEX tracks_disc_number ON tracks(disc_number);

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
    alias BOOL NOT NULL,
    PRIMARY KEY (release_id, artist, role, alias)
);
CREATE INDEX releases_artists_release_id ON releases_artists(release_id);
CREATE INDEX releases_artists_artist ON releases_artists(artist);
CREATE INDEX releases_artists_artist_sanitized ON releases_artists(artist_sanitized);

CREATE TABLE tracks_artists (
    track_id TEXT REFERENCES tracks(id) ON DELETE CASCADE,
    artist TEXT,
    artist_sanitized TEXT NOT NULL,
    role TEXT REFERENCES artist_role_enum(value) NOT NULL,
    alias BOOL NOT NULL,
    PRIMARY KEY (track_id, artist, role, alias)
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
