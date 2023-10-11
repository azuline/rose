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

from rose.cache.read import (
    artist_exists,
    cover_exists,
    genre_exists,
    get_release_files,
    label_exists,
    list_artists,
    list_genres,
    list_labels,
    list_releases,
    release_exists,
    track_exists,
)
from rose.foundation.conf import Config
from rose.virtualfs.sanitize import sanitize_filename

logger = logging.getLogger(__name__)

fuse.fuse_python_api = (0, 2)


class VirtualFS(fuse.Fuse):  # type: ignore
    def __init__(self, config: Config):
        self.config = config
        super().__init__()

    def getattr(self, path: str) -> fuse.Stat:
        logger.debug(f"Received getattr for {path}")
        p = parse_virtual_path(path)
        logger.debug(f"Parsed getattr path as {p}")

        if p.view == "root":
            return mkstat("dir")
        elif p.album and p.file:
            if tp := track_exists(self.config, p.album, p.file):
                return mkstat("file", tp)
            if cp := cover_exists(self.config, p.album, p.file):
                return mkstat("file", cp)
        elif p.album:
            if rp := release_exists(self.config, p.album):
                return mkstat("dir", rp)
        elif p.artist:
            if artist_exists(self.config, p.artist):
                return mkstat("dir")
        elif p.genre:
            if genre_exists(self.config, p.genre):
                return mkstat("dir")
        elif p.label:
            if label_exists(self.config, p.label):
                return mkstat("dir")
        else:
            return mkstat("dir")

        raise OSError(errno.ENOENT, "No such file or directory")

    def readdir(self, path: str, _: Any) -> Iterator[fuse.Direntry]:
        logger.debug(f"Received readdir for {path}")
        p = parse_virtual_path(path)
        logger.debug(f"Parsed readdir path as {p}")

        yield from [fuse.Direntry("."), fuse.Direntry("..")]

        if p.view == "root":
            yield from [
                fuse.Direntry("albums"),
                fuse.Direntry("artists"),
                fuse.Direntry("genres"),
                fuse.Direntry("labels"),
            ]
        elif p.album:
            rf = get_release_files(self.config, p.album)
            for track in rf.tracks:
                yield fuse.Direntry(track.virtual_filename)
            if rf.cover:
                yield fuse.Direntry(rf.cover.name)
        elif p.artist or p.genre or p.label or p.view == "albums":
            for album in list_releases(
                self.config,
                sanitized_artist_filter=p.artist,
                sanitized_genre_filter=p.genre,
                sanitized_label_filter=p.label,
            ):
                yield fuse.Direntry(album.virtual_dirname)
        elif p.view == "artists":
            for artist in list_artists(self.config):
                yield fuse.Direntry(sanitize_filename(artist))
        elif p.view == "genres":
            for genre in list_genres(self.config):
                yield fuse.Direntry(sanitize_filename(genre))
        elif p.view == "labels":
            for label in list_labels(self.config):
                yield fuse.Direntry(sanitize_filename(label))
        else:
            raise OSError(errno.ENOENT, "No such file or directory")

    def read(self, path: str, size: int, offset: int) -> bytes:
        logger.debug(f"Received read for {path=} {size=} {offset=}")
        p = parse_virtual_path(path)
        logger.debug(f"Parsed read path as {p}")

        if p.album and p.file:
            rf = get_release_files(self.config, p.album)
            if rf.cover and p.file == rf.cover.name:
                with rf.cover.open("rb") as fp:
                    fp.seek(offset)
                    return fp.read(size)
            for track in rf.tracks:
                if track.virtual_filename == p.file:
                    with track.source_path.open("rb") as fp:
                        fp.seek(offset)
                        return fp.read(size)

        raise OSError(errno.ENOENT, "No such file or directory")

    def open(self, path: str, flags: int) -> None:
        logger.debug(f"Received open for {path=} {flags=}")

        # Raise an ENOENT if the file does not exist.
        self.getattr(path)

        # Read-only file system.
        accmode = os.O_RDONLY | os.O_WRONLY | os.O_RDWR
        if (flags & accmode) != os.O_RDONLY:
            raise OSError(errno.EACCES, "Access denied")

        return None


@dataclass
class ParsedPath:
    view: Literal["root", "albums", "artists", "genres", "labels"] | None
    artist: str | None = None
    genre: str | None = None
    label: str | None = None
    album: str | None = None
    file: str | None = None


def parse_virtual_path(path: str) -> ParsedPath:
    parts = path.split("/")[1:]  # First part is always empty string.

    if len(parts) == 1 and parts[0] == "":
        return ParsedPath(view="root")

    if parts[0] == "albums":
        if len(parts) == 1:
            return ParsedPath(view="albums")
        if len(parts) == 2:
            return ParsedPath(view="albums", album=parts[1])
        if len(parts) == 3:
            return ParsedPath(view="albums", album=parts[1], file=parts[2])
        raise OSError(errno.ENOENT, "No such file or directory")

    if parts[0] == "artists":
        if len(parts) == 1:
            return ParsedPath(view="artists")
        if len(parts) == 2:
            return ParsedPath(view="artists", artist=parts[1])
        if len(parts) == 3:
            return ParsedPath(view="artists", artist=parts[1], album=parts[2])
        if len(parts) == 4:
            return ParsedPath(view="artists", artist=parts[1], album=parts[2], file=parts[3])
        raise OSError(errno.ENOENT, "No such file or directory")

    if parts[0] == "genres":
        if len(parts) == 1:
            return ParsedPath(view="genres")
        if len(parts) == 2:
            return ParsedPath(view="genres", genre=parts[1])
        if len(parts) == 3:
            return ParsedPath(view="genres", genre=parts[1], album=parts[2])
        if len(parts) == 4:
            return ParsedPath(view="genres", genre=parts[1], album=parts[2], file=parts[3])
        raise OSError(errno.ENOENT, "No such file or directory")

    if parts[0] == "labels":
        if len(parts) == 1:
            return ParsedPath(view="labels")
        if len(parts) == 2:
            return ParsedPath(view="labels", label=parts[1])
        if len(parts) == 3:
            return ParsedPath(view="labels", label=parts[1], album=parts[2])
        if len(parts) == 4:
            return ParsedPath(view="labels", label=parts[1], album=parts[2], file=parts[3])
        raise OSError(errno.ENOENT, "No such file or directory")

    raise OSError(errno.ENOENT, "No such file or directory")


def mkstat(mode: Literal["dir", "file"], file: Path | None = None) -> fuse.Stat:
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

    return fuse.Stat(
        st_nlink=4,
        st_mode=(stat.S_IFDIR | 0o555) if mode == "dir" else (stat.S_IFREG | 0o444),
        st_size=st_size,
        st_uid=os.getuid(),
        st_gid=os.getgid(),
        st_atime=st_atime,
        st_mtime=st_mtime,
        st_ctime=st_ctime,
    )


def mount_virtualfs(c: Config, mount_args: list[str]) -> None:
    server = VirtualFS(c)
    server.parse([str(c.fuse_mount_dir), *mount_args])
    server.main()


def unmount_virtualfs(c: Config) -> None:
    subprocess.run(["umount", str(c.fuse_mount_dir)])
