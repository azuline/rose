from __future__ import annotations

import contextlib
import errno
import logging
import os
import random
import re
import stat
import subprocess
import time
from collections.abc import Iterator
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Literal

import llfuse

from rose.cache import (
    artist_exists,
    collage_exists,
    cover_exists,
    genre_exists,
    get_playlist,
    get_release,
    label_exists,
    list_artists,
    list_collage_releases,
    list_collages,
    list_genres,
    list_labels,
    list_playlists,
    list_releases,
    release_exists,
    track_exists,
)
from rose.collages import (
    add_release_to_collage,
    create_collage,
    delete_collage,
    remove_release_from_collage,
    rename_collage,
)
from rose.config import Config
from rose.playlists import (
    create_playlist,
    delete_playlist,
    remove_track_from_playlist,
    rename_playlist,
)
from rose.releases import ReleaseDoesNotExistError, delete_release, toggle_release_new

logger = logging.getLogger(__name__)

# In collages, playlists, and releases, we print directories with position of the release/track in
# the collage. When parsing, strip it out. Otherwise we will have to handle this parsing in every
# method.
POSITION_REGEX = re.compile(r"^([^.]+)\. ")
# In recently added, we print the date that the release was added to the library. When parsing,
# strip it out.
ADDED_AT_REGEX = re.compile(r"^\[[\d-]{10}\] ")


