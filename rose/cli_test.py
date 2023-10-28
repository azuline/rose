import pytest

from rose.cli import parse_release_from_potential_path
from rose.config import Config
from rose.virtualfs_test import start_virtual_fs


@pytest.mark.usefixtures("seeded_cache")
def test_parse_release_from_path(config: Config) -> None:
    with start_virtual_fs(config):
        # Directory is resolved.
        path = str(config.fuse_mount_dir / "1. Releases" / "r1")
        assert parse_release_from_potential_path(config, path) == "r1"
        # Normal string is no-opped.
        assert parse_release_from_potential_path(config, "r1") == "r1"
        # Non-existent path is no-opped.
        path = str(config.fuse_mount_dir / "1. Releases" / "lalala")
        assert parse_release_from_potential_path(config, path) == path
        # Non-release directory is no-opped.
        path = str(config.fuse_mount_dir / "1. Releases")
        assert parse_release_from_potential_path(config, path) == path
        # File is no-opped.
        path = str(config.fuse_mount_dir / "1. Releases" / "r1" / "01.m4a")
        assert parse_release_from_potential_path(config, path) == path
