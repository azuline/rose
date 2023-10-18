import shutil

import pytest

from conftest import TEST_RELEASE_1
from rose.cache import connect, update_cache
from rose.config import Config
from rose.releases import (
    ReleaseDoesNotExistError,
    delete_release,
    dump_releases,
    resolve_release_ids,
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