@dataclass
class VirtualPath:
    view: (
        Literal[
            "Root",
            "Releases",
            "Artists",
            "Genres",
            "Labels",
            "Collages",
            "Playlists",
            "New",
            "Recently Added",
        ]
        | None
    )
    artist: str | None = None
    genre: str | None = None
    label: str | None = None
    collage: str | None = None
    playlist: str | None = None
    release: str | None = None
    release_position: str | None = None
    file: str | None = None
    file_position: str | None = None

    @classmethod
    def parse(cls, path: Path, *, parse_release_position: bool = True) -> VirtualPath:
        parts = str(path.resolve()).split("/")[1:]  # First part is always empty string.

        if len(parts) == 1 and parts[0] == "":
            return VirtualPath(view="Root")

        if parts[0] == "1. Releases":
            if len(parts) == 1:
                return VirtualPath(view="Releases")
            if len(parts) == 2:
                return VirtualPath(view="Releases", release=parts[1])
            if len(parts) == 3:
                return VirtualPath(
                    view="Releases",
                    release=parts[1],
                    file=POSITION_REGEX.sub("", parts[2]),
                    file_position=m[1] if (m := POSITION_REGEX.match(parts[2])) else None,
                )
            raise llfuse.FUSEError(errno.ENOENT)

        if parts[0] == "2. Releases - New":
            if len(parts) == 1:
                return VirtualPath(view="New")
            if len(parts) == 2:
                return VirtualPath(view="New", release=parts[1])
            if len(parts) == 3:
                return VirtualPath(
                    view="New",
                    release=parts[1],
                    file=POSITION_REGEX.sub("", parts[2]),
                    file_position=m[1] if (m := POSITION_REGEX.match(parts[2])) else None,
                )
            raise llfuse.FUSEError(errno.ENOENT)

        if parts[0] == "3. Releases - Recently Added":
            if len(parts) == 1:
                return VirtualPath(view="Recently Added")
            if len(parts) == 2 and ADDED_AT_REGEX.match(parts[1]):
                return VirtualPath(view="Recently Added", release=ADDED_AT_REGEX.sub("", parts[1]))
            if len(parts) == 3 and ADDED_AT_REGEX.match(parts[1]):
                return VirtualPath(
                    view="Recently Added",
                    release=ADDED_AT_REGEX.sub("", parts[1]),
                    file=POSITION_REGEX.sub("", parts[2]),
                    file_position=m[1] if (m := POSITION_REGEX.match(parts[2])) else None,
                )
            raise llfuse.FUSEError(errno.ENOENT)

        if parts[0] == "4. Artists":
            if len(parts) == 1:
                return VirtualPath(view="Artists")
            if len(parts) == 2:
                return VirtualPath(view="Artists", artist=parts[1])
            if len(parts) == 3:
                return VirtualPath(view="Artists", artist=parts[1], release=parts[2])
            if len(parts) == 4:
                return VirtualPath(
                    view="Artists",
                    artist=parts[1],
                    release=parts[2],
                    file=POSITION_REGEX.sub("", parts[3]),
                    file_position=m[1] if (m := POSITION_REGEX.match(parts[3])) else None,
                )
            raise llfuse.FUSEError(errno.ENOENT)

        if parts[0] == "5. Genres":
            if len(parts) == 1:
                return VirtualPath(view="Genres")
            if len(parts) == 2:
                return VirtualPath(view="Genres", genre=parts[1])
            if len(parts) == 3:
                return VirtualPath(view="Genres", genre=parts[1], release=parts[2])
            if len(parts) == 4:
                return VirtualPath(
                    view="Genres",
                    genre=parts[1],
                    release=parts[2],
                    file=POSITION_REGEX.sub("", parts[3]),
                    file_position=m[1] if (m := POSITION_REGEX.match(parts[3])) else None,
                )
            raise llfuse.FUSEError(errno.ENOENT)

        if parts[0] == "6. Labels":
            if len(parts) == 1:
                return VirtualPath(view="Labels")
            if len(parts) == 2:
                return VirtualPath(view="Labels", label=parts[1])
            if len(parts) == 3:
                return VirtualPath(view="Labels", label=parts[1], release=parts[2])
            if len(parts) == 4:
                return VirtualPath(
                    view="Labels",
                    label=parts[1],
                    release=parts[2],
                    file=POSITION_REGEX.sub("", parts[3]),
                    file_position=m[1] if (m := POSITION_REGEX.match(parts[3])) else None,
                )
            raise llfuse.FUSEError(errno.ENOENT)

        if parts[0] == "7. Collages":
            if len(parts) == 1:
                return VirtualPath(view="Collages")
            if len(parts) == 2:
                return VirtualPath(view="Collages", collage=parts[1])
            if len(parts) == 3:
                return VirtualPath(
                    view="Collages",
                    collage=parts[1],
                    release=POSITION_REGEX.sub("", parts[2])
                    if parse_release_position
                    else parts[2],
                    release_position=m[1]
                    if parse_release_position and (m := POSITION_REGEX.match(parts[2]))
                    else None,
                )
            if len(parts) == 4:
                return VirtualPath(
                    view="Collages",
                    collage=parts[1],
                    release=POSITION_REGEX.sub("", parts[2])
                    if parse_release_position
                    else parts[2],
                    release_position=m[1]
                    if parse_release_position and (m := POSITION_REGEX.match(parts[2]))
                    else None,
                    file=POSITION_REGEX.sub("", parts[3]),
                    file_position=m[1] if (m := POSITION_REGEX.match(parts[3])) else None,
                )
            raise llfuse.FUSEError(errno.ENOENT)

        if parts[0] == "8. Playlists":
            if len(parts) == 1:
                return VirtualPath(view="Playlists")
            if len(parts) == 2:
                return VirtualPath(view="Playlists", playlist=parts[1])
            if len(parts) == 3:
                return VirtualPath(
                    view="Playlists",
                    playlist=parts[1],
                    file=POSITION_REGEX.sub("", parts[2]),
                    file_position=m[1] if (m := POSITION_REGEX.match(parts[2])) else None,
                )
            raise llfuse.FUSEError(errno.ENOENT)

        raise llfuse.FUSEError(errno.ENOENT)


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
        self._label_w = None
        self._label_b = None

        if config.fuse_artists_whitelist:
            self._artist_w = set(config.fuse_artists_whitelist)
        if config.fuse_artists_blacklist:
            self._artist_b = set(config.fuse_artists_blacklist)
        if config.fuse_genres_whitelist:
            self._genre_w = set(config.fuse_genres_whitelist)
        if config.fuse_genres_blacklist:
            self._genre_b = set(config.fuse_genres_blacklist)
        if config.fuse_labels_whitelist:
            self._label_w = set(config.fuse_labels_whitelist)
        if config.fuse_labels_blacklist:
            self._label_b = set(config.fuse_labels_blacklist)

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

    def label(self, label: str) -> bool:
        if self._label_w:
            return label in self._label_w
        elif self._label_b:
            return label not in self._label_b
        return True


