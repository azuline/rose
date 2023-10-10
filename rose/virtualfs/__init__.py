import errno
import logging
import os
import stat
import subprocess
from collections.abc import Iterator
from typing import Any, Literal

import fuse

from rose.cache.read import (
    artist_exists,
    genre_exists,
    label_exists,
    list_artists,
    list_genres,
    list_labels,
    list_releases,
    release_exists,
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

        def get_mode_type(path: str) -> Literal["dir", "file", "missing"]:
            if path == "/":
                return "dir"

            if path.startswith("/albums"):
                if path.count("/") == 1:
                    return "dir"
                if path.count("/") == 2:
                    exists = release_exists(self.config, path.split("/")[2])
                    return "dir" if exists else "missing"
                return "missing"

            if path.startswith("/artists"):
                if path.count("/") == 1:
                    return "dir"
                if path.count("/") == 2:
                    exists = artist_exists(self.config, path.split("/")[2])
                    return "dir" if exists else "missing"
                if path.count("/") == 3:
                    exists = artist_exists(self.config, path.split("/")[2])
                    exists = exists and release_exists(self.config, path.split("/")[3])
                    return "dir" if exists else "missing"
                return "missing"

            if path.startswith("/genres"):
                if path.count("/") == 1:
                    return "dir"
                if path.count("/") == 2:
                    exists = genre_exists(self.config, path.split("/")[2])
                    return "dir" if exists else "missing"
                if path.count("/") == 3:
                    exists = genre_exists(self.config, path.split("/")[2])
                    exists = exists and release_exists(self.config, path.split("/")[3])
                    return "dir" if exists else "missing"
                return "missing"

            if path.startswith("/labels"):
                if path.count("/") == 1:
                    return "dir"
                if path.count("/") == 2:
                    exists = label_exists(self.config, path.split("/")[2])
                    return "dir" if exists else "missing"
                if path.count("/") == 3:
                    exists = label_exists(self.config, path.split("/")[2])
                    exists = exists and release_exists(self.config, path.split("/")[3])
                    return "dir" if exists else "missing"
                return "missing"

            return "missing"

        mode_type = get_mode_type(path)
        if mode_type == "missing":
            raise fuse.FuseError(errno.ENOENT)

        return fuse.Stat(
            st_nlink=1,
            st_mode=(stat.S_IFDIR | 0o755) if mode_type == "dir" else (stat.S_IFREG | 0o644),
            st_uid=os.getuid(),
            st_gid=os.getgid(),
        )

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

        if path.startswith("/albums"):
            if path.count("/") == 1:
                yield from [fuse.Direntry("."), fuse.Direntry("..")]
                for album in list_releases(self.config):
                    yield fuse.Direntry(album.virtual_dirname)
                return
            return

        if path.startswith("/artists"):
            if path.count("/") == 1:
                yield from [fuse.Direntry("."), fuse.Direntry("..")]
                for artist in list_artists(self.config):
                    yield fuse.Direntry(sanitize_filename(artist))
                return
            if path.count("/") == 2:
                yield from [fuse.Direntry("."), fuse.Direntry("..")]
                for album in list_releases(self.config, sanitized_artist_filter=path.split("/")[2]):
                    yield fuse.Direntry(album.virtual_dirname)
                return
            return

        if path.startswith("/genres"):
            if path.count("/") == 1:
                yield from [fuse.Direntry("."), fuse.Direntry("..")]
                for genre in list_genres(self.config):
                    yield fuse.Direntry(sanitize_filename(genre))
                return
            if path.count("/") == 2:
                yield from [fuse.Direntry("."), fuse.Direntry("..")]
                for album in list_releases(self.config, sanitized_genre_filter=path.split("/")[2]):
                    yield fuse.Direntry(album.virtual_dirname)
                return
            return

        if path.startswith("/labels"):
            if path.count("/") == 1:
                yield from [fuse.Direntry("."), fuse.Direntry("..")]
                for label in list_labels(self.config):
                    yield fuse.Direntry(sanitize_filename(label))
                return
            if path.count("/") == 2:
                yield from [fuse.Direntry("."), fuse.Direntry("..")]
                for album in list_releases(self.config, sanitized_label_filter=path.split("/")[2]):
                    yield fuse.Direntry(album.virtual_dirname)
                return
            return


def mount_virtualfs(c: Config) -> None:
    server = VirtualFS(c)
    server.parse([str(c.fuse_mount_dir)])
    server.main()


def unmount_virtualfs(c: Config) -> None:
    subprocess.run(["umount", str(c.fuse_mount_dir)])
