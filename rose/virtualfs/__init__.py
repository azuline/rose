import errno
import logging
import os
import stat
import subprocess
from collections.abc import Iterator
from pathlib import Path
from typing import IO, Any, Literal

import fuse

from rose.cache.read import (
    artist_exists,
    genre_exists,
    label_exists,
    list_artists,
    list_genres,
    list_labels,
    list_releases,
    list_tracks,
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

        def mkstat(mode: Literal["dir", "file"], fsize: int = 4096) -> fuse.Stat:
            return fuse.Stat(
                st_nlink=1,
                st_mode=(stat.S_IFDIR | 0o755) if mode == "dir" else (stat.S_IFREG | 0o644),
                st_size=fsize,
                st_uid=os.getuid(),
                st_gid=os.getgid(),
            )

        if path == "/":
            return mkstat("dir")

        parts = path.split("/")[1:]  # First part is always empty string.

        if parts[0] == "albums":
            if len(parts) == 1:
                return mkstat("dir")
            if not release_exists(self.config, parts[1]):
                raise OSError(errno.ENOENT, "No such file or directory")
            if len(parts) == 2:
                return mkstat("dir")
            if len(parts) == 3 and (tp := track_exists(self.config, parts[1], parts[2])):
                return mkstat("file", tp.stat().st_size)
            raise OSError(errno.ENOENT, "No such file or directory")

        if parts[0] == "artists":
            if len(parts) == 1:
                return mkstat("dir")
            if not artist_exists(self.config, parts[1]):
                raise OSError(errno.ENOENT, "No such file or directory")
            if len(parts) == 2:
                return mkstat("dir")
            if not release_exists(self.config, parts[2]):
                raise OSError(errno.ENOENT, "No such file or directory")
            if len(parts) == 3:
                return mkstat("dir")
            if len(parts) == 4 and (tp := track_exists(self.config, parts[2], parts[3])):
                return mkstat("file", tp.stat().st_size)
            raise OSError(errno.ENOENT, "No such file or directory")

        if parts[0] == "genres":
            if len(parts) == 1:
                return mkstat("dir")
            if not genre_exists(self.config, parts[1]):
                raise OSError(errno.ENOENT, "No such file or directory")
            if len(parts) == 2:
                return mkstat("dir")
            if not release_exists(self.config, parts[2]):
                raise OSError(errno.ENOENT, "No such file or directory")
            if len(parts) == 3:
                return mkstat("dir")
            if len(parts) == 4 and (tp := track_exists(self.config, parts[2], parts[3])):
                return mkstat("file", tp.stat().st_size)
            raise OSError(errno.ENOENT, "No such file or directory")

        if parts[0] == "labels":
            if len(parts) == 1:
                return mkstat("dir")
            if not label_exists(self.config, parts[1]):
                raise OSError(errno.ENOENT, "No such file or directory")
            if len(parts) == 2:
                return mkstat("dir")
            if not release_exists(self.config, parts[2]):
                raise OSError(errno.ENOENT, "No such file or directory")
            if len(parts) == 3:
                return mkstat("dir")
            if len(parts) == 4 and (tp := track_exists(self.config, parts[2], parts[3])):
                return mkstat("file", tp.stat().st_size)
            raise OSError(errno.ENOENT, "No such file or directory")

        raise OSError(errno.ENOENT, "No such file or directory")

    def readdir(self, path: str, _: Any) -> Iterator[fuse.Direntry]:
        logger.debug(f"Received readdir for {path}")
        if path == "/":
            yield from [
                fuse.Direntry("."),
                fuse.Direntry(".."),
                fuse.Direntry("albums"),
                fuse.Direntry("artists"),
                fuse.Direntry("genres"),
                fuse.Direntry("labels"),
            ]
            return

        parts = path.split("/")[1:]  # First part is always empty string.

        if parts[0] == "albums":
            if len(parts) == 1:
                yield from [fuse.Direntry("."), fuse.Direntry("..")]
                for album in list_releases(self.config):
                    yield fuse.Direntry(album.virtual_dirname)
                return
            if len(parts) == 2:
                yield from [fuse.Direntry("."), fuse.Direntry("..")]
                for track in list_tracks(self.config, parts[1]):
                    yield fuse.Direntry(track.virtual_filename)
                return
            return

        if parts[0] == "artists":
            if len(parts) == 1:
                yield from [fuse.Direntry("."), fuse.Direntry("..")]
                for artist in list_artists(self.config):
                    yield fuse.Direntry(sanitize_filename(artist))
                return
            if len(parts) == 2:
                yield from [fuse.Direntry("."), fuse.Direntry("..")]
                for album in list_releases(self.config, sanitized_artist_filter=parts[1]):
                    yield fuse.Direntry(album.virtual_dirname)
                return
            if len(parts) == 3:
                yield from [fuse.Direntry("."), fuse.Direntry("..")]
                for track in list_tracks(self.config, parts[2]):
                    yield fuse.Direntry(track.virtual_filename)
                return
            return

        if parts[0] == "genres":
            if len(parts) == 1:
                yield from [fuse.Direntry("."), fuse.Direntry("..")]
                for genre in list_genres(self.config):
                    yield fuse.Direntry(sanitize_filename(genre))
                return
            if len(parts) == 2:
                yield from [fuse.Direntry("."), fuse.Direntry("..")]
                for album in list_releases(self.config, sanitized_genre_filter=parts[1]):
                    yield fuse.Direntry(album.virtual_dirname)
                return
            if len(parts) == 3:
                yield from [fuse.Direntry("."), fuse.Direntry("..")]
                for track in list_tracks(self.config, parts[2]):
                    yield fuse.Direntry(track.virtual_filename)
                return
            return

        if parts[0] == "labels":
            if len(parts) == 1:
                yield from [fuse.Direntry("."), fuse.Direntry("..")]
                for label in list_labels(self.config):
                    yield fuse.Direntry(sanitize_filename(label))
                return
            if len(parts) == 2:
                yield from [fuse.Direntry("."), fuse.Direntry("..")]
                for album in list_releases(self.config, sanitized_label_filter=parts[1]):
                    yield fuse.Direntry(album.virtual_dirname)
                return
            if len(parts) == 3:
                yield from [fuse.Direntry("."), fuse.Direntry("..")]
                for track in list_tracks(self.config, parts[2]):
                    yield fuse.Direntry(track.virtual_filename)
                return
            return

        raise OSError(errno.ENOENT, "No such file or directory")

    def read(self, path: str, size: int, offset: int) -> bytes:
        logger.debug(f"Received read for {path}")

        def read_bytes(p: Path) -> bytes:
            with p.open("rb") as fp:
                fp.seek(offset)
                return fp.read(size)

        parts = path.split("/")[1:]  # First part is always empty string.

        if parts[0] == "albums":
            if len(parts) != 3:
                raise OSError(errno.ENOENT, "No such file or directory")
            for track in list_tracks(self.config, parts[1]):
                if track.virtual_filename == parts[2]:
                    return read_bytes(track.source_path)
            raise OSError(errno.ENOENT, "No such file or directory")
        if parts[0] in ["artists", "genres", "labels"]:
            if len(parts) != 4:
                raise OSError(errno.ENOENT, "No such file or directory")
            for track in list_tracks(self.config, parts[2]):
                if track.virtual_filename == parts[3]:
                    return read_bytes(track.source_path)
            raise OSError(errno.ENOENT, "No such file or directory")
        raise OSError(errno.ENOENT, "No such file or directory")

    def open(self, path: str, flags: str) -> IO[Any]:
        logger.debug(f"Received open for {path}")

        parts = path.split("/")[1:]  # First part is always empty string.

        if parts[0] == "albums":
            if len(parts) != 3:
                raise OSError(errno.ENOENT, "No such file or directory")
            for track in list_tracks(self.config, parts[1]):
                if track.virtual_filename == parts[2]:
                    return track.source_path.open(flags)
            raise OSError(errno.ENOENT, "No such file or directory")
        if parts[0] in ["artists", "genres", "labels"]:
            if len(parts) != 4:
                raise OSError(errno.ENOENT, "No such file or directory")
            for track in list_tracks(self.config, parts[2]):
                if track.virtual_filename == parts[3]:
                    return track.source_path.open(flags)
            raise OSError(errno.ENOENT, "No such file or directory")
        raise OSError(errno.ENOENT, "No such file or directory")


def mount_virtualfs(c: Config, mount_args: list[str]) -> None:
    server = VirtualFS(c)
    server.parse([str(c.fuse_mount_dir), *mount_args])
    server.main()


def unmount_virtualfs(c: Config) -> None:
    subprocess.run(["umount", str(c.fuse_mount_dir)])
