import hashlib
import logging
import shutil
import sqlite3
import time
from collections.abc import Iterator
from pathlib import Path

import pytest
from click.testing import CliRunner

from rose.cache import CACHE_SCHEMA_PATH, process_string_for_fts, update_cache
from rose.common import VERSION
from rose.config import Config
from rose.templates import PathTemplateConfig

logger = logging.getLogger(__name__)

TESTDATA = Path(__file__).resolve().parent / "testdata"
TEST_RELEASE_1 = TESTDATA / "Test Release 1"
TEST_RELEASE_2 = TESTDATA / "Test Release 2"
TEST_RELEASE_3 = TESTDATA / "Test Release 3"
TEST_COLLAGE_1 = TESTDATA / "Collage 1"
TEST_PLAYLIST_1 = TESTDATA / "Playlist 1"
TEST_TAGGER = TESTDATA / "Tagger"


@pytest.fixture(autouse=True)
def debug_logging() -> None:
    logging.getLogger().setLevel(logging.DEBUG)


@pytest.fixture()
def isolated_dir() -> Iterator[Path]:
    with CliRunner().isolated_filesystem():
        yield Path.cwd()


@pytest.fixture()
def config(isolated_dir: Path) -> Config:
    cache_dir = isolated_dir / "cache"
    cache_dir.mkdir()

    cache_database_path = cache_dir / "cache.sqlite3"
    with sqlite3.connect(cache_database_path) as conn:
        with CACHE_SCHEMA_PATH.open("r") as fp:
            conn.executescript(fp.read())
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
        with CACHE_SCHEMA_PATH.open("rb") as fp:
            schema_hash = hashlib.sha256(fp.read()).hexdigest()
        conn.execute(
            "INSERT INTO _schema_hash (schema_hash, config_hash, version) VALUES (?, ?, ?)",
            (schema_hash, "00ff", VERSION),
        )

    music_source_dir = isolated_dir / "source"
    music_source_dir.mkdir()

    mount_dir = isolated_dir / "mount"
    mount_dir.mkdir()

    return Config(
        music_source_dir=music_source_dir,
        fuse_mount_dir=mount_dir,
        cache_dir=cache_dir,
        max_proc=2,
        artist_aliases_map={},
        artist_aliases_parents_map={},
        fuse_artists_whitelist=None,
        fuse_genres_whitelist=None,
        fuse_descriptors_whitelist=None,
        fuse_labels_whitelist=None,
        fuse_artists_blacklist=None,
        fuse_genres_blacklist=None,
        fuse_descriptors_blacklist=None,
        fuse_labels_blacklist=None,
        hide_genres_with_only_new_releases=False,
        hide_descriptors_with_only_new_releases=False,
        hide_labels_with_only_new_releases=False,
        cover_art_stems=["cover", "folder", "art", "front"],
        valid_art_exts=["jpg", "jpeg", "png"],
        max_filename_bytes=180,
        path_templates=PathTemplateConfig.with_defaults(),
        rename_source_files=False,
        ignore_release_directories=[],
        stored_metadata_rules=[],
    )


