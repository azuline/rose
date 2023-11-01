import hashlib
import logging
import shutil
import sqlite3
import time
from collections.abc import Iterator
from pathlib import Path

import pytest
from click.testing import CliRunner

from rose.cache import CACHE_SCHEMA_PATH, update_cache
from rose.common import VERSION
from rose.config import Config

logger = logging.getLogger(__name__)

TESTDATA = Path(__file__).resolve().parent / "testdata"
TEST_RELEASE_1 = TESTDATA / "Test Release 1"
TEST_RELEASE_2 = TESTDATA / "Test Release 2"
TEST_RELEASE_3 = TESTDATA / "Test Release 3"
TEST_COLLAGE_1 = TESTDATA / "Collage 1"
TEST_PLAYLIST_1 = TESTDATA / "Playlist 1"
TEST_TAGGER = TESTDATA / "tagger"


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
        fuse_labels_whitelist=None,
        fuse_artists_blacklist=None,
        fuse_genres_blacklist=None,
        fuse_labels_blacklist=None,
        cover_art_stems=["cover", "folder", "art", "front"],
        valid_art_exts=["jpg", "jpeg", "png"],
        ignore_release_directories=[],
        hash="00ff",
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
       (id  , source_path    , cover_image_path , added_at                   , datafile_mtime, virtual_dirname, title      , release_type, release_year, multidisc, new  , formatted_artists)
VALUES ('r1', '{dirpaths[0]}', null             , '0000-01-01T00:00:00+00:00', '999'         , 'r1'           , 'Release 1', 'album'     , 2023        , false    , false, 'Techno Man;Bass Man')
     , ('r2', '{dirpaths[1]}', '{imagepaths[0]}', '0000-01-01T00:00:00+00:00', '999'         , 'r2'           , 'Release 2', 'album'     , 2021        , false    , false, 'Violin Woman feat. Conductor Woman')
     , ('r3', '{dirpaths[2]}', null             , '0000-01-01T00:00:00+00:00', '999'         , '{{NEW}} r3'     , 'Release 3', 'album'     , 2021        , false    , true , '');

INSERT INTO releases_genres
       (release_id, genre       , genre_sanitized)
VALUES ('r1'      , 'Techno'    , 'Techno')
     , ('r1'      , 'Deep House', 'Deep House')
     , ('r2'      , 'Classical' , 'Classical');

INSERT INTO releases_labels
       (release_id, label         , label_sanitized)
VALUES ('r1'      , 'Silk Music'  , 'Silk Music')
     , ('r2'      , 'Native State', 'Native State');

INSERT INTO tracks
       (id  , source_path      , source_mtime, virtual_filename, formatted_release_position, title    , release_id, track_number, disc_number, duration_seconds, formatted_artists)
VALUES ('t1', '{musicpaths[0]}', '999'       , '01.m4a'        , '01'                      , 'Track 1', 'r1'      , '01'        , '01'       , 120             , 'Techno Man;Bass Man')
     , ('t2', '{musicpaths[1]}', '999'       , '02.m4a'        , '02'                      , 'Track 2', 'r1'      , '02'        , '01'       , 240             , 'Techno Man;Bass Man')
     , ('t3', '{musicpaths[2]}', '999'       , '01.m4a'        , '01'                      , 'Track 1', 'r2'      , '01'        , '01'       , 120             , 'Violin Woman feat. Conductor Woman')
     , ('t4', '{musicpaths[3]}', '999'       , '01.m4a'        , '02'                      , 'Track 1', 'r3'      , '01'        , '01'       , 120             , '');

INSERT INTO releases_artists
       (release_id, artist           , artist_sanitized , role   , alias)
VALUES ('r1'      , 'Techno Man'     , 'Techno Man'     , 'main' , false)
     , ('r1'      , 'Bass Man'       , 'Bass Man'       , 'main' , false)
     , ('r2'      , 'Violin Woman'   , 'Violin Woman'   , 'main' , false)
     , ('r2'      , 'Conductor Woman', 'Conductor Woman', 'guest', false);

INSERT INTO tracks_artists
       (track_id, artist           , artist_sanitized , role   , alias)
VALUES ('t1'    , 'Techno Man'     , 'Techno Man'     , 'main' , false)
     , ('t1'    , 'Bass Man'       , 'Bass Man'       , 'main' , false)
     , ('t2'    , 'Techno Man'     , 'Techno Man'     , 'main' , false)
     , ('t2'    , 'Bass Man'       , 'Bass Man'       , 'main' , false)
     , ('t3'    , 'Violin Woman'   , 'Violin Woman'   , 'main' , false)
     , ('t3'    , 'Conductor Woman', 'Conductor Woman', 'guest', false);

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
            """  # noqa: E501
        )

    (config.music_source_dir / "!collages").mkdir()
    (config.music_source_dir / "!playlists").mkdir()

    for d in dirpaths:
        d.parent.mkdir(parents=True, exist_ok=True)
        d.mkdir()
    for f in musicpaths + imagepaths:
        f.parent.mkdir(parents=True, exist_ok=True)
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
