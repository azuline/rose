import dataclasses
import hashlib
import shutil
import time
from pathlib import Path

import pytest
import tomllib

from conftest import TEST_COLLAGE_1, TEST_PLAYLIST_1, TEST_RELEASE_1, TEST_RELEASE_2, TEST_RELEASE_3
from rose.audiotags import AudioTags
from rose.cache import (
    CACHE_SCHEMA_PATH,
    STORED_DATA_FILE_REGEX,
    CachedCollage,
    CachedPlaylist,
    CachedRelease,
    CachedTrack,
    _unpack,
    artist_exists,
    connect,
    genre_exists,
    get_collage,
    get_playlist,
    get_release,
    get_release_logtext,
    get_releases_associated_with_tracks,
    get_track,
    get_track_logtext,
    get_tracks_associated_with_release,
    get_tracks_associated_with_releases,
    label_exists,
    list_artists,
    list_collages,
    list_genres,
    list_labels,
    list_playlists,
    list_releases,
    list_tracks,
    lock,
    maybe_invalidate_cache_database,
    update_cache,
    update_cache_evict_nonexistent_releases,
    update_cache_for_releases,
)
from rose.common import VERSION, Artist, ArtistMapping
from rose.config import Config


def test_schema(config: Config) -> None:
    """Test that the schema successfully bootstraps."""
    with CACHE_SCHEMA_PATH.open("rb") as fp:
        schema_hash = hashlib.sha256(fp.read()).hexdigest()
    maybe_invalidate_cache_database(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT schema_hash, config_hash, version FROM _schema_hash")
        row = cursor.fetchone()
        assert row["schema_hash"] == schema_hash
        assert row["config_hash"] is not None
        assert row["version"] == VERSION


def test_migration(config: Config) -> None:
    """Test that "migrating" the database correctly migrates it."""
    config.cache_database_path.unlink()
    with connect(config) as conn:
        conn.execute(
            """
            CREATE TABLE _schema_hash (
                schema_hash TEXT
              , config_hash TEXT
              , version TEXT
              , PRIMARY KEY (schema_hash, config_hash, version)
            )
            """
        )
        conn.execute(
            """
            INSERT INTO _schema_hash (schema_hash, config_hash, version)
            VALUES ('haha', 'lala', 'blabla')
            """,
        )

    with CACHE_SCHEMA_PATH.open("rb") as fp:
        latest_schema_hash = hashlib.sha256(fp.read()).hexdigest()
    maybe_invalidate_cache_database(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT schema_hash, config_hash, version FROM _schema_hash")
        row = cursor.fetchone()
        assert row["schema_hash"] == latest_schema_hash
        assert row["config_hash"] is not None
        assert row["version"] == VERSION
        cursor = conn.execute("SELECT COUNT(*) FROM _schema_hash")
        assert cursor.fetchone()[0] == 1


def test_locks(config: Config) -> None:
    """Test that taking locks works. The times are a bit loose b/c GH Actions is slow."""
    lock_name = "lol"

    # Test that the locking and timeout work.
    start = time.time()
    with lock(config, lock_name, timeout=0.2):
        lock1_acq = time.time()
        with lock(config, lock_name, timeout=0.2):
            lock2_acq = time.time()
    # Assert that we had to wait ~0.1sec to get the second lock.
    assert lock1_acq - start < 0.08
    assert lock2_acq - lock1_acq > 0.17

    # Test that releasing a lock actually works.
    start = time.time()
    with lock(config, lock_name, timeout=0.2):
        lock1_acq = time.time()
    with lock(config, lock_name, timeout=0.2):
        lock2_acq = time.time()
    # Assert that we had to wait negligible time to get the second lock.
    assert lock1_acq - start < 0.08
    assert lock2_acq - lock1_acq < 0.08


def test_update_cache_all(config: Config) -> None:
    """Test that the update all function works."""
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)

    # Test that we prune deleted releases too.
    with connect(config) as conn:
        conn.execute(
            """
            INSERT INTO releases (id, source_path, added_at, datafile_mtime, title, releasetype, multidisc)
            VALUES ('aaaaaa', '0000-01-01T00:00:00+00:00', '999', 'nonexistent', 'aa', 'unknown', false)
            """
        )

    update_cache(config)

    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 2
        cursor = conn.execute("SELECT COUNT(*) FROM tracks")
        assert cursor.fetchone()[0] == 4


def test_update_cache_multiprocessing(config: Config) -> None:
    """Test that the update all function works."""
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    update_cache_for_releases(config, force_multiprocessing=True)
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 2
        cursor = conn.execute("SELECT COUNT(*) FROM tracks")
        assert cursor.fetchone()[0] == 4


def test_update_cache_releases(config: Config) -> None:
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    update_cache_for_releases(config, [release_dir])

    # Check that the release directory was given a UUID.
    release_id: str | None = None
    for f in release_dir.iterdir():
        if m := STORED_DATA_FILE_REGEX.match(f.name):
            release_id = m[1]
    assert release_id is not None

    # Assert that the release metadata was read correctly.
    with connect(config) as conn:
        cursor = conn.execute(
            """
            SELECT id, source_path, title, releasetype, year, new
            FROM releases WHERE id = ?
            """,
            (release_id,),
        )
        row = cursor.fetchone()
        assert row["source_path"] == str(release_dir)
        assert row["title"] == "I Love Blackpink"
        assert row["releasetype"] == "album"
        assert row["year"] == 1990
        assert row["new"]

        cursor = conn.execute(
            "SELECT genre FROM releases_genres WHERE release_id = ?",
            (release_id,),
        )
        genres = {r["genre"] for r in cursor.fetchall()}
        assert genres == {"K-Pop", "Pop"}

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
            ("BLACKPINK", "main"),
        }

        for f in release_dir.iterdir():
            if f.suffix != ".m4a":
                continue

            # Assert that the track metadata was read correctly.
            cursor = conn.execute(
                """
                SELECT
                    id, source_path, title, release_id, tracknumber, discnumber, duration_seconds
                FROM tracks WHERE source_path = ?
                """,
                (str(f),),
            )
            row = cursor.fetchone()
            track_id = row["id"]
            assert row["title"].startswith("Track")
            assert row["release_id"] == release_id
            assert row["tracknumber"] != ""
            assert row["discnumber"] == "1"
            assert row["duration_seconds"] == 2

            cursor = conn.execute(
                "SELECT artist, role FROM tracks_artists WHERE track_id = ?",
                (track_id,),
            )
            artists = {(r["artist"], r["role"]) for r in cursor.fetchall()}
            assert artists == {
                ("BLACKPINK", "main"),
            }


