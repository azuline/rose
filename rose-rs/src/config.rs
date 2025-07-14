"""
The config module provides the config spec and parsing logic.

We take special care to optimize the configuration experience: Rose provides detailed errors when an
invalid configuration is detected, and emits warnings when unrecognized keys are found.
"""

from __future__ import annotations

import contextlib
import functools
import logging
import multiprocessing
import tomllib
from collections import defaultdict, deque
from copy import deepcopy
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import appdirs

from rose.common import RoseExpectedError
from rose.rule_parser import Rule, RuleSyntaxError
from rose.templates import (
    DEFAULT_TEMPLATE_PAIR,
    InvalidPathTemplateError,
    PathTemplate,
    PathTemplateConfig,
)

XDG_CONFIG_ROSE = Path(appdirs.user_config_dir("rose"))
XDG_CONFIG_ROSE.mkdir(parents=True, exist_ok=True)
CONFIG_PATH = XDG_CONFIG_ROSE / "config.toml"

XDG_CACHE_ROSE = Path(appdirs.user_cache_dir("rose"))
XDG_CACHE_ROSE.mkdir(parents=True, exist_ok=True)

logger = logging.getLogger(__name__)


class ConfigNotFoundError(RoseExpectedError):
    pass


class ConfigDecodeError(RoseExpectedError):
    pass


class MissingConfigKeyError(RoseExpectedError):
    pass


class InvalidConfigValueError(RoseExpectedError, ValueError):
    pass


