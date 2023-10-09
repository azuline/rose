from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path

import tomllib

from rose.foundation.errors import RoseError

XDG_CONFIG_HOME = Path(os.environ.get("XDG_CONFIG_HOME", os.environ["HOME"] + "/.config"))
CONFIG_PATH = XDG_CONFIG_HOME / "rose" / "config.toml"
XDG_CACHE_HOME = Path(os.environ.get("XDG_CACHE_HOME", os.environ["HOME"] + "/.cache"))
CACHE_PATH = XDG_CACHE_HOME / "rose"

MIGRATIONS_PATH = Path(__file__).resolve().parent.parent.parent / "migrations"


class ConfigNotFoundError(RoseError):
    pass


class MissingConfigKeyError(RoseError):
    pass


@dataclass
class Config:
    music_source_dir: Path
    fuse_mount_dir: Path
    cache_dir: Path
    cache_database_path: Path

    @classmethod
    def read(cls, config_path_override: Path | None = None) -> Config:
        cfgpath = config_path_override or CONFIG_PATH
        try:
            with cfgpath.open("rb") as fp:
                data = tomllib.load(fp)
        except FileNotFoundError as e:
            raise ConfigNotFoundError(f"Configuration file not found ({CONFIG_PATH})") from e

        cache_dir = CACHE_PATH
        if "cache_dir" in data:
            cache_dir = Path(data["cache_dir"]).expanduser()
        cache_dir.mkdir(exist_ok=True)

        try:
            return cls(
                music_source_dir=Path(data["music_source_dir"]).expanduser(),
                fuse_mount_dir=Path(data["fuse_mount_dir"]).expanduser(),
                cache_dir=cache_dir,
                cache_database_path=cache_dir / "cache.sqlite3",
            )
        except KeyError as e:
            raise MissingConfigKeyError(f"Missing key in configuration file: {e}") from e