class FileDescriptorGenerator:
    """
    FileDescriptorGenerator generates file descriptors and handles wrapping so that we do not go
    over the int size. Assumes that we do not cycle 10k file descriptors before the first descriptor
    is released.
    """

    def __init__(self) -> None:
        self._state = 10

    def next(self) -> int:
        self._state = (self._state + 1 % 10_000) + 10
        return self._state


class RoseLogicalFS:
    def __init__(self, config: Config, fhgen: FileDescriptorGenerator):
        self.config = config
        # We use this object to determine whether we should show an artist/genre/label
        self.can_show = CanShower(config)
        # We implement the "add track to playlist" operation in a slightly special way. Unlike
        # releases, where the virtual dirname is globally unique, track filenames are not globally
        # unique. Rather, they clash quite often. So instead of running a lookup on the virtual
        # filename, we must instead inspect the bytes that get written upon copy, because within the
        # copied audio file is the `track_id` tag (aka `roseid`).
        #
        # In order to be able to inspect the written bytes, we must store state across several
        # syscalls (open, write, release). So the process goes:
        #
        # 1. Upon file open, if the syscall is intended to create a new file in a playlist, treat it
        #    as a playlist addition instead. Mock the file descriptor with an in-memory sentinel.
        # 2. On subsequent write requests to the same path and sentinel file descriptor, take the
        #    bytes-to-write and store them in the in-memory state.
        # 3. On release, write all the bytes to a temporary file and load the audio file up into an
        #    AudioFile dataclass (which parses tags). Look for the track ID tag, and if it exists,
        #    add it to the playlist.
        #
        # The state is a mapping of (path, fh) -> (playlist_name, ext, bytes).
        self.playlist_additions_in_progress: dict[tuple[str, int], tuple[str, str, bytearray]] = {}
        self.fhgen = fhgen
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
            s = realpath.stat()
            attrs["st_size"] = s.st_size
            attrs["st_atime_ns"] = s.st_atime
            attrs["st_mtime_ns"] = s.st_mtime
            attrs["st_ctime_ns"] = s.st_ctime

        return attrs

    def getattr(self, path: Path) -> dict[str, Any]:
        logger.debug(f"Received getattr for {path=}")
        p = VirtualPath.parse(path)
        logger.debug(f"Parsed getattr path as {p}")

        # TODO: IN PROGRESS PLAYLIST ADDITION
        # # We need this here in order to support fgetattr during the file write operation.
        # if fh and self.playlist_additions_in_progress.get((path, fh), None):
        #     logger.debug("Matched read to an in-progress playlist addition.")
        #     return mkstat("file")

        # Common logic that gets called for each release.
        def getattr_release(rp: Path) -> dict[str, Any]:
            assert p.release is not None
            # If no file, return stat for the release dir.
            if not p.file:
                return self.stat("dir", rp)
            # If there is a file, getattr the file.
            if tp := track_exists(self.config, p.release, p.file):
                return self.stat("file", tp)
            if cp := cover_exists(self.config, p.release, p.file):
                return self.stat("file", cp)
            # If no file matches, return errno.ENOENT.
            raise llfuse.FUSEError(errno.ENOENT)

        # 8. Playlists
        if p.playlist:
            try:
                playlist, tracks = get_playlist(self.config, p.playlist)  # type: ignore
            except TypeError as e:
                raise llfuse.FUSEError(errno.ENOENT) from e
            if p.file:
                if p.file_position:
                    for idx, track in enumerate(tracks):
                        if track.virtual_filename == p.file and idx + 1 == int(p.file_position):
                            return self.stat("file", track.source_path)
                if playlist.cover_path and f"cover{playlist.cover_path.suffix}" == p.file:
                    return self.stat("file", playlist.cover_path)
                raise llfuse.FUSEError(errno.ENOENT)
            return self.stat("dir")

        # 7. Collages
        if p.collage:
            if not collage_exists(self.config, p.collage):
                raise llfuse.FUSEError(errno.ENOENT)
            if p.release:
                for _, virtual_dirname, src_path in list_collage_releases(self.config, p.collage):
                    if virtual_dirname == p.release:
                        return getattr_release(src_path)
                raise llfuse.FUSEError(errno.ENOENT)
            return self.stat("dir")

        # 6. Labels
        if p.label:
            if not label_exists(self.config, p.label) or not self.can_show.label(p.label):
                raise llfuse.FUSEError(errno.ENOENT)
            if p.release:
                for r in list_releases(self.config, sanitized_label_filter=p.label):
                    if r.virtual_dirname == p.release:
                        return getattr_release(r.source_path)
                raise llfuse.FUSEError(errno.ENOENT)
            return self.stat("dir")

        # 5. Genres
        if p.genre:
            if not genre_exists(self.config, p.genre) or not self.can_show.genre(p.genre):
                raise llfuse.FUSEError(errno.ENOENT)
            if p.release:
                for r in list_releases(self.config, sanitized_genre_filter=p.genre):
                    if r.virtual_dirname == p.release:
                        return getattr_release(r.source_path)
                raise llfuse.FUSEError(errno.ENOENT)
            return self.stat("dir")

        # 4. Artists
        if p.artist:
            if not artist_exists(self.config, p.artist) or not self.can_show.artist(p.artist):
                raise llfuse.FUSEError(errno.ENOENT)
            if p.release:
                for r in list_releases(self.config, sanitized_artist_filter=p.artist):
                    if r.virtual_dirname == p.release:
                        return getattr_release(r.source_path)
                raise llfuse.FUSEError(errno.ENOENT)
            return self.stat("dir")

        # {1,2,3}. Releases
        if p.release:
            if p.view == "New" and not p.release.startswith("{NEW} "):
                raise llfuse.FUSEError(errno.ENOENT)
            if rp := release_exists(self.config, p.release):
                return getattr_release(rp)
            raise llfuse.FUSEError(errno.ENOENT)

        # 0. Root
        elif p.view:
            return self.stat("dir")

        # -1. Wtf are you doing here?
        raise llfuse.FUSEError(errno.ENOENT)

    def readdir(self, path: Path) -> Iterator[tuple[str, dict[str, Any]]]:
        logger.debug(f"Received readdir for {path=}")
        p = VirtualPath.parse(path)
        logger.debug(f"Parsed readdir path as {p}")

        # Call getattr to validate existence. We can now assume that the provided path exists. This
        # for example includes checks that a given album belongs to the artist/genre/label/collage
        # its nested under.
        logger.debug(f"Invoking getattr in readdir to validate existence of {path}")
        self.getattr(path)

        yield from [
            (".", self.stat("dir")),
            ("..", self.stat("dir")),
        ]

        if p.view == "Root":
            yield from [
                ("1. Releases", self.stat("dir")),
                ("2. Releases - New", self.stat("dir")),
                ("3. Releases - Recently Added", self.stat("dir")),
                ("4. Artists", self.stat("dir")),
                ("5. Genres", self.stat("dir")),
                ("6. Labels", self.stat("dir")),
                ("7. Collages", self.stat("dir")),
                ("8. Playlists", self.stat("dir")),
            ]
            return

        if p.release:
            if cachedata := get_release(self.config, p.release):
                release, tracks = cachedata
                for track in tracks:
                    filename = f"{track.formatted_release_position}. {track.virtual_filename}"
                    yield filename, self.stat("file", track.source_path)
                if release.cover_image_path:
                    yield release.cover_image_path.name, self.stat("file", release.cover_image_path)
                return
            raise llfuse.FUSEError(errno.ENOENT)

        if p.artist or p.genre or p.label or p.view == "Releases" or p.view == "New":
            for release in list_releases(
                self.config,
                sanitized_artist_filter=p.artist,
                sanitized_genre_filter=p.genre,
                sanitized_label_filter=p.label,
                new=True if p.view == "New" else None,
            ):
                yield release.virtual_dirname, self.stat("dir", release.source_path)
            return

        if p.view == "Recently Added":
            for release in list_releases(self.config):
                dirname = f"[{release.added_at[:10]}] {release.virtual_dirname}"
                yield dirname, self.stat("dir", release.source_path)
            return

        elif p.view == "Artists":
            for artist, sanitized_artist in list_artists(self.config):
                if not self.can_show.artist(artist):
                    continue
                yield sanitized_artist, self.stat("dir")
            return

        if p.view == "Genres":
            for genre, sanitized_genre in list_genres(self.config):
                if not self.can_show.genre(genre):
                    continue
                yield sanitized_genre, self.stat("dir")
            return

        if p.view == "Labels":
            for label, sanitized_label in list_labels(self.config):
                if not self.can_show.label(label):
                    continue
                yield sanitized_label, self.stat("dir")
            return

        if p.view == "Collages" and p.collage:
            releases = list(list_collage_releases(self.config, p.collage))
            # Two zeros because `max(single_arg)` assumes that the single_arg is enumerable.
            pad_size = max(0, 0, *[len(str(r[0])) for r in releases])
            for idx, virtual_dirname, source_dir in releases:
                v = f"{str(idx).zfill(pad_size)}. {virtual_dirname}"
                yield v, self.stat("dir", source_dir)
            return

        if p.view == "Collages":
            # Don't need to sanitize because the collage names come from filenames.
            for collage in list_collages(self.config):
                yield collage, self.stat("dir")
            return

        if p.view == "Playlists" and p.playlist:
            pdata = get_playlist(self.config, p.playlist)
            if pdata is None:
                raise llfuse.FUSEError(errno.ENOENT)
            playlist, tracks = pdata
            pad_size = max(0, 0, *[len(str(i + 1)) for i, _ in enumerate(tracks)])
            for idx, track in enumerate(tracks):
                v = f"{str(idx+1).zfill(pad_size)}. {track.virtual_filename}"
                yield v, self.stat("file", track.source_path)
            if playlist.cover_path:
                v = f"cover{playlist.cover_path.suffix}"
                yield v, self.stat("file", playlist.cover_path)
            return

        if p.view == "Playlists":
            # Don't need to sanitize because the playlist names come from filenames.
            for pname in list_playlists(self.config):
                yield pname, self.stat("dir")
            return

        raise llfuse.FUSEError(errno.ENOENT)

    def open(self, path: Path, flags: int) -> int:
        logger.debug(f"Received open for {path=} {flags=}")
        p = VirtualPath.parse(path)
        logger.debug(f"Parsed open path as {p}")

        err = errno.ENOENT
        if flags & os.O_CREAT == os.O_CREAT:
            err = errno.EACCES

        if p.release and p.file and (rdata := get_release(self.config, p.release)):
            release, tracks = rdata
            if release.cover_image_path and p.file == release.cover_image_path.name:
                return os.open(str(release.cover_image_path), flags)
            for track in tracks:
                if track.virtual_filename == p.file:
                    return os.open(str(track.source_path), flags)
            raise llfuse.FUSEError(err)
        if p.playlist and p.file:
            try:
                playlist, tracks = get_playlist(self.config, p.playlist)  # type: ignore
            except TypeError as e:
                raise llfuse.FUSEError(errno.ENOENT) from e
            # If we are trying to create a file in the playlist, enter the "add file to playlist"
            # operation sequence. See the __init__ for more details.
            if flags & os.O_CREAT == os.O_CREAT:
                fh = self.fhgen.next()
                self.playlist_additions_in_progress[(str(path), fh)] = (
                    p.playlist,
                    Path(p.file).suffix,
                    bytearray(),
                )
                return fh
            # Otherwise, continue on...
            if p.file_position:
                for idx, track in enumerate(tracks):
                    if track.virtual_filename == p.file and idx + 1 == int(p.file_position):
                        return os.open(str(track.source_path), flags)
            if playlist.cover_path and f"cover{playlist.cover_path.suffix}" == p.file:
                return os.open(playlist.cover_path, flags)
            raise llfuse.FUSEError(err)

        raise llfuse.FUSEError(err)

    def unlink(self, path: Path) -> None:
        logger.debug(f"Received unlink for {path=}")
        p = VirtualPath.parse(path)
        logger.debug(f"Parsed unlink path as {p}")

        # Possible actions:
        # 1. Delete a playlist.
        # 2. Delete a track from a playlist.
        if p.view == "Playlists" and p.playlist and p.file is None:
            delete_playlist(self.config, p.playlist)
            return
        if (
            p.view == "Playlists"
            and p.playlist
            and p.file
            and p.file_position
            and (pdata := get_playlist(self.config, p.playlist))
        ):
            for idx, track in enumerate(pdata[1]):
                if track.virtual_filename == p.file and idx + 1 == int(p.file_position):
                    remove_track_from_playlist(self.config, p.playlist, track.id)
                    return
            raise llfuse.FUSEError(errno.ENOENT)

        # Otherwise, noop. If we return an error, that prevents rmdir from being called when we rm.

    def mkdir(self, path: Path) -> None:
        logger.debug(f"Received mkdir for {path=}")
        p = VirtualPath.parse(path, parse_release_position=False)
        logger.debug(f"Parsed mkdir path as {p}")

        # Possible actions:
        # 1. Add a release to an existing collage.
        # 2. Create a new collage.
        # 3. Create a new playlist.
        if p.collage and p.release is None:
            create_collage(self.config, p.collage)
            return
        if p.collage and p.release:
            try:
                add_release_to_collage(self.config, p.collage, p.release)
                return
            except ReleaseDoesNotExistError as e:
                logger.debug(
                    f"Failed adding release {p.release} to collage {p.collage}: release not found."
                )
                raise llfuse.FUSEError(errno.ENOENT) from e
        if p.playlist and p.file is None:
            create_playlist(self.config, p.playlist)
            return

        raise llfuse.FUSEError(errno.EACCES)

    def rmdir(self, path: Path) -> None:
        logger.debug(f"Received rmdir for {path=}")
        p = VirtualPath.parse(path)
        logger.debug(f"Parsed rmdir path as {p}")

        # Possible actions:
        # 1. Delete a collage.
        # 2. Delete a release from an existing collage.
        # 3. Delete a playlist.
        # 4. Delete a release.
        if p.view == "Collages" and p.collage and p.release is None:
            delete_collage(self.config, p.collage)
            return
        if p.view == "Collages" and p.collage and p.release:
            remove_release_from_collage(self.config, p.collage, p.release)
            return
        if p.view == "Playlists" and p.playlist and p.file is None:
            delete_playlist(self.config, p.playlist)
            return
        if p.view != "Collages" and p.release is not None:
            delete_release(self.config, p.release)
            return

        raise llfuse.FUSEError(errno.EACCES)

    def rename(self, old: Path, new: Path) -> None:
        logger.debug(f"Received rename for {old=} {new=}")
        op = VirtualPath.parse(old)
        logger.debug(f"Parsed rename old path as {op}")
        np = VirtualPath.parse(new)
        logger.debug(f"Parsed rename new path as {np}")

        # Possible actions:
        # 1. Toggle a release's new status.
        # 2. Rename a collage.
        # 3. Rename a playlist.
        # TODO: Consider allowing renaming artist/genre/label here?
        if (
            (op.release and np.release)
            and op.release.removeprefix("{NEW} ") == np.release.removeprefix("{NEW} ")
            and (not op.file and not np.file)
            and op.release.startswith("{NEW} ") != np.release.startswith("{NEW} ")
        ):
            toggle_release_new(self.config, op.release)
            return
        if (
            op.view == "Collages"
            and np.view == "Collages"
            and (op.collage and np.collage)
            and op.collage != np.collage
            and (not op.release and not np.release)
        ):
            rename_collage(self.config, op.collage, np.collage)
            return
        if (
            op.view == "Playlists"
            and np.view == "Playlists"
            and (op.playlist and np.playlist)
            and op.playlist != np.playlist
            and (not op.file and not np.file)
        ):
            rename_playlist(self.config, op.playlist, np.playlist)
            return

        raise llfuse.FUSEError(errno.EACCES)


