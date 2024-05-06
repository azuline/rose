"""
The virtualfs module renders a virtual filesystem from the read cache. It is written in an
Object-Oriented style, against my typical sensibilities, because that's how the FUSE libraries tend
to be implemented. But it's OK :)

Since this is a pretty hefty module, we'll cover the organization. This module contains 9 classes:

1. TTLCache: A wrapper around dict that expires key/value pairs after a given TTL.

2. VirtualPath: A semantic representation of a path in the virtual filesystem along with a parser.
   All virtual filesystem paths are parsed by this class into a far more ergonomic dataclass.

3. VirtualNameGenerator: A class that generates virtual directory and filenames given releases and
   tracks, and maintains inverse mappings for resolving release IDs from virtual paths.

4. Sanitizer: A class that sanitizes artist/genre/label names and maintains a mapping of
   sanitized->unsanitized for core library calls.

5. "CanShow"er: An abstraction that encapsulates the logic of whether an artist, genre, or label
   should be shown in their respective virtual views, based on the whitelist/blacklist configuration
   parameters.

6. FileHandleGenerator: A class that keeps generates new file handles. It is a counter that wraps
   back to 5 when the file handles exceed ~10k, as to avoid any overflows.

7. RoseLogicalCore: A logical representation of Rose's filesystem logic, freed from the annoying
   implementation details that a low-level library like `llfuse` comes with.

8. INodeMapper: A class that tracks the INode <-> Path mappings. It is used to convert inodes to
   paths in VirtualFS.

9. VirtualFS: The main Virtual Filesystem class, which manages the annoying implementation details
   of a low-level virtual filesystem, and delegates logic to the above classes. It uses INodeMapper
   and VirtualPath to translate inodes into semantically useful dataclasses, and then passes them
   into RoseLogicalCore.
"""

from __future__ import annotations

import collections
import contextlib
import errno
import logging
import os
import random
import re
import stat
import subprocess
import tempfile
import time
from collections.abc import Iterator
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Generic, Literal, TypeVar

import llfuse

from rose import (
    SUPPORTED_AUDIO_EXTENSIONS,
    AudioTags,
    Config,
    PathContext,
    PathTemplate,
    Release,
    RoseError,
    Track,
    add_release_to_collage,
    add_track_to_playlist,
    artist_exists,
    create_collage,
    create_playlist,
    delete_collage,
    delete_playlist,
    delete_playlist_cover_art,
    delete_release,
    descriptor_exists,
    evaluate_release_template,
    evaluate_track_template,
    genre_exists,
    get_collage,
    get_collage_releases,
    get_playlist,
    get_playlist_tracks,
    get_release,
    get_track,
    get_tracks_of_release,
    label_exists,
    list_artists,
    list_collages,
    list_descriptors,
    list_genres,
    list_labels,
    list_playlists,
    make_release_logtext,
    make_track_logtext,
    release_within_collage,
    remove_release_from_collage,
    remove_track_from_playlist,
    rename_collage,
    rename_playlist,
    sanitize_dirname,
    sanitize_filename,
    set_playlist_cover_art,
    set_release_cover_art,
    track_within_playlist,
    track_within_release,
    update_cache_for_releases,
)
from rose.cache import get_tracks_of_releases, list_releases, list_tracks
from rose.releases import find_releases_matching_rule
from rose.rule_parser import Matcher, Pattern
from rose.tracks import find_tracks_matching_rule

logger = logging.getLogger(__name__)

STORED_DATA_FILE_REGEX = re.compile(r"\.rose\.([^.]+)\.toml")

K = TypeVar("K")
V = TypeVar("V")
T = TypeVar("T")


class TTLCache(Generic[K, V]):
    """
    TTLCache is a dictionary with a time-to-live (TTL) for each key/value pair. After the TTL
    passes, the key/value pair is no longer accessible.

    We do not currently free entries in this cache, because we expect little churn to occur in
    entries in normal operation. We do not have a great time to clear the cache that does not affect
    performance. We will probably implement freeing entries later when we give more of a shit or
    someone complains about the memory usage. I happen to have a lot of free RAM!
    """

    def __init__(self, ttl_seconds: int = 5):
        self.ttl_seconds = ttl_seconds
        self.__backing: dict[K, tuple[V, float]] = {}

    def __contains__(self, key: K) -> bool:
        try:
            _, insert_time = self.__backing[key]
        except KeyError:
            return False
        return time.time() - insert_time <= self.ttl_seconds

    def __getitem__(self, key: K) -> V:
        v, insert_time = self.__backing[key]
        if time.time() - insert_time > self.ttl_seconds:
            raise KeyError(key)
        self.__backing[key] = (v, time.time())
        return v

    def __setitem__(self, key: K, value: V) -> None:
        self.__backing[key] = (value, time.time())

    def __delitem__(self, key: K) -> None:
        del self.__backing[key]

    def get(self, key: K, default: T) -> V | T:
        try:
            return self[key]
        except KeyError:
            return default


ALL_TRACKS = "!All Tracks"


@dataclass(frozen=True, slots=True)
class VirtualPath:
    view: (
        Literal[
            "Root",
            "Releases",
            "Artists",
            "Genres",
            "Descriptors",
            "Labels",
            "Collages",
            "Playlists",
            "New",
            "Added On",
            "Released On",
        ]
        | None
    )
    artist: str | None = None
    genre: str | None = None
    descriptor: str | None = None
    label: str | None = None
    collage: str | None = None
    playlist: str | None = None
    # Release may be set to `ALL_TRACKS`, in which case it is never attempted to be resolved to a
    # release. Instead, it is treated as a special directory. There may be name conflicts; I don't
    # care.
    release: str | None = None
    file: str | None = None

    @property
    def release_parent(self) -> VirtualPath:
        """Parent path of a release: Used as an input to the VirtualNameGenerator."""
        return VirtualPath(
            view=self.view,
            artist=self.artist,
            genre=self.genre,
            descriptor=self.descriptor,
            label=self.label,
            collage=self.collage,
        )

    @property
    def track_parent(self) -> VirtualPath:
        """Parent path of a track: Used as an input to the VirtualNameGenerator."""
        return VirtualPath(
            view=self.view,
            artist=self.artist,
            genre=self.genre,
            descriptor=self.descriptor,
            label=self.label,
            collage=self.collage,
            playlist=self.playlist,
            release=self.release,
        )

    @property
    def artist_parent(self) -> VirtualPath:
        """Parent path of an artist: Used as an input to the Sanitizer."""
        return VirtualPath(view=self.view)

    @property
    def genre_parent(self) -> VirtualPath:
        """Parent path of a genre: Used as an input to the Sanitizer."""
        return VirtualPath(view=self.view)

    @property
    def descriptor_parent(self) -> VirtualPath:
        """Parent path of a descriptor: Used as an input to the Sanitizer."""
        return VirtualPath(view=self.view)

    @property
    def label_parent(self) -> VirtualPath:
        """Parent path of a label: Used as an input to the Sanitizer."""
        return VirtualPath(view=self.view)

    @property
    def collage_parent(self) -> VirtualPath:
        """Parent path of a collage: Used as an input to the Sanitizer."""
        return VirtualPath(view=self.view)

    @property
    def playlist_parent(self) -> VirtualPath:
        """Parent path of a playlist: Used as an input to the Sanitizer."""
        return VirtualPath(view=self.view)

    @classmethod
    def parse(cls, path: Path) -> VirtualPath:
        parts = str(path.resolve()).split("/")[1:]  # First part is always empty string.

        if len(parts) == 1 and parts[0] == "":
            return VirtualPath(view="Root")

        # Let's abort early if we recognize a path that we _know_ is not valid. This is because
        # invalid file accesses trigger a recalculation of virtual file paths, which we decided to
        # do under the assumption that invalid file accesses would be _rare_. That's not true if we
        # keep getting requests for these stupid paths from shell plugins.
        if parts[-1] in [".git", ".DS_Store", ".Trash", ".Trash-1000", "HEAD", ".envrc"]:
            logger.debug(
                f"Raising ENOENT early in the VirtualPath parser because last path part {parts[-1]} in blacklist."
            )
            raise llfuse.FUSEError(errno.ENOENT)

        if parts[0] == "1. Releases":
            if len(parts) == 1:
                return VirtualPath(view="Releases")
            if len(parts) == 2:
                return VirtualPath(view="Releases", release=parts[1])
            if len(parts) == 3:
                return VirtualPath(view="Releases", release=parts[1], file=parts[2])
            raise llfuse.FUSEError(errno.ENOENT)

        if parts[0] == "1. Releases - New":
            if len(parts) == 1:
                return VirtualPath(view="New")
            if len(parts) == 2:
                return VirtualPath(view="New", release=parts[1])
            if len(parts) == 3:
                return VirtualPath(view="New", release=parts[1], file=parts[2])
            raise llfuse.FUSEError(errno.ENOENT)

        if parts[0] == "1. Releases - Added On":
            if len(parts) == 1:
                return VirtualPath(view="Added On")
            if len(parts) == 2:
                return VirtualPath(view="Added On", release=parts[1])
            if len(parts) == 3:
                return VirtualPath(view="Added On", release=parts[1], file=parts[2])
            raise llfuse.FUSEError(errno.ENOENT)

        if parts[0] == "1. Releases - Released On":
            if len(parts) == 1:
                return VirtualPath(view="Released On")
            if len(parts) == 2:
                return VirtualPath(view="Released On", release=parts[1])
            if len(parts) == 3:
                return VirtualPath(view="Released On", release=parts[1], file=parts[2])
            raise llfuse.FUSEError(errno.ENOENT)

        if parts[0] == "2. Artists":
            if len(parts) == 1:
                return VirtualPath(view="Artists")
            if len(parts) == 2:
                return VirtualPath(view="Artists", artist=parts[1])
            if len(parts) == 3:
                return VirtualPath(view="Artists", artist=parts[1], release=parts[2])
            if len(parts) == 4:
                return VirtualPath(view="Artists", artist=parts[1], release=parts[2], file=parts[3])
            raise llfuse.FUSEError(errno.ENOENT)

        if parts[0] == "3. Genres":
            if len(parts) == 1:
                return VirtualPath(view="Genres")
            if len(parts) == 2:
                return VirtualPath(view="Genres", genre=parts[1])
            if len(parts) == 3:
                return VirtualPath(view="Genres", genre=parts[1], release=parts[2])
            if len(parts) == 4:
                return VirtualPath(view="Genres", genre=parts[1], release=parts[2], file=parts[3])
            raise llfuse.FUSEError(errno.ENOENT)

        if parts[0] == "4. Descriptors":
            if len(parts) == 1:
                return VirtualPath(view="Descriptors")
            if len(parts) == 2:
                return VirtualPath(view="Descriptors", descriptor=parts[1])
            if len(parts) == 3:
                return VirtualPath(view="Descriptors", descriptor=parts[1], release=parts[2])
            if len(parts) == 4:
                return VirtualPath(
                    view="Descriptors", descriptor=parts[1], release=parts[2], file=parts[3]
                )
            raise llfuse.FUSEError(errno.ENOENT)

        if parts[0] == "5. Labels":
            if len(parts) == 1:
                return VirtualPath(view="Labels")
            if len(parts) == 2:
                return VirtualPath(view="Labels", label=parts[1])
            if len(parts) == 3:
                return VirtualPath(view="Labels", label=parts[1], release=parts[2])
            if len(parts) == 4:
                return VirtualPath(view="Labels", label=parts[1], release=parts[2], file=parts[3])
            raise llfuse.FUSEError(errno.ENOENT)

        if parts[0] == "6. Collages":
            if len(parts) == 1:
                return VirtualPath(view="Collages")
            if len(parts) == 2:
                return VirtualPath(view="Collages", collage=parts[1])
            if len(parts) == 3:
                return VirtualPath(view="Collages", collage=parts[1], release=parts[2])
            if len(parts) == 4:
                return VirtualPath(
                    view="Collages", collage=parts[1], release=parts[2], file=parts[3]
                )
            raise llfuse.FUSEError(errno.ENOENT)

        if parts[0] == "7. Playlists":
            if len(parts) == 1:
                return VirtualPath(view="Playlists")
            if len(parts) == 2:
                return VirtualPath(view="Playlists", playlist=parts[1])
            if len(parts) == 3:
                return VirtualPath(
                    view="Playlists",
                    playlist=parts[1],
                    file=parts[2],
                )
            raise llfuse.FUSEError(errno.ENOENT)

        raise llfuse.FUSEError(errno.ENOENT)


