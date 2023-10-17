import errno
import logging
import os
import stat
import subprocess
from collections.abc import Iterator
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Literal

import fuse

from rose.cache import (
    artist_exists,
    collage_exists,
    cover_exists,
    genre_exists,
    get_release_files,
    label_exists,
    list_artists,
    list_collage_releases,
    list_collages,
    list_genres,
    list_labels,
    list_releases,
    release_exists,
    track_exists,
)
from rose.config import Config
from rose.sanitize import sanitize_filename

logger = logging.getLogger(__name__)


class VirtualFS(fuse.Operations):  # type: ignore
    def __init__(self, config: Config):
        self.config = config
        self.hide_artists_set = set(config.fuse_hide_artists)
        self.hide_genres_set = set(config.fuse_hide_genres)
        self.hide_labels_set = set(config.fuse_hide_labels)
        self.getattr_cache: dict[str, dict[str, Any]] = {}
        super().__init__()

    def getattr(self, path: str, _: int) -> dict[str, Any]:
        logger.debug(f"Received getattr for {path}")

        # We cache the getattr call with lru_cache because this is called _extremely_ often. Like
        # for every node that we see in the output of `ls`.
        try:
            return self.getattr_cache[path]
        except KeyError:
            pass

        logger.debug(f"Recomputing uncached getattr for {path}")
        p = parse_virtual_path(path)
        logger.debug(f"Parsed getattr path as {p}")

        if p.album and p.file:
            if tp := track_exists(self.config, p.album, p.file):
                return mkstat("file", tp)
            if cp := cover_exists(self.config, p.album, p.file):
                return mkstat("file", cp)
        elif p.album:
            if rp := release_exists(self.config, p.album):
                return mkstat("dir", rp)
        elif p.artist:
            if artist_exists(self.config, p.artist) and p.artist not in self.hide_artists_set:
                return mkstat("dir")
        elif p.genre:
            if genre_exists(self.config, p.genre) and p.genre not in self.hide_genres_set:
                return mkstat("dir")
        elif p.label:
            if label_exists(self.config, p.label) and p.label not in self.hide_labels_set:
                return mkstat("dir")
        elif p.collage:
            if collage_exists(self.config, p.collage):
                return mkstat("dir")
        elif p.view:
            return mkstat("dir")

        raise fuse.FuseOSError(errno.ENOENT)

    def readdir(self, path: str, _: int) -> Iterator[str]:
        logger.debug(f"Received readdir for {path}")
        p = parse_virtual_path(path)
        logger.debug(f"Parsed readdir path as {p}")

        yield from [".", ".."]

        if p.view == "Root":
            yield from [
                "Albums",
                "Artists",
                "Genres",
                "Labels",
                "Collages",
            ]
        elif p.album:
            rf = get_release_files(self.config, p.album)
            for track in rf.tracks:
                yield track.virtual_filename
            if rf.cover:
                yield rf.cover.name
        elif p.artist or p.genre or p.label or p.view == "Albums":
            if (
                (p.artist and p.artist in self.hide_artists_set)
                or (p.genre and p.genre in self.hide_genres_set)
                or (p.label and p.label in self.hide_labels_set)
            ):
                raise fuse.FuseOSError(errno.ENOENT)
            for album in list_releases(
                self.config,
                sanitized_artist_filter=p.artist,
                sanitized_genre_filter=p.genre,
                sanitized_label_filter=p.label,
            ):
                yield album.virtual_dirname
        elif p.view == "Artists":
            for artist in list_artists(self.config):
                if artist in self.hide_artists_set:
                    continue
                yield sanitize_filename(artist)
        elif p.view == "Genres":
            for genre in list_genres(self.config):
                if genre in self.hide_genres_set:
                    continue
                yield sanitize_filename(genre)
        elif p.view == "Labels":
            for label in list_labels(self.config):
                if label in self.hide_labels_set:
                    continue
                yield sanitize_filename(label)
        elif p.view == "Collages" and p.collage:
            releases = list(list_collage_releases(self.config, p.collage))
            pad_size = max(len(str(r[0])) for r in releases)
            for idx, virtual_dirname in releases:
                yield f"{str(idx).zfill(pad_size)}. {virtual_dirname}"
        elif p.view == "Collages":
            # Don't need to sanitize because the collage names come from filenames.
            yield from list_collages(self.config)
        else:
            raise fuse.FuseOSError(errno.ENOENT)

    def open(self, path: str, flags: int) -> int:
        logger.debug(f"Received open for {path=} {flags=}")

        # Enforce a read-only file system.
        accmode = os.O_RDONLY | os.O_WRONLY | os.O_RDWR
        if (flags & accmode) != os.O_RDONLY:
            logger.debug("Raising EACCES due to a write-access open request")
            raise fuse.FuseOSError(errno.EACCES)

        p = parse_virtual_path(path)
        logger.debug(f"Parsed open path as {p}")

        if p.album and p.file:
            rf = get_release_files(self.config, p.album)
            if rf.cover and p.file == rf.cover.name:
                return os.open(str(rf.cover), flags)
            for track in rf.tracks:
                if track.virtual_filename == p.file:
                    return os.open(str(track.source_path), flags)

        raise fuse.FuseOSError(errno.ENOENT)

    def read(self, path: str, length: int, offset: int, fh: int) -> bytes:
        logger.debug(f"Received read for {path=} {length=} {offset=} {fh=}")
        os.lseek(fh, offset, os.SEEK_SET)
        return os.read(fh, length)


