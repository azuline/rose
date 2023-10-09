-- bootstrap
-- depends: 

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
    id INTEGER PRIMARY KEY,
    source_path TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    release_type TEXT NOT NULL REFERENCES release_type_enum(value),
    release_year INTEGER
);
CREATE INDEX releases_source_path ON releases(source_path);
CREATE INDEX releases_release_year ON releases(release_year);

CREATE TABLE tracks (
    id INTEGER PRIMARY KEY,
    source_path TEXT NOT NULL UNIQUE,
    source_mtime TIMESTAMP NOT NULL,
    title TEXT NOT NULL,
    release_id INTEGER NOT NULL REFERENCES releases(id),
    track_number TEXT NOT NULL,
    disc_number TEXT NOT NULL,
    duration_seconds INTEGER NOT NULL
);
CREATE INDEX tracks_source_path ON tracks(source_path);
CREATE INDEX tracks_release_id ON tracks(release_id);
CREATE INDEX tracks_ordering ON tracks(release_id, disc_number, track_number);

CREATE TABLE artists (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);
CREATE INDEX artists_name ON artists(name);

CREATE TABLE artist_role_enum (value TEXT PRIMARY KEY);
INSERT INTO artist_role_enum (value) VALUES
    ('main'),
    ('feature'),
    ('remixer'),
    ('producer'),
    ('composer'),
    ('conductor'),
    ('djmixer');

CREATE TABLE releases_artists (
    release_id INTEGER REFERENCES releases(id) ON DELETE CASCADE,
    artist_id INTEGER REFERENCES artists(id) ON DELETE CASCADE,
    role TEXT REFERENCES artist_role_enum(value),
    PRIMARY KEY (release_id, artist_id)
);
CREATE INDEX releases_artists_release_id ON releases_artists(release_id);
CREATE INDEX releases_artists_artist_id ON releases_artists(artist_id);

CREATE TABLE tracks_artists (
    track_id INTEGER REFERENCES tracks(id) ON DELETE CASCADE,
    artist_id INTEGER REFERENCES artists(id) ON DELETE CASCADE,
    role TEXT REFERENCES artist_role_enum(value),
    PRIMARY KEY (track_id, artist_id)
);
CREATE INDEX tracks_artists_track_id ON tracks_artists(track_id);
CREATE INDEX tracks_artists_artist_id ON tracks_artists(artist_id);

CREATE TABLE collections (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    source_path TEXT UNIQUE NOT NULL
    source_mtime TIMESTAMP NOT NULL
);
CREATE INDEX collections_source_path ON collections(source_path);

CREATE TABLE collections_releases (
    collection_id INTEGER REFERENCES collections(id) ON DELETE CASCADE,
    release_id INTEGER REFERENCES releases(id) ON DELETE CASCADE,
    position INTEGER NOT NULL
);
CREATE INDEX collections_releases_collection_id ON collections_releases(collection_id);
CREATE INDEX collections_releases_release_id ON collections_releases(release_id);
CREATE UNIQUE INDEX collections_releases_collection_position ON collections_releases(collection_id, position);

CREATE TABLE playlists (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    source_path TEXT UNIQUE NOT NULL,
    source_mtime TIMESTAMP NOT NULL
);
CREATE INDEX playlists_source_path ON playlists(source_path);

CREATE TABLE playlists_tracks (
    playlist_id INTEGER REFERENCES playlists(id) ON DELETE CASCADE,
    track_id INTEGER REFERENCES tracks(id) ON DELETE CASCADE,
    position INTEGER NOT NULL
);
CREATE INDEX playlists_tracks_playlist_id ON playlists_tracks(playlist_id);
CREATE INDEX playlists_tracks_track_id ON playlists_tracks(track_id);
CREATE UNIQUE INDEX playlists_tracks_playlist_position ON playlists_tracks(playlist_id, position);