class VirtualNameGenerator:
    """
    Generates virtual dirnames and filenames for releases and tracks, and maintains an inverse
    mapping for looking up release/track UUIDs from their virtual paths.

    This object's data has the following lifecycle:

    1. RoseLogicalCore calls `list_virtual_x_paths` to generate all paths in a directory.
    2. Once generated, path->ID can be looked up.

    This means that RoseLogicalCore is responsible for invoking `list_virtual_x_paths` upon cache
    misses / missing file accesses. We end up invoking `list_virtual_x_path` whenever a non-existent
    path is getattr'ed, which is somewhat excessive, however, we can decouple the virtual templates
    from the cache this way, and the lookup _miss_ case should be rather rare in normal operations

    The VirtualNameGenerator also remembers all previous path mappings for 2 hours since last use.
    This allows Rose to continue to serving accesses to old paths, even after the file metadata
    changed. This is useful, for example, if a directory or file is renamed (due to a metadata
    change) while its tracks are in a mpv playlist. mpv's requests to the old paths will still
    resolve, but the old paths will not show up in a readdir call. If old paths collide with new
    paths, new paths will take precedence.
    """

    def __init__(self, config: Config, sanitizer: Sanitizer):
        # fmt: off
        self._config = config
        self._sanitizer = sanitizer
        # These are the stateful maps that we use to remember path mappings. They are maps from the
        # (parent_path, virtual path) -> entity ID.
        #
        # Entries expire after 2 hours, which implements the "serve accesses to previous paths"
        # behavior as specified in the class docstring.
        self._release_store: TTLCache[tuple[VirtualPath, str], str] = TTLCache(ttl_seconds=60 * 60 * 2)
        self._track_store: TTLCache[tuple[VirtualPath, str], str] = TTLCache(ttl_seconds=60 * 60 * 2)
        # Cache template evaluations because they're expensive.
        self._release_template_eval_cache: dict[tuple[VirtualPath, PathTemplate, str, str | None], str] = {}
        self._track_template_eval_cache: dict[tuple[VirtualPath, PathTemplate, str, str | None], str] = {}
        # fmt: on

    def list_release_paths(
        self,
        release_parent: VirtualPath,
        releases: list[Release],
    ) -> Iterator[tuple[Release, str]]:
        """
        Given a parent directory and a list of releases, calculates the virtual directory names
        for those releases, and returns a zipped iterator of the releases and their virtual
        directory names.
        """
        # For collision number generation.
        seen: set[str] = {ALL_TRACKS}
        prefix_pad_size = len(str(len(releases)))
        for idx, release in enumerate(releases):
            # Determine the proper template.
            template = None
            if release_parent.view == "Releases":
                template = self._config.path_templates.releases.release
            elif release_parent.view == "New":
                template = self._config.path_templates.releases_new.release
            elif release_parent.view == "Added On":
                template = self._config.path_templates.releases_added_on.release
            elif release_parent.view == "Released On":
                template = self._config.path_templates.releases_released_on.release
            elif release_parent.view == "Artists":
                template = self._config.path_templates.artists.release
            elif release_parent.view == "Genres":
                template = self._config.path_templates.genres.release
            elif release_parent.view == "Descriptors":
                template = self._config.path_templates.descriptors.release
            elif release_parent.view == "Labels":
                template = self._config.path_templates.labels.release
            elif release_parent.view == "Collages":
                template = self._config.path_templates.collages.release
            else:
                raise RoseError(f"VNAMES: No release template found for {release_parent=}.")

            logtext = make_release_logtext(
                title=release.releasetitle,
                releasedate=release.releasedate,
                artists=release.releaseartists,
            )

            # Generate a position if we're in a collage.
            position = None
            if release_parent.collage:
                position = f"{str(idx+1).zfill(prefix_pad_size)}"

            # Generate the virtual name.
            time_start = time.time()
            cachekey = (release_parent, template, release.metahash, position)
            try:
                vname = self._release_template_eval_cache[cachekey]
                logger.debug(
                    f"VNAMES: Reused cached virtual dirname {vname} for release {logtext} in {time.time()-time_start} seconds"
                )
            except KeyError:
                context = PathContext(
                    genre=self._sanitizer.unsanitize(
                        release_parent.genre,
                        release_parent.genre_parent,
                    )
                    if release_parent.genre
                    else None,
                    descriptor=self._sanitizer.unsanitize(
                        release_parent.descriptor,
                        release_parent.descriptor_parent,
                    )
                    if release_parent.descriptor
                    else None,
                    label=self._sanitizer.unsanitize(
                        release_parent.label,
                        release_parent.label_parent,
                    )
                    if release_parent.label
                    else None,
                    artist=self._sanitizer.unsanitize(
                        release_parent.artist,
                        release_parent.artist_parent,
                    )
                    if release_parent.artist
                    else None,
                    collage=release_parent.collage,
                    playlist=None,
                )
                vname = evaluate_release_template(template, release, context, position)
                vname = sanitize_dirname(self._config, vname, False)
                self._release_template_eval_cache[cachekey] = vname
                logger.debug(
                    f"VNAMES: Generated virtual dirname {vname} for release {logtext} in {time.time()-time_start} seconds"
                )

            # Handle name collisions by appending a unique discriminator to the end.
            original_vname = vname
            collision_no = 2
            while True:
                if vname not in seen:
                    break
                vname = f"{original_vname} [{collision_no}]"
                collision_no += 1
                logger.debug(f"VNAMES: Added collision number to virtual dirname {vname}")

            # Store the generated release name in the cache.
            time_start = time.time()
            self._release_store[(release_parent, vname)] = release.id
            seen.add(vname)
            logger.debug(
                f"VNAMES: Time cost of storing the virtual dirname: {time.time()-time_start=} seconds"
            )

            yield release, vname

    def list_track_paths(
        self,
        track_parent: VirtualPath,
        tracks: list[Track],
    ) -> Iterator[tuple[Track, str]]:
        """
        Given a parent directory and a list of tracks, calculates the virtual filenames for those
        tracks, and returns a zipped iterator of the tracks and their virtual filenames.
        """
        # For collision number generation.
        seen: set[str] = set()
        prefix_pad_size = len(str(len(tracks)))
        for idx, track in enumerate(tracks):
            # Determine the proper template.
            template = None
            if track_parent.release == ALL_TRACKS:
                if track_parent.view == "Releases":
                    template = self._config.path_templates.releases.all_tracks
                elif track_parent.view == "New":
                    template = self._config.path_templates.releases_new.all_tracks
                elif track_parent.view == "Added On":
                    template = self._config.path_templates.releases_added_on.all_tracks
                elif track_parent.view == "Released On":
                    template = self._config.path_templates.releases_released_on.all_tracks
                elif track_parent.view == "Artists":
                    template = self._config.path_templates.artists.all_tracks
                elif track_parent.view == "Genres":
                    template = self._config.path_templates.genres.all_tracks
                elif track_parent.view == "Descriptors":
                    template = self._config.path_templates.descriptors.all_tracks
                elif track_parent.view == "Labels":
                    template = self._config.path_templates.labels.all_tracks
                elif track_parent.view == "Collages":
                    template = self._config.path_templates.collages.all_tracks
            else:
                if track_parent.view == "Releases":
                    template = self._config.path_templates.releases.track
                elif track_parent.view == "New":
                    template = self._config.path_templates.releases_new.track
                elif track_parent.view == "Added On":
                    template = self._config.path_templates.releases_added_on.track
                elif track_parent.view == "Released On":
                    template = self._config.path_templates.releases_released_on.track
                elif track_parent.view == "Artists":
                    template = self._config.path_templates.artists.track
                elif track_parent.view == "Genres":
                    template = self._config.path_templates.genres.track
                elif track_parent.view == "Descriptors":
                    template = self._config.path_templates.descriptors.track
                elif track_parent.view == "Labels":
                    template = self._config.path_templates.labels.track
                elif track_parent.view == "Collages":
                    template = self._config.path_templates.collages.track
                elif track_parent.view == "Playlists":
                    template = self._config.path_templates.playlists
            if not template:
                raise RoseError(f"VNAMES: No track template found for {track_parent=}.")

            logtext = make_track_logtext(
                title=track.tracktitle,
                artists=track.trackartists,
                releasedate=track.release.releasedate,
                suffix=track.source_path.suffix,
            )

            # Generate a position if we're in a playlist.
            position = None
            if track_parent.playlist:
                position = f"{str(idx+1).zfill(prefix_pad_size)}"
            # Generate the virtual filename.
            time_start = time.time()
            cachekey = (track_parent, template, track.metahash, position)
            try:
                vname = self._track_template_eval_cache[cachekey]
            except KeyError:
                context = PathContext(
                    genre=self._sanitizer.unsanitize(
                        track_parent.genre,
                        track_parent.genre_parent,
                    )
                    if track_parent.genre
                    else None,
                    descriptor=self._sanitizer.unsanitize(
                        track_parent.descriptor,
                        track_parent.descriptor_parent,
                    )
                    if track_parent.descriptor
                    else None,
                    label=self._sanitizer.unsanitize(
                        track_parent.label,
                        track_parent.label_parent,
                    )
                    if track_parent.label
                    else None,
                    artist=self._sanitizer.unsanitize(
                        track_parent.artist,
                        track_parent.artist_parent,
                    )
                    if track_parent.artist
                    else None,
                    collage=track_parent.collage,
                    playlist=track_parent.playlist,
                )
                vname = evaluate_track_template(template, track, context, position)
                vname = sanitize_filename(self._config, vname, False)
                logger.debug(
                    f"VNAMES: Generated virtual filename {vname} for track {logtext} in {time.time() - time_start} seconds"
                )
                self._track_template_eval_cache[cachekey] = vname

            # And in case of a name collision, add an extra number at the end. Iterate to find
            # the first unused number.
            original_vname = vname
            collision_no = 2
            while True:
                if vname not in seen:
                    break
                # Write the collision number before the file extension.
                pov = Path(original_vname)
                vname = f"{pov.stem} [{collision_no}]{pov.suffix}"
                collision_no += 1
                logger.debug(f"VNAMES: Added collision number to virtual filepath {vname}")
            seen.add(vname)

            # Store the generated track name in the cache.
            time_start = time.time()
            self._track_store[(track_parent, vname)] = track.id
            seen.add(vname)
            logger.debug(
                f"VNAMES: Time cost of storing the virtual filename: {time.time()-time_start=} seconds"
            )

            yield track, vname

    def lookup_release(self, p: VirtualPath) -> str | None:
        """Given a release path, return the associated release ID."""
        assert p.release is not None
        try:
            # Bumps the expiration time for another 15 minutes.
            r = self._release_store[(p.release_parent, p.release)]
            logger.debug(f"VNAMES: Successfully resolved release virtual name {p} to {r}")
            return r
        except KeyError:
            logger.debug(f"VNAMES: Failed to resolve release virtual name {p}")
            return None

    def lookup_track(self, p: VirtualPath) -> str | None:
        """Given a track path, return the associated track ID."""
        assert p.file is not None
        try:
            # Bumps the expiration time for another 15 minutes.
            r = self._track_store[(p.track_parent, p.file)]
            logger.debug(f"VNAMES: Successfully resolved track virtual name {p} to {r}")
            return r
        except KeyError:
            logger.debug(f"VNAMES: Failed to resolve track virtual name {p}")
            return None


