from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path

import tomllib

from rose.base_error import RoseError

CONFIG_HOME = Path(os.environ.get("XDG_CONFIG_HOME", os.environ["HOME"] + "/.config"))
CONFIG_PATH = CONFIG_HOME / "rose" / "config.toml"


class ConfigNotFoundError(RoseError):
    pass


class MissingConfigKeyError(RoseError):
    pass


@dataclass
class Config:
    music_source_dir: Path
    fuse_mount_dir: Path

    @classmethod
    def read(cls, config_path_override: Path | None = None) -> Config:
        cfgpath = config_path_override or CONFIG_PATH
        try:
            with cfgpath.open("rb") as fp:
                data = tomllib.load(fp)
        except FileNotFoundError as e:
            raise ConfigNotFoundError(f"Configuration file not found ({CONFIG_PATH})") from e

        try:
            return cls(
                music_source_dir=Path(data["music_source_dir"]).expanduser(),
                fuse_mount_dir=Path(data["fuse_mount_dir"]).expanduser(),
            )
        except KeyError as e:
            raise MissingConfigKeyError(f"Missing key in configuration file: {e}") from e