@pytest.fixture()
def seeded_cache(config: Config) -> None:
    dirpaths = [
        config.music_source_dir / "r1",
        config.music_source_dir / "r2",
        config.music_source_dir / "r3",
    ]
    musicpaths = [
        config.music_source_dir / "r1" / "01.m4a",
        config.music_source_dir / "r1" / "02.m4a",
        config.music_source_dir / "r2" / "01.m4a",
        config.music_source_dir / "r3" / "01.m4a",
    ]
    imagepaths = [
        config.music_source_dir / "r2" / "cover.jpg",
        config.music_source_dir / "!playlists" / "Lala Lisa.jpg",
    ]

    with sqlite3.connect(config.cache_database_path) as conn:
        conn.executescript(
            f"""\
INSERT INTO releases
       (id  , source_path    , cover_image_path , added_at                   , datafile_mtime, title      , releasetype, releasedate , originaldate, compositiondate, catalognumber, edition , disctotal, new  , metahash)
VALUES ('r1', '{dirpaths[0]}', null             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 1', 'album'    , '2023'      , null        , null           , null         , null    , 1        , false, '1')
     , ('r2', '{dirpaths[1]}', '{imagepaths[0]}', '0000-01-01T00:00:00+00:00', '999'         , 'Release 2', 'album'    , '2021'      , '2019'      , null           , 'DG-001'     , 'Deluxe', 1        , true , '2')
     , ('r3', '{dirpaths[2]}', null             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 3', 'album'    , '2021-04-20', null        , '1780'         , 'DG-002'     , null    , 1        , false, '3');

INSERT INTO releases_genres
       (release_id, genre             , position)
VALUES ('r1'      , 'Techno'          , 1)
     , ('r1'      , 'Deep House'      , 2)
     , ('r2'      , 'Modern Classical', 1);

INSERT INTO releases_secondary_genres
       (release_id, genre       , position)
VALUES ('r1'      , 'Rominimal' , 1)
     , ('r1'      , 'Ambient'   , 2)
     , ('r2'      , 'Orchestral', 1);

INSERT INTO releases_descriptors
       (release_id, descriptor, position)
VALUES ('r1'      , 'Warm'    , 1)
     , ('r1'      , 'Hot'     , 2)
     , ('r2'      , 'Wet'     , 1);

INSERT INTO releases_labels
       (release_id, label         , position)
VALUES ('r1'      , 'Silk Music'  , 1)
     , ('r2'      , 'Native State', 1);

INSERT INTO tracks
       (id  , source_path      , source_mtime, title    , release_id, tracknumber, tracktotal, discnumber, duration_seconds, metahash)
VALUES ('t1', '{musicpaths[0]}', '999'       , 'Track 1', 'r1'      , '01'       , 2         , '01'      , 120             , '1')
     , ('t2', '{musicpaths[1]}', '999'       , 'Track 2', 'r1'      , '02'       , 2         , '01'      , 240             , '2')
     , ('t3', '{musicpaths[2]}', '999'       , 'Track 1', 'r2'      , '01'       , 1         , '01'      , 120             , '3')
     , ('t4', '{musicpaths[3]}', '999'       , 'Track 1', 'r3'      , '01'       , 1         , '01'      , 120             , '4');

INSERT INTO releases_artists
       (release_id, artist           , role   , position)
VALUES ('r1'      , 'Techno Man'     , 'main' , 1)
     , ('r1'      , 'Bass Man'       , 'main' , 2)
     , ('r2'      , 'Violin Woman'   , 'main' , 1)
     , ('r2'      , 'Conductor Woman', 'guest', 2);

INSERT INTO tracks_artists
       (track_id, artist           , role   , position)
VALUES ('t1'    , 'Techno Man'     , 'main' , 1)
     , ('t1'    , 'Bass Man'       , 'main' , 2)
     , ('t2'    , 'Techno Man'     , 'main' , 1)
     , ('t2'    , 'Bass Man'       , 'main' , 2)
     , ('t3'    , 'Violin Woman'   , 'main' , 1)
     , ('t3'    , 'Conductor Woman', 'guest', 2);

INSERT INTO collages
       (name       , source_mtime)
VALUES ('Rose Gold', '999')
     , ('Ruby Red' , '999');

INSERT INTO collages_releases
       (collage_name, release_id, position, missing)
VALUES ('Rose Gold' , 'r1'      , 1       , false)
     , ('Rose Gold' , 'r2'      , 2       , false);

INSERT INTO playlists
       (name           , source_mtime, cover_path)
VALUES ('Lala Lisa'    , '999',        '{imagepaths[1]}')
     , ('Turtle Rabbit', '999',        null);

INSERT INTO playlists_tracks
       (playlist_name, track_id, position, missing)
VALUES ('Lala Lisa'  , 't1'    , 1       , false)
     , ('Lala Lisa'  , 't3'    , 2       , false);
            """
        )

        # And update the FTS index too...
        conn.create_function("process_string_for_fts", 1, process_string_for_fts)
        conn.execute(
            """
            INSERT INTO rules_engine_fts (
                rowid
              , tracktitle
              , tracknumber
              , discnumber
              , releasetitle
              , releasedate
              , originaldate
              , compositiondate
              , catalognumber
              , edition
              , releasetype
              , genre
              , secondarygenre
              , descriptor
              , label
              , releaseartist
              , trackartist
            )
            SELECT
                t.rowid
              , process_string_for_fts(t.title) AS tracktitle
              , process_string_for_fts(t.tracknumber) AS tracknumber
              , process_string_for_fts(t.discnumber) AS discnumber
              , process_string_for_fts(r.title) AS releasetitle
              , process_string_for_fts(r.releasedate) AS releasedate
              , process_string_for_fts(r.originaldate) AS originaldate
              , process_string_for_fts(r.compositiondate) AS compositiondate
              , process_string_for_fts(r.catalognumber) AS catalognumber
              , process_string_for_fts(r.edition) AS edition
              , process_string_for_fts(r.releasetype) AS releasetype
              , process_string_for_fts(COALESCE(GROUP_CONCAT(rg.genre, ' '), '')) AS genre
              , process_string_for_fts(COALESCE(GROUP_CONCAT(rs.genre, ' '), '')) AS secondarygenre
              , process_string_for_fts(COALESCE(GROUP_CONCAT(rd.descriptor, ' '), '')) AS descriptor
              , process_string_for_fts(COALESCE(GROUP_CONCAT(rl.label, ' '), '')) AS label
              , process_string_for_fts(COALESCE(GROUP_CONCAT(ra.artist, ' '), '')) AS releaseartist
              , process_string_for_fts(COALESCE(GROUP_CONCAT(ta.artist, ' '), '')) AS trackartist
            FROM tracks t
            JOIN releases r ON r.id = t.release_id
            LEFT JOIN releases_genres rg ON rg.release_id = r.id
            LEFT JOIN releases_secondary_genres rs ON rs.release_id = r.id
            LEFT JOIN releases_descriptors rd ON rd.release_id = r.id
            LEFT JOIN releases_labels rl ON rl.release_id = r.id
            LEFT JOIN releases_artists ra ON ra.release_id = r.id
            LEFT JOIN tracks_artists ta ON ta.track_id = t.id
            GROUP BY t.id
            """,
        )

    (config.music_source_dir / "!collages").mkdir()
    (config.music_source_dir / "!playlists").mkdir()

    for d in dirpaths:
        d.mkdir()
        (d / f".rose.{d.name}.toml").touch()
    for f in musicpaths + imagepaths:
        f.touch()
    for cn in ["Rose Gold", "Ruby Red"]:
        (config.music_source_dir / "!collages" / f"{cn}.toml").touch()
    for pn in ["Lala Lisa", "Turtle Rabbit"]:
        (config.music_source_dir / "!playlists" / f"{pn}.toml").touch()


@pytest.fixture()
def source_dir(config: Config) -> Path:
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_RELEASE_3, config.music_source_dir / TEST_RELEASE_3.name)
    shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
    shutil.copytree(TEST_PLAYLIST_1, config.music_source_dir / "!playlists")
    update_cache(config)
    return config.music_source_dir


def retry_for_sec(timeout_sec: float) -> Iterator[None]:
    start = time.time()
    while True:
        yield
        time.sleep(0.01)
        if time.time() - start >= timeout_sec:
            break