def test_update_cache_releases_uncached_with_existing_id(config: Config) -> None:
    """Test that IDs in filenames are read and preserved."""
    release_dir = config.music_source_dir / TEST_RELEASE_2.name
    shutil.copytree(TEST_RELEASE_2, release_dir)
    update_cache_for_releases(config, [release_dir])

    # Check that the release directory was given a UUID.
    release_id: str | None = None
    for f in release_dir.iterdir():
        if m := STORED_DATA_FILE_REGEX.match(f.name):
            release_id = m[1]
    assert release_id == "ilovecarly"  # Hardcoded ID for testing.


def test_update_cache_releases_preserves_track_ids_across_rebuilds(config: Config) -> None:
    """Test that track IDs are preserved across cache rebuilds."""
    release_dir = config.music_source_dir / TEST_RELEASE_3.name
    shutil.copytree(TEST_RELEASE_3, release_dir)
    update_cache_for_releases(config, [release_dir])
    with connect(config) as conn:
        cursor = conn.execute("SELECT id FROM tracks")
        first_track_ids = {r["id"] for r in cursor}

    # Nuke the database.
    config.cache_database_path.unlink()
    maybe_invalidate_cache_database(config)

    # Repeat cache population.
    update_cache_for_releases(config, [release_dir])
    with connect(config) as conn:
        cursor = conn.execute("SELECT id FROM tracks")
        second_track_ids = {r["id"] for r in cursor}

    # Assert IDs are equivalent.
    assert first_track_ids == second_track_ids


def test_update_cache_releases_writes_ids_to_tags(config: Config) -> None:
    """Test that track IDs and release IDs are written to files."""
    release_dir = config.music_source_dir / TEST_RELEASE_3.name
    shutil.copytree(TEST_RELEASE_3, release_dir)

    af = AudioTags.from_file(release_dir / "01.m4a")
    assert af.id is None
    assert af.release_id is None
    af = AudioTags.from_file(release_dir / "02.m4a")
    assert af.id is None
    assert af.release_id is None

    update_cache_for_releases(config, [release_dir])

    af = AudioTags.from_file(release_dir / "01.m4a")
    assert af.id is not None
    assert af.release_id is not None
    af = AudioTags.from_file(release_dir / "02.m4a")
    assert af.id is not None
    assert af.release_id is not None


def test_update_cache_releases_already_fully_cached(config: Config) -> None:
    """Test that a fully cached release No Ops when updated again."""
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    update_cache_for_releases(config, [release_dir])
    update_cache_for_releases(config, [release_dir])

    # Assert that the release metadata was read correctly.
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT id, source_path, title, releasetype, year, new FROM releases",
        )
        row = cursor.fetchone()
        assert row["source_path"] == str(release_dir)
        assert row["title"] == "I Love Blackpink"
        assert row["releasetype"] == "album"
        assert row["year"] == 1990
        assert row["new"]


def test_update_cache_releases_disk_update_to_previously_cached(config: Config) -> None:
    """Test that a cached release is updated after a track updates."""
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    update_cache_for_releases(config, [release_dir])
    # I'm too lazy to mutagen update the files, so instead we're going to update the database. And
    # then touch a file to signify that "we modified it."
    with connect(config) as conn:
        conn.execute("UPDATE releases SET title = 'An Uncool Album'")
        (release_dir / "01.m4a").touch()
    update_cache_for_releases(config, [release_dir])

    # Assert that the release metadata was re-read and updated correctly.
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT id, source_path, title, releasetype, year, new FROM releases",
        )
        row = cursor.fetchone()
        assert row["source_path"] == str(release_dir)
        assert row["title"] == "I Love Blackpink"
        assert row["releasetype"] == "album"
        assert row["year"] == 1990
        assert row["new"]


def test_update_cache_releases_disk_update_to_datafile(config: Config) -> None:
    """Test that a cached release is updated after a datafile updates."""
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    update_cache_for_releases(config, [release_dir])
    with connect(config) as conn:
        conn.execute("UPDATE releases SET datafile_mtime = '0' AND new = false")
    update_cache_for_releases(config, [release_dir])

    # Assert that the release metadata was re-read and updated correctly.
    with connect(config) as conn:
        cursor = conn.execute("SELECT new, added_at FROM releases")
        row = cursor.fetchone()
        assert row["new"]
        assert row["added_at"]


def test_update_cache_releases_disk_upgrade_old_datafile(config: Config) -> None:
    """Test that a legacy invalid datafile is upgraded on index."""
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    datafile = release_dir / ".rose.lalala.toml"
    datafile.touch()
    update_cache_for_releases(config, [release_dir])

    # Assert that the release metadata was re-read and updated correctly.
    with connect(config) as conn:
        cursor = conn.execute("SELECT id, new, added_at FROM releases")
        row = cursor.fetchone()
        assert row["id"] == "lalala"
        assert row["new"]
        assert row["added_at"]
    with datafile.open("r") as fp:
        data = fp.read()
        assert "new = true" in data
        assert "added_at = " in data