class Sanitizer:
    """
    Sanitizes artist/genre/label names and maintains a mapping of sanitized->unsanitized for core
    library calls.
    """

    def __init__(self, config: Config, rose: RoseLogicalCore) -> None:
        self._config = config
        self._rose = rose
        self._to_sanitized: dict[str, str] = {}
        self._to_unsanitized: dict[str, str] = {}

    def sanitize(self, unsanitized: str) -> str:
        try:
            return self._to_sanitized[unsanitized]
        except KeyError:
            sanitized = sanitize_dirname(self._config, unsanitized, enforce_maxlen=True)
            self._to_sanitized[unsanitized] = sanitized
            self._to_unsanitized[sanitized] = unsanitized
            return sanitized

    def unsanitize(self, sanitized: str, parent: VirtualPath) -> str:
        with contextlib.suppress(KeyError):
            return self._to_unsanitized[sanitized]

        # This should never happen for a valid path.
        logger.debug(
            f"SANITIZER: Failed to find corresponding unsanitized string for '{sanitized}'."
        )
        logger.debug(
            f"SANITIZER: Invoking readdir before retrying unsanitized resolution on {sanitized}"
        )
        # Performant way to consume an iterator completely.
        collections.deque(self._rose.readdir(parent), maxlen=0)
        logger.debug(
            f"SANITIZER: Finished readdir call: retrying file virtual name resolution on {sanitized}"
        )
        try:
            return self._to_unsanitized[sanitized]
        except KeyError as e:
            raise llfuse.FUSEError(errno.ENOENT) from e


class CanShower:
    """
    I'm great at naming things. This is "can show"-er, determining whether we can show an
    artist/genre/label based on the configured whitelists and blacklists.
    """

    def __init__(self, config: Config):
        self._config = config
        self._artist_w = None
        self._artist_b = None
        self._genre_w = None
        self._genre_b = None
        self._descriptor_w = None
        self._descriptor_b = None
        self._label_w = None
        self._label_b = None

        if config.vfs.artists_whitelist:
            self._artist_w = set(config.vfs.artists_whitelist)
        if config.vfs.artists_blacklist:
            self._artist_b = set(config.vfs.artists_blacklist)
        if config.vfs.genres_whitelist:
            self._genre_w = set(config.vfs.genres_whitelist)
        if config.vfs.genres_blacklist:
            self._genre_b = set(config.vfs.genres_blacklist)
        if config.vfs.descriptors_whitelist:
            self._descriptor_w = set(config.vfs.descriptors_whitelist)
        if config.vfs.descriptors_blacklist:
            self._descriptor_b = set(config.vfs.descriptors_blacklist)
        if config.vfs.labels_whitelist:
            self._label_w = set(config.vfs.labels_whitelist)
        if config.vfs.labels_blacklist:
            self._label_b = set(config.vfs.labels_blacklist)

    def artist(self, artist: str) -> bool:
        if self._artist_w:
            return artist in self._artist_w
        elif self._artist_b:
            return artist not in self._artist_b
        return True

    def genre(self, genre: str) -> bool:
        if self._genre_w:
            return genre in self._genre_w
        elif self._genre_b:
            return genre not in self._genre_b
        return True

    def descriptor(self, descriptor: str) -> bool:
        if self._descriptor_w:
            return descriptor in self._descriptor_w
        elif self._descriptor_b:
            return descriptor not in self._descriptor_b
        return True

    def label(self, label: str) -> bool:
        if self._label_w:
            return label in self._label_w
        elif self._label_b:
            return label not in self._label_b
        return True


class FileHandleManager:
    """
    FileDescriptorGenerator generates file descriptors and handles wrapping so that we do not go
    over the int size. Assumes that we do not cycle 10k file descriptors before the first descriptor
    is released.
    """

    def __init__(self) -> None:
        self._state = 10
        # Fake sentinel for file handler. The VirtualFS class implements this file handle as a black
        # hole.
        self.dev_null = 9
        # We translate Rose's Virtual Filesystem file handles to the host machine file handles. This
        # means that every file handle from the host system has a corresponding "wrapper" file
        # handle in Rose, and we only return Rose's file handles from the virtual fs.
        #
        # When we receive a Rose file handle that maps to a host filesystem operation, we "unwrap"
        # the file handle back to the host file handle, and then use it.
        #
        # This prevents any accidental collisions, where Rose generates a file handle that ends up
        # being the same number as a file handle that the host system generates.
        self._rose_to_host_map: dict[int, int] = {}

    def next(self) -> int:
        self._state = max(10, self._state + 1 % 10_000)
        return self._state

    def wrap_host(self, host_fh: int) -> int:
        rose_fh = self.next()
        self._rose_to_host_map[rose_fh] = host_fh
        return rose_fh

    def unwrap_host(self, rose_fh: int) -> int:
        try:
            return self._rose_to_host_map[rose_fh]
        except KeyError as e:
            raise llfuse.FUSEError(errno.EBADF) from e


FileCreationSpecialOp = Literal["add-track-to-playlist", "new-cover-art"]


