from pathlib import Path

from rose.cache.database import connect
from rose.cache.dataclasses import CachedArtist, CachedRelease, CachedTrack
from rose.cache.read import (
    artist_exists,
    genre_exists,
    label_exists,
    list_artists,
    list_genres,
    list_labels,
    list_releases,
    list_tracks,
    release_exists,
    track_exists,
)
from rose.foundation.conf import Config


def seed_data(c: Config) -> None:
    with connect(c) as conn:
        conn.executescript(
            """\
INSERT INTO releases (id, source_path, virtual_dirname, title, release_type, release_year, new)
VALUES ('r1', '/tmp/r1', 'r1', 'Release 1', 'album', 2023, true)
     , ('r2', '/tmp/r2', 'r2', 'Release 2', 'album', 2021, false);

INSERT INTO releases_genres (release_id, genre, genre_sanitized)
VALUES ('r1', 'Techno', 'Techno')
     , ('r1', 'Deep House', 'Deep House')
     , ('r2', 'Classical', 'Classical');

INSERT INTO releases_labels (release_id, label, label_sanitized)
VALUES ('r1', 'Silk Music', 'Silk Music')
     , ('r2', 'Native State', 'Native State');

INSERT INTO tracks
(id, source_path, virtual_filename, title, release_id, track_number, disc_number, duration_seconds)
VALUES ('t1', '/tmp/r1/01.m4a', '01.m4a', 'Track 1', 'r1', '01', '01', 120)
     , ('t2', '/tmp/r1/02.m4a', '02.m4a', 'Track 2', 'r1', '02', '01', 240)
     , ('t3', '/tmp/r2/01.m4a', '01.m4a', 'Track 1', 'r2', '01', '01', 120);

INSERT INTO releases_artists (release_id, artist, artist_sanitized, role)
VALUES ('r1', 'Techno Man', 'Techno Man', 'main')
     , ('r1', 'Bass Man', 'Bass Man', 'main')
     , ('r2', 'Violin Woman', 'Violin Woman', 'main')
     , ('r2', 'Conductor Woman', 'Conductor Woman', 'guest');

INSERT INTO tracks_artists (track_id, artist, artist_sanitized, role)
VALUES ('t1', 'Techno Man', 'Techno Man', 'main')
     , ('t1', 'Bass Man', 'Bass Man', 'main')
     , ('t2', 'Techno Man', 'Techno Man', 'main')
     , ('t2', 'Bass Man', 'Bass Man', 'main')
     , ('t3', 'Violin Woman', 'Violin Woman', 'main')
     , ('t3', 'Conductor Woman', 'Conductor Woman', 'guest');
        """
        )


def test_list_releases(config: Config) -> None:
    seed_data(config)
    albums = list(list_releases(config))
    assert albums == [
        CachedRelease(
            id="r1",
            source_path=Path("/tmp/r1"),
            virtual_dirname="r1",
            title="Release 1",
            release_type="album",
            release_year=2023,
            new=True,
            genres=["Deep House", "Techno"],
            labels=["Silk Music"],
            artists=[
                CachedArtist(name="Techno Man", role="main"),
                CachedArtist(name="Bass Man", role="main"),
            ],
        ),
        CachedRelease(
            id="r2",
            source_path=Path("/tmp/r2"),
            virtual_dirname="r2",
            title="Release 2",
            release_type="album",
            release_year=2021,
            new=False,
            genres=["Classical"],
            labels=["Native State"],
            artists=[
                CachedArtist(name="Violin Woman", role="main"),
                CachedArtist(name="Conductor Woman", role="guest"),
            ],
        ),
    ]

    assert list(list_releases(config, sanitized_artist_filter="Techno Man")) == [
        CachedRelease(
            id="r1",
            source_path=Path("/tmp/r1"),
            virtual_dirname="r1",
            title="Release 1",
            release_type="album",
            release_year=2023,
            new=True,
            genres=["Deep House", "Techno"],
            labels=["Silk Music"],
            artists=[
                CachedArtist(name="Techno Man", role="main"),
                CachedArtist(name="Bass Man", role="main"),
            ],
        ),
    ]

    assert list(list_releases(config, sanitized_genre_filter="Techno")) == [
        CachedRelease(
            id="r1",
            source_path=Path("/tmp/r1"),
            virtual_dirname="r1",
            title="Release 1",
            release_type="album",
            release_year=2023,
            new=True,
            genres=["Deep House", "Techno"],
            labels=["Silk Music"],
            artists=[
                CachedArtist(name="Techno Man", role="main"),
                CachedArtist(name="Bass Man", role="main"),
            ],
        ),
    ]

    assert list(list_releases(config, sanitized_label_filter="Silk Music")) == [
        CachedRelease(
            id="r1",
            source_path=Path("/tmp/r1"),
            virtual_dirname="r1",
            title="Release 1",
            release_type="album",
            release_year=2023,
            new=True,
            genres=["Deep House", "Techno"],
            labels=["Silk Music"],
            artists=[
                CachedArtist(name="Techno Man", role="main"),
                CachedArtist(name="Bass Man", role="main"),
            ],
        ),
    ]


def test_list_tracks(config: Config) -> None:
    seed_data(config)
    tracks = list(list_tracks(config, "r1"))
    assert tracks == [
        CachedTrack(
            id="t1",
            source_path=Path("/tmp/r1/01.m4a"),
            virtual_filename="01.m4a",
            title="Track 1",
            release_id="r1",
            track_number="01",
            disc_number="01",
            duration_seconds=120,
            artists=[
                CachedArtist(name="Techno Man", role="main"),
                CachedArtist(name="Bass Man", role="main"),
            ],
        ),
        CachedTrack(
            id="t2",
            source_path=Path("/tmp/r1/02.m4a"),
            virtual_filename="02.m4a",
            title="Track 2",
            release_id="r1",
            track_number="02",
            disc_number="01",
            duration_seconds=240,
            artists=[
                CachedArtist(name="Techno Man", role="main"),
                CachedArtist(name="Bass Man", role="main"),
            ],
        ),
    ]


def test_list_artists(config: Config) -> None:
    seed_data(config)
    artists = list(list_artists(config))
    assert set(artists) == {"Techno Man", "Bass Man", "Violin Woman", "Conductor Woman"}


def test_list_genres(config: Config) -> None:
    seed_data(config)
    genres = list(list_genres(config))
    assert set(genres) == {"Techno", "Deep House", "Classical"}


def test_list_labels(config: Config) -> None:
    seed_data(config)
    labels = list(list_labels(config))
    assert set(labels) == {"Silk Music", "Native State"}


def test_release_exists(config: Config) -> None:
    seed_data(config)
    assert release_exists(config, "r1")
    assert not release_exists(config, "lalala")


def test_track_exists(config: Config) -> None:
    seed_data(config)
    assert track_exists(config, "r1", "01.m4a")
    assert not track_exists(config, "lalala", "lalala")
    assert not track_exists(config, "r1", "lalala")


def test_artist_exists(config: Config) -> None:
    seed_data(config)
    assert artist_exists(config, "Bass Man")
    assert not artist_exists(config, "lalala")


def test_genre_exists(config: Config) -> None:
    seed_data(config)
    assert genre_exists(config, "Deep House")
    assert not genre_exists(config, "lalala")


def test_label_exists(config: Config) -> None:
    seed_data(config)
    assert label_exists(config, "Silk Music")
    assert not label_exists(config, "Cotton Music")
