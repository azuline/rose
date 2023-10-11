import shutil
from pathlib import Path

import tomllib

from rose.cache.database import connect
from rose.cache.update import (
    STORED_DATA_FILE_NAME,
    update_cache_for_all_releases,
    update_cache_for_release,
)
from rose.foundation.conf import Config

TESTDATA = Path(__file__).resolve().parent / "testdata"
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
