from pathlib import Path

from rose.cache.database import connect
from rose.cache.dataclasses import CachedArtist, CachedRelease
from rose.cache.read import list_albums
from rose.foundation.conf import Config


def seed_data(c: Config) -> None:
    with connect(c) as conn:
        conn.executescript(
            """\
INSERT INTO releases (id, source_path, title, release_type, release_year, new)
VALUES ('r1', '/tmp/r1', 'Release 1', 'album', 2023, true)
     , ('r2', '/tmp/r2', 'Release 2', 'album', 2021, false);

INSERT INTO releases_genres (release_id, genre)
VALUES ('r1', 'Techno')
     , ('r1', 'Deep House')
     , ('r2', 'Classical');

INSERT INTO releases_labels (release_id, label)
VALUES ('r1', 'Silk Music')
     , ('r2', 'Native State');

INSERT INTO tracks (id, source_path, title, release_id, track_number, disc_number, duration_seconds)
VALUES ('t1', '/tmp/r1/01.m4a', 'Track 1', 'r1', '01', '01', 120)
     , ('t2', '/tmp/r1/02.m4a', 'Track 2', 'r1', '02', '01', 240)
     , ('t3', '/tmp/r2/01.m4a', 'Track 1', 'r2', '01', '01', 120);

INSERT INTO releases_artists (release_id, artist, role)
VALUES ('r1', 'Techno Man', 'main')
     , ('r1', 'Bass Man', 'main')
     , ('r2', 'Violin Woman', 'main')
     , ('r2', 'Conductor Woman', 'guest');

INSERT INTO tracks_artists (track_id, artist, role)
VALUES ('t1', 'Techno Man', 'main')
     , ('t1', 'Bass Man', 'main')
     , ('t2', 'Techno Man', 'main')
     , ('t2', 'Bass Man', 'main')
     , ('t3', 'Violin Woman', 'main')
     , ('t3', 'Conductor Woman', 'guest');
        """
        )


def test_list_albums(config: Config) -> None:
    seed_data(config)
    albums = list(list_albums(config))
    assert albums == [
        CachedRelease(
            id="r1",
            source_path=Path("/tmp/r1"),
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