class RoseLogicalCore:
    def __init__(self, config: Config, fhandler: FileHandleManager):
        self.config = config
        self.fhandler = fhandler
        self.sanitizer = Sanitizer(config, self)
        self.vnames = VirtualNameGenerator(config, self.sanitizer)
        self.can_show = CanShower(config)
        # This map stores the state for "file creation" operations. We currently have two file
        # creation operations:
        #
        # 1. Add Track to Playlist: Because track filenames are not globally unique, the best way to
        #    figure out the track ID is to record the data written, and then parse the written bytes
        #    to find the track ID.
        # 2. New Cover Art: When replacing the cover art of a release or playlist, the new cover art
        #    may have a different "filename" from the virtual `cover.{ext}` filename. We accept any
        #    of the supported filenames as configured by the user. When a new file matching the
        #    cover art filenames is written, it replaces the existing cover art.
        #
        # In order to be able to inspect the written bytes, we must store state across several
        # syscalls (open, write, release). So the process goes:
        #
        # 1. Upon file open, if the syscall matches one of the supported file creation operations,
        #    store the file descriptor in this map instead.
        # 2. On subsequent write requests to the same path and sentinel file descriptor, take the
        #    bytes-to-write and store them in the map.
        # 3. On release, process the written bytes and execute the real operation against the music
        #    library.
        #
        # The state is a mapping of fh -> (operation, identifier, ext, bytes). Identifier is typed
        # based on the operation, and is used to identify the playlist/release being modified.
        self.file_creation_special_ops: dict[
            int, tuple[FileCreationSpecialOp, Any, str, bytearray]
        ] = {}
        # We want to trigger a cache update whenever we notice that a file has been updated through
        # the virtual filesystem. To do this, we insert the file handle and release ID on open, and
        # then trigger the cache update on release. We use this variable to transport that state
        # between the two syscalls.
        self.update_release_on_fh_close: dict[int, str] = {}
        super().__init__()

    @staticmethod
    def stat(mode: Literal["dir", "file"], realpath: Path | None = None) -> dict[str, Any]:
        attrs: dict[str, Any] = {}
        attrs["st_mode"] = (stat.S_IFDIR | 0o755) if mode == "dir" else (stat.S_IFREG | 0o644)
        attrs["st_nlink"] = 4
        attrs["st_uid"] = os.getuid()
        attrs["st_gid"] = os.getgid()

        attrs["st_size"] = 4096
        attrs["st_atime_ns"] = 0.0
        attrs["st_mtime_ns"] = 0.0
        attrs["st_ctime_ns"] = 0.0
        if realpath:
            with contextlib.suppress(FileNotFoundError):
                s = realpath.stat()
                attrs["st_size"] = s.st_size
                attrs["st_atime_ns"] = s.st_atime
                attrs["st_mtime_ns"] = s.st_mtime
                attrs["st_ctime_ns"] = s.st_ctime

        return attrs

    def _get_track_id(self, p: VirtualPath) -> str:
        """Common logic that gets called for each track."""
        track_id = self.vnames.lookup_track(p)
        if not track_id:
            logger.debug(
                f"LOGICAL: Invoking readdir before retrying file virtual name resolution on {p}"
            )
            # Performant way to consume an iterator completely.
            collections.deque(self.readdir(p.track_parent), maxlen=0)
            logger.debug(
                f"LOGICAL: Finished readdir call: retrying file virtual name resolution on {p}"
            )
            track_id = self.vnames.lookup_track(p)
            if not track_id:
                raise llfuse.FUSEError(errno.ENOENT)

        return track_id

    def _getattr_release(self, p: VirtualPath) -> dict[str, Any]:
        """Common logic that gets called for each release."""
        release_id = self.vnames.lookup_release(p)
        if not release_id:
            logger.debug(
                f"LOGICAL: Invoking readdir before retrying release virtual name resolution on {p}"
            )
            # Performant way to consume an iterator completely.
            collections.deque(self.readdir(p.release_parent), maxlen=0)
            logger.debug(
                f"LOGICAL: Finished readdir call: retrying release virtual name resolution on {p}"
            )
            release_id = self.vnames.lookup_release(p)
            if not release_id:
                raise llfuse.FUSEError(errno.ENOENT)

        release = get_release(self.config, release_id)
        # Handle a potential release deletion here.
        if release is None:
            logger.debug("LOGICAL: Resolved release_id does not exist in cache")
            raise llfuse.FUSEError(errno.ENOENT)

        # If no file, return stat for the release dir.
        if not p.file:
            return self.stat("dir", release.source_path)
        # Look for files:
        if release.cover_image_path and p.file == f"cover{release.cover_image_path.suffix}":
            return self.stat("file", release.cover_image_path)
        if p.file == f".rose.{release.id}.toml":
            return self.stat("file")
        track_id = self._get_track_id(p)
        if not track_within_release(self.config, track_id, release.id):
            logger.debug("LOGICAL: Resolved track_id not found in the given release")
            raise llfuse.FUSEError(errno.ENOENT)
        if track := get_track(self.config, track_id):
            return self.stat("file", track.source_path)
        raise RoseError(
            "Impossible: Resolved track_id after track_within_release check does not exist"
        )

    def getattr(self, p: VirtualPath) -> dict[str, Any]:
        logger.debug(f"LOGICAL: Received getattr for {p=}")

        # 7. Playlists
        if p.playlist:
            playlist = get_playlist(self.config, p.playlist)
            if not playlist:
                raise llfuse.FUSEError(errno.ENOENT)
            if p.file:
                if playlist.cover_path and f"cover{playlist.cover_path.suffix}" == p.file:
                    return self.stat("file", playlist.cover_path)
                track_id = self._get_track_id(p)
                if not track_within_playlist(self.config, track_id, p.playlist):
                    raise llfuse.FUSEError(errno.ENOENT)
                if track := get_track(self.config, track_id):
                    return self.stat("file", track.source_path)
                raise RoseError(
                    "Impossible: Resolved track_id after track_within_playlist check does not exist"
                )
            return self.stat("dir")

        # 6. Collages
        if p.collage:
            if not get_collage(self.config, p.collage):
                raise llfuse.FUSEError(errno.ENOENT)
            if p.release == ALL_TRACKS:
                if not p.file:
                    return self.stat("dir")
                if (track := get_track(self.config, self._get_track_id(p))) and (
                    release_within_collage(self.config, track.release.id, p.collage)
                ):
                    return self.stat("file", track.source_path)
                raise llfuse.FUSEError(errno.ENOENT)
            if p.release:
                return self._getattr_release(p)
            return self.stat("dir")

        # 5. Labels
        if p.label:
            la = self.sanitizer.unsanitize(p.label, p.label_parent)
            if not label_exists(self.config, la) or not self.can_show.label(la):
                raise llfuse.FUSEError(errno.ENOENT)
            if p.release == ALL_TRACKS:
                if not p.file:
                    return self.stat("dir")
                if (track := get_track(self.config, self._get_track_id(p))) and (
                    p.label in track.release.labels
                ):
                    return self.stat("file", track.source_path)
                raise llfuse.FUSEError(errno.ENOENT)
            if p.release:
                return self._getattr_release(p)
            return self.stat("dir")

        # 4. Descriptors
        if p.descriptor:
            d = self.sanitizer.unsanitize(p.descriptor, p.descriptor_parent)
            if not descriptor_exists(self.config, d) or not self.can_show.descriptor(d):
                raise llfuse.FUSEError(errno.ENOENT)
            if p.release == ALL_TRACKS:
                if not p.file:
                    return self.stat("dir")
                if (track := get_track(self.config, self._get_track_id(p))) and (
                    p.descriptor in track.release.descriptors
                ):
                    return self.stat("file", track.source_path)
                raise llfuse.FUSEError(errno.ENOENT)
            if p.release:
                return self._getattr_release(p)
            return self.stat("dir")

        # 3. Genres
        if p.genre:
            g = self.sanitizer.unsanitize(p.genre, p.genre_parent)
            if not genre_exists(self.config, g) or not self.can_show.genre(g):
                raise llfuse.FUSEError(errno.ENOENT)
            if p.release == ALL_TRACKS:
                if not p.file:
                    return self.stat("dir")
                if (track := get_track(self.config, self._get_track_id(p))) and (
                    p.genre in track.release.genres
                    or p.genre in track.release.parent_genres
                    or p.genre in track.release.secondary_genres
                    or p.genre in track.release.parent_secondary_genres
                ):
                    return self.stat("file", track.source_path)
                raise llfuse.FUSEError(errno.ENOENT)
            if p.release:
                return self._getattr_release(p)
            return self.stat("dir")

        # 2. Artists
        if p.artist:
            a = self.sanitizer.unsanitize(p.artist, p.artist_parent)
            if not artist_exists(self.config, a) or not self.can_show.artist(a):
                raise llfuse.FUSEError(errno.ENOENT)
            if p.release == ALL_TRACKS:
                if not p.file:
                    return self.stat("dir")
                if (track := get_track(self.config, self._get_track_id(p))) and any(
                    p.artist == a.name
                    for _, artists in track.release.releaseartists.items()
                    for a in artists
                ):
                    return self.stat("file", track.source_path)
                raise llfuse.FUSEError(errno.ENOENT)
            if p.release:
                return self._getattr_release(p)
            return self.stat("dir")

        # 1. Releases
        if p.release:
            if p.release == ALL_TRACKS:
                if not p.file:
                    return self.stat("dir")
                if (track := get_track(self.config, self._get_track_id(p))) and (
                    p.view != "New" or track.release.new
                ):
                    return self.stat("file", track.source_path)
                raise llfuse.FUSEError(errno.ENOENT)
            return self._getattr_release(p)

        # 0. Root
        elif p.view:
            return self.stat("dir")

        # -1. Wtf are you doing here?
        raise llfuse.FUSEError(errno.ENOENT)

    def readdir(self, p: VirtualPath) -> Iterator[tuple[str, dict[str, Any]]]:
        logger.debug(f"LOGICAL: Received readdir for {p=}")

        # Call getattr to validate existence. We can now assume that the provided path exists. This
        # for example includes checks that a given release belongs to the artist/genre/label/collage
        # its nested under.
        logger.debug(f"LOGICAL: Invoking getattr in readdir to validate existence of {p}")
        self.getattr(p)

        yield from [
            (".", self.stat("dir")),
            ("..", self.stat("dir")),
        ]

        if p.view == "Root":
            yield from [
                ("1. Releases", self.stat("dir")),
                ("1. Releases - New", self.stat("dir")),
                ("1. Releases - Added On", self.stat("dir")),
                ("1. Releases - Released On", self.stat("dir")),
                ("2. Artists", self.stat("dir")),
                ("3. Genres", self.stat("dir")),
                ("4. Descriptors", self.stat("dir")),
                ("5. Labels", self.stat("dir")),
                ("6. Collages", self.stat("dir")),
                ("7. Playlists", self.stat("dir")),
            ]
            return

        if p.release == ALL_TRACKS and (
            p.artist
            or p.genre
            or p.descriptor
            or p.label
            or p.view in ["Releases", "Released On", "Added On", "New"]
        ):
            matcher = None
            if p.artist:
                matcher = Matcher(["artist"], Pattern(p.artist, strict=True))
            if p.genre:
                matcher = Matcher(["genre"], Pattern(p.genre, strict=True))
            if p.descriptor:
                matcher = Matcher(["descriptor"], Pattern(p.descriptor, strict=True))
            if p.label:
                matcher = Matcher(["label"], Pattern(p.label, strict=True))
            if p.view == "New":
                matcher = Matcher(["new"], Pattern("true", strict=True))

            tracks = (
                find_tracks_matching_rule(self.config, matcher)
                if matcher
                else list_tracks(self.config)
            )
            for trk, vname in self.vnames.list_track_paths(p, tracks):
                yield vname, self.stat("file", trk.source_path)
            return

        if p.release == ALL_TRACKS and p.collage:
            releases = get_collage_releases(self.config, p.collage)
            for _, tracks in get_tracks_of_releases(self.config, releases):
                for trk, vname in self.vnames.list_track_paths(p, tracks):
                    yield vname, self.stat("file", trk.source_path)
            return

        if p.release:
            if (release_id := self.vnames.lookup_release(p)) and (
                release := get_release(self.config, release_id)
            ):
                tracks = get_tracks_of_release(self.config, release)
                for trk, vname in self.vnames.list_track_paths(p, tracks):
                    yield vname, self.stat("file", trk.source_path)
                if release.cover_image_path:
                    yield release.cover_image_path.name, self.stat("file", release.cover_image_path)
                yield f".rose.{release.id}.toml", self.stat("file")
                return
            raise llfuse.FUSEError(errno.ENOENT)

        if (
            p.artist
            or p.genre
            or p.descriptor
            or p.label
            or p.view in ["Releases", "New", "Added On", "Released On"]
        ):
            matcher = None
            if p.artist:
                matcher = Matcher(["releaseartist"], Pattern(p.artist, strict=True))
            if p.genre:
                matcher = Matcher(["genre"], Pattern(p.genre, strict=True))
            if p.descriptor:
                matcher = Matcher(["descriptor"], Pattern(p.descriptor, strict=True))
            if p.label:
                matcher = Matcher(["label"], Pattern(p.label, strict=True))
            if p.view == "New":
                matcher = Matcher(["new"], Pattern("true", strict=True))

            releases = (
                find_releases_matching_rule(self.config, matcher)
                if matcher
                else list_releases(self.config)
            )

            yield ALL_TRACKS, self.stat("dir")
            for rls, vname in self.vnames.list_release_paths(p, releases):
                yield vname, self.stat("dir", rls.source_path)
            return

        if p.view == "Artists":
            for artist in list_artists(self.config):
                if not self.can_show.artist(artist):
                    continue
                yield self.sanitizer.sanitize(artist), self.stat("dir")
            return

        if p.view == "Genres":
            for e1 in list_genres(self.config):
                if not self.can_show.genre(e1.genre):
                    continue
                if self.config.vfs.hide_genres_with_only_new_releases and e1.only_new_releases:
                    continue
                yield self.sanitizer.sanitize(e1.genre), self.stat("dir")
            return

        if p.view == "Descriptors":
            for e2 in list_descriptors(self.config):
                if not self.can_show.descriptor(e2.descriptor):
                    continue
                if self.config.vfs.hide_descriptors_with_only_new_releases and e2.only_new_releases:
                    continue
                yield self.sanitizer.sanitize(e2.descriptor), self.stat("dir")
            return

        if p.view == "Labels":
            for e3 in list_labels(self.config):
                if not self.can_show.label(e3.label):
                    continue
                if self.config.vfs.hide_labels_with_only_new_releases and e3.only_new_releases:
                    continue
                yield self.sanitizer.sanitize(e3.label), self.stat("dir")
            return

        if p.view == "Collages" and p.collage:
            releases = get_collage_releases(self.config, p.collage)
            for rls, vname in self.vnames.list_release_paths(p, releases):
                yield vname, self.stat("dir", rls.source_path)
            yield ALL_TRACKS, self.stat("dir")
            return

        if p.view == "Collages":
            # Don't need to sanitize because the collage names come from filenames.
            for collage in list_collages(self.config):
                yield collage, self.stat("dir")
            return

        if p.view == "Playlists" and p.playlist:
            playlist = get_playlist(self.config, p.playlist)
            if playlist is None:
                raise llfuse.FUSEError(errno.ENOENT)
            if playlist.cover_path:
                v = f"cover{playlist.cover_path.suffix}"
                yield v, self.stat("file", playlist.cover_path)
            tracks = get_playlist_tracks(self.config, p.playlist)
            for trk, vname in self.vnames.list_track_paths(p, tracks):
                yield vname, self.stat("file", trk.source_path)
            return

        if p.view == "Playlists":
            # Don't need to sanitize because the playlist names come from filenames.
            for pname in list_playlists(self.config):
                yield pname, self.stat("dir")
            return

        raise llfuse.FUSEError(errno.ENOENT)

    def unlink(self, p: VirtualPath) -> None:
        logger.debug(f"LOGICAL: Received unlink for {p=}")

        # Possible actions:
        # 1. Delete a track from a playlist.
        # 2. Delete cover art from a playlist.
        #
        # Note: We do not support deleting cover art from a release via the delete operation. This
        # is because when removing a release from a collage via `rm -r`, `unlink` gets called
        # recurisvely on every file, including each release's cover art. If we supported removing
        # cover art, all cover art would be removed when we removed a release from a collage.
        if (
            p.view == "Playlists"
            and p.playlist
            and p.file
            and p.file.lower() in self.config.valid_cover_arts
            and (playlist := get_playlist(self.config, p.playlist))
        ):
            delete_playlist_cover_art(self.config, playlist.name)
            return
        if (
            p.view == "Playlists"
            and p.playlist
            and p.file
            and get_playlist(self.config, p.playlist) is not None
            and (track_id := self.vnames.lookup_track(p))
        ):
            remove_track_from_playlist(self.config, p.playlist, track_id)
            return

        # Otherwise, noop. If we return an error, that prevents rmdir from being called when we rm.

    def mkdir(self, p: VirtualPath) -> None:
        logger.debug(f"LOGICAL: Received mkdir for {p=}")

        # Possible actions:
        # 1. Create a new collage.
        # 2. Create a new playlist.
        if p.collage and p.release is None:
            create_collage(self.config, p.collage)
            return
        if p.playlist and p.file is None:
            create_playlist(self.config, p.playlist)
            return

        raise llfuse.FUSEError(errno.EACCES)

    def rmdir(self, p: VirtualPath) -> None:
        logger.debug(f"LOGICAL: Received rmdir for {p=}")

        # Possible actions:
        # 1. Delete a collage.
        # 2. Delete a release from an existing collage.
        # 3. Delete a playlist.
        # 4. Delete a release.
        if p.view == "Collages" and p.collage and p.release is None:
            delete_collage(self.config, p.collage)
            return
        if (
            p.view == "Collages"
            and p.collage
            and p.release
            and (release_id := self.vnames.lookup_release(p))
        ):
            remove_release_from_collage(self.config, p.collage, release_id)
            return
        if p.view == "Playlists" and p.playlist and p.file is None:
            delete_playlist(self.config, p.playlist)
            return
        if p.view != "Collages" and p.release and (release_id := self.vnames.lookup_release(p)):
            delete_release(self.config, release_id)
            return

        raise llfuse.FUSEError(errno.EACCES)

    def rename(self, old: VirtualPath, new: VirtualPath) -> None:
        logger.debug(f"LOGICAL: Received rename for {old=} {new=}")

        # Possible actions:
        # 1. Rename a collage.
        # 2. Rename a playlist.
        if (
            old.view == "Collages"
            and new.view == "Collages"
            and (old.collage and new.collage)
            and old.collage != new.collage
            and (not old.release and not new.release)
        ):
            rename_collage(self.config, old.collage, new.collage)
            return
        if (
            old.view == "Playlists"
            and new.view == "Playlists"
            and (old.playlist and new.playlist)
            and old.playlist != new.playlist
            and (not old.file and not new.file)
        ):
            rename_playlist(self.config, old.playlist, new.playlist)
            return

        raise llfuse.FUSEError(errno.EACCES)

    def open(self, p: VirtualPath, flags: int) -> int:
        logger.debug(f"LOGICAL: Received open for {p=} {flags=}")

        err = errno.ENOENT
        if flags & os.O_CREAT == os.O_CREAT:
            err = errno.EACCES

        # Possible actions:
        # 1. Add a release to a collage (by writing the .rose.{uuid}.toml file) (O_CREAT).
        # 2. Read/write existing music files, cover arts, and .rose.{uuid}.toml files.
        # 3. Replace the cover art of a release (O_CREAT).
        # 4. Add a track to a playlist (O_CREAT).
        # 5. Replace the cover art of a playlist (O_CREAT).
        if (
            p.collage
            and p.release
            and p.file
            and flags & os.O_CREAT == os.O_CREAT
            and (m := STORED_DATA_FILE_REGEX.match(p.file))
        ):
            release_id = m[1]
            logger.debug(
                f"LOGICAL: Add release {release_id} to collage {p.collage}, reached goal of collage addition sequence"
            )
            add_release_to_collage(self.config, p.collage, release_id)
            return self.fhandler.dev_null
        if p.release == ALL_TRACKS and p.file:
            if (track_id := self.vnames.lookup_track(p)) and (
                track := get_track(self.config, track_id)
            ):
                fh = self.fhandler.wrap_host(os.open(str(track.source_path), flags))
                if flags & os.O_WRONLY == os.O_WRONLY or flags & os.O_RDWR == os.O_RDWR:
                    self.update_release_on_fh_close[fh] = track.release.id
                return fh
            raise llfuse.FUSEError(err)
        if (
            p.release
            and p.file
            and (release_id := self.vnames.lookup_release(p))
            and (release := get_release(self.config, release_id))
        ):
            # If the file is a music file, handle it as a music file.
            if (track_id := self.vnames.lookup_track(p)) and (
                track := get_track(self.config, track_id)
            ):
                fh = self.fhandler.wrap_host(os.open(str(track.source_path), flags))
                if flags & os.O_WRONLY == os.O_WRONLY or flags & os.O_RDWR == os.O_RDWR:
                    self.update_release_on_fh_close[fh] = release.id
                return fh
            # If the file is the datafile, pass it through.
            if p.file == f".rose.{release.id}.toml":
                return self.fhandler.wrap_host(os.open(str(release.source_path / p.file), flags))
            # If the file matches the current cover image, then simply pass it through.
            if release.cover_image_path and p.file == f"cover{release.cover_image_path.suffix}":
                return self.fhandler.wrap_host(os.open(str(release.cover_image_path), flags))
            # Otherwise, if we are writing a brand new cover image, initiate the "new-cover-art"
            # sequence.
            if p.file.lower() in self.config.valid_cover_arts and flags & os.O_CREAT == os.O_CREAT:
                fh = self.fhandler.next()
                logtext = make_release_logtext(
                    title=release.releasetitle,
                    releasedate=release.releasedate,
                    artists=release.releaseartists,
                )
                logger.debug(
                    f"LOGICAL: Begin new cover art sequence for release "
                    f"{logtext=}, {p.file=}, and {fh=}"
                )
                self.file_creation_special_ops[fh] = (
                    "new-cover-art",
                    ("release", release.id),
                    Path(p.file).suffix,
                    bytearray(),
                )
                return fh
            raise llfuse.FUSEError(err)
        if p.playlist and p.file:
            playlist = get_playlist(self.config, p.playlist)
            if not playlist:
                raise llfuse.FUSEError(errno.ENOENT)
            # If we are trying to create an audio file in the playlist, enter the
            # "add-track-to-playlist" operation sequence. See the __init__ for more details.
            pf = Path(p.file)
            if pf.suffix.lower() in SUPPORTED_AUDIO_EXTENSIONS and flags & os.O_CREAT == os.O_CREAT:
                fh = self.fhandler.next()
                logger.debug(
                    f"LOGICAL: Begin playlist addition operation sequence for "
                    f"{playlist.name=}, {p.file=}, and {fh=}"
                )
                self.file_creation_special_ops[fh] = (
                    "add-track-to-playlist",
                    p.playlist,
                    pf.suffix,
                    bytearray(),
                )
                return fh
            # If we are trying to create a cover image in the playlist, enter the "new-cover-art"
            # sequence for the playlist.
            if p.file.lower() in self.config.valid_cover_arts and flags & os.O_CREAT == os.O_CREAT:
                fh = self.fhandler.next()
                logger.debug(
                    f"LOGICAL: Begin new cover art sequence for playlist"
                    f"{playlist.name=}, {p.file=}, and {fh=}"
                )
                self.file_creation_special_ops[fh] = (
                    "new-cover-art",
                    ("playlist", p.playlist),
                    pf.suffix,
                    bytearray(),
                )
                return fh
            # Otherwise, continue on...
            if (track_id := self.vnames.lookup_track(p)) and (
                track := get_track(self.config, track_id)
            ):
                fh = self.fhandler.wrap_host(os.open(str(track.source_path), flags))
                if flags & os.O_WRONLY == os.O_WRONLY or flags & os.O_RDWR == os.O_RDWR:
                    self.update_release_on_fh_close[fh] = track.release.id
                return fh
            if playlist.cover_path and f"cover{playlist.cover_path.suffix}" == p.file:
                return self.fhandler.wrap_host(os.open(playlist.cover_path, flags))
            raise llfuse.FUSEError(err)

        raise llfuse.FUSEError(err)

    def read(self, fh: int, offset: int, length: int) -> bytes:
        logger.debug(f"LOGICAL: Received read for {fh=} {offset=} {length=}")
        if sop := self.file_creation_special_ops.get(fh, None):
            logger.debug("LOGICAL: Matched read to a file creation special op")
            _, _, _, b = sop
            return b[offset : offset + length]
        fh = self.fhandler.unwrap_host(fh)
        os.lseek(fh, offset, os.SEEK_SET)
        return os.read(fh, length)

    def write(self, fh: int, offset: int, data: bytes) -> int:
        logger.debug(f"LOGICAL: Received write for {fh=} {offset=} {len(data)=}")
        if sop := self.file_creation_special_ops.get(fh, None):
            logger.debug("LOGICAL: Matched write to a file creation special op")
            _, _, _, b = sop
            del b[offset:]
            b.extend(data)
            return len(data)
        fh = self.fhandler.unwrap_host(fh)
        os.lseek(fh, offset, os.SEEK_SET)
        return os.write(fh, data)

    def release(self, fh: int) -> None:
        logger.debug(f"LOGICAL: Received release for {fh=}")
        if sop := self.file_creation_special_ops.get(fh, None):
            logger.debug("LOGICAL: Matched release to a file creation special op")
            operation, ident, ext, b = sop
            if not b:
                logger.debug("LOGICAL: Aborting file creation special oprelease: no bytes to write")
                return
            if operation == "add-track-to-playlist":
                logger.debug("LOGICAL: Narrowed file creation special op to add track to playlist")
                playlist = ident
                with tempfile.TemporaryDirectory() as tmpdir:
                    audiopath = Path(tmpdir) / f"f{ext}"
                    with audiopath.open("wb") as fp:
                        fp.write(b)
                    audiofile = AudioTags.from_file(audiopath)
                    track_id = audiofile.id
                if not track_id:
                    logger.warning(
                        "LOGICAL: Failed to parse track_id from file in playlist addition "
                        f"operation sequence: {track_id=} {fh=} {playlist=} {audiofile}"
                    )
                    return
                add_track_to_playlist(self.config, playlist, track_id)
                del self.file_creation_special_ops[fh]
                return
            if operation == "new-cover-art":
                entity_type, entity_id = ident
                if entity_type == "release":
                    logger.debug(
                        "LOGICAL: Narrowed file creation special op to write release cover art"
                    )
                    with tempfile.TemporaryDirectory() as tmpdir:
                        imagepath = Path(tmpdir) / f"f{ext}"
                        with imagepath.open("wb") as fp:
                            fp.write(b)
                        set_release_cover_art(self.config, entity_id, imagepath)
                    del self.file_creation_special_ops[fh]
                    return
                if entity_type == "playlist":
                    logger.debug(
                        "LOGICAL: Narrowed file creation special op to write playlist cover art"
                    )
                    with tempfile.TemporaryDirectory() as tmpdir:
                        imagepath = Path(tmpdir) / f"f{ext}"
                        with imagepath.open("wb") as fp:
                            fp.write(b)
                        set_playlist_cover_art(self.config, entity_id, imagepath)
                    del self.file_creation_special_ops[fh]
                    return
            raise RoseError(f"Impossible: unknown file creation special op: {operation=} {ident=}")
        if release_id := self.update_release_on_fh_close.get(fh, None):
            logger.debug(
                f"LOGICAL: Triggering cache update for release {release_id} after release syscall"
            )
            if release := get_release(self.config, release_id):
                update_cache_for_releases(self.config, [release.source_path])
        fh = self.fhandler.unwrap_host(fh)
        os.close(fh)


