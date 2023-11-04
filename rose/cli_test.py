import os
import uuid
from typing import Any

import pytest
from click.testing import CliRunner

from rose.cli import Context, InvalidReleaseArgError, parse_release_argument, unwatch, watch
from rose.config import Config
from rose.virtualfs_test import start_virtual_fs


@pytest.mark.usefixtures("seeded_cache")
def test_parse_release_from_path(config: Config) -> None:
    with start_virtual_fs(config):
        # Directory is resolved.
        path = str(config.fuse_mount_dir / "1. Releases" / "r1")
        assert parse_release_argument(path) == "r1"
        # UUID is no-opped.
        uuid_value = str(uuid.uuid4())
        assert parse_release_argument(uuid_value) == uuid_value
        # Non-existent path raises error.
        with pytest.raises(InvalidReleaseArgError):
            assert parse_release_argument(str(config.fuse_mount_dir / "1. Releases" / "lalala"))
        # Non-release directory raises error.
        with pytest.raises(InvalidReleaseArgError):
            assert parse_release_argument(str(config.fuse_mount_dir / "1. Releases"))
        # File is no-opped.
        with pytest.raises(InvalidReleaseArgError):
            assert parse_release_argument(
                str(config.fuse_mount_dir / "1. Releases" / "r1" / "01.m4a")
            )


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
