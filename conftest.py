import hashlib
import logging
import shutil
import sqlite3
from collections.abc import Iterator
from pathlib import Path

import _pytest.pathlib
import pytest
from click.testing import CliRunner

from rose.cache import CACHE_SCHEMA_PATH, update_cache
from rose.config import Config

logger = logging.getLogger(__name__)

TESTDATA = Path(__file__).resolve().parent / "testdata"
TEST_RELEASE_1 = TESTDATA / "Test Release 1"
TEST_RELEASE_2 = TESTDATA / "Test Release 2"
TEST_RELEASE_3 = TESTDATA / "Test Release 3"
TEST_COLLAGE_1 = TESTDATA / "Collage 1"
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
        conn.execute("CREATE TABLE _schema_hash (value TEXT PRIMARY KEY)")
        with CACHE_SCHEMA_PATH.open("rb") as fp:
            latest_schema_hash = hashlib.sha256(fp.read()).hexdigest()
        conn.execute("INSERT INTO _schema_hash (value) VALUES (?)", (latest_schema_hash,))

    music_source_dir = isolated_dir / "source"
    music_source_dir.mkdir()

    mount_dir = isolated_dir / "mount"
    mount_dir.mkdir()

    return Config(
        music_source_dir=music_source_dir,
        fuse_mount_dir=mount_dir,
        cache_dir=cache_dir,
        cache_database_path=cache_database_path,
        fuse_hide_artists=[],
        fuse_hide_genres=[],
        fuse_hide_labels=[],
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
    ]

    with sqlite3.connect(config.cache_database_path) as conn:
        conn.executescript(
            f"""\
INSERT INTO releases
       (id  , source_path    , cover_image_path , datafile_mtime, virtual_dirname, title      , release_type, release_year, multidisc, new  , formatted_artists)
VALUES ('r1', '{dirpaths[0]}', null             , '999'         , 'r1'           , 'Release 1', 'album'     , 2023        , false    , false, 'Techno Man;Bass Man')
     , ('r2', '{dirpaths[1]}', '{imagepaths[0]}', '999'         , 'r2'           , 'Release 2', 'album'     , 2021        , false    , false, 'Violin Woman feat. Conductor Woman')
     , ('r3', '{dirpaths[2]}', null             , '999'         , '[NEW] r3'     , 'Release 3', 'album'     , 2021        , false    , true , '');

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
       (id  , source_path      , source_mtime, virtual_filename, title    , release_id, track_number, disc_number, duration_seconds, formatted_artists)
VALUES ('t1', '{musicpaths[0]}', '999'       , '01.m4a'        , 'Track 1', 'r1'      , '01'        , '01'       , 120             , 'Techno Man;Bass Man')
     , ('t2', '{musicpaths[1]}', '999'       , '02.m4a'        , 'Track 2', 'r1'      , '02'        , '01'       , 240             , 'Techno Man;Bass Man')
     , ('t3', '{musicpaths[2]}', '999'       , '01.m4a'        , 'Track 1', 'r2'      , '01'        , '01'       , 120             , 'Violin Woman feat. Conductor Woman')
     , ('t4', '{musicpaths[3]}', '999'       , '01.m4a'        , 'Track 1', 'r3'      , '01'        , '01'       , 120             , '');

INSERT INTO releases_artists
       (release_id, artist           , artist_sanitized , role)
VALUES ('r1'      , 'Techno Man'     , 'Techno Man'     , 'main')
     , ('r1'      , 'Bass Man'       , 'Bass Man'       , 'main')
     , ('r2'      , 'Violin Woman'   , 'Violin Woman'   , 'main')
     , ('r2'      , 'Conductor Woman', 'Conductor Woman', 'guest');

INSERT INTO tracks_artists
       (track_id, artist           , artist_sanitized , role)
VALUES ('t1'    , 'Techno Man'     , 'Techno Man'     , 'main')
     , ('t1'    , 'Bass Man'       , 'Bass Man'       , 'main')
     , ('t2'    , 'Techno Man'     , 'Techno Man'     , 'main')
     , ('t2'    , 'Bass Man'       , 'Bass Man'       , 'main')
     , ('t3'    , 'Violin Woman'   , 'Violin Woman'   , 'main')
     , ('t3'    , 'Conductor Woman', 'Conductor Woman', 'guest');

INSERT INTO collages
       (name       , source_mtime)
VALUES ('Rose Gold', '999')
     , ('Ruby Red' , '999');

INSERT INTO collages_releases
       (collage_name, release_id, position)
VALUES ('Rose Gold' , 'r1'      , 0)
     , ('Rose Gold' , 'r2'      , 1);
            """  # noqa: E501
        )

    for d in dirpaths:
        d.mkdir()
    for f in musicpaths + imagepaths:
        f.touch()


@pytest.fixture()
def source_dir(config: Config) -> Path:
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_RELEASE_3, config.music_source_dir / TEST_RELEASE_3.name)
    shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
    update_cache(config)
    return config.music_source_dir


def freeze_database_time(conn: sqlite3.Connection) -> None:
    """
    This function freezes the CURRENT_TIMESTAMP function in SQLite3 to
    "2020-01-01 01:01:01". This should only be used in testing.
    """
    conn.create_function(
        "CURRENT_TIMESTAMP",
        0,
        _return_fake_timestamp,
        deterministic=True,
    )


def _return_fake_timestamp() -> str:
    return "2020-01-01 01:01:01"


# Pytest has a bug where it doesn't handle namespace packages and treats same-name files
# in different packages as a naming collision. https://stackoverflow.com/a/72366347

resolve_pkg_path_orig = _pytest.pathlib.resolve_package_path
namespace_pkg_dirs = [str(d) for d in Path(__file__).parent.iterdir() if d.is_dir()]


# patched method
def resolve_package_path(path: Path) -> Path | None:
    # call original lookup
    result = resolve_pkg_path_orig(path)
    if result is None:
        result = path  # let's search from the current directory upwards
    for parent in result.parents:  # pragma: no cover
        if str(parent) in namespace_pkg_dirs:
            return parent
    return None  # pragma: no cover


# apply patch
_pytest.pathlib.resolve_package_path = resolve_package_path
