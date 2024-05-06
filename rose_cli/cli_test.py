import os
import uuid
from pathlib import Path
from typing import Any

import pytest
from click.testing import CliRunner

from rose.audiotags import AudioTags
from rose.config import Config
from rose_cli.cli import (
    Context,
    InvalidReleaseArgError,
    InvalidTrackArgError,
    parse_collage_argument,
    parse_playlist_argument,
    parse_release_argument,
    parse_track_argument,
    unwatch,
    watch,
)
from rose_vfs.virtualfs_test import start_virtual_fs


@pytest.mark.usefixtures("seeded_cache")
def test_parse_release_from_path(config: Config) -> None:
    with start_virtual_fs(config):
        # Directory is resolved.
        path = str(config.vfs.mount_dir / "1. Releases" / "Techno Man & Bass Man - 2023. Release 1")
        assert parse_release_argument(path) == "r1"
        # UUID is no-opped.
        uuid_value = str(uuid.uuid4())
        assert parse_release_argument(uuid_value) == uuid_value
        # Non-existent path raises error.
        with pytest.raises(InvalidReleaseArgError):
            assert parse_release_argument(str(config.vfs.mount_dir / "1. Releases" / "lalala"))
        # Non-release directory raises error.
        with pytest.raises(InvalidReleaseArgError):
            assert parse_release_argument(str(config.vfs.mount_dir / "1. Releases"))
        # File raises error.
        with pytest.raises(InvalidReleaseArgError):
            assert parse_release_argument(
                str(
                    config.vfs.mount_dir
                    / "1. Releases"
                    / "Techno Man & Bass Man - 2023. Release 1"
                    / "01 - Track 1.m4a"
                )
            )


def test_parse_track_id_from_path(config: Config, source_dir: Path) -> None:
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    track_id = af.id
    assert track_id is not None
    with start_virtual_fs(config):
        # Track path is resolved.
        path = str(source_dir / "Test Release 1" / "01.m4a")
        assert parse_track_argument(path) == track_id
        # UUID is no-opped.
        assert parse_track_argument(track_id) == track_id
        # Non-existent path raises error.
        with pytest.raises(InvalidTrackArgError):
            assert parse_track_argument(str(config.vfs.mount_dir / "1. Releases" / "lalala"))
        # Directory raises error.
        with pytest.raises(InvalidTrackArgError):
            assert parse_track_argument(str(source_dir / "Test Release 1"))
        # Weirdly named directory raises error.
        (source_dir / "hi.m4a").mkdir()
        with pytest.raises(InvalidTrackArgError):
            assert parse_track_argument(str(source_dir / "hi.m4a"))


def test_parse_collage_name_from_path(config: Config, source_dir: Path) -> None:
    with start_virtual_fs(config):
        # Directory path is resolved.
        path = str(config.vfs.mount_dir / "6. Collages" / "Rose Gold")
        assert parse_collage_argument(path) == "Rose Gold"
        # File path is resolved.
        path = str(source_dir / "!collages" / "Rose Gold.toml")
        assert parse_collage_argument(path) == "Rose Gold"
        # Name is no-opped.
        assert parse_collage_argument("Rose Gold") == "Rose Gold"


def test_parse_playlist_name_from_path(config: Config, source_dir: Path) -> None:
    with start_virtual_fs(config):
        # Directory path is resolved.
        path = str(config.vfs.mount_dir / "7. Playlists" / "Lala Lisa")
        assert parse_playlist_argument(path)
        # File path is resolved.
        path = str(source_dir / "!playlists" / "Lala Lisa.toml")
        assert parse_playlist_argument(path) == "Lala Lisa"
        # Name is no-opped.
        assert parse_playlist_argument("Lala Lisa") == "Lala Lisa"


def test_cache_watch_unwatch(monkeypatch: Any, config: Config) -> None:
    # Mock os._exit so that it doesn't just kill the test runner lol.
    def mock_exit(x: int) -> None:
        raise SystemExit(x)

    monkeypatch.setattr(os, "_exit", mock_exit)

    ctx = Context(config=config)
    runner = CliRunner()
    # Start the watchdog.
    res = runner.invoke(watch, obj=ctx)
    assert res.exit_code == 0
    assert config.watchdog_pid_path.is_file()
    with config.watchdog_pid_path.open("r") as fp:
        pid = int(fp.read())
    # Assert that the process is running. Signal 0 doesn't do anything, but it will error if the
    # process does not exist.
    try:
        os.kill(pid, 0)
    except OSError as e:
        raise AssertionError from e
    # Assert that we cannot start another watchdog.
    res = runner.invoke(watch, obj=ctx)
    assert res.exit_code == 1
    # Kill the watchdog.
    res = runner.invoke(unwatch, obj=ctx)
    assert res.exit_code == 0
    assert not config.watchdog_pid_path.exists()
    # Assert that we can't kill a non-existent watchdog.
    res = runner.invoke(unwatch, obj=ctx)
    assert res.exit_code == 1
