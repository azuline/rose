from __future__ import annotations

import os
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path

import tomllib

from rose.common import RoseError

XDG_CONFIG_HOME = Path(os.environ.get("XDG_CONFIG_HOME", os.environ["HOME"] + "/.config"))
CONFIG_PATH = XDG_CONFIG_HOME / "rose" / "config.toml"
XDG_CACHE_HOME = Path(os.environ.get("XDG_CACHE_HOME", os.environ["HOME"] + "/.cache"))
CACHE_PATH = XDG_CACHE_HOME / "rose"


class ConfigNotFoundError(RoseError):
    pass


class MissingConfigKeyError(RoseError):
    pass


@dataclass(frozen=True)
class Config:
    music_source_dir: Path
    fuse_mount_dir: Path
    cache_dir: Path
    cache_database_path: Path

    # A map from parent artist -> subartists.
    artist_aliases_map: dict[str, list[str]]
    # A map from subartist -> parent artists.
    artist_aliases_parents_map: dict[str, list[str]]

    fuse_hide_artists: list[str]
    fuse_hide_genres: list[str]
    fuse_hide_labels: list[str]

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

        artist_aliases_map: dict[str, list[str]] = defaultdict(list)
        artist_aliases_parents_map: dict[str, list[str]] = defaultdict(list)
        for parent, subs in data.get("artist_aliases", []):
            artist_aliases_map[parent] = subs
            for s in subs:
                artist_aliases_parents_map[s].append(parent)

        try:
            return cls(
                music_source_dir=Path(data["music_source_dir"]).expanduser(),
                fuse_mount_dir=Path(data["fuse_mount_dir"]).expanduser(),
                cache_dir=cache_dir,
                cache_database_path=cache_dir / "cache.sqlite3",
                artist_aliases_map=artist_aliases_map,
                artist_aliases_parents_map=artist_aliases_parents_map,
                fuse_hide_artists=data.get("fuse_hide_artists", []),
                fuse_hide_genres=data.get("fuse_hide_genres", []),
                fuse_hide_labels=data.get("fuse_hide_labels", []),
            )
        except KeyError as e:
            raise MissingConfigKeyError(f"Missing key in configuration file: {e}") from e
