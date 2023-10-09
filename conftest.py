import logging
import sqlite3
from collections.abc import Iterator
from pathlib import Path

import _pytest.pathlib
import pytest
from click.testing import CliRunner

from rose.foundation.conf import Config

logger = logging.getLogger(__name__)


@pytest.fixture()
def isolated_dir() -> Iterator[Path]:
    with CliRunner().isolated_filesystem():
        yield Path.cwd()


@pytest.fixture()
def config(isolated_dir: Path) -> Config:
    (isolated_dir / "cache").mkdir()
    return Config(
        music_source_dir=isolated_dir / "source",
        fuse_mount_dir=isolated_dir / "mount",
        cache_dir=isolated_dir / "cache",
        cache_database_path=isolated_dir / "cache" / "cache.sqlite3",
    )


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
