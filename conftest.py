import hashlib
import logging
import sqlite3
from collections.abc import Iterator
from pathlib import Path

import _pytest.pathlib
import pytest
from click.testing import CliRunner

from rose.foundation.conf import SCHEMA_PATH, Config

logger = logging.getLogger(__name__)


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
        with SCHEMA_PATH.open("r") as fp:
            conn.executescript(fp.read())
        conn.execute("CREATE TABLE _schema_hash (value TEXT PRIMARY KEY)")
        with SCHEMA_PATH.open("rb") as fp:
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
    )


@pytest.fixture()
def seeded_cache(config: Config) -> Iterator[None]:
    dirpaths = [
        config.music_source_dir / "r1",
        config.music_source_dir / "r2",
    ]
    filepaths = [
        config.music_source_dir / "r1" / "01.m4a",
        config.music_source_dir / "r1" / "02.m4a",
        config.music_source_dir / "r2" / "01.m4a",
    ]

    with sqlite3.connect(config.cache_database_path) as conn:
        conn.executescript(
            f"""\
INSERT INTO releases (id, source_path, virtual_dirname, title, release_type, release_year, new)
VALUES ('r1', '{dirpaths[0]}', 'r1', 'Release 1', 'album', 2023, true)
     , ('r2', '{dirpaths[1]}', 'r2', 'Release 2', 'album', 2021, false);

INSERT INTO releases_genres (release_id, genre, genre_sanitized)
VALUES ('r1', 'Techno', 'Techno')
     , ('r1', 'Deep House', 'Deep House')
     , ('r2', 'Classical', 'Classical');

INSERT INTO releases_labels (release_id, label, label_sanitized)
VALUES ('r1', 'Silk Music', 'Silk Music')
     , ('r2', 'Native State', 'Native State');

INSERT INTO tracks
(id, source_path, virtual_filename, title, release_id, track_number, disc_number, duration_seconds)
VALUES ('t1', '{filepaths[0]}', '01.m4a', 'Track 1', 'r1', '01', '01', 120)
     , ('t2', '{filepaths[1]}', '02.m4a', 'Track 2', 'r1', '02', '01', 240)
     , ('t3', '{filepaths[2]}', '01.m4a', 'Track 1', 'r2', '01', '01', 120);

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

    for d in dirpaths:
        d.mkdir()
    for f in filepaths:
        f.touch()

    yield
    return None


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
