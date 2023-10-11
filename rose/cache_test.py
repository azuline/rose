import hashlib
import shutil
from pathlib import Path

import pytest
import tomllib

from rose.cache import (
    CACHE_SCHEMA_PATH,
    STORED_DATA_FILE_NAME,
    CachedArtist,
    CachedRelease,
    CachedTrack,
    artist_exists,
    connect,
    cover_exists,
    genre_exists,
    get_release_files,
    label_exists,
    list_artists,
    list_genres,
    list_labels,
    list_releases,
    migrate_database,
    release_exists,
    track_exists,
    update_cache_for_all_releases,
    update_cache_for_release,
)
from rose.config import Config


def test_schema(config: Config) -> None:
    # Test that the schema successfully bootstraps.
    with CACHE_SCHEMA_PATH.open("rb") as fp:
        latest_schema_hash = hashlib.sha256(fp.read()).hexdigest()
    migrate_database(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT value FROM _schema_hash")
        assert cursor.fetchone()[0] == latest_schema_hash


def test_migration(config: Config) -> None:
    # Test that "migrating" the database correctly migrates it.
    config.cache_database_path.unlink()
    with connect(config) as conn:
        conn.execute("CREATE TABLE _schema_hash (value TEXT PRIMARY KEY)")
        conn.execute("INSERT INTO _schema_hash (value) VALUES ('haha')")

    with CACHE_SCHEMA_PATH.open("rb") as fp:
        latest_schema_hash = hashlib.sha256(fp.read()).hexdigest()
    migrate_database(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT value FROM _schema_hash")
        assert cursor.fetchone()[0] == latest_schema_hash
        cursor = conn.execute("SELECT COUNT(*) FROM _schema_hash")
        assert cursor.fetchone()[0] == 1


TESTDATA = Path(__file__).resolve().parent.parent / "testdata" / "cache"
TEST_RELEASE_1 = TESTDATA / "Test Release 1"
TEST_RELEASE_2 = TESTDATA / "Test Release 2"


def test_update_cache_for_release(config: Config) -> None:
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    update_cache_for_release(config, release_dir)

    # Check that the release directory was given a UUID.
    release_id: str | None = None
    for f in release_dir.iterdir():
        if f.name == STORED_DATA_FILE_NAME:
            with f.open("rb") as fp:
                release_id = tomllib.load(fp)["uuid"]
    assert release_id is not None

    # Assert that the release metadata was read correctly.
    with connect(config) as conn:
        cursor = conn.execute(
            """
            SELECT id, source_path, title, release_type, release_year, new
            FROM releases WHERE id = ?
            """,
            (release_id,),
        )
        row = cursor.fetchone()
        assert row["source_path"] == str(release_dir)
        assert row["title"] == "A Cool Album"
        assert row["release_type"] == "album"
        assert row["release_year"] == 1990
        assert row["new"]

        cursor = conn.execute(
            "SELECT genre FROM releases_genres WHERE release_id = ?",
            (release_id,),
        )
        genres = {r["genre"] for r in cursor.fetchall()}
        assert genres == {"Electronic", "House"}

        cursor = conn.execute(
            "SELECT label FROM releases_labels WHERE release_id = ?",
            (release_id,),
        )
        labels = {r["label"] for r in cursor.fetchall()}
        assert labels == {"A Cool Label"}

        cursor = conn.execute(
            "SELECT artist, role FROM releases_artists WHERE release_id = ?",
            (release_id,),
        )
        artists = {(r["artist"], r["role"]) for r in cursor.fetchall()}
        assert artists == {
            ("Artist A", "main"),
            ("Artist B", "main"),
        }

        for f in release_dir.iterdir():
            if f.suffix != ".m4a":
                continue

            # Assert that the track metadata was read correctly.
            cursor = conn.execute(
                """
                SELECT
                    id, source_path, title, release_id, track_number, disc_number, duration_seconds
                FROM tracks WHERE source_path = ?
                """,
                (str(f),),
            )
            row = cursor.fetchone()
            track_id = row["id"]
            assert row["title"] == "Title"
            assert row["release_id"] == release_id
            assert row["track_number"] != ""
            assert row["disc_number"] == "1"
            assert row["duration_seconds"] == 2

            cursor = conn.execute(
                "SELECT artist, role FROM tracks_artists WHERE track_id = ?",
                (track_id,),
            )
            artists = {(r["artist"], r["role"]) for r in cursor.fetchall()}
            assert artists == {
                ("Artist GH", "main"),
                ("Artist HI", "main"),
                ("Artist C", "guest"),
                ("Artist A", "guest"),
                ("Artist AB", "remixer"),
                ("Artist BC", "remixer"),
                ("Artist CD", "producer"),
                ("Artist DE", "producer"),
                ("Artist EF", "composer"),
                ("Artist FG", "composer"),
                ("Artist IJ", "djmixer"),
                ("Artist JK", "djmixer"),
            }


def test_update_cache_with_existing_id(config: Config) -> None:
    """Test that IDs in filenames are read and preserved."""
    release_dir = config.music_source_dir / TEST_RELEASE_2.name
    shutil.copytree(TEST_RELEASE_2, release_dir)
    update_cache_for_release(config, release_dir)

    # Check that the release directory was given a UUID.
    release_id: str | None = None
    for f in release_dir.iterdir():
        if f.name == STORED_DATA_FILE_NAME:
            with f.open("rb") as fp:
                release_id = tomllib.load(fp)["uuid"]
    assert release_id == "ilovecarly"  # Hardcoded ID for testing.


def test_update_cache_for_all_releases(config: Config) -> None:
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)

    # Test that we prune deleted releases too.
    with connect(config) as conn:
        conn.execute(
            """
            INSERT INTO releases (id, source_path, virtual_dirname, title, release_type)
            VALUES ('aaaaaa', '/nonexistent', 'nonexistent', 'aa', 'unknown')
            """
        )

    update_cache_for_all_releases(config)

    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 2
        cursor = conn.execute("SELECT COUNT(*) FROM tracks")
        assert cursor.fetchone()[0] == 4


@pytest.mark.usefixtures("seeded_cache")
def test_list_releases(config: Config) -> None:
    albums = list(list_releases(config))
    assert albums == [
        CachedRelease(
            id="r1",
            source_path=Path(config.music_source_dir / "r1"),
            cover_image_path=None,
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
            source_path=Path(config.music_source_dir / "r2"),
            cover_image_path=Path(config.music_source_dir / "r2" / "cover.jpg"),
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
            source_path=Path(config.music_source_dir / "r1"),
            cover_image_path=None,
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
            source_path=Path(config.music_source_dir / "r1"),
            cover_image_path=None,
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
            source_path=Path(config.music_source_dir / "r1"),
            cover_image_path=None,
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


@pytest.mark.usefixtures("seeded_cache")
def test_get_release_files(config: Config) -> None:
    rf = get_release_files(config, "r1")
    assert rf.tracks == [
        CachedTrack(
            id="t1",
            source_path=Path(config.music_source_dir / "r1" / "01.m4a"),
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
            source_path=Path(config.music_source_dir / "r1" / "02.m4a"),
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
    assert rf.cover is None

    rf = get_release_files(config, "r2")
    assert rf.cover == config.music_source_dir / "r2" / "cover.jpg"


@pytest.mark.usefixtures("seeded_cache")
def test_list_artists(config: Config) -> None:
    artists = list(list_artists(config))
    assert set(artists) == {"Techno Man", "Bass Man", "Violin Woman", "Conductor Woman"}


@pytest.mark.usefixtures("seeded_cache")
def test_list_genres(config: Config) -> None:
    genres = list(list_genres(config))
    assert set(genres) == {"Techno", "Deep House", "Classical"}


@pytest.mark.usefixtures("seeded_cache")
def test_list_labels(config: Config) -> None:
    labels = list(list_labels(config))
    assert set(labels) == {"Silk Music", "Native State"}


@pytest.mark.usefixtures("seeded_cache")
def test_release_exists(config: Config) -> None:
    assert release_exists(config, "r1")
    assert not release_exists(config, "lalala")


@pytest.mark.usefixtures("seeded_cache")
def test_track_exists(config: Config) -> None:
    assert track_exists(config, "r1", "01.m4a")
    assert not track_exists(config, "lalala", "lalala")
    assert not track_exists(config, "r1", "lalala")


@pytest.mark.usefixtures("seeded_cache")
def test_cover_exists(config: Config) -> None:
    assert cover_exists(config, "r2", "cover.jpg")
    assert not cover_exists(config, "r2", "cover.png")
    assert not cover_exists(config, "r1", "cover.jpg")


@pytest.mark.usefixtures("seeded_cache")
def test_artist_exists(config: Config) -> None:
    assert artist_exists(config, "Bass Man")
    assert not artist_exists(config, "lalala")


@pytest.mark.usefixtures("seeded_cache")
def test_genre_exists(config: Config) -> None:
    assert genre_exists(config, "Deep House")
    assert not genre_exists(config, "lalala")


@pytest.mark.usefixtures("seeded_cache")
def test_label_exists(config: Config) -> None:
    assert label_exists(config, "Silk Music")
    assert not label_exists(config, "Cotton Music")
