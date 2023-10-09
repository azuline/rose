import os
import tempfile
from pathlib import Path

import pytest

from rose.config import Config, ConfigNotFoundError, MissingConfigKeyError


def test_config() -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        with path.open("w") as fp:
            fp.write(
                """\
    music_source_dir = "~/.music-src"
    fuse_mount_dir = "~/music"
    """
            )

        c = Config.read(config_path_override=path)
        assert str(c.music_source_dir) == os.environ["HOME"] + "/.music-src"
        assert str(c.fuse_mount_dir) == os.environ["HOME"] + "/music"


def test_config_not_found() -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        with pytest.raises(ConfigNotFoundError):
            Config.read(config_path_override=path)


def test_config_missing_key() -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        path.touch()
        with pytest.raises(MissingConfigKeyError) as excinfo:
            Config.read(config_path_override=path)
        assert str(excinfo.value) == "Missing key in configuration file: 'music_source_dir'"
