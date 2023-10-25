from __future__ import annotations

import multiprocessing
import os
from collections import defaultdict
from dataclasses import dataclass
from hashlib import sha256
from pathlib import Path

import tomllib

from rose.common import RoseError

XDG_CONFIG_HOME = Path(os.environ.get("XDG_CONFIG_HOME", os.environ["HOME"] + "/.config"))
CONFIG_PATH = XDG_CONFIG_HOME / "rose" / "config.toml"
XDG_CACHE_HOME = Path(os.environ.get("XDG_CACHE_HOME", os.environ["HOME"] + "/.cache"))
CACHE_PATH = XDG_CACHE_HOME / "rose"


class ConfigNotFoundError(RoseError):
    pass


class ConfigDecodeError(RoseError):
    pass


class MissingConfigKeyError(RoseError):
    pass


class InvalidConfigValueError(RoseError):
    pass


@dataclass(frozen=True)
class Config:
    music_source_dir: Path
    fuse_mount_dir: Path
    cache_dir: Path
    cache_database_path: Path
    # Maximum parallel processes for cache updates. Defaults to nproc/2.
    max_proc: int

    # A map from parent artist -> subartists.
    artist_aliases_map: dict[str, list[str]]
    # A map from subartist -> parent artists.
    artist_aliases_parents_map: dict[str, list[str]]

    fuse_hide_artists: list[str]
    fuse_hide_genres: list[str]
    fuse_hide_labels: list[str]

    hash: str

    @classmethod
    def read(cls, config_path_override: Path | None = None) -> Config:
        cfgpath = config_path_override or CONFIG_PATH
        cfgtext = ""
        try:
            with cfgpath.open("r") as fp:
                cfgtext = fp.read()
                data = tomllib.loads(cfgtext)
        except FileNotFoundError as e:
            raise ConfigNotFoundError(f"Configuration file not found ({cfgpath})") from e
        except tomllib.TOMLDecodeError as e:
            raise ConfigDecodeError("Failed to decode configuration file: invalid TOML") from e

        try:
            music_source_dir = Path(data["music_source_dir"]).expanduser()
        except KeyError as e:
            raise MissingConfigKeyError(
                f"Missing key music_source_dir in configuration file ({cfgpath})"
            ) from e
        except (ValueError, TypeError) as e:
            raise InvalidConfigValueError(
                f"Invalid value for music_source_dir in configuration file ({cfgpath}): "
                "must be a path"
            ) from e

        try:
            fuse_mount_dir = Path(data["fuse_mount_dir"]).expanduser()
        except KeyError as e:
            raise MissingConfigKeyError(
                f"Missing key fuse_mount_dir in configuration file ({cfgpath})"
            ) from e
        except (ValueError, TypeError) as e:
            raise InvalidConfigValueError(
                f"Invalid value for fuse_mount_dir in configuration file ({cfgpath}): "
                "must be a path"
            ) from e

        try:
            cache_dir = Path(data["cache_dir"]).expanduser()
        except KeyError:
            cache_dir = CACHE_PATH
        except (TypeError, ValueError) as e:
            raise InvalidConfigValueError(
                f"Invalid value for cache_dir in configuration file ({cfgpath}): must be a path"
            ) from e
        cache_dir.mkdir(exist_ok=True)

        try:
            max_proc = int(data["max_proc"])
            if max_proc <= 0:
                raise ValueError(f"max_proc must be a positive integer: got {max_proc}")
        except KeyError:
            max_proc = max(1, multiprocessing.cpu_count() // 2)
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for max_proc in configuration file ({cfgpath}): "
                "must be a positive integer"
            ) from e

        artist_aliases_map: dict[str, list[str]] = defaultdict(list)
        artist_aliases_parents_map: dict[str, list[str]] = defaultdict(list)
        try:
            for entry in data.get("artist_aliases", []):
                if not isinstance(entry["artist"], str):
                    raise ValueError(f"Artists must be of type str: got {type(entry['artist'])}")
                artist_aliases_map[entry["artist"]] = entry["aliases"]
                if not isinstance(entry["aliases"], list):
                    raise ValueError(
                        f"Aliases must be of type list[str]: got {type(entry['aliases'])}"
                    )
                for s in entry["aliases"]:
                    if not isinstance(s, str):
                        raise ValueError(f"Each alias must be of type str: got {type(s)}")
                    artist_aliases_parents_map[s].append(entry["artist"])
        except (ValueError, TypeError, KeyError) as e:
            raise InvalidConfigValueError(
                f"Invalid value for artist_aliases in configuration file ({cfgpath}): "
                "must be a list of { artist = str, aliases = list[str] } records"
            ) from e

        try:
            fuse_hide_artists = data["fuse_hide_artists"]
            if not isinstance(fuse_hide_artists, list):
                raise ValueError(
                    f"fuse_hide_artists must be a list[str]: got {type(fuse_hide_artists)}"
                )
            for s in fuse_hide_artists:
                if not isinstance(s, str):
                    raise ValueError(f"Each artist must be of type str: got {type(s)}")
        except KeyError:
            fuse_hide_artists = []
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for fuse_hide_artists in configuration file ({cfgpath}): "
                "must be a list of strings"
            ) from e

        try:
            fuse_hide_genres = data["fuse_hide_genres"]
            if not isinstance(fuse_hide_genres, list):
                raise ValueError(
                    f"fuse_hide_genres must be a list[str]: got {type(fuse_hide_genres)}"
                )
            for s in fuse_hide_genres:
                if not isinstance(s, str):
                    raise ValueError(f"Each genre must be of type str: got {type(s)}")
        except KeyError:
            fuse_hide_genres = []
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for fuse_hide_genres in configuration file ({cfgpath}): "
                "must be a list of strings"
            ) from e

        try:
            fuse_hide_labels = data["fuse_hide_labels"]
            if not isinstance(fuse_hide_labels, list):
                raise ValueError(
                    f"fuse_hide_labels must be a list[str]: got {type(fuse_hide_labels)}"
                )
            for s in fuse_hide_labels:
                if not isinstance(s, str):
                    raise ValueError(f"Each label must be of type str: got {type(s)}")
        except KeyError:
            fuse_hide_labels = []
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for fuse_hide_labels in configuration file ({cfgpath}): "
                "must be a list of strings"
            ) from e

        return cls(
            music_source_dir=music_source_dir,
            fuse_mount_dir=fuse_mount_dir,
            cache_dir=cache_dir,
            cache_database_path=cache_dir / "cache.sqlite3",
            max_proc=max_proc,
            artist_aliases_map=artist_aliases_map,
            artist_aliases_parents_map=artist_aliases_parents_map,
            fuse_hide_artists=fuse_hide_artists,
            fuse_hide_genres=fuse_hide_genres,
            fuse_hide_labels=fuse_hide_labels,
            hash=sha256(cfgtext.encode()).hexdigest(),
        )