@dataclass(frozen=True)
class VirtualFSConfig:
    mount_dir: Path

    artists_whitelist: list[str] | None
    genres_whitelist: list[str] | None
    descriptors_whitelist: list[str] | None
    labels_whitelist: list[str] | None
    artists_blacklist: list[str] | None
    genres_blacklist: list[str] | None
    descriptors_blacklist: list[str] | None
    labels_blacklist: list[str] | None

    hide_genres_with_only_new_releases: bool
    hide_descriptors_with_only_new_releases: bool
    hide_labels_with_only_new_releases: bool

    @classmethod
    def parse(cls, cfgpath: Path, data: dict[str, Any]) -> VirtualFSConfig:
        """Modifies `config` by deleting any keys read."""
        try:
            mount_dir = Path(data["mount_dir"]).expanduser()
            del data["mount_dir"]
        except KeyError as e:
            raise MissingConfigKeyError(f"Missing key vfs.mount_dir in configuration file ({cfgpath})") from e
        except (ValueError, TypeError) as e:
            raise InvalidConfigValueError(
                f"Invalid value for vfs.mount_dir in configuration file ({cfgpath}): must be a path"
            ) from e

        try:
            artists_whitelist = data["artists_whitelist"]
            del data["artists_whitelist"]
            if not isinstance(artists_whitelist, list):
                raise ValueError(f"Must be a list[str]: got {type(artists_whitelist)}")
            for s in artists_whitelist:
                if not isinstance(s, str):
                    raise ValueError(f"Each artist must be of type str: got {type(s)}")
        except KeyError:
            artists_whitelist = None
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for vfs.artists_whitelist in configuration file ({cfgpath}): {e}"
            ) from e

        try:
            genres_whitelist = data["genres_whitelist"]
            del data["genres_whitelist"]
            if not isinstance(genres_whitelist, list):
                raise ValueError(f"Must be a list[str]: got {type(genres_whitelist)}")
            for s in genres_whitelist:
                if not isinstance(s, str):
                    raise ValueError(f"Each genre must be of type str: got {type(s)}")
        except KeyError:
            genres_whitelist = None
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for vfs.genres_whitelist in configuration file ({cfgpath}): {e}"
            ) from e

        try:
            descriptors_whitelist = data["descriptors_whitelist"]
            del data["descriptors_whitelist"]
            if not isinstance(descriptors_whitelist, list):
                raise ValueError(f"Must be a list[str]: got {type(descriptors_whitelist)}")
            for s in descriptors_whitelist:
                if not isinstance(s, str):
                    raise ValueError(f"Each descriptor must be of type str: got {type(s)}")
        except KeyError:
            descriptors_whitelist = None
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for vfs.descriptors_whitelist in configuration file ({cfgpath}): {e}"
            ) from e

        try:
            labels_whitelist = data["labels_whitelist"]
            del data["labels_whitelist"]
            if not isinstance(labels_whitelist, list):
                raise ValueError(f"Must be a list[str]: got {type(labels_whitelist)}")
            for s in labels_whitelist:
                if not isinstance(s, str):
                    raise ValueError(f"Each label must be of type str: got {type(s)}")
        except KeyError:
            labels_whitelist = None
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for vfs.labels_whitelist in configuration file ({cfgpath}): {e}"
            ) from e

        try:
            artists_blacklist = data["artists_blacklist"]
            del data["artists_blacklist"]
            if not isinstance(artists_blacklist, list):
                raise ValueError(f"Must be a list[str]: got {type(artists_blacklist)}")
            for s in artists_blacklist:
                if not isinstance(s, str):
                    raise ValueError(f"Each artist must be of type str: got {type(s)}")
        except KeyError:
            artists_blacklist = None
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for vfs.artists_blacklist in configuration file ({cfgpath}): {e}"
            ) from e

        try:
            genres_blacklist = data["genres_blacklist"]
            del data["genres_blacklist"]
            if not isinstance(genres_blacklist, list):
                raise ValueError(f"Must be a list[str]: got {type(genres_blacklist)}")
            for s in genres_blacklist:
                if not isinstance(s, str):
                    raise ValueError(f"Each genre must be of type str: got {type(s)}")
        except KeyError:
            genres_blacklist = None
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for vfs.genres_blacklist in configuration file ({cfgpath}): {e}"
            ) from e

        try:
            descriptors_blacklist = data["descriptors_blacklist"]
            del data["descriptors_blacklist"]
            if not isinstance(descriptors_blacklist, list):
                raise ValueError(f"Must be a list[str]: got {type(descriptors_blacklist)}")
            for s in descriptors_blacklist:
                if not isinstance(s, str):
                    raise ValueError(f"Each descriptor must be of type str: got {type(s)}")
        except KeyError:
            descriptors_blacklist = None
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for vfs.descriptors_blacklist in configuration file ({cfgpath}): {e}"
            ) from e

        try:
            labels_blacklist = data["labels_blacklist"]
            del data["labels_blacklist"]
            if not isinstance(labels_blacklist, list):
                raise ValueError(f"Must be a list[str]: got {type(labels_blacklist)}")
            for s in labels_blacklist:
                if not isinstance(s, str):
                    raise ValueError(f"Each label must be of type str: got {type(s)}")
        except KeyError:
            labels_blacklist = None
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for vfs.labels_blacklist in configuration file ({cfgpath}): {e}"
            ) from e

        if artists_whitelist and artists_blacklist:
            raise InvalidConfigValueError(
                f"Cannot specify both vfs.artists_whitelist and vfs.artists_blacklist in configuration file ({cfgpath}): must specify only one or the other"
            )
        if genres_whitelist and genres_blacklist:
            raise InvalidConfigValueError(
                f"Cannot specify both vfs.genres_whitelist and vfs.genres_blacklist in configuration file ({cfgpath}): must specify only one or the other"
            )
        if labels_whitelist and labels_blacklist:
            raise InvalidConfigValueError(
                f"Cannot specify both vfs.labels_whitelist and vfs.labels_blacklist in configuration file ({cfgpath}): must specify only one or the other"
            )

        try:
            hide_genres_with_only_new_releases = data["hide_genres_with_only_new_releases"]
            del data["hide_genres_with_only_new_releases"]
            if not isinstance(hide_genres_with_only_new_releases, bool):
                raise ValueError(f"Must be a bool: got {type(hide_genres_with_only_new_releases)}")
        except KeyError:
            hide_genres_with_only_new_releases = False
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for vfs.hide_genres_with_only_new_releases in configuration file ({cfgpath}): {e}"
            ) from e

        try:
            hide_descriptors_with_only_new_releases = data["hide_descriptors_with_only_new_releases"]
            del data["hide_descriptors_with_only_new_releases"]
            if not isinstance(hide_descriptors_with_only_new_releases, bool):
                raise ValueError(f"Must be a bool: got {type(hide_descriptors_with_only_new_releases)}")
        except KeyError:
            hide_descriptors_with_only_new_releases = False
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for vfs.hide_descriptors_with_only_new_releases in configuration file ({cfgpath}): {e}"
            ) from e

        try:
            hide_labels_with_only_new_releases = data["hide_labels_with_only_new_releases"]
            del data["hide_labels_with_only_new_releases"]
            if not isinstance(hide_labels_with_only_new_releases, bool):
                raise ValueError(f"Must be a bool: got {type(hide_labels_with_only_new_releases)}")
        except KeyError:
            hide_labels_with_only_new_releases = False
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for vfs.hide_labels_with_only_new_releases in configuration file ({cfgpath}): {e}"
            ) from e

        return VirtualFSConfig(
            mount_dir=mount_dir,
            artists_whitelist=artists_whitelist,
            genres_whitelist=genres_whitelist,
            descriptors_whitelist=descriptors_whitelist,
            labels_whitelist=labels_whitelist,
            artists_blacklist=artists_blacklist,
            genres_blacklist=genres_blacklist,
            descriptors_blacklist=descriptors_blacklist,
            labels_blacklist=labels_blacklist,
            hide_genres_with_only_new_releases=hide_genres_with_only_new_releases,
            hide_descriptors_with_only_new_releases=hide_descriptors_with_only_new_releases,
            hide_labels_with_only_new_releases=hide_labels_with_only_new_releases,
        )