class INodeMapper:
    """
    INodeMapper manages the mapping of inodes to paths in our filesystem. We have this because the
    llfuse library makes us manage the inodes...
    """

    def __init__(self, config: Config):
        self.config = config

        self._inode_to_path_map: dict[int, Path] = {llfuse.ROOT_INODE: Path("/")}
        self._path_to_inode_map: dict[str, int] = {"/": llfuse.ROOT_INODE}
        self._next_inode_ctr: int = llfuse.ROOT_INODE + 1

    def _next_inode(self) -> int:
        # Increment to infinity.
        cur = self._next_inode_ctr
        self._next_inode_ctr += 1
        return cur

    def get_path(self, inode: int, name: bytes | None = None) -> Path:
        """
        Raises ENOENT if the inode doesn't exist. If the inode is of a directory, you can optionally
        pass `name`, which will be concatenated to the directory.
        """
        try:
            path = self._inode_to_path_map[inode]
            if not name or name == b".":
                return path
            if name == b"..":
                return path.parent
            return path / name.decode()
        except KeyError as e:
            raise llfuse.FUSEError(errno.ENOENT) from e
        except UnicodeDecodeError as e:
            raise llfuse.FUSEError(errno.EINVAL) from e

    def calc_inode(self, path: Path) -> int:
        """
        Get the inode of a path. If we've seen the path before, return the cached inode. Otherwise,
        generate a new inode and cache it for future accesses.
        """
        path = path.resolve()
        spath = str(path)
        try:
            return self._path_to_inode_map[spath]
        except KeyError:
            inode = self._next_inode()
            self._path_to_inode_map[spath] = inode
            self._inode_to_path_map[inode] = path
            return inode

    def remove_path(self, path: Path) -> None:
        spath = str(path.resolve())
        try:
            inode = self._path_to_inode_map[spath]
        except KeyError:
            return
        del self._path_to_inode_map[spath]
        del self._inode_to_path_map[inode]

    def rename_path(self, old_path: Path, new_path: Path) -> None:
        sold = str(old_path.resolve())
        snew = str(new_path.resolve())
        try:
            inode = self._path_to_inode_map[sold]
        except KeyError:
            return
        self._inode_to_path_map[inode] = new_path
        self._path_to_inode_map[snew] = inode
        del self._path_to_inode_map[sold]


