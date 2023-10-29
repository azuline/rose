"""
The config module encapsulates the configuration parsing logic. The config module is written with
special care to provide early and comprehensible errors when the configuration is invalid.
"""

from __future__ import annotations

import functools
import multiprocessing
from collections import defaultdict
from dataclasses import dataclass
from hashlib import sha256
from pathlib import Path

import appdirs
import tomllib

from rose.common import RoseError

XDG_CONFIG_ROSE = Path(appdirs.user_config_dir("rose"))
XDG_CONFIG_ROSE.mkdir(parents=True, exist_ok=True)
CONFIG_PATH = XDG_CONFIG_ROSE / "config.toml"

XDG_CACHE_ROSE = Path(appdirs.user_cache_dir("rose"))
XDG_CACHE_ROSE.mkdir(parents=True, exist_ok=True)


class ConfigNotFoundError(RoseError):
    pass


class ConfigDecodeError(RoseError):
    pass


class MissingConfigKeyError(RoseError):
    pass


class InvalidConfigValueError(RoseError, ValueError):
    pass


@dataclass(frozen=True)
class Config:
    music_source_dir: Path
    fuse_mount_dir: Path
    cache_dir: Path
    # Maximum parallel processes for cache updates. Defaults to nproc/2.
    max_proc: int

    # A map from parent artist -> subartists.
    artist_aliases_map: dict[str, list[str]]
    # A map from subartist -> parent artists.
    artist_aliases_parents_map: dict[str, list[str]]

    fuse_artists_whitelist: list[str] | None
    fuse_genres_whitelist: list[str] | None
    fuse_labels_whitelist: list[str] | None
    fuse_artists_blacklist: list[str] | None
    fuse_genres_blacklist: list[str] | None
    fuse_labels_blacklist: list[str] | None

    cover_art_stems: list[str]
    valid_art_exts: list[str]

    ignore_release_directories: list[str]

    hash: str

    @classmethod
    def parse(cls, config_path_override: Path | None = None) -> Config:
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
            cache_dir = XDG_CACHE_ROSE
        except (TypeError, ValueError) as e:
            raise InvalidConfigValueError(
                f"Invalid value for cache_dir in configuration file ({cfgpath}): must be a path"
            ) from e
        cache_dir.mkdir(parents=True, exist_ok=True)

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
            fuse_artists_whitelist = data["fuse_artists_whitelist"]
            if not isinstance(fuse_artists_whitelist, list):
                raise ValueError(
                    f"fuse_artists_whitelist must be a list[str]: "
                    f"got {type(fuse_artists_whitelist)}"
                )
            for s in fuse_artists_whitelist:
                if not isinstance(s, str):
                    raise ValueError(f"Each artist must be of type str: got {type(s)}")
        except KeyError:
            fuse_artists_whitelist = None
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for fuse_artists_whitelist in configuration file ({cfgpath}): "
                "must be a list of strings"
            ) from e

        try:
            fuse_genres_whitelist = data["fuse_genres_whitelist"]
            if not isinstance(fuse_genres_whitelist, list):
                raise ValueError(
                    f"fuse_genres_whitelist must be a list[str]: got {type(fuse_genres_whitelist)}"
                )
            for s in fuse_genres_whitelist:
                if not isinstance(s, str):
                    raise ValueError(f"Each genre must be of type str: got {type(s)}")
        except KeyError:
            fuse_genres_whitelist = None
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for fuse_genres_whitelist in configuration file ({cfgpath}): "
                "must be a list of strings"
            ) from e

        try:
            fuse_labels_whitelist = data["fuse_labels_whitelist"]
            if not isinstance(fuse_labels_whitelist, list):
                raise ValueError(
                    f"fuse_labels_whitelist must be a list[str]: got {type(fuse_labels_whitelist)}"
                )
            for s in fuse_labels_whitelist:
                if not isinstance(s, str):
                    raise ValueError(f"Each label must be of type str: got {type(s)}")
        except KeyError:
            fuse_labels_whitelist = None
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for fuse_labels_whitelist in configuration file ({cfgpath}): "
                "must be a list of strings"
            ) from e

        try:
            fuse_artists_blacklist = data["fuse_artists_blacklist"]
            if not isinstance(fuse_artists_blacklist, list):
                raise ValueError(
                    f"fuse_artists_blacklist must be a list[str]: "
                    f"got {type(fuse_artists_blacklist)}"
                )
            for s in fuse_artists_blacklist:
                if not isinstance(s, str):
                    raise ValueError(f"Each artist must be of type str: got {type(s)}")
        except KeyError:
            fuse_artists_blacklist = None
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for fuse_artists_blacklist in configuration file ({cfgpath}): "
                "must be a list of strings"
            ) from e

        try:
            fuse_genres_blacklist = data["fuse_genres_blacklist"]
            if not isinstance(fuse_genres_blacklist, list):
                raise ValueError(
                    f"fuse_genres_blacklist must be a list[str]: got {type(fuse_genres_blacklist)}"
                )
            for s in fuse_genres_blacklist:
                if not isinstance(s, str):
                    raise ValueError(f"Each genre must be of type str: got {type(s)}")
        except KeyError:
            fuse_genres_blacklist = None
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for fuse_genres_blacklist in configuration file ({cfgpath}): "
                "must be a list of strings"
            ) from e

        try:
            fuse_labels_blacklist = data["fuse_labels_blacklist"]
            if not isinstance(fuse_labels_blacklist, list):
                raise ValueError(
                    f"fuse_labels_blacklist must be a list[str]: got {type(fuse_labels_blacklist)}"
                )
            for s in fuse_labels_blacklist:
                if not isinstance(s, str):
                    raise ValueError(f"Each label must be of type str: got {type(s)}")
        except KeyError:
            fuse_labels_blacklist = None
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for fuse_labels_blacklist in configuration file ({cfgpath}): "
                "must be a list of strings"
            ) from e

        if fuse_artists_whitelist and fuse_artists_blacklist:
            raise InvalidConfigValueError(
                "Cannot specify both fuse_artists_whitelist and fuse_artists_blacklist in "
                f"configuration file ({cfgpath}): must specify only one or the other"
            )
        if fuse_genres_whitelist and fuse_genres_blacklist:
            raise InvalidConfigValueError(
                "Cannot specify both fuse_genres_whitelist and fuse_genres_blacklist in "
                f"configuration file ({cfgpath}): must specify only one or the other"
            )
        if fuse_labels_whitelist and fuse_labels_blacklist:
            raise InvalidConfigValueError(
                "Cannot specify both fuse_labels_whitelist and fuse_labels_blacklist in "
                f"configuration file ({cfgpath}): must specify only one or the other"
            )

        try:
            cover_art_stems = data.get("cover_art_stems", ["folder", "cover", "art", "front"])
            if not isinstance(cover_art_stems, list):
                raise ValueError(
                    f"cover_art_stems must be a list[str]: got {type(cover_art_stems)}"
                )
            for s in cover_art_stems:
                if not isinstance(s, str):
                    raise ValueError(f"Each cover art stem must be of type str: got {type(s)}")
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for cover_art_stems in configuration file ({cfgpath}): "
                "must be a list of strings"
            ) from e

        try:
            valid_art_exts = data.get("valid_art_exts", ["jpg", "jpeg", "png"])
            if not isinstance(valid_art_exts, list):
                raise ValueError(f"valid_art_exts must be a list[str]: got {type(valid_art_exts)}")
            for s in valid_art_exts:
                if not isinstance(s, str):
                    raise ValueError(f"Each art extension must be of type str: got {type(s)}")
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for valid_art_exts in configuration file ({cfgpath}): "
                "must be a list of strings"
            ) from e

        cover_art_stems = [x.lower() for x in cover_art_stems]
        valid_art_exts = [x.lower() for x in valid_art_exts]

        try:
            ignore_release_directories = data.get("ignore_release_directories", [])
            if not isinstance(ignore_release_directories, list):
                raise ValueError(
                    "ignore_release_directories must be a list[str]: "
                    f"got {type(ignore_release_directories)}"
                )
            for s in ignore_release_directories:
                if not isinstance(s, str):
                    raise ValueError(f"Each release directory must be of type str: got {type(s)}")
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for ignore_release_directories in configuration file ({cfgpath}): "
                "must be a list of strings"
            ) from e

        return cls(
            music_source_dir=music_source_dir,
            fuse_mount_dir=fuse_mount_dir,
            cache_dir=cache_dir,
            max_proc=max_proc,
            artist_aliases_map=artist_aliases_map,
            artist_aliases_parents_map=artist_aliases_parents_map,
            fuse_artists_whitelist=fuse_artists_whitelist,
            fuse_genres_whitelist=fuse_genres_whitelist,
            fuse_labels_whitelist=fuse_labels_whitelist,
            fuse_artists_blacklist=fuse_artists_blacklist,
            fuse_genres_blacklist=fuse_genres_blacklist,
            fuse_labels_blacklist=fuse_labels_blacklist,
            cover_art_stems=cover_art_stems,
            valid_art_exts=valid_art_exts,
            ignore_release_directories=ignore_release_directories,
            hash=sha256(cfgtext.encode()).hexdigest(),
        )

    @functools.cached_property
    def valid_cover_arts(self) -> list[str]:
        return [s + "." + e for s in self.cover_art_stems for e in self.valid_art_exts]

    @functools.cached_property
    def cache_database_path(self) -> Path:
        return self.cache_dir / "cache.sqlite3"

    @functools.cached_property
    def watchdog_pid_path(self) -> Path:
        return self.cache_dir / "watchdog.pid"