def test_update_cache_releases_source_path_renamed(config: Config) -> None:
    """Test that a cached release is updated after a directory rename."""
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    update_cache_for_releases(config, [release_dir])
    moved_release_dir = config.music_source_dir / "moved lol"
    release_dir.rename(moved_release_dir)
    update_cache_for_releases(config, [moved_release_dir])

    # Assert that the release metadata was re-read and updated correctly.
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT id, source_path, title, releasetype, year, new FROM releases",
        )
        row = cursor.fetchone()
        assert row["source_path"] == str(moved_release_dir)
        assert row["title"] == "I Love Blackpink"
        assert row["releasetype"] == "album"
        assert row["year"] == 1990
        assert row["new"]


def test_update_cache_releases_delete_nonexistent(config: Config) -> None:
    """Test that deleted releases that are no longer on disk are cleared from cache."""
    with connect(config) as conn:
        conn.execute(
            """
            INSERT INTO releases (id, source_path, added_at, datafile_mtime, title, releasetype, multidisc)
            VALUES ('aaaaaa', '0000-01-01T00:00:00+00:00', '999', 'nonexistent', 'aa', 'unknown', false)
            """
        )
    update_cache_evict_nonexistent_releases(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 0


def test_update_cache_releases_skips_empty_directory(config: Config) -> None:
    """Test that an directory with no audio files is skipped."""
    rd = config.music_source_dir / "lalala"
    rd.mkdir()
    (rd / "ignoreme.file").touch()
    update_cache_for_releases(config, [rd])
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 0


def test_update_cache_releases_uncaches_empty_directory(config: Config) -> None:
    """Test that a previously-cached directory with no audio files now is cleared from cache."""
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    update_cache_for_releases(config, [release_dir])
    shutil.rmtree(release_dir)
    release_dir.mkdir()
    update_cache_for_releases(config, [release_dir])
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 0


def test_update_cache_releases_evicts_relations(config: Config) -> None:
    """
    Test that related entities (artist, genre, label) that have been removed from the tags are
    properly evicted from the cache on update.
    """
    release_dir = config.music_source_dir / TEST_RELEASE_2.name
    shutil.copytree(TEST_RELEASE_2, release_dir)
    # Initial cache population.
    update_cache_for_releases(config, [release_dir])
    # Pretend that we have more artists in the cache.
    with connect(config) as conn:
        conn.execute(
            """
            INSERT INTO releases_genres (release_id, genre, genre_sanitized, position)
            VALUES ('ilovecarly', 'lalala', 'lalala', 2)
            """,
        )
        conn.execute(
            """
            INSERT INTO releases_labels (release_id, label, label_sanitized, position)
            VALUES ('ilovecarly', 'lalala', 'lalala', 1)
            """,
        )
        conn.execute(
            """
            INSERT INTO releases_artists (release_id, artist, artist_sanitized, role, position)
            VALUES ('ilovecarly', 'lalala', 'lalala', 'main', 1)
            """,
        )
        conn.execute(
            """
            INSERT INTO tracks_artists (track_id, artist, artist_sanitized, role, position)
            SELECT id, 'lalala', 'lalala', 'main', 1 FROM tracks
            """,
        )
    # Second cache refresh.
    update_cache_for_releases(config, [release_dir], force=True)
    # Assert that all of the above were evicted.
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT EXISTS (SELECT * FROM releases_genres WHERE genre = 'lalala')"
        )
        assert not cursor.fetchone()[0]
        cursor = conn.execute(
            "SELECT EXISTS (SELECT * FROM releases_labels WHERE label = 'lalala')"
        )
        assert not cursor.fetchone()[0]
        cursor = conn.execute(
            "SELECT EXISTS (SELECT * FROM releases_artists WHERE artist = 'lalala')"
        )
        assert not cursor.fetchone()[0]
        cursor = conn.execute(
            "SELECT EXISTS (SELECT * FROM tracks_artists WHERE artist = 'lalala')"
        )
        assert not cursor.fetchone()[0]


