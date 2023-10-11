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
    virtual_dirname TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    release_type TEXT NOT NULL REFERENCES release_type_enum(value),
    release_year INTEGER,
    new BOOLEAN NOT NULL DEFAULT true
);
CREATE INDEX releases_source_path ON releases(source_path);
CREATE INDEX releases_release_year ON releases(release_year);

CREATE TABLE releases_genres (
    release_id TEXT,
    genre TEXT,
    genre_sanitized TEXT NOT NULL,
    PRIMARY KEY (release_id, genre)
);
CREATE INDEX releases_genres_genre ON releases_genres(genre);
CREATE INDEX releases_genres_genre_sanitized ON releases_genres(genre_sanitized);

CREATE TABLE releases_labels (
    release_id TEXT,
    label TEXT,
    label_sanitized TEXT NOT NULL,
    PRIMARY KEY (release_id, label)
);
CREATE INDEX releases_labels_label ON releases_labels(label);
CREATE INDEX releases_labels_label_sanitized ON releases_labels(label_sanitized);

CREATE TABLE tracks (
    id TEXT PRIMARY KEY,
    source_path TEXT NOT NULL UNIQUE,
    virtual_filename TEXT NOT NULL,
    title TEXT NOT NULL,
    release_id TEXT NOT NULL REFERENCES releases(id),
    track_number TEXT NOT NULL,
    disc_number TEXT NOT NULL,
    duration_seconds INTEGER NOT NULL,
    UNIQUE (release_id, virtual_filename)
);
CREATE INDEX tracks_source_path ON tracks(source_path);
CREATE INDEX tracks_release_id ON tracks(release_id);
CREATE INDEX tracks_ordering ON tracks(release_id, disc_number, track_number);

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
    PRIMARY KEY (release_id, artist)
);
CREATE INDEX releases_artists_release_id ON releases_artists(release_id);
CREATE INDEX releases_artists_artist ON releases_artists(artist);
CREATE INDEX releases_artists_artist_sanitized ON releases_artists(artist_sanitized);

CREATE TABLE tracks_artists (
    track_id TEXT REFERENCES tracks(id) ON DELETE CASCADE,
    artist TEXT,
    artist_sanitized TEXT NOT NULL,
    role TEXT REFERENCES artist_role_enum(value) NOT NULL,
    PRIMARY KEY (track_id, artist)
);
CREATE INDEX tracks_artists_track_id ON tracks_artists(track_id);
CREATE INDEX tracks_artists_artist ON tracks_artists(artist);
CREATE INDEX tracks_artists_artist_sanitized ON tracks_artists(artist_sanitized);

CREATE TABLE collections (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    source_path TEXT UNIQUE NOT NULL
);
CREATE INDEX collections_source_path ON collections(source_path);

CREATE TABLE collections_releases (
    collection_id TEXT REFERENCES collections(id) ON DELETE CASCADE,
    release_id TEXT REFERENCES releases(id) ON DELETE CASCADE,
    position INTEGER NOT NULL
);
CREATE INDEX collections_releases_collection_id ON collections_releases(collection_id);
CREATE INDEX collections_releases_release_id ON collections_releases(release_id);
CREATE UNIQUE INDEX collections_releases_collection_position ON collections_releases(collection_id, position);

CREATE TABLE playlists (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    source_path TEXT UNIQUE NOT NULL
);
CREATE INDEX playlists_source_path ON playlists(source_path);

CREATE TABLE playlists_tracks (
    playlist_id TEXT REFERENCES playlists(id) ON DELETE CASCADE,
    track_id TEXT REFERENCES tracks(id) ON DELETE CASCADE,
    position INTEGER NOT NULL
);
CREATE INDEX playlists_tracks_playlist_id ON playlists_tracks(playlist_id);
CREATE INDEX playlists_tracks_track_id ON playlists_tracks(track_id);
CREATE UNIQUE INDEX playlists_tracks_playlist_position ON playlists_tracks(playlist_id, position);
