import shutil
from pathlib import Path

from rose.cache.database import connect
from rose.cache.update import ID_REGEX, update_cache_for_all_releases, update_cache_for_release
from rose.foundation.conf import Config

TESTDATA = Path(__file__).resolve().parent / "testdata"
TEST_RELEASE_1 = TESTDATA / "Test Release 1"
TEST_RELEASE_2 = TESTDATA / "Test Release 2 {id=ilovecarly}"


def test_update_cache_for_release(config: Config) -> None:
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    updated_release_dir = update_cache_for_release(config, release_dir)

    # Check that the release directory was given a UUID.
    m = ID_REGEX.search(updated_release_dir.name)
    assert m is not None
    release_id = m[1]

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
        assert row["source_path"] == str(updated_release_dir)
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

        for f in updated_release_dir.iterdir():
            if f.suffix != ".m4a":
                continue

            # Check that the track file was given a UUID.
            m = ID_REGEX.search(f.stem)
            assert m is not None
            track_id = m[1]

            # Assert that the track metadata was read correctly.
            cursor = conn.execute(
                """
                SELECT
                    id, source_path, title, release_id, track_number, disc_number, duration_seconds
                FROM tracks WHERE id = ?
                """,
                (track_id,),
            )
            row = cursor.fetchone()
            assert row["source_path"] == str(f)
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
    updated_release_dir = update_cache_for_release(config, release_dir)
    assert release_dir == updated_release_dir

    with connect(config) as conn:
        m = ID_REGEX.search(release_dir.name)
        assert m is not None
        release_id = m[1]
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM releases WHERE id = ?)", (release_id,))
        assert cursor.fetchone()[0]

        for f in release_dir.iterdir():
            if f.suffix != ".m4a":
                continue

            # Check that the track file was given a UUID.
            m = ID_REGEX.search(f.stem)
            assert m is not None
            track_id = m[1]
            cursor = conn.execute("SELECT EXISTS(SELECT * FROM tracks WHERE id = ?)", (track_id,))
            assert cursor.fetchone()[0]


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


def test_update_cache_with_dotted_dirname(config: Config) -> None:
    # Regression test: If we use with_stem on a directory with a dot, then the directory will be
    # renamed to like Put.ID.After.The {id=abc}.Dot" which we don't want.
    release_dir = config.music_source_dir / "Put.ID.After.The.Dot"
    shutil.copytree(TEST_RELEASE_1, release_dir)
    updated_release_dir = update_cache_for_release(config, release_dir)
    m = ID_REGEX.search(updated_release_dir.name)
    assert m is not None

    # Regression test 2: Don't create a new ID; read the existing ID.
    updated_release_dir2 = update_cache_for_release(config, updated_release_dir)
    assert updated_release_dir2 == updated_release_dir