@dataclass
class ParsedPath:
    view: Literal["Root", "Albums", "Artists", "Genres", "Labels", "Collages"] | None
    artist: str | None = None
    genre: str | None = None
    label: str | None = None
    collage: str | None = None
    album: str | None = None
    file: str | None = None


def parse_virtual_path(path: str) -> ParsedPath:
    parts = path.split("/")[1:]  # First part is always empty string.

    if len(parts) == 1 and parts[0] == "":
        return ParsedPath(view="Root")

    if parts[0] == "Albums":
        if len(parts) == 1:
            return ParsedPath(view="Albums")
        if len(parts) == 2:
            return ParsedPath(view="Albums", album=parts[1])
        if len(parts) == 3:
            return ParsedPath(view="Albums", album=parts[1], file=parts[2])
        raise fuse.FuseOSError(errno.ENOENT)

    if parts[0] == "Artists":
        if len(parts) == 1:
            return ParsedPath(view="Artists")
        if len(parts) == 2:
            return ParsedPath(view="Artists", artist=parts[1])
        if len(parts) == 3:
            return ParsedPath(view="Artists", artist=parts[1], album=parts[2])
        if len(parts) == 4:
            return ParsedPath(view="Artists", artist=parts[1], album=parts[2], file=parts[3])
        raise fuse.FuseOSError(errno.ENOENT)

    if parts[0] == "Genres":
        if len(parts) == 1:
            return ParsedPath(view="Genres")
        if len(parts) == 2:
            return ParsedPath(view="Genres", genre=parts[1])
        if len(parts) == 3:
            return ParsedPath(view="Genres", genre=parts[1], album=parts[2])
        if len(parts) == 4:
            return ParsedPath(view="Genres", genre=parts[1], album=parts[2], file=parts[3])
        raise fuse.FuseOSError(errno.ENOENT)

    if parts[0] == "Labels":
        if len(parts) == 1:
            return ParsedPath(view="Labels")
        if len(parts) == 2:
            return ParsedPath(view="Labels", label=parts[1])
        if len(parts) == 3:
            return ParsedPath(view="Labels", label=parts[1], album=parts[2])
        if len(parts) == 4:
            return ParsedPath(view="Labels", label=parts[1], album=parts[2], file=parts[3])
        raise fuse.FuseOSError(errno.ENOENT)

    # In collages, we print directories with position of the release in the collage. When parsing,
    # strip it out. Otherwise we will have to handle this parsing in every method.
    if parts[0] == "Collages":
        if len(parts) == 1:
            return ParsedPath(view="Collages")
        if len(parts) == 2:
            return ParsedPath(view="Collages", collage=parts[1])
        if len(parts) == 3:
            try:
                return ParsedPath(
                    view="Collages",
                    collage=parts[1],
                    album=parts[2].split(". ", maxsplit=1)[1],
                )
            except IndexError:
                pass
        if len(parts) == 4:
            try:
                return ParsedPath(
                    view="Collages",
                    collage=parts[1],
                    album=parts[2].split(". ", maxsplit=1)[1],
                    file=parts[3],
                )
            except IndexError:
                pass
        raise fuse.FuseOSError(errno.ENOENT)

    raise fuse.FuseOSError(errno.ENOENT)


def mkstat(mode: Literal["dir", "file"], file: Path | None = None) -> dict[str, Any]:
    st_size = 4096
    st_atime = 0.0
    st_mtime = 0.0
    st_ctime = 0.0

    if file:
        s = file.stat()
        st_size = s.st_size
        st_atime = s.st_atime
        st_mtime = s.st_mtime
        st_ctime = s.st_ctime

    return {
        "st_nlink": 4,
        "st_mode": (stat.S_IFDIR | 0o555) if mode == "dir" else (stat.S_IFREG | 0o444),
        "st_size": st_size,
        "st_uid": os.getuid(),
        "st_gid": os.getgid(),
        "st_atime": st_atime,
        "st_mtime": st_mtime,
        "st_ctime": st_ctime,
    }


def mount_virtualfs(c: Config, foreground: bool = False) -> None:
    fuse.FUSE(VirtualFS(c), str(c.fuse_mount_dir), foreground=foreground)


def unmount_virtualfs(c: Config) -> None:
    subprocess.run(["umount", str(c.fuse_mount_dir)])