def test_update_cache_releases_ignores_directories(config: Config) -> None:
    """Test that the ignore_release_directories configuration value works."""
    config = dataclasses.replace(config, ignore_release_directories=["lalala"])
    release_dir = config.music_source_dir / "lalala"
    shutil.copytree(TEST_RELEASE_1, release_dir)

    # Test that both arg+no-arg ignore the directory.
    update_cache_for_releases(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 0

    update_cache_for_releases(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 0


def test_update_cache_releases_notices_deleted_track(config: Config) -> None:
    """Test that we notice when a track is deleted."""
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    update_cache(config)

    (release_dir / "02.m4a").unlink()
    update_cache(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM tracks")
        assert cursor.fetchone()[0] == 1


def test_update_cache_releases_ignores_partially_written_directory(config: Config) -> None:
    """Test that a partially-written cached release is ignored."""
    # 1. Write the directory and index it. This should give it IDs and shit.
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    update_cache(config)

    # 2. Move the directory and "remove" the ID file.
    renamed_release_dir = config.music_source_dir / "lalala"
    release_dir.rename(renamed_release_dir)
    datafile = next(f for f in renamed_release_dir.iterdir() if f.stem.startswith(".rose"))
    tmpfile = datafile.with_name("tmp")
    datafile.rename(tmpfile)

    # 3. Re-update cache. We should see an empty cache now.
    update_cache(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 0

    # 4. Put the datafile back. We should now see the release cache again properly.
    datafile.with_name("tmp").rename(datafile)
    update_cache(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 1

    # 5. Rename and remove the ID file again. We should see an empty cache again.
    release_dir = renamed_release_dir
    renamed_release_dir = config.music_source_dir / "bahaha"
    release_dir.rename(renamed_release_dir)
    next(f for f in renamed_release_dir.iterdir() if f.stem.startswith(".rose")).unlink()
    update_cache(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 0

    # 6. Run with force=True. This should index the directory and make a new .rose.toml file.
    update_cache(config, force=True)
    assert (renamed_release_dir / datafile.name).is_file()
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 1


def test_update_cache_rename_source_files(config: Config) -> None:
    """Test that we properly rename the source directory on cache update."""
    config = dataclasses.replace(config, rename_source_files=True)
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    update_cache(config)

    expected_dir = config.music_source_dir / "BLACKPINK - 1990. I Love Blackpink [NEW]"
    assert expected_dir in list(config.music_source_dir.iterdir())

    files_in_dir = list(expected_dir.iterdir())
    assert expected_dir / "01. Track 1.m4a" in files_in_dir
    assert expected_dir / "02. Track 2.m4a" in files_in_dir

    with connect(config) as conn:
        cursor = conn.execute("SELECT source_path FROM releases")
        assert Path(cursor.fetchone()[0]) == expected_dir
        cursor = conn.execute("SELECT source_path FROM tracks")
        assert {Path(r[0]) for r in cursor} == {
            expected_dir / "01. Track 1.m4a",
            expected_dir / "02. Track 2.m4a",
        }


def test_update_cache_rename_source_files_nested_file_directories(config: Config) -> None:
    """Test that we properly rename arbitrarily nested files and clean up the empty dirs."""
    config = dataclasses.replace(config, rename_source_files=True)
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    (config.music_source_dir / TEST_RELEASE_1.name / "lala").mkdir()
    (config.music_source_dir / TEST_RELEASE_1.name / "01.m4a").rename(
        config.music_source_dir / TEST_RELEASE_1.name / "lala" / "1.m4a"
    )
    update_cache(config)

    expected_dir = config.music_source_dir / "BLACKPINK - 1990. I Love Blackpink [NEW]"
    assert expected_dir in list(config.music_source_dir.iterdir())

    files_in_dir = list(expected_dir.iterdir())
    assert expected_dir / "01. Track 1.m4a" in files_in_dir
    assert expected_dir / "02. Track 2.m4a" in files_in_dir
    assert expected_dir / "lala" not in files_in_dir

    with connect(config) as conn:
        cursor = conn.execute("SELECT source_path FROM releases")
        assert Path(cursor.fetchone()[0]) == expected_dir
        cursor = conn.execute("SELECT source_path FROM tracks")
        assert {Path(r[0]) for r in cursor} == {
            expected_dir / "01. Track 1.m4a",
            expected_dir / "02. Track 2.m4a",
        }


def test_update_cache_rename_source_files_collisions(config: Config) -> None:
    """Test that we properly rename arbitrarily nested files and clean up the empty dirs."""
    config = dataclasses.replace(config, rename_source_files=True)
    # Three copies of the same directory, and two instances of Track 1.
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    shutil.copyfile(
        config.music_source_dir / TEST_RELEASE_1.name / "01.m4a",
        config.music_source_dir / TEST_RELEASE_1.name / "haha.m4a",
    )
    shutil.copytree(
        config.music_source_dir / TEST_RELEASE_1.name, config.music_source_dir / "Number 2"
    )
    shutil.copytree(
        config.music_source_dir / TEST_RELEASE_1.name, config.music_source_dir / "Number 3"
    )
    update_cache(config)

    release_dirs = list(config.music_source_dir.iterdir())
    for expected_dir in [
        config.music_source_dir / "BLACKPINK - 1990. I Love Blackpink [NEW]",
        config.music_source_dir / "BLACKPINK - 1990. I Love Blackpink [NEW] [2]",
        config.music_source_dir / "BLACKPINK - 1990. I Love Blackpink [NEW] [3]",
    ]:
        assert expected_dir in release_dirs

        files_in_dir = list(expected_dir.iterdir())
        assert expected_dir / "01. Track 1.m4a" in files_in_dir
        assert expected_dir / "01. Track 1 [2].m4a" in files_in_dir
        assert expected_dir / "02. Track 2.m4a" in files_in_dir

        with connect(config) as conn:
            cursor = conn.execute(
                "SELECT id FROM releases WHERE source_path = ?", (str(expected_dir),)
            )
            release_id = cursor.fetchone()[0]
            assert release_id
            cursor = conn.execute(
                "SELECT source_path FROM tracks WHERE release_id = ?", (release_id,)
            )
            assert {Path(r[0]) for r in cursor} == {
                expected_dir / "01. Track 1.m4a",
                expected_dir / "01. Track 1 [2].m4a",
                expected_dir / "02. Track 2.m4a",
            }


def test_update_cache_releases_updates_full_text_search(config: Config) -> None:
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)

    update_cache_for_releases(config, [release_dir])
    with connect(config) as conn:
        cursor = conn.execute(
            """
            SELECT rowid, * FROM rules_engine_fts
            """
        )
        print([dict(x) for x in cursor])
        cursor = conn.execute(
            """
            SELECT rowid, * FROM tracks
            """
        )
        print([dict(x) for x in cursor])
    with connect(config) as conn:
        cursor = conn.execute(
            """
            SELECT t.source_path
            FROM rules_engine_fts s
            JOIN tracks t ON t.rowid = s.rowid
            WHERE s.tracktitle MATCH 'r a c k'
            """
        )
        fnames = {Path(r["source_path"]) for r in cursor}
        assert fnames == {
            release_dir / "01.m4a",
            release_dir / "02.m4a",
        }

    # And then test the DELETE+INSERT behavior. And that the query still works.
    update_cache_for_releases(config, [release_dir], force=True)
    with connect(config) as conn:
        cursor = conn.execute(
            """
            SELECT t.source_path
            FROM rules_engine_fts s
            JOIN tracks t ON t.rowid = s.rowid
            WHERE s.tracktitle MATCH 'r a c k'
            """
        )
        fnames = {Path(r["source_path"]) for r in cursor}
        assert fnames == {
            release_dir / "01.m4a",
            release_dir / "02.m4a",
        }


def test_update_cache_collages(config: Config) -> None:
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
    update_cache(config)

    # Assert that the collage metadata was read correctly.
    with connect(config) as conn:
        cursor = conn.execute("SELECT name, source_mtime FROM collages")
        rows = cursor.fetchall()
        assert len(rows) == 1
        row = rows[0]
        assert row["name"] == "Rose Gold"
        assert row["source_mtime"]

        cursor = conn.execute(
            "SELECT collage_name, release_id, position FROM collages_releases WHERE NOT missing"
        )
        rows = cursor.fetchall()
        assert len(rows) == 1
        row = rows[0]
        assert row["collage_name"] == "Rose Gold"
        assert row["release_id"] == "ilovecarly"
        assert row["position"] == 1


def test_update_cache_collages_missing_release_id(config: Config) -> None:
    shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
    update_cache(config)

    # Assert that the releases in the collage were read as missing.
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM collages_releases WHERE missing")
        assert cursor.fetchone()[0] == 2
    # Assert that source file was updated to set the releases missing.
    with (config.music_source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
        data = tomllib.load(fp)
    assert len(data["releases"]) == 2
    assert len([r for r in data["releases"] if r["missing"]]) == 2

    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_RELEASE_3, config.music_source_dir / TEST_RELEASE_3.name)
    update_cache(config)

    # Assert that the releases in the collage were unflagged as missing.
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM collages_releases WHERE NOT missing")
        assert cursor.fetchone()[0] == 2
    # Assert that source file was updated to remove the missing flag.
    with (config.music_source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
        data = tomllib.load(fp)
    assert len([r for r in data["releases"] if "missing" not in r]) == 2


def test_update_cache_collages_missing_release_id_multiprocessing(config: Config) -> None:
    shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
    update_cache(config)

    # Assert that the releases in the collage were read as missing.
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM collages_releases WHERE missing")
        assert cursor.fetchone()[0] == 2
    # Assert that source file was updated to set the releases missing.
    with (config.music_source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
        data = tomllib.load(fp)
    assert len(data["releases"]) == 2
    assert len([r for r in data["releases"] if r["missing"]]) == 2

    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_RELEASE_3, config.music_source_dir / TEST_RELEASE_3.name)
    update_cache(config, force_multiprocessing=True)

    # Assert that the releases in the collage were unflagged as missing.
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM collages_releases WHERE NOT missing")
        assert cursor.fetchone()[0] == 2
    # Assert that source file was updated to remove the missing flag.
    with (config.music_source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
        data = tomllib.load(fp)
    assert len([r for r in data["releases"] if "missing" not in r]) == 2


def test_update_cache_collages_on_release_rename(config: Config) -> None:
    """
    Test that a renamed release source directory does not remove the release from any collages. This
    can occur because the rename operation is executed in SQL as release deletion followed by
    release creation.
    """
    shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_RELEASE_3, config.music_source_dir / TEST_RELEASE_3.name)
    update_cache(config)

    (config.music_source_dir / TEST_RELEASE_2.name).rename(config.music_source_dir / "lalala")
    update_cache(config)

    with connect(config) as conn:
        cursor = conn.execute("SELECT collage_name, release_id, position FROM collages_releases")
        rows = [dict(r) for r in cursor]
        assert rows == [
            {"collage_name": "Rose Gold", "release_id": "ilovecarly", "position": 1},
            {"collage_name": "Rose Gold", "release_id": "ilovenewjeans", "position": 2},
        ]

    # Assert that source file was not updated to remove the release.
    with (config.music_source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
        data = tomllib.load(fp)
    assert not [r for r in data["releases"] if "missing" in r]
    assert len(data["releases"]) == 2


def test_update_cache_playlists(config: Config) -> None:
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_PLAYLIST_1, config.music_source_dir / "!playlists")
    update_cache(config)

    # Assert that the playlist metadata was read correctly.
    with connect(config) as conn:
        cursor = conn.execute("SELECT name, source_mtime, cover_path FROM playlists")
        rows = cursor.fetchall()
        assert len(rows) == 1
        row = rows[0]
        assert row["name"] == "Lala Lisa"
        assert row["source_mtime"] is not None
        assert row["cover_path"] == str(config.music_source_dir / "!playlists" / "Lala Lisa.jpg")

        cursor = conn.execute(
            "SELECT playlist_name, track_id, position FROM playlists_tracks ORDER BY position"
        )
        assert [dict(r) for r in cursor] == [
            {"playlist_name": "Lala Lisa", "track_id": "iloveloona", "position": 1},
            {"playlist_name": "Lala Lisa", "track_id": "ilovetwice", "position": 2},
        ]


def test_update_cache_playlists_missing_track_id(config: Config) -> None:
    shutil.copytree(TEST_PLAYLIST_1, config.music_source_dir / "!playlists")
    update_cache(config)

    # Assert that the tracks in the playlist were read as missing.
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM playlists_tracks WHERE missing")
        assert cursor.fetchone()[0] == 2
    # Assert that source file was updated to set the tracks missing.
    with (config.music_source_dir / "!playlists" / "Lala Lisa.toml").open("rb") as fp:
        data = tomllib.load(fp)
    assert len(data["tracks"]) == 2
    assert len([r for r in data["tracks"] if r["missing"]]) == 2

    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    update_cache(config)

    # Assert that the tracks in the playlist were unflagged as missing.
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM playlists_tracks WHERE NOT missing")
        assert cursor.fetchone()[0] == 2
    # Assert that source file was updated to remove the missing flag.
    with (config.music_source_dir / "!playlists" / "Lala Lisa.toml").open("rb") as fp:
        data = tomllib.load(fp)
    assert len([r for r in data["tracks"] if "missing" not in r]) == 2


@pytest.mark.parametrize("multiprocessing", [True, False])
def test_update_releases_updates_collages_description_meta(
    config: Config, multiprocessing: bool
) -> None:
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_RELEASE_3, config.music_source_dir / TEST_RELEASE_3.name)
    shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
    cpath = config.music_source_dir / "!collages" / "Rose Gold.toml"

    # First cache update: releases are inserted, collage is new. This should update the collage
    # TOML.
    update_cache(config)
    with cpath.open("r") as fp:
        assert (
            fp.read()
            == """\
releases = [
    { uuid = "ilovecarly", description_meta = "Carly Rae Jepsen - 1990. I Love Carly" },
    { uuid = "ilovenewjeans", description_meta = "NewJeans - 1990. I Love NewJeans" },
]
"""
        )

    # Now prep for the second update. Reset the TOML to have garbage again, and update the database
    # such that the virtual dirnames are also incorrect.
    with cpath.open("w") as fp:
        fp.write(
            """\
[[releases]]
uuid = "ilovecarly"
description_meta = "lalala"
[[releases]]
uuid = "ilovenewjeans"
description_meta = "hahaha"
"""
        )

    # Second cache update: releases exist, collages exist, release is "updated." This should also
    # trigger a metadata update.
    update_cache_for_releases(config, force=True, force_multiprocessing=multiprocessing)
    with cpath.open("r") as fp:
        assert (
            fp.read()
            == """\
releases = [
    { uuid = "ilovecarly", description_meta = "Carly Rae Jepsen - 1990. I Love Carly" },
    { uuid = "ilovenewjeans", description_meta = "NewJeans - 1990. I Love NewJeans" },
]
"""
        )


@pytest.mark.parametrize("multiprocessing", [True, False])
def test_update_tracks_updates_playlists_description_meta(
    config: Config, multiprocessing: bool
) -> None:
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_PLAYLIST_1, config.music_source_dir / "!playlists")
    ppath = config.music_source_dir / "!playlists" / "Lala Lisa.toml"

    # First cache update: tracks are inserted, playlist is new. This should update the playlist
    # TOML.
    update_cache(config)
    with ppath.open("r") as fp:
        assert (
            fp.read()
            == """\
tracks = [
    { uuid = "iloveloona", description_meta = "Carly Rae Jepsen - Track 1.m4a" },
    { uuid = "ilovetwice", description_meta = "Carly Rae Jepsen - Track 2.m4a" },
]
"""
        )

    # Now prep for the second update. Reset the TOML to have garbage again, and update the database
    # such that the virtual filenames are also incorrect.
    with ppath.open("w") as fp:
        fp.write(
            """\
[[tracks]]
uuid = "iloveloona"
description_meta = "lalala"
[[tracks]]
uuid = "ilovetwice"
description_meta = "hahaha"
"""
        )

    # Second cache update: tracks exist, playlists exist, track is "updated." This should also
    # trigger a metadata update.
    update_cache_for_releases(config, force=True, force_multiprocessing=multiprocessing)
    with ppath.open("r") as fp:
        assert (
            fp.read()
            == """\
tracks = [
    { uuid = "iloveloona", description_meta = "Carly Rae Jepsen - Track 1.m4a" },
    { uuid = "ilovetwice", description_meta = "Carly Rae Jepsen - Track 2.m4a" },
]
"""
        )


def test_update_cache_playlists_on_release_rename(config: Config) -> None:
    """
    Test that a renamed release source directory does not remove any of its tracks any playlists.
    This can occur because when a release is renamed, we remove all tracks from the database and
    then reinsert them.
    """
    shutil.copytree(TEST_PLAYLIST_1, config.music_source_dir / "!playlists")
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    update_cache(config)

    (config.music_source_dir / TEST_RELEASE_2.name).rename(config.music_source_dir / "lalala")
    update_cache(config)

    with connect(config) as conn:
        cursor = conn.execute("SELECT playlist_name, track_id, position FROM playlists_tracks")
        rows = [dict(r) for r in cursor]
        assert rows == [
            {"playlist_name": "Lala Lisa", "track_id": "iloveloona", "position": 1},
            {"playlist_name": "Lala Lisa", "track_id": "ilovetwice", "position": 2},
        ]

    # Assert that source file was not updated to remove the track.
    with (config.music_source_dir / "!playlists" / "Lala Lisa.toml").open("rb") as fp:
        data = tomllib.load(fp)
    assert not [t for t in data["tracks"] if "missing" in t]
    assert len(data["tracks"]) == 2


@pytest.mark.usefixtures("seeded_cache")
def test_list_releases(config: Config) -> None:
    expected = [
        CachedRelease(
            datafile_mtime="999",
            id="r1",
            source_path=Path(config.music_source_dir / "r1"),
            cover_image_path=None,
            added_at="0000-01-01T00:00:00+00:00",
            title="Release 1",
            releasetype="album",
            year=2023,
            multidisc=False,
            new=False,
            genres=["Techno", "Deep House"],
            labels=["Silk Music"],
            artists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
        ),
        CachedRelease(
            datafile_mtime="999",
            id="r2",
            source_path=Path(config.music_source_dir / "r2"),
            cover_image_path=Path(config.music_source_dir / "r2" / "cover.jpg"),
            added_at="0000-01-01T00:00:00+00:00",
            title="Release 2",
            releasetype="album",
            year=2021,
            multidisc=False,
            new=False,
            genres=["Classical"],
            labels=["Native State"],
            artists=ArtistMapping(main=[Artist("Violin Woman")], guest=[Artist("Conductor Woman")]),
        ),
        CachedRelease(
            datafile_mtime="999",
            id="r3",
            source_path=Path(config.music_source_dir / "r3"),
            cover_image_path=None,
            added_at="0000-01-01T00:00:00+00:00",
            title="Release 3",
            releasetype="album",
            year=2021,
            multidisc=False,
            new=True,
            genres=[],
            labels=[],
            artists=ArtistMapping(),
        ),
    ]

    assert list_releases(config) == expected
    assert list_releases(config, ["r1"]) == expected[:1]


@pytest.mark.usefixtures("seeded_cache")
def test_get_release_and_associated_tracks(config: Config) -> None:
    release = get_release(config, "r1")
    assert release is not None
    assert release == CachedRelease(
        datafile_mtime="999",
        id="r1",
        source_path=Path(config.music_source_dir / "r1"),
        cover_image_path=None,
        added_at="0000-01-01T00:00:00+00:00",
        title="Release 1",
        releasetype="album",
        year=2023,
        multidisc=False,
        new=False,
        genres=["Techno", "Deep House"],
        labels=["Silk Music"],
        artists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
    )

    expected_tracks = [
        CachedTrack(
            id="t1",
            source_path=config.music_source_dir / "r1" / "01.m4a",
            source_mtime="999",
            title="Track 1",
            release_id="r1",
            tracknumber="01",
            discnumber="01",
            duration_seconds=120,
            artists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
            release_multidisc=False,
        ),
        CachedTrack(
            id="t2",
            source_path=config.music_source_dir / "r1" / "02.m4a",
            source_mtime="999",
            title="Track 2",
            release_id="r1",
            tracknumber="02",
            discnumber="01",
            duration_seconds=240,
            artists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
            release_multidisc=False,
        ),
    ]

    assert get_tracks_associated_with_release(config, release) == expected_tracks
    assert get_tracks_associated_with_releases(config, [release]) == [(release, expected_tracks)]


@pytest.mark.usefixtures("seeded_cache")
def test_get_releases_associated_with_tracks(config: Config) -> None:
    t1 = get_track(config, "t1")
    t2 = get_track(config, "t2")
    assert t1 is not None
    assert t2 is not None

    release = CachedRelease(
        datafile_mtime="999",
        id="r1",
        source_path=Path(config.music_source_dir / "r1"),
        cover_image_path=None,
        added_at="0000-01-01T00:00:00+00:00",
        title="Release 1",
        releasetype="album",
        year=2023,
        multidisc=False,
        new=False,
        genres=["Techno", "Deep House"],
        labels=["Silk Music"],
        artists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
    )

    assert get_releases_associated_with_tracks(config, [t1, t2]) == [
        (t1, release),
        (t2, release),
    ]


@pytest.mark.usefixtures("seeded_cache")
def test_get_release_applies_artist_aliases(config: Config) -> None:
    config = dataclasses.replace(
        config,
        artist_aliases_map={"Hype Boy": ["Bass Man"]},
        artist_aliases_parents_map={"Bass Man": ["Hype Boy"]},
    )
    release = get_release(config, "r1")
    assert release is not None
    assert release.artists == ArtistMapping(
        main=[Artist("Techno Man"), Artist("Bass Man"), Artist("Hype Boy", True)],
    )
    tracks = get_tracks_associated_with_release(config, release)
    for t in tracks:
        assert t.artists == ArtistMapping(
            main=[Artist("Techno Man"), Artist("Bass Man"), Artist("Hype Boy", True)],
        )


@pytest.mark.usefixtures("seeded_cache")
def test_get_release_logtext(config: Config) -> None:
    assert get_release_logtext(config, "r1") == "Techno Man & Bass Man - 2023. Release 1"


@pytest.mark.usefixtures("seeded_cache")
def test_list_tracks(config: Config) -> None:
    expected = [
        CachedTrack(
            id="t1",
            source_path=config.music_source_dir / "r1" / "01.m4a",
            source_mtime="999",
            title="Track 1",
            release_id="r1",
            tracknumber="01",
            discnumber="01",
            duration_seconds=120,
            artists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
            release_multidisc=False,
        ),
        CachedTrack(
            id="t2",
            source_path=config.music_source_dir / "r1" / "02.m4a",
            source_mtime="999",
            title="Track 2",
            release_id="r1",
            tracknumber="02",
            discnumber="01",
            duration_seconds=240,
            artists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
            release_multidisc=False,
        ),
        CachedTrack(
            id="t3",
            source_path=config.music_source_dir / "r2" / "01.m4a",
            source_mtime="999",
            title="Track 1",
            release_id="r2",
            tracknumber="01",
            discnumber="01",
            duration_seconds=120,
            artists=ArtistMapping(main=[Artist("Violin Woman")], guest=[Artist("Conductor Woman")]),
            release_multidisc=False,
        ),
        CachedTrack(
            id="t4",
            source_path=config.music_source_dir / "r3" / "01.m4a",
            source_mtime="999",
            title="Track 1",
            release_id="r3",
            tracknumber="01",
            discnumber="01",
            duration_seconds=120,
            artists=ArtistMapping(),
            release_multidisc=False,
        ),
    ]

    assert list_tracks(config) == expected
    assert list_tracks(config, ["t1", "t2"]) == expected[:2]


@pytest.mark.usefixtures("seeded_cache")
def test_get_track(config: Config) -> None:
    assert get_track(config, "t1") == CachedTrack(
        id="t1",
        source_path=config.music_source_dir / "r1" / "01.m4a",
        source_mtime="999",
        title="Track 1",
        release_id="r1",
        tracknumber="01",
        discnumber="01",
        duration_seconds=120,
        artists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
        release_multidisc=False,
    )


@pytest.mark.usefixtures("seeded_cache")
def test_get_track_logtext(config: Config) -> None:
    assert get_track_logtext(config, "t1") == "Techno Man & Bass Man - Track 1.m4a"


@pytest.mark.usefixtures("seeded_cache")
def test_list_artists(config: Config) -> None:
    artists = list_artists(config)
    assert set(artists) == {
        ("Techno Man", "Techno Man"),
        ("Bass Man", "Bass Man"),
        ("Violin Woman", "Violin Woman"),
        ("Conductor Woman", "Conductor Woman"),
    }


@pytest.mark.usefixtures("seeded_cache")
def test_list_genres(config: Config) -> None:
    genres = list_genres(config)
    assert set(genres) == {
        ("Techno", "Techno"),
        ("Deep House", "Deep House"),
        ("Classical", "Classical"),
    }


@pytest.mark.usefixtures("seeded_cache")
def test_list_labels(config: Config) -> None:
    labels = list_labels(config)
    assert set(labels) == {("Silk Music", "Silk Music"), ("Native State", "Native State")}


@pytest.mark.usefixtures("seeded_cache")
def test_list_collages(config: Config) -> None:
    collages = list_collages(config)
    assert set(collages) == {"Rose Gold", "Ruby Red"}


@pytest.mark.usefixtures("seeded_cache")
def test_get_collage(config: Config) -> None:
    cdata = get_collage(config, "Rose Gold")
    assert cdata is not None
    collage, releases = cdata
    assert collage == CachedCollage(
        name="Rose Gold",
        source_mtime="999",
        release_ids=["r1", "r2"],
    )
    assert releases == [
        CachedRelease(
            id="r1",
            source_path=config.music_source_dir / "r1",
            cover_image_path=None,
            added_at="0000-01-01T00:00:00+00:00",
            datafile_mtime="999",
            title="Release 1",
            releasetype="album",
            year=2023,
            new=False,
            multidisc=False,
            genres=["Techno", "Deep House"],
            labels=["Silk Music"],
            artists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
        ),
        CachedRelease(
            id="r2",
            source_path=config.music_source_dir / "r2",
            cover_image_path=config.music_source_dir / "r2" / "cover.jpg",
            added_at="0000-01-01T00:00:00+00:00",
            datafile_mtime="999",
            title="Release 2",
            releasetype="album",
            year=2021,
            new=False,
            multidisc=False,
            genres=["Classical"],
            labels=["Native State"],
            artists=ArtistMapping(main=[Artist("Violin Woman")], guest=[Artist("Conductor Woman")]),
        ),
    ]

    cdata = get_collage(config, "Ruby Red")
    assert cdata is not None
    collage, releases = cdata
    assert collage == CachedCollage(
        name="Ruby Red",
        source_mtime="999",
        release_ids=[],
    )
    assert releases == []

    # lalalisa
    pdata = get_playlist(config, "Lala Lisa")
    assert pdata is not None
    playlist, tracks = pdata
    assert playlist == CachedPlaylist(
        name="Lala Lisa",
        source_mtime="999",
        cover_path=config.music_source_dir / "!playlists" / "Lala Lisa.jpg",
        track_ids=["t1", "t3"],
    )
    assert tracks == [
        CachedTrack(
            id="t1",
            source_path=config.music_source_dir / "r1" / "01.m4a",
            source_mtime="999",
            title="Track 1",
            release_id="r1",
            tracknumber="01",
            discnumber="01",
            duration_seconds=120,
            artists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
            release_multidisc=False,
        ),
        CachedTrack(
            id="t3",
            source_path=config.music_source_dir / "r2" / "01.m4a",
            source_mtime="999",
            title="Track 1",
            release_id="r2",
            tracknumber="01",
            discnumber="01",
            duration_seconds=120,
            artists=ArtistMapping(main=[Artist("Violin Woman")], guest=[Artist("Conductor Woman")]),
            release_multidisc=False,
        ),
    ]


@pytest.mark.usefixtures("seeded_cache")
def test_list_playlists(config: Config) -> None:
    playlists = list_playlists(config)
    assert set(playlists) == {"Lala Lisa", "Turtle Rabbit"}


@pytest.mark.usefixtures("seeded_cache")
def test_get_playlist(config: Config) -> None:
    pdata = get_playlist(config, "Lala Lisa")
    assert pdata is not None
    playlist, tracks = pdata
    assert playlist == CachedPlaylist(
        name="Lala Lisa",
        source_mtime="999",
        cover_path=config.music_source_dir / "!playlists" / "Lala Lisa.jpg",
        track_ids=["t1", "t3"],
    )
    assert tracks == [
        CachedTrack(
            id="t1",
            source_path=config.music_source_dir / "r1" / "01.m4a",
            source_mtime="999",
            title="Track 1",
            release_id="r1",
            tracknumber="01",
            discnumber="01",
            duration_seconds=120,
            artists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
            release_multidisc=False,
        ),
        CachedTrack(
            id="t3",
            source_path=config.music_source_dir / "r2" / "01.m4a",
            source_mtime="999",
            title="Track 1",
            release_id="r2",
            tracknumber="01",
            discnumber="01",
            duration_seconds=120,
            artists=ArtistMapping(main=[Artist("Violin Woman")], guest=[Artist("Conductor Woman")]),
            release_multidisc=False,
        ),
    ]


@pytest.mark.usefixtures("seeded_cache")
def test_artist_exists(config: Config) -> None:
    assert artist_exists(config, "Bass Man")
    assert not artist_exists(config, "lalala")


@pytest.mark.usefixtures("seeded_cache")
def test_artist_exists_with_alias(config: Config) -> None:
    config = dataclasses.replace(
        config,
        artist_aliases_map={"Hype Boy": ["Bass Man"]},
        artist_aliases_parents_map={"Bass Man": ["Hype Boy"]},
    )
    assert artist_exists(config, "Hype Boy")


@pytest.mark.usefixtures("seeded_cache")
def test_genre_exists(config: Config) -> None:
    assert genre_exists(config, "Deep House")
    assert not genre_exists(config, "lalala")


@pytest.mark.usefixtures("seeded_cache")
def test_label_exists(config: Config) -> None:
    assert label_exists(config, "Silk Music")
    assert not label_exists(config, "Cotton Music")


def test_unpack() -> None:
    i = _unpack("Rose ¬ Lisa ¬ Jisoo ¬ Jennie", r"vocal ¬ dance ¬ visual ¬ vocal")
    assert list(i) == [
        ("Rose", "vocal"),
        ("Lisa", "dance"),
        ("Jisoo", "visual"),
        ("Jennie", "vocal"),
    ]
    assert list(_unpack("", "")) == []