class VirtualFS(llfuse.Operations):  # type: ignore
    """
    This is the virtual filesystem class, which implements commands by delegating the Rose-specific
    logic to RoseLogicalCore and the inode/fd<->path tracking to INodeMapper. This architecture
    allows us to have a fairly clean logical implementation for Rose despite a fairly low-level
    llfuse library.
    """

    def __init__(self, config: Config):
        self.fhandler = FileHandleManager()
        self.rose = RoseLogicalCore(config, self.fhandler)
        self.inodes = INodeMapper(config)
        self.default_attrs = {
            # Well, this should be ok for now. I really don't want to track this... we indeed change
            # inodes across FS restarts.
            "generation": random.randint(0, 1000000),
            # Have a 30 second entry timeout by default.
            "entry_timeout": 30,
        }
        # We cache some items for getattr and lookup for performance reasons--after a ls, getattr is
        # serially called for each item in the directory, and sequential 1k SQLite reads is quite
        # slow in any universe. So whenever we have a readdir, we do a batch read and populate the
        # getattr and lookup caches. The cache is valid for only 2 seconds, which prevents stale
        # results from being read from it.
        #
        # The dict is a map of paths to entry attributes.
        self.getattr_cache: TTLCache[int, llfuse.EntryAttributes]
        self.lookup_cache: TTLCache[tuple[int, bytes], llfuse.EntryAttributes]
        self.reset_getattr_caches()
        # We handle state for readdir calls here. Because programs invoke readdir multiple times
        # with offsets, we end up with many readdir calls for a single directory. However, we do not
        # want to actually invoke the logical Rose readdir call that many times. So we load it once
        # in `opendir`, associate the results with a file handle, and yield results from that handle
        # in `readdir`. We delete the state in `releasedir`.
        #
        # Map of file handle -> (parent inode, child name, child attributes).
        self.readdir_cache: dict[int, list[tuple[int, bytes, llfuse.EntryAttributes]]] = {}
        # Ghost Files: We pretend some files exist in the filesystem, despite them not actually
        # existing. We do this in order to be compatible with the expectations that tools have for
        # filesystems.
        #
        # For example, when we use file writing to add a file to a playlist, that file is
        # immediately renamed to its correct playlist-specific filename upon release. However, `cp`
        # exits with an error, for it followed up the release with an attempt to set file
        # permissions and attributes on a now non-existent file.
        #
        # In order to pretend to tools that we are a Real Filesystem and not some shitty hack of a
        # filesystem, we have these ghost files that exist for a period of time following an
        # operation.
        # All values in this maps are `true`; we just don't have TTLSet :)
        self.ghost_existing_files: TTLCache[str, bool] = TTLCache(ttl_seconds=5)
        # We support adding releases to collages by "copying" a release directory into the collage
        # directory. What we actually do is:
        #
        # 1. Record the `mkdir`-ed release directory, and pretend that it exists for 5 seconds.
        # 2. Allow arbitrary file opens in that directory. They're all ghost files and therefore
        #    routed to /dev/null.
        # 3. If we receive an O_CREAT open for a `.rose.{uuid}.toml` file, forward that to
        #    RoseLogicalCore so it can handle the release addition to collage.
        self.in_progress_collage_additions: TTLCache[str, bool] = TTLCache(ttl_seconds=5)

    def reset_getattr_caches(self) -> None:
        # When a write happens, clear these caches. These caches are very short-lived and intended
        # to make readdir's subsequent getattrs more performant, so this is harmless.
        self.getattr_cache = TTLCache(ttl_seconds=1)
        self.lookup_cache = TTLCache(ttl_seconds=1)

    def make_entry_attributes(self, attrs: dict[str, Any]) -> llfuse.EntryAttributes:
        for k, v in self.default_attrs.items():
            if k not in attrs:
                attrs[k] = v
        entry = llfuse.EntryAttributes()
        for k, v in attrs.items():
            setattr(entry, k, v)
        return entry

    def getattr(self, inode: int, _: Any) -> llfuse.EntryAttributes:
        logger.debug(f"FUSE: Received getattr for {inode=}")
        # For performance, pull from the getattr cache if possible.
        with contextlib.suppress(KeyError):
            attrs = self.getattr_cache[inode]
            logger.debug(f"FUSE: Resolved getattr for {inode=} to {attrs.__getstate__()=}")
            return attrs
        spath = self.inodes.get_path(inode)
        logger.debug(f"FUSE: Resolved getattr {inode=} to {spath=}")
        # If this path is a ghost file path; pretend here!
        if self.ghost_existing_files.get(str(spath), False):
            logger.debug(f"FUSE: Resolved getattr for {spath=} as ghost existing file")
            attrs = self.rose.stat("file")
            attrs["st_ino"] = inode
            return self.make_entry_attributes(attrs)
        # If this directory is a ghost directory path; pretend here!
        if self.in_progress_collage_additions.get(str(spath), False):
            logger.debug(f"FUSE: Resolved lookup for {spath=} as in progress collage addition")
            attrs = self.rose.stat("dir")
            attrs["st_ino"] = inode
            return self.make_entry_attributes(attrs)

        vpath = VirtualPath.parse(spath)
        logger.debug(f"FUSE: Parsed getattr {spath=} to {vpath=}")
        try:
            attrs = self.rose.getattr(vpath)
        except OSError as e:
            raise llfuse.FUSEError(e.errno) from e
        attrs["st_ino"] = inode
        return self.make_entry_attributes(attrs)

    def lookup(self, parent_inode: int, name: bytes, _: Any) -> llfuse.EntryAttributes:
        logger.debug(f"FUSE: Received lookup for {parent_inode=}/{name=}")
        # For performance, pull from the lookup cache if possible.
        with contextlib.suppress(KeyError):
            attrs = self.lookup_cache[(parent_inode, name)]
            logger.debug(
                f"FUSE: Resolved lookup {parent_inode=}/{name=} to {attrs.__getstate__()=}"
            )
            return attrs
        spath = self.inodes.get_path(parent_inode, name)
        inode = self.inodes.calc_inode(spath)
        logger.debug(f"FUSE: Resolved lookup {parent_inode=}/{name=} to {spath=}")
        # If this path is a ghost file path; pretend here!
        if self.ghost_existing_files.get(str(spath), False):
            logger.debug(f"FUSE: Resolved getattr for {spath=} as ghost existing file")
            attrs = self.rose.stat("file")
            attrs["st_ino"] = inode
            return self.make_entry_attributes(attrs)
        # If this directory is a ghost directory path; pretend here!
        if self.in_progress_collage_additions.get(str(spath.parent), False):
            logger.debug(f"FUSE: Resolved lookup for {spath=} as in progress collage addition")
            raise llfuse.FUSEError(errno.ENOENT)

        vpath = VirtualPath.parse(spath)
        logger.debug(f"FUSE: Parsed lookup {spath=} to {vpath=}")
        try:
            attrs = self.rose.getattr(vpath)
        except OSError as e:
            raise llfuse.FUSEError(e.errno) from e
        attrs["st_ino"] = inode
        return self.make_entry_attributes(attrs)

    def opendir(self, inode: int, _: Any) -> int:
        logger.debug(f"FUSE: Received opendir for {inode=}")
        spath = self.inodes.get_path(inode)
        logger.debug(f"FUSE: Resolved opendir {inode=} to {spath=}")
        # If this directory is a ghost directory path; pretend here!
        if self.in_progress_collage_additions.get(str(spath), False):
            logger.debug(f"FUSE: Resolved lookup for {spath=} as in progress collage addition")
            entries: list[tuple[int, bytes, llfuse.EntryAttributes]] = []
            for node in [".", ".."]:
                attrs = self.rose.stat("dir")
                attrs["st_ino"] = self.inodes.calc_inode(spath / node)
                entry = self.make_entry_attributes(attrs)
                entries.append((inode, node.encode(), entry))
            fh = self.fhandler.next()
            self.readdir_cache[fh] = entries
            return fh

        vpath = VirtualPath.parse(spath)
        logger.debug(f"FUSE: Parsed opendir {spath=} to {vpath=}")
        entries = []
        try:
            for namestr, attrs in self.rose.readdir(vpath):
                name = namestr.encode()
                attrs["st_ino"] = self.inodes.calc_inode(spath / namestr)
                entry = self.make_entry_attributes(attrs)
                entries.append((inode, name, entry))
        except OSError as e:
            raise llfuse.FUSEError(e.errno) from e
        fh = self.fhandler.next()
        self.readdir_cache[fh] = entries
        logger.debug(f"FUSE: Stored {len(entries)=} nodes into the readdir cache for {fh=}")
        return fh

    def releasedir(self, fh: int) -> None:
        with contextlib.suppress(KeyError):
            del self.readdir_cache[fh]

    def readdir(
        self,
        fd: int,
        offset: int = 0,
    ) -> Iterator[tuple[bytes, llfuse.EntryAttributes, int]]:
        logger.debug(f"FUSE: Received readdir for {fd=} {offset=}")
        try:
            entries = self.readdir_cache[fd]
        except KeyError:
            return
        for i, (parent_inode, name, entry) in enumerate(entries[offset:]):
            self.getattr_cache[entry.st_ino] = entry
            self.lookup_cache[(parent_inode, name)] = entry
            yield name, entry, i + offset + 1
            logger.debug(f"FUSE: Yielded entry {i + offset=} in readdir of {fd=}")

    def open(self, inode: int, flags: int, _: Any) -> int:
        logger.debug(f"FUSE: Received open for {inode=} {flags=}")
        spath = self.inodes.get_path(inode)
        logger.debug(f"FUSE: Resolved open {inode=} to {spath=}")
        vpath = VirtualPath.parse(spath)
        logger.debug(f"FUSE: Parsed open {spath=} to {vpath=}")

        # We black hole all files written to an in-progress collage addition, EXCEPT for the Rose
        # datafile, which we pass through to RoseLogicalCore.
        if self.in_progress_collage_additions.get(str(spath.parent), False) and not (
            vpath.file
            and STORED_DATA_FILE_REGEX.match(vpath.file)
            and flags & os.O_CREAT == os.O_CREAT
        ):
            logger.debug(f"FUSE: Resolved open for {spath=} as in progress collage addition")
            self.ghost_existing_files[str(spath)] = True
            return self.fhandler.dev_null

        try:
            fh = self.rose.open(vpath, flags)
        except OSError as e:
            raise llfuse.FUSEError(e.errno) from e
        # If this was a create operation, and Rose succeeded, flag the filepath as a ghost file and
        # _always_ pretend it exists for the following short duration.
        if flags & os.O_CREAT == os.O_CREAT:
            logger.debug(f"FUSE: Setting {spath=} as ghost existing file for next 3 seconds")
            self.ghost_existing_files[str(spath)] = True
        return fh

    def read(self, fh: int, offset: int, length: int) -> bytes:
        logger.debug(f"FUSE: Received read for {fh=} {offset=} {length=}")
        if fh == self.fhandler.dev_null:
            logger.debug(f"FUSE: Matched {fh=} to /dev/null sentinel")
            return b""
        try:
            return self.rose.read(fh, offset, length)
        except OSError as e:
            raise llfuse.FUSEError(e.errno) from e

    def write(self, fh: int, offset: int, data: bytes) -> int:
        logger.debug(f"FUSE: Received write for {fh=} {offset=} {len(data)=}")
        if fh == self.fhandler.dev_null:
            logger.debug(f"FUSE: Matched {fh=} to /dev/null sentinel")
            return len(data)
        try:
            return self.rose.write(fh, offset, data)
        except OSError as e:
            raise llfuse.FUSEError(e.errno) from e

    def release(self, fh: int) -> None:
        logger.debug(f"FUSE: Received release for {fh=}")
        if fh == self.fhandler.dev_null:
            logger.debug(f"FUSE: Matched {fh=} to /dev/null sentinel")
            return
        try:
            self.rose.release(fh)
        except OSError as e:
            raise llfuse.FUSEError(e.errno) from e

    def ftruncate(self, fh: int, length: int = 0) -> None:
        logger.debug(f"FUSE: Received ftruncate for {fh=} {length=}")
        if fh == self.fhandler.dev_null:
            logger.debug(f"FUSE: Matched {fh=} to /dev/null sentinel")
            return
        fh = self.fhandler.unwrap_host(fh)
        return os.ftruncate(fh, length)

    def create(
        self,
        parent_inode: int,
        name: bytes,
        _mode: int,
        flags: int,
        ctx: Any,
    ) -> tuple[int, llfuse.EntryAttributes]:
        logger.debug(f"FUSE: Received create for {parent_inode=}/{name=} {flags=}")
        path = self.inodes.get_path(parent_inode, name)
        logger.debug(f"FUSE: Resolved create {parent_inode=}/{name=} to {path=}")
        inode = self.inodes.calc_inode(path)
        logger.debug(f"FUSE: Created inode {inode=} for {path=}; now delegating to open call")
        try:
            fh = self.open(inode, flags, ctx)
        except OSError as e:
            raise llfuse.FUSEError(e.errno) from e
        self.reset_getattr_caches()
        attrs = self.rose.stat("file")
        attrs["st_ino"] = inode
        return fh, self.make_entry_attributes(attrs)

    def unlink(self, parent_inode: int, name: bytes, _: Any) -> None:
        logger.debug(f"FUSE: Received unlink for {parent_inode=}/{name=}")
        spath = self.inodes.get_path(parent_inode, name)
        logger.debug(f"FUSE: Resolved unlink {parent_inode=}/{name=} to {spath=}")
        vpath = VirtualPath.parse(spath)
        logger.debug(f"FUSE: Parsed unlink {spath=} to {vpath=}")
        try:
            self.rose.unlink(vpath)
        except OSError as e:
            raise llfuse.FUSEError(e.errno) from e
        self.reset_getattr_caches()
        self.inodes.remove_path(spath)
        with contextlib.suppress(KeyError):
            del self.ghost_existing_files[str(spath)]

    def mkdir(self, parent_inode: int, name: bytes, _mode: int, _: Any) -> llfuse.EntryAttributes:
        logger.debug(f"FUSE: Received mkdir for {parent_inode=}/{name=}")
        spath = self.inodes.get_path(parent_inode, name)
        logger.debug(f"FUSE: Resolved mkdir {parent_inode=}/{name=} to {spath=}")
        vpath = VirtualPath.parse(spath)
        logger.debug(f"FUSE: Parsed mkdir {spath=} to {vpath=}")

        if vpath.collage and vpath.release:
            # If we're creating a release in a collage, then this is the collage addition sequence.
            # Flag the directory and exit with the standard response. See the comment in __init__ to
            # learn more.
            logger.debug(
                f"FUSE: Setting {spath=} as in progress collage addition for next 5 seconds"
            )
            self.in_progress_collage_additions[str(spath)] = True
            inode = self.inodes.calc_inode(spath)
            attrs = self.rose.stat("dir")
            attrs["st_ino"] = inode
            return self.make_entry_attributes(attrs)

        try:
            self.rose.mkdir(vpath)
        except OSError as e:
            raise llfuse.FUSEError(e.errno) from e
        self.reset_getattr_caches()
        inode = self.inodes.calc_inode(spath)
        attrs = self.rose.stat("dir")
        attrs["st_ino"] = inode
        return self.make_entry_attributes(attrs)

    def rmdir(self, parent_inode: int, name: bytes, _: Any) -> None:
        logger.debug(f"FUSE: Received rmdir for {parent_inode=}/{name=}")
        spath = self.inodes.get_path(parent_inode, name)
        logger.debug(f"FUSE: Resolved rmdir {parent_inode=}/{name=} to {spath=}")
        vpath = VirtualPath.parse(spath)
        logger.debug(f"FUSE: Parsed rmdir {spath=} to {vpath=}")
        try:
            self.rose.rmdir(vpath)
        except OSError as e:
            raise llfuse.FUSEError(e.errno) from e
        self.reset_getattr_caches()
        self.inodes.remove_path(spath)
        with contextlib.suppress(KeyError):
            del self.in_progress_collage_additions[str(spath)]

    def rename(
        self,
        old_parent_inode: int,
        old_name: bytes,
        new_parent_inode: int,
        new_name: bytes,
        _: Any,
    ) -> None:
        logger.debug(
            f"FUSE: Received rename for {old_parent_inode=}/{old_name=} "
            f"to {new_parent_inode=}/{new_name=}"
        )
        old_spath = self.inodes.get_path(old_parent_inode, old_name)
        new_spath = self.inodes.get_path(new_parent_inode, new_name)
        logger.debug(
            f"FUSE: Received rename for {old_parent_inode=}/{old_name=} to {old_spath=}"
            f"and for {new_parent_inode=}/{new_name=} to {new_spath=}"
        )
        old_vpath = VirtualPath.parse(old_spath)
        new_vpath = VirtualPath.parse(new_spath)
        logger.debug(
            f"FUSE: Parsed rmdir {old_spath=} to {old_vpath=} and {old_vpath=} to {new_vpath=}"
        )
        try:
            self.rose.rename(old_vpath, new_vpath)
        except OSError as e:
            raise llfuse.FUSEError(e.errno) from e
        self.reset_getattr_caches()
        self.inodes.rename_path(old_spath, new_spath)

    # ============================================================================================
    # Unimplemented stubs. Tools expect these syscalls to exist, so we implement versions of them
    # that do not error, but also do not do anything.
    # ============================================================================================

    def forget(self, inode_list: list[tuple[int, int]]) -> None:
        logger.debug(f"FUSE: Received forget for {inode_list=}")
        # Clear the cache in case someone makes a request later...
        self.reset_getattr_caches()

    def mknod(self, parent_inode: int, name: bytes, _mode: int, _: Any) -> llfuse.EntryAttributes:
        logger.debug(f"FUSE: Received mknod for {parent_inode=}/{name=}")
        attrs = self.rose.stat("file")
        attrs["st_ino"] = self.inodes.calc_inode(self.inodes.get_path(parent_inode, name))
        return self.make_entry_attributes(attrs)

    def flush(self, fh: int) -> None:
        logger.debug(f"FUSE: Received flush for {fh=}")
        pass

    def setattr(
        self,
        inode: int,
        attr: llfuse.EntryAttributes,
        fields: llfuse.SetattrFields,
        fh: int | None,
        ctx: Any,
    ) -> llfuse.EntryAttributes:
        logger.debug(f"FUSE: Received setattr for {inode=} {attr=} {fields=} {fh=}")
        return self.getattr(inode, ctx)

    def getxattr(self, inode: int, name: bytes, _: Any) -> bytes:
        logger.debug(f"FUSE: Received getxattr for {inode=} {name=}")
        raise llfuse.FUSEError(llfuse.ENOATTR)

    def setxattr(self, inode: int, name: bytes, value: bytes, _: Any) -> None:
        logger.debug(f"FUSE: Received setxattr for {inode=} {name=} {value=}")

    def listxattr(self, inode: int, _: Any) -> Iterator[bytes]:
        logger.debug(f"FUSE: Received listxattr for {inode=}")
        return iter([])

    def removexattr(self, inode: int, name: bytes, _: Any) -> None:
        logger.debug(f"FUSE: Received removexattr for {inode=} {name=}")
        raise llfuse.FUSEError(llfuse.ENOATTR)


def mount_virtualfs(c: Config, debug: bool = False) -> None:
    options = set(llfuse.default_options)
    options.add("fsname=rose")
    if debug:
        options.add("debug")
    llfuse.init(VirtualFS(c), str(c.vfs.mount_dir), options)
    try:
        llfuse.main(workers=c.max_proc)
    except:
        llfuse.close()
        raise
    llfuse.close()


def unmount_virtualfs(c: Config) -> None:
    subprocess.run(["umount", str(c.vfs.mount_dir)])