@dataclass(frozen=True)
class Config:
    music_source_dir: Path
    cache_dir: Path
    # Maximum parallel processes for cache updates. Defaults to nproc/2.
    max_proc: int
    ignore_release_directories: list[str]

    rename_source_files: bool
    max_filename_bytes: int
    cover_art_stems: list[str]
    valid_art_exts: list[str]
    write_parent_genres: bool

    # A map from parent artist -> subartists.
    artist_aliases_map: dict[str, list[str]]
    # A map from subartist -> parent artists.
    artist_aliases_parents_map: dict[str, list[str]]

    path_templates: PathTemplateConfig
    stored_metadata_rules: list[Rule]

    vfs: VirtualFSConfig

    @classmethod
    def parse(cls, config_path_override: Path | None = None) -> Config:
        # As we parse, delete consumed values from the data dictionary. If any are left over at the
        # end of the config, warn that unknown config keys were found.
        cfgpath = config_path_override or CONFIG_PATH
        cfgtext = ""
        try:
            with cfgpath.open("r") as fp:
                cfgtext = fp.read()
                data = tomllib.loads(cfgtext)
        except FileNotFoundError as e:
            raise ConfigNotFoundError(f"Configuration file not found ({cfgpath})") from e
        except tomllib.TOMLDecodeError as e:
            raise ConfigDecodeError(f"Failed to decode configuration file: invalid TOML: {e}") from e

        try:
            music_source_dir = Path(data["music_source_dir"]).expanduser()
            del data["music_source_dir"]
        except KeyError as e:
            raise MissingConfigKeyError(f"Missing key music_source_dir in configuration file ({cfgpath})") from e
        except (ValueError, TypeError) as e:
            raise InvalidConfigValueError(
                f"Invalid value for music_source_dir in configuration file ({cfgpath}): must be a path"
            ) from e

        try:
            cache_dir = Path(data["cache_dir"]).expanduser()
            del data["cache_dir"]
        except KeyError:
            cache_dir = XDG_CACHE_ROSE
        except (TypeError, ValueError) as e:
            raise InvalidConfigValueError(
                f"Invalid value for cache_dir in configuration file ({cfgpath}): must be a path"
            ) from e
        cache_dir.mkdir(parents=True, exist_ok=True)

        try:
            max_proc = int(data["max_proc"])
            del data["max_proc"]
            if max_proc <= 0:
                raise ValueError(f"must be a positive integer: got {max_proc}")
        except KeyError:
            max_proc = max(1, multiprocessing.cpu_count() // 2)
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for max_proc in configuration file ({cfgpath}): must be a positive integer"
            ) from e

        artist_aliases_map: dict[str, list[str]] = defaultdict(list)
        artist_aliases_parents_map: dict[str, list[str]] = defaultdict(list)
        try:
            for entry in data.get("artist_aliases", []):
                if not isinstance(entry["artist"], str):
                    raise ValueError(f"Artists must be of type str: got {type(entry["artist"])}")
                artist_aliases_map[entry["artist"]] = entry["aliases"]
                if not isinstance(entry["aliases"], list):
                    raise ValueError(f"Aliases must be of type list[str]: got {type(entry["aliases"])}")
                for s in entry["aliases"]:
                    if not isinstance(s, str):
                        raise ValueError(f"Each alias must be of type str: got {type(s)}")
                    artist_aliases_parents_map[s].append(entry["artist"])
            with contextlib.suppress(KeyError):
                del data["artist_aliases"]
        except (ValueError, TypeError, KeyError) as e:
            raise InvalidConfigValueError(
                f"Invalid value for artist_aliases in configuration file ({cfgpath}): must be a list of {{ artist = str, aliases = list[str] }} records"
            ) from e

        try:
            cover_art_stems = data["cover_art_stems"]
            del data["cover_art_stems"]
            if not isinstance(cover_art_stems, list):
                raise ValueError(f"Must be a list[str]: got {type(cover_art_stems)}")
            for s in cover_art_stems:
                if not isinstance(s, str):
                    raise ValueError(f"Each cover art stem must be of type str: got {type(s)}")
        except KeyError:
            cover_art_stems = ["folder", "cover", "art", "front"]
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for cover_art_stems in configuration file ({cfgpath}): {e}"
            ) from e

        try:
            valid_art_exts = data["valid_art_exts"]
            del data["valid_art_exts"]
            if not isinstance(valid_art_exts, list):
                raise ValueError(f"Must be a list[str]: got {type(valid_art_exts)}")
            for s in valid_art_exts:
                if not isinstance(s, str):
                    raise ValueError(f"Each art extension must be of type str: got {type(s)}")
        except KeyError:
            valid_art_exts = ["jpg", "jpeg", "png"]
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for valid_art_exts in configuration file ({cfgpath}): {e}"
            ) from e

        cover_art_stems = [x.lower() for x in cover_art_stems]
        valid_art_exts = [x.lower() for x in valid_art_exts]

        try:
            write_parent_genres = data["write_parent_genres"]
            del data["write_parent_genres"]
            if not isinstance(write_parent_genres, bool):
                raise ValueError(f"Must be a bool: got {type(write_parent_genres)}")
        except KeyError:
            write_parent_genres = False
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for write_parent_genres in configuration file ({cfgpath}): {e}"
            ) from e

        try:
            max_filename_bytes = data["max_filename_bytes"]
            del data["max_filename_bytes"]
            if not isinstance(max_filename_bytes, int):
                raise ValueError(f"Must be an int: got {type(max_filename_bytes)}")
        except KeyError:
            max_filename_bytes = 180
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for max_filename_bytes in configuration file ({cfgpath}): {e}"
            ) from e

        try:
            rename_source_files = data["rename_source_files"]
            del data["rename_source_files"]
            if not isinstance(rename_source_files, bool):
                raise ValueError(f"Must be a bool: got {type(rename_source_files)}")
        except KeyError:
            rename_source_files = False
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for rename_source_files in configuration file ({cfgpath}): {e}"
            ) from e

        try:
            ignore_release_directories = data["ignore_release_directories"]
            del data["ignore_release_directories"]
            if not isinstance(ignore_release_directories, list):
                raise ValueError(f"Must be a list[str]: got {type(ignore_release_directories)}")
            for s in ignore_release_directories:
                if not isinstance(s, str):
                    raise ValueError(f"Each release directory must be of type str: got {type(s)}")
        except KeyError:
            ignore_release_directories = []
        except ValueError as e:
            raise InvalidConfigValueError(
                f"Invalid value for ignore_release_directories in configuration file ({cfgpath}): {e}"
            ) from e

        stored_metadata_rules: list[Rule] = []
        for d in data.get("stored_metadata_rules", []):
            if not isinstance(d, dict):
                raise InvalidConfigValueError(
                    f"Invalid value in stored_metadata_rules in configuration file ({cfgpath}): list values must be a dict: got {type(d)}"
                )

            try:
                matcher = d["matcher"]
            except KeyError as e:
                raise InvalidConfigValueError(
                    f"Missing key `matcher` in stored_metadata_rules in configuration file ({cfgpath}): rule {d}"
                ) from e
            if not isinstance(matcher, str):
                raise InvalidConfigValueError(
                    f"Invalid value for `matcher` in stored_metadata_rules in configuration file ({cfgpath}): rule {d}: must be a string"
                )

            try:
                actions = d["actions"]
            except KeyError as e:
                raise InvalidConfigValueError(
                    f"Missing key `actions` in stored_metadata_rules in configuration file ({cfgpath}): rule {d}"
                ) from e
            if not isinstance(actions, list):
                raise InvalidConfigValueError(
                    f"Invalid value for `actions` in stored_metadata_rules in configuration file ({cfgpath}): rule {d}: must be a list of strings"
                )
            for action in actions:
                if not isinstance(action, str):
                    raise InvalidConfigValueError(
                        f"Invalid value for `actions` in stored_metadata_rules in configuration file ({cfgpath}): rule {d}: must be a list of strings: got {type(action)}"
                    )

            ignore = d.get("ignore", [])
            if not isinstance(ignore, list):
                raise InvalidConfigValueError(
                    f"Invalid value for `ignore` in stored_metadata_rules in configuration file ({cfgpath}): rule {d}: must be a list of strings"
                )
            for i in ignore:
                if not isinstance(i, str):
                    raise InvalidConfigValueError(
                        f"Invalid value for `ignore` in stored_metadata_rules in configuration file ({cfgpath}): rule {d}: must be a list of strings: got {type(i)}"
                    )

            try:
                stored_metadata_rules.append(Rule.parse(matcher, actions, ignore))
            except RuleSyntaxError as e:
                raise InvalidConfigValueError(
                    f"Failed to parse stored_metadata_rules in configuration file ({cfgpath}): rule {d}: {e}"
                ) from e
        if "stored_metadata_rules" in data:
            del data["stored_metadata_rules"]

        # Get the potential default template before evaluating the rest.
        default_templates = deepcopy(DEFAULT_TEMPLATE_PAIR)
        with contextlib.suppress(KeyError):
            default_templates.release = PathTemplate(data["path_templates"]["default"]["release"])
            del data["path_templates"]["default"]["release"]
        with contextlib.suppress(KeyError):
            default_templates.track = PathTemplate(data["path_templates"]["default"]["track"])
            del data["path_templates"]["default"]["track"]
        with contextlib.suppress(KeyError):
            default_templates.all_tracks = PathTemplate(data["path_templates"]["default"]["all_tracks"])
            del data["path_templates"]["default"]["all_tracks"]
        with contextlib.suppress(KeyError):
            if not data["path_templates"]["default"]:
                del data["path_templates"]["default"]

        path_templates = PathTemplateConfig.with_defaults(default_templates)
        if tmpl_config := data.get("path_templates", None):
            for key in [
                "source",
                "releases",
                "releases_new",
                "releases_added_on",
                "releases_released_on",
                "artists",
                "genres",
                "descriptors",
                "labels",
                "loose_tracks",
                "collages",
            ]:
                with contextlib.suppress(KeyError):
                    getattr(path_templates, key).release = PathTemplate(tmpl_config[key]["release"])
                    del tmpl_config[key]["release"]
                with contextlib.suppress(KeyError):
                    getattr(path_templates, key).track = PathTemplate(tmpl_config[key]["track"])
                    del tmpl_config[key]["track"]
                with contextlib.suppress(KeyError):
                    getattr(path_templates, key).all_tracks = PathTemplate(tmpl_config[key]["all_tracks"])
                    del tmpl_config[key]["all_tracks"]
                with contextlib.suppress(KeyError):
                    if not tmpl_config[key]:
                        del tmpl_config[key]

            with contextlib.suppress(KeyError):
                path_templates.playlists = PathTemplate(tmpl_config["playlists"])
                del tmpl_config["playlists"]
        with contextlib.suppress(KeyError):
            if not data["path_templates"]:
                del data["path_templates"]

        vfs_config = VirtualFSConfig.parse(cfgpath, data.get("vfs", {}))

        if data:
            unrecognized_accessors: list[str] = []
            # Do a DFS over the data keys to assemble the map of unknown keys. State is a tuple of
            # ("accessor", node).
            dfs_state: deque[tuple[str, dict[str, Any]]] = deque([("", data)])
            while dfs_state:
                accessor, node = dfs_state.pop()
                if isinstance(node, dict):
                    for k, v in node.items():
                        child_accessor = k if not accessor else f"{accessor}.{k}"
                        dfs_state.append((child_accessor, v))
                    continue
                unrecognized_accessors.append(accessor)
            if unrecognized_accessors:
                logger.warning(f"Unrecognized options found in configuration file: {", ".join(unrecognized_accessors)}")

        return Config(
            music_source_dir=music_source_dir,
            cache_dir=cache_dir,
            max_proc=max_proc,
            artist_aliases_map=artist_aliases_map,
            artist_aliases_parents_map=artist_aliases_parents_map,
            cover_art_stems=cover_art_stems,
            valid_art_exts=valid_art_exts,
            write_parent_genres=write_parent_genres,
            max_filename_bytes=max_filename_bytes,
            path_templates=path_templates,
            rename_source_files=rename_source_files,
            ignore_release_directories=ignore_release_directories,
            stored_metadata_rules=stored_metadata_rules,
            vfs=vfs_config,
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

    def validate_path_templates_expensive(self) -> None:
        """
        Validate all the path templates. This is expensive, so we don't do it when reading the
        configuration, only on demand.
        """
        try:
            self.path_templates.parse()
        except InvalidPathTemplateError as e:
            raise InvalidConfigValueError(f"Invalid path template in for template {e.key}: {e}") from e

# TESTS

import tempfile
from pathlib import Path

import click
import pytest

from rose.config import (
    Config,
    ConfigNotFoundError,
    InvalidConfigValueError,
    MissingConfigKeyError,
    VirtualFSConfig,
)
from rose.rule_parser import (
    Action,
    Matcher,
    Pattern,
    ReplaceAction,
    Rule,
    SplitAction,
)
from rose.templates import PathTemplate, PathTemplateConfig, PathTemplateTriad


def test_config_minimal() -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        with path.open("w") as fp:
            fp.write(
                """
                music_source_dir = "~/.music-src"
                vfs.mount_dir = "~/music"
                """
            )

        c = Config.parse(config_path_override=path)
        assert c.music_source_dir == Path.home() / ".music-src"
        assert c.vfs.mount_dir == Path.home() / "music"


def test_config_full() -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        cache_dir = Path(tmpdir) / "cache"
        with path.open("w") as fp:
            fp.write(
                f"""
                music_source_dir = "~/.music-src"
                cache_dir = "{cache_dir}"
                max_proc = 8
                artist_aliases = [
                  {{ artist = "Abakus", aliases = ["Cinnamon Chasers"] }},
                  {{ artist = "tripleS", aliases = ["EVOLution", "LOVElution", "+(KR)ystal Eyes", "Acid Angel From Asia", "Acid Eyes"] }},
                ]

                cover_art_stems = [ "aa", "bb" ]
                valid_art_exts = [ "tiff" ]
                write_parent_genres = true
                max_filename_bytes = 255
                ignore_release_directories = [ "dummy boy" ]
                rename_source_files = true

                [[stored_metadata_rules]]
                matcher = "tracktitle:lala"
                actions = ["replace:hihi"]

                [[stored_metadata_rules]]
                matcher = "trackartist[main]:haha"
                actions = ["replace:bibi", "split: "]
                ignore = ["releasetitle:blabla"]

                [path_templates]
                default.release = "{{{{ title }}}}"
                default.track = "{{{{ title }}}}"
                default.all_tracks = "{{{{ title }}}}"
                source.release = "{{{{ title }}}}"
                source.track = "{{{{ title }}}}"
                source.all_tracks = "{{{{ title }}}}"
                releases.release = "{{{{ title }}}}"
                releases.track = "{{{{ title }}}}"
                releases.all_tracks = "{{{{ title }}}}"
                releases_new.release = "{{{{ title }}}}"
                releases_new.track = "{{{{ title }}}}"
                releases_new.all_tracks = "{{{{ title }}}}"
                releases_added_on.release = "{{{{ title }}}}"
                releases_added_on.track = "{{{{ title }}}}"
                releases_added_on.all_tracks = "{{{{ title }}}}"
                releases_released_on.release = "{{{{ title }}}}"
                releases_released_on.track = "{{{{ title }}}}"
                releases_released_on.all_tracks = "{{{{ title }}}}"
                artists.release = "{{{{ title }}}}"
                artists.track = "{{{{ title }}}}"
                artists.all_tracks = "{{{{ title }}}}"
                labels.release = "{{{{ title }}}}"
                labels.track = "{{{{ title }}}}"
                labels.all_tracks = "{{{{ title }}}}"
                loose_tracks.release = "{{{{ title }}}}"
                loose_tracks.track = "{{{{ title }}}}"
                loose_tracks.all_tracks = "{{{{ title }}}}"
                collages.release = "{{{{ title }}}}"
                collages.track = "{{{{ title }}}}"
                collages.all_tracks = "{{{{ title }}}}"
                # Genres and descriptors omitted to test the defaults.
                playlists = "{{{{ title }}}}"

                [vfs]
                mount_dir = "~/music"
                artists_blacklist = [ "www" ]
                genres_blacklist = [ "xxx" ]
                descriptors_blacklist = [ "yyy" ]
                labels_blacklist = [ "zzz" ]
                hide_genres_with_only_new_releases = true
                hide_descriptors_with_only_new_releases = true
                hide_labels_with_only_new_releases = true
                """
            )

        c = Config.parse(config_path_override=path)
        assert c == Config(
            music_source_dir=Path.home() / ".music-src",
            cache_dir=cache_dir,
            max_proc=8,
            artist_aliases_map={
                "Abakus": ["Cinnamon Chasers"],
                "tripleS": [
                    "EVOLution",
                    "LOVElution",
                    "+(KR)ystal Eyes",
                    "Acid Angel From Asia",
                    "Acid Eyes",
                ],
            },
            artist_aliases_parents_map={
                "Cinnamon Chasers": ["Abakus"],
                "EVOLution": ["tripleS"],
                "LOVElution": ["tripleS"],
                "+(KR)ystal Eyes": ["tripleS"],
                "Acid Angel From Asia": ["tripleS"],
                "Acid Eyes": ["tripleS"],
            },
            cover_art_stems=["aa", "bb"],
            valid_art_exts=["tiff"],
            write_parent_genres=True,
            max_filename_bytes=255,
            rename_source_files=True,
            path_templates=PathTemplateConfig(
                source=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                releases=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                releases_new=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                releases_added_on=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                releases_released_on=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                artists=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                genres=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                descriptors=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                labels=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                loose_tracks=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                collages=PathTemplateTriad(
                    release=PathTemplate("{{ title }}"),
                    track=PathTemplate("{{ title }}"),
                    all_tracks=PathTemplate("{{ title }}"),
                ),
                playlists=PathTemplate("{{ title }}"),
            ),
            ignore_release_directories=["dummy boy"],
            stored_metadata_rules=[
                Rule(
                    matcher=Matcher(["tracktitle"], Pattern("lala")),
                    actions=[
                        Action(
                            behavior=ReplaceAction(replacement="hihi"),
                            tags=["tracktitle"],
                            pattern=Pattern("lala"),
                        )
                    ],
                    ignore=[],
                ),
                Rule(
                    matcher=Matcher(["trackartist[main]"], Pattern("haha")),
                    actions=[
                        Action(
                            behavior=ReplaceAction(replacement="bibi"),
                            tags=["trackartist[main]"],
                            pattern=Pattern("haha"),
                        ),
                        Action(
                            behavior=SplitAction(delimiter=" "),
                            tags=["trackartist[main]"],
                            pattern=Pattern("haha"),
                        ),
                    ],
                    ignore=[Matcher(["releasetitle"], Pattern("blabla"))],
                ),
            ],
            vfs=VirtualFSConfig(
                mount_dir=Path.home() / "music",
                artists_whitelist=None,
                genres_whitelist=None,
                descriptors_whitelist=None,
                labels_whitelist=None,
                hide_genres_with_only_new_releases=True,
                hide_descriptors_with_only_new_releases=True,
                hide_labels_with_only_new_releases=True,
                artists_blacklist=["www"],
                genres_blacklist=["xxx"],
                descriptors_blacklist=["yyy"],
                labels_blacklist=["zzz"],
            ),
        )


def test_config_whitelist() -> None:
    """Since whitelist and blacklist are mutually exclusive, we can't test them in the same test."""
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        with path.open("w") as fp:
            fp.write(
                """
                music_source_dir = "~/.music-src"
                vfs.mount_dir = "~/music"
                vfs.artists_whitelist = [ "www" ]
                vfs.genres_whitelist = [ "xxx" ]
                vfs.descriptors_whitelist = [ "yyy" ]
                vfs.labels_whitelist = [ "zzz" ]
                """
            )

        c = Config.parse(config_path_override=path)
        assert c.vfs.artists_whitelist == ["www"]
        assert c.vfs.genres_whitelist == ["xxx"]
        assert c.vfs.descriptors_whitelist == ["yyy"]
        assert c.vfs.labels_whitelist == ["zzz"]
        assert c.vfs.artists_blacklist is None
        assert c.vfs.genres_blacklist is None
        assert c.vfs.descriptors_blacklist is None
        assert c.vfs.labels_blacklist is None


def test_config_not_found() -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        with pytest.raises(ConfigNotFoundError):
            Config.parse(config_path_override=path)


def test_config_missing_key_validation() -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        path.touch()

        def append(x: str) -> None:
            with path.open("a") as fp:
                fp.write("\n" + x)

        append('music_source_dir = "/"')
        with pytest.raises(MissingConfigKeyError) as excinfo:
            Config.parse(config_path_override=path)
        assert str(excinfo.value) == f"Missing key vfs.mount_dir in configuration file ({path})"


def test_config_value_validation() -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        path.touch()

        def write(x: str) -> None:
            with path.open("w") as fp:
                fp.write(x)

        config = ""

        # music_source_dir
        write("music_source_dir = 123")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value) == f"Invalid value for music_source_dir in configuration file ({path}): must be a path"
        )
        config += '\nmusic_source_dir = "~/.music-src"'

        # cache_dir
        write(config + "\ncache_dir = 123")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert str(excinfo.value) == f"Invalid value for cache_dir in configuration file ({path}): must be a path"
        config += '\ncache_dir = "~/.cache/rose"'

        # max_proc
        write(config + '\nmax_proc = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for max_proc in configuration file ({path}): must be a positive integer"
        )
        config += "\nmax_proc = 8"

        # artist_aliases
        write(config + '\nartist_aliases = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"
        )
        write(config + '\nartist_aliases = ["lalala"]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"
        )
        write(config + '\nartist_aliases = [["lalala"]]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"
        )
        write(config + '\nartist_aliases = [{artist="lalala", aliases="lalala"}]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"
        )
        write(config + '\nartist_aliases = [{artist="lalala", aliases=[123]}]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for artist_aliases in configuration file ({path}): must be a list of {{ artist = str, aliases = list[str] }} records"
        )
        config += '\nartist_aliases = [{artist="tripleS", aliases=["EVOLution"]}]'

        # cover_art_stems
        write(config + '\ncover_art_stems = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for cover_art_stems in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\ncover_art_stems = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for cover_art_stems in configuration file ({path}): Each cover art stem must be of type str: got <class 'int'>"
        )
        config += '\ncover_art_stems = [ "cover" ]'

        # valid_art_exts
        write(config + '\nvalid_art_exts = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for valid_art_exts in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\nvalid_art_exts = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for valid_art_exts in configuration file ({path}): Each art extension must be of type str: got <class 'int'>"
        )
        config += '\nvalid_art_exts = [ "jpg" ]'

        # write_parent_genres
        write(config + '\nwrite_parent_genres = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for write_parent_genres in configuration file ({path}): Must be a bool: got <class 'str'>"
        )

        # max_filename_bytes
        write(config + '\nmax_filename_bytes = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for max_filename_bytes in configuration file ({path}): Must be an int: got <class 'str'>"
        )
        config += "\nmax_filename_bytes = 240"

        # ignore_release_directories
        write(config + '\nignore_release_directories = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for ignore_release_directories in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\nignore_release_directories = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for ignore_release_directories in configuration file ({path}): Each release directory must be of type str: got <class 'int'>"
        )
        config += '\nignore_release_directories = [ ".stversions" ]'

        # stored_metadata_rules
        write(config + '\nstored_metadata_rules = ["lalala"]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value in stored_metadata_rules in configuration file ({path}): list values must be a dict: got <class 'str'>"
        )
        write(config + '\nstored_metadata_rules = [{ matcher = "tracktitle:hi", actions = ["delete:hi"] }]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            click.unstyle(str(excinfo.value))
            == f"""\
Failed to parse stored_metadata_rules in configuration file ({path}): rule {{'matcher': 'tracktitle:hi', 'actions': ['delete:hi']}}: Failed to parse action 1, invalid syntax:

    delete:hi
           ^
           Found another section after the action kind, but the delete action has no parameters. Please remove this section.
"""
        )
        write(
            config
            + '\nstored_metadata_rules = [{ matcher = "tracktitle:hi", actions = ["delete"], ignore = ["tracktitle:bye:"] }]'
        )
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            click.unstyle(str(excinfo.value))
            == f"""\
Failed to parse stored_metadata_rules in configuration file ({path}): rule {{'matcher': 'tracktitle:hi', 'actions': ['delete'], 'ignore': ['tracktitle:bye:']}}: Failed to parse ignore, invalid syntax:

    tracktitle:bye:
                   ^
                   No flags specified: Please remove this section (by deleting the colon) or specify one of the supported flags: `i` (case insensitive).
"""
        )

        # rename_source_files
        write(config + '\nrename_source_files = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for rename_source_files in configuration file ({path}): Must be a bool: got <class 'str'>"
        )


