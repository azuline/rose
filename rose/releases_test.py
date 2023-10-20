import shutil

import pytest
import tomllib

from conftest import TEST_RELEASE_1
from rose.cache import connect, update_cache
from rose.config import Config
from rose.releases import (
    ReleaseDoesNotExistError,
    delete_release,
    dump_releases,
    resolve_release_ids,
    toggle_release_new,
)


def test_dump_releases(config: Config) -> None:
    assert dump_releases(config) == "[]"


def test_delete_release(config: Config) -> None:
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    update_cache(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT id, virtual_dirname FROM releases")
        release_id = cursor.fetchone()["id"]
    delete_release(config, release_id)
    assert not (config.music_source_dir / TEST_RELEASE_1.name).exists()
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 0


def test_toggle_release_new(config: Config) -> None:
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    update_cache(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT id, virtual_dirname FROM releases")
        release_id = cursor.fetchone()["id"]
    datafile = config.music_source_dir / TEST_RELEASE_1.name / f".rose.{release_id}.toml"

    # Set not new.
    toggle_release_new(config, release_id)
    with datafile.open("rb") as fp:
        data = tomllib.load(fp)
        assert data["new"] is False
    with connect(config) as conn:
        cursor = conn.execute("SELECT virtual_dirname FROM releases")
        assert not cursor.fetchone()["virtual_dirname"].startswith("[NEW] ")

    # Set new.
    toggle_release_new(config, release_id)
    with datafile.open("rb") as fp:
        data = tomllib.load(fp)
        assert data["new"] is True
    with connect(config) as conn:
        cursor = conn.execute("SELECT virtual_dirname FROM releases")
        assert cursor.fetchone()["virtual_dirname"].startswith("[NEW] ")


def test_resolve_release_ids(config: Config) -> None:
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    update_cache(config)

    with connect(config) as conn:
        cursor = conn.execute("SELECT id, virtual_dirname FROM releases")
        row = cursor.fetchone()
        release_id = row["id"]
        virtual_dirname = row["virtual_dirname"]

    assert resolve_release_ids(config, release_id) == (release_id, virtual_dirname)
    assert resolve_release_ids(config, virtual_dirname) == (release_id, virtual_dirname)
    with pytest.raises(ReleaseDoesNotExistError):
        resolve_release_ids(config, "lalala")