class INodeManager:
    """
    INodeManager manages the mapping of inodes to paths in our filesystem. We have this because the
    llfuse library makes us manage the inodes...
    """

    def __init__(self, config: Config):
        self.config = config

        self._inode_to_path_map: dict[int, Path] = {llfuse.ROOT_INODE: Path("/")}
        self._path_to_inode_map: dict[str, int] = {"/": llfuse.ROOT_INODE}
        self._next_inode_ctr: int = llfuse.ROOT_INODE + 1

        # We cache some items for getattr for performance reasons--after a ls, getattr is serially
        # called for each item in the directory, and sequential 1k SQLite reads is quite slow in any
        # universe. So whenever we have a readdir, we do a batch read and populate the getattr
        # cache. The getattr cache is valid for only 1 second, which prevents stale results from
        # being read from it.
        #
        # The dict is a map of paths to (timestamp, mkstat_args). The timestamp should be checked
        # upon access. If the cache entry is valid, mkstat should be called with the provided args.
        self.getattr_cache: dict[int, tuple[float, llfuse.EntryAttributes]] = {}

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
                path = path.parent
            return path / name.decode()
        except KeyError as e:
            raise llfuse.FUSEError(errno.ENOENT) from e

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


class VirtualFS(llfuse.Operations):  # type: ignore
    """
    This is the virtual filesystem class, which implements commands by delegating the Rose-specific
    logic to RoseLogicalFS and the inode/fd<->path tracking to INodeManager. This architecture
    allows us to have a fairly clean logical implementation for Rose despite a fairly low-level
    llfuse library.
    """

    def __init__(self, config: Config):
        fhgen = FileDescriptorGenerator()
        self.rose = RoseLogicalFS(config, fhgen)
        self.inodes = INodeManager(config)
        self.default_attrs = {
            # TODO: Well, this should be ok for now.
            "generation": random.randint(0, 1000000),
        }

    @staticmethod
    def make_entry_attributes(attrs: dict[str, Any]) -> llfuse.EntryAttributes:
        entry = llfuse.EntryAttributes()
        for k, v in attrs.items():
            setattr(entry, k, v)
        return entry

    def getattr(self, inode: int, _: Any) -> llfuse.EntryAttributes:
        # For performance, pull from the getattr cache if possible.
        with contextlib.suppress(KeyError):
            ts, attrs = self.getattr_cache[inode]
            if time.time() - ts < 1.0:
                logger.debug(f"Returning cached getattr result for {inode=}")
                return attrs

        attrs = self.rose.getattr(self.inodes.get_path(inode))
        attrs["st_ino"] = inode
        attrs.update(self.default_attrs)
        return self.make_entry_attributes(attrs)

    def lookup(self, parent_inode: int, name: bytes, _: Any) -> llfuse.EntryAttributes:
        path = self.inodes.get_path(parent_inode, name)
        attrs = self.rose.getattr(path)
        attrs["st_ino"] = self.inodes.calc_inode(path)
        attrs.update(self.default_attrs)
        return self.make_entry_attributes(attrs)

    def opendir(self, inode: int, _: Any) -> int:
        # This should return a file handle, but we simply re-use the inode as the fh.
        return inode

    def releasedir(self, _: int) -> None:
        # No op; since we are using the inode as the fh, we have nothing to release.
        pass

    def readdir(
        self,
        inode: int,
        offset: int = 0,
    ) -> Iterator[tuple[bytes, llfuse.EntryAttributes, int]]:
        path = self.inodes.get_path(inode)
        for i, (name, attrs) in enumerate(self.rose.readdir(path)):
            if i < offset:
                continue
            attrs["st_ino"] = self.inodes.calc_inode(path)
            attrs.update(self.default_attrs)
            entry = self.make_entry_attributes(attrs)
            yield name.encode(), entry, i + 1

    def open(self, inode: int, flags: int, _: Any) -> int:
        path = self.inodes.get_path(inode)
        return self.rose.open(path, flags)

    def read(self, fh: int, offset: int, length: int) -> bytes:
        # TODO: IN PROGRESS PLAYLIST ADDITION
        # if pap := self.playlist_additions_in_progress.get((path, fh), None):
        #     logger.debug("Matched read to an in-progress playlist addition.")
        #     _, _, b = pap
        #     return b[offset : offset + length]
        os.lseek(fh, offset, os.SEEK_SET)
        return os.read(fh, length)

    def write(self, fh: int, offset: int, data: bytes) -> int:
        # TODO: IN PROGRESS PLAYLIST ADDITION
        # if pap := self.playlist_additions_in_progress.get((path, fh), None):
        #     logger.debug("Matched write to an in-progress playlist addition.")
        #     _, _, b = pap
        #     del b[offset:]
        #     b.extend(data)
        #     return len(data)
        os.lseek(fh, offset, os.SEEK_SET)
        return os.write(fh, data)

    def release(self, fh: int) -> None:
        # TODO:
        # if pap := self.playlist_additions_in_progress.get((path, fh), None):
        #     logger.debug("Matched release to an in-progress playlist addition.")
        #     playlist, ext, b = pap
        #     if not b:
        #         logger.debug("Aborting playlist addition release: no bytes to write.")
        #         return
        #     with tempfile.TemporaryDirectory() as tmpdir:
        #         audiopath = Path(tmpdir) / f"f{ext}"
        #         with audiopath.open("wb") as fp:
        #             fp.write(b)
        #         audiofile = AudioFile.from_file(audiopath)
        #         track_id = audiofile.id
        #     if not track_id:
        #         logger.warning(
        #             "Failed to parse track_id from file in playlist addition operation "
        #             f"sequence: {path} {audiofile}"
        #         )
        #         return
        #     add_track_to_playlist(self.config, playlist, track_id)
        #     del self.playlist_additions_in_progress[(path, fh)]
        #     return
        os.close(fh)

    def ftruncate(self, fh: int, length: int = 0) -> None:
        # TODO: IN PROGRESS PLAYLIST ADDITION
        return os.ftruncate(fh, length)

    def create(
        self,
        parent_inode: int,
        name: bytes,
        _mode: int,
        flags: int,
        ctx: Any,
    ) -> tuple[int, llfuse.EntryAttributes]:
        path = self.inodes.get_path(parent_inode, name)
        inode = self.inodes.calc_inode(path)
        fh = self.open(inode, flags, ctx)
        return fh, self.rose.stat("file")

    def unlink(self, parent_inode: int, name: bytes, _: Any) -> None:
        path = self.inodes.get_path(parent_inode, name)
        self.rose.unlink(path)

    def mkdir(self, parent_inode: int, name: bytes, _mode: int, _: Any) -> llfuse.EntryAttributes:
        path = self.inodes.get_path(parent_inode, name)
        self.rose.mkdir(path)

    def rmdir(self, parent_inode: int, name: bytes, _: Any) -> None:
        path = self.inodes.get_path(parent_inode, name)
        self.rose.rmdir(path)

    def rename(
        self,
        old_parent_inode: int,
        old_name: bytes,
        new_parent_inode: int,
        new_name: bytes,
        _: Any,
    ) -> None:
        old_path = self.inodes.get_path(old_parent_inode, old_name)
        new_path = self.inodes.get_path(new_parent_inode, new_name)
        self.rose.rename(old_path, new_path)


def mount_virtualfs(c: Config, debug: bool = False) -> None:
    options = set(llfuse.default_options)
    options.add("fsname=rose")
    if debug:
        options.add("debug")
    llfuse.init(VirtualFS(c), str(c.fuse_mount_dir), options)
    try:
        llfuse.main(workers=1)
    except:
        llfuse.close()
        raise
    llfuse.close()


def unmount_virtualfs(c: Config) -> None:
    subprocess.run(["umount", str(c.fuse_mount_dir)])