def test_vfs_config_value_validation() -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "config.toml"
        path.touch()

        def write(x: str) -> None:
            with path.open("w") as fp:
                fp.write(x)

        config = 'music_source_dir = "~/.music-src"\n[vfs]\n'
        write(config)

        # mount_dir
        write(config + "\nmount_dir = 123")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert str(excinfo.value) == f"Invalid value for vfs.mount_dir in configuration file ({path}): must be a path"
        config += '\nmount_dir = "~/music"'

        # artists_whitelist
        write(config + '\nartists_whitelist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.artists_whitelist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\nartists_whitelist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.artists_whitelist in configuration file ({path}): Each artist must be of type str: got <class 'int'>"
        )

        # genres_whitelist
        write(config + '\ngenres_whitelist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.genres_whitelist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\ngenres_whitelist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.genres_whitelist in configuration file ({path}): Each genre must be of type str: got <class 'int'>"
        )

        # labels_whitelist
        write(config + '\nlabels_whitelist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.labels_whitelist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\nlabels_whitelist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.labels_whitelist in configuration file ({path}): Each label must be of type str: got <class 'int'>"
        )

        # artists_blacklist
        write(config + '\nartists_blacklist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.artists_blacklist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\nartists_blacklist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.artists_blacklist in configuration file ({path}): Each artist must be of type str: got <class 'int'>"
        )

        # genres_blacklist
        write(config + '\ngenres_blacklist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.genres_blacklist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\ngenres_blacklist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.genres_blacklist in configuration file ({path}): Each genre must be of type str: got <class 'int'>"
        )

        # descriptors_blacklist
        write(config + '\ndescriptors_blacklist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.descriptors_blacklist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\ndescriptors_blacklist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.descriptors_blacklist in configuration file ({path}): Each descriptor must be of type str: got <class 'int'>"
        )

        # labels_blacklist
        write(config + '\nlabels_blacklist = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.labels_blacklist in configuration file ({path}): Must be a list[str]: got <class 'str'>"
        )
        write(config + "\nlabels_blacklist = [123]")
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.labels_blacklist in configuration file ({path}): Each label must be of type str: got <class 'int'>"
        )

        # artists_whitelist + artists_blacklist
        write(config + '\nartists_whitelist = ["a"]\nartists_blacklist = ["b"]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Cannot specify both vfs.artists_whitelist and vfs.artists_blacklist in configuration file ({path}): must specify only one or the other"
        )

        # genres_whitelist + genres_blacklist
        write(config + '\ngenres_whitelist = ["a"]\ngenres_blacklist = ["b"]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Cannot specify both vfs.genres_whitelist and vfs.genres_blacklist in configuration file ({path}): must specify only one or the other"
        )

        # labels_whitelist + labels_blacklist
        write(config + '\nlabels_whitelist = ["a"]\nlabels_blacklist = ["b"]')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Cannot specify both vfs.labels_whitelist and vfs.labels_blacklist in configuration file ({path}): must specify only one or the other"
        )

        # hide_genres_with_only_new_releases
        write(config + '\nhide_genres_with_only_new_releases = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.hide_genres_with_only_new_releases in configuration file ({path}): Must be a bool: got <class 'str'>"
        )

        # hide_descriptors_with_only_new_releases
        write(config + '\nhide_descriptors_with_only_new_releases = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.hide_descriptors_with_only_new_releases in configuration file ({path}): Must be a bool: got <class 'str'>"
        )

        # hide_labels_with_only_new_releases
        write(config + '\nhide_labels_with_only_new_releases = "lalala"')
        with pytest.raises(InvalidConfigValueError) as excinfo:
            Config.parse(config_path_override=path)
        assert (
            str(excinfo.value)
            == f"Invalid value for vfs.hide_labels_with_only_new_releases in configuration file ({path}): Must be a bool: got <class 'str'>"
        )
