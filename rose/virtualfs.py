import errno
import logging
import os
import re
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
    collage_has_release,
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
from rose.collages import (
    add_release_to_collage,
    create_collage,
    delete_collage,
    delete_release_from_collage,
    rename_collage,
)
from rose.common import sanitize_filename
from rose.config import Config
from rose.releases import ReleaseDoesNotExistError, delete_release

logger = logging.getLogger(__name__)


# IDK how to get coverage on this thing.
class VirtualFS(fuse.Operations):  # type: ignore
    def __init__(self, config: Config):
        self.config = config
        self.hide_artists_set = set(config.fuse_hide_artists)
        self.hide_genres_set = set(config.fuse_hide_genres)
        self.hide_labels_set = set(config.fuse_hide_labels)
        self.getattr_cache: dict[str, dict[str, Any]] = {}
        super().__init__()

    def getattr(self, path: str, fh: int) -> dict[str, Any]:
        logger.debug(f"Received getattr for {path=} {fh=}")

        # We cache the getattr call with lru_cache because this is called _extremely_ often. Like
        # for every node that we see in the output of `ls`.
        try:
            return self.getattr_cache[path]
        except KeyError:
            pass

        logger.debug(f"Recomputing uncached getattr for {path}")
        p = parse_virtual_path(path)
        logger.debug(f"Parsed getattr path as {p}")

        # Some early guards just in case.
        if p.release and p.collage and not collage_has_release(self.config, p.collage, p.release):
            raise fuse.FuseOSError(errno.ENOENT)

        if p.release and p.file:
            if tp := track_exists(self.config, p.release, p.file):
                return mkstat("file", tp)
            if cp := cover_exists(self.config, p.release, p.file):
                return mkstat("file", cp)
        elif p.release:
            if rp := release_exists(self.config, p.release):
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
                "Artists",
                "Collages",
                "Genres",
                "Labels",
                "Releases",
            ]
        elif p.release:
            rf = get_release_files(self.config, p.release)
            for track in rf.tracks:
                yield track.virtual_filename
            if rf.cover:
                yield rf.cover.name
        elif p.artist or p.genre or p.label or p.view == "Releases":
            if (
                (p.artist and p.artist in self.hide_artists_set)
                or (p.genre and p.genre in self.hide_genres_set)
                or (p.label and p.label in self.hide_labels_set)
            ):
                raise fuse.FuseOSError(errno.ENOENT)
            for release in list_releases(
                self.config,
                sanitized_artist_filter=p.artist,
                sanitized_genre_filter=p.genre,
                sanitized_label_filter=p.label,
            ):
                yield release.virtual_dirname
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
        p = parse_virtual_path(path)
        logger.debug(f"Parsed open path as {p}")

        if p.release and p.file:
            rf = get_release_files(self.config, p.release)
            if rf.cover and p.file == rf.cover.name:
                return os.open(str(rf.cover), flags)
            for track in rf.tracks:
                if track.virtual_filename == p.file:
                    return os.open(str(track.source_path), flags)

        if flags & os.O_CREAT == os.O_CREAT:
            raise fuse.FuseOSError(errno.EACCES)
        raise fuse.FuseOSError(errno.ENOENT)

    def read(self, path: str, length: int, offset: int, fh: int) -> bytes:
        logger.debug(f"Received read for {path=} {length=} {offset=} {fh=}")
        os.lseek(fh, offset, os.SEEK_SET)
        return os.read(fh, length)

    def write(self, path: str, data: bytes, offset: int, fh: int) -> int:
        logger.debug(f"Received write for {path=} {data=} {offset=} {fh=}")
        os.lseek(fh, offset, os.SEEK_SET)
        return os.write(fh, data)

    def truncate(self, path: str, length: int, fh: int | None = None) -> None:
        logger.debug(f"Received truncate for {path=} {length=} {fh=}")
        if fh:
            os.ftruncate(fh, length)
        else:
            p = parse_virtual_path(path)
            logger.debug(f"Parsed truncate path as {p}")
            if p.release and p.file:
                rf = get_release_files(self.config, p.release)
                if rf.cover and p.file == rf.cover.name:
                    os.truncate(str(rf.cover), length)
                for track in rf.tracks:
                    if track.virtual_filename == p.file:
                        return os.truncate(str(track.source_path), length)

    def release(self, path: str, fh: int) -> None:
        logger.debug(f"Received release for {path=} {fh=}")
        os.close(fh)

    def mkdir(self, path: str, mode: int) -> None:
        logger.debug(f"Received mkdir for {path=} {mode=}")
        p = parse_virtual_path(path)

        # Possible actions:
        # 1. Add a release to an existing collage.
        # 2. Create a new collage.
        if p.view != "Collages" or (p.collage is None and p.release is None):
            raise fuse.FuseOSError(errno.EACCES)
        elif p.collage and p.release is None:
            create_collage(self.config, p.collage)
        elif p.collage and p.release:
            try:
                add_release_to_collage(self.config, p.collage, p.release)
            except ReleaseDoesNotExistError as e:
                logger.debug(
                    f"Failed adding release {p.release} to collage {p.collage}: release not found."
                )
                raise fuse.FuseOSError(errno.ENOENT) from e
        else:
            raise fuse.FuseOSError(errno.EACCES)

    def rmdir(self, path: str) -> None:
        logger.debug(f"Received rmdir for {path=}")
        p = parse_virtual_path(path)

        # Possible actions:
        # 1. Delete a release from an existing collage.
        # 2. Delete a collage.
        if p.view == "Collages":
            if p.collage and p.release is None:
                delete_collage(self.config, p.collage)
            elif p.collage and p.release:
                delete_release_from_collage(self.config, p.collage, p.release)
            else:
                raise fuse.FuseOSError(errno.EACCES)
        elif p.release is not None:
            delete_release(self.config, p.release)
        else:
            raise fuse.FuseOSError(errno.EACCES)

    def rename(self, old: str, new: str) -> None:
        logger.debug(f"Received rename for {old=} {new=}")
        op = parse_virtual_path(old)
        np = parse_virtual_path(new)

        # Possible actions:
        # 1. Rename a collage
        if op.view == "Collages" and np.view == "Collages":
            if op.collage and np.collage and not op.release and not np.release:
                rename_collage(self.config, op.collage, np.collage)
            else:
                raise fuse.FuseOSError(errno.EACCES)
        else:
            raise fuse.FuseOSError(errno.EACCES)
        # TODO: Consider allowing renaming artist/genre/label here?

    # To investigate:
    # - opendir/releasedir (edit collage?)
    #
    # Unimplemented:
    # - readlink
    # - mknod
    # - unlink
    # - symlink
    # - link
    # - chmod
    # - chown
    # - statfs
    # - flush
    # - fsync
    # - readdir
    # - fsyncdir
    # - destroy
    # - access
    # - create
    # - ftruncate
    # - fgetattr
    # - lock
    # - utimens
    #
    # Dummy implementations below:

    def chmod(self, *_, **__) -> None:  # type: ignore
        pass

    def chown(self, *_, **__) -> None:  # type: ignore
        pass

    def unlink(self, *_, **__) -> None:  # type: ignore
        pass

    def create(self, *_, **__) -> None:  # type: ignore
        raise fuse.FuseOSError(errno.ENOTSUP)


@dataclass
class ParsedPath:
    view: Literal["Root", "Releases", "Artists", "Genres", "Labels", "Collages"] | None
    artist: str | None = None
    genre: str | None = None
    label: str | None = None
    collage: str | None = None
    release: str | None = None
    file: str | None = None


def parse_virtual_path(path: str) -> ParsedPath:
    parts = path.split("/")[1:]  # First part is always empty string.

    if len(parts) == 1 and parts[0] == "":
        return ParsedPath(view="Root")

    if parts[0] == "Releases":
        if len(parts) == 1:
            return ParsedPath(view="Releases")
        if len(parts) == 2:
            return ParsedPath(view="Releases", release=parts[1])
        if len(parts) == 3:
            return ParsedPath(view="Releases", release=parts[1], file=parts[2])
        raise fuse.FuseOSError(errno.ENOENT)

    if parts[0] == "Artists":
        if len(parts) == 1:
            return ParsedPath(view="Artists")
        if len(parts) == 2:
            return ParsedPath(view="Artists", artist=parts[1])
        if len(parts) == 3:
            return ParsedPath(view="Artists", artist=parts[1], release=parts[2])
        if len(parts) == 4:
            return ParsedPath(view="Artists", artist=parts[1], release=parts[2], file=parts[3])
        raise fuse.FuseOSError(errno.ENOENT)

    if parts[0] == "Genres":
        if len(parts) == 1:
            return ParsedPath(view="Genres")
        if len(parts) == 2:
            return ParsedPath(view="Genres", genre=parts[1])
        if len(parts) == 3:
            return ParsedPath(view="Genres", genre=parts[1], release=parts[2])
        if len(parts) == 4:
            return ParsedPath(view="Genres", genre=parts[1], release=parts[2], file=parts[3])
        raise fuse.FuseOSError(errno.ENOENT)

    if parts[0] == "Labels":
        if len(parts) == 1:
            return ParsedPath(view="Labels")
        if len(parts) == 2:
            return ParsedPath(view="Labels", label=parts[1])
        if len(parts) == 3:
            return ParsedPath(view="Labels", label=parts[1], release=parts[2])
        if len(parts) == 4:
            return ParsedPath(view="Labels", label=parts[1], release=parts[2], file=parts[3])
        raise fuse.FuseOSError(errno.ENOENT)

    if parts[0] == "Collages":
        if len(parts) == 1:
            return ParsedPath(view="Collages")
        if len(parts) == 2:
            return ParsedPath(view="Collages", collage=parts[1])
        if len(parts) == 3:
            return ParsedPath(view="Collages", collage=parts[1], release=rm_position(parts[2]))
        if len(parts) == 4:
            return ParsedPath(
                view="Collages", collage=parts[1], release=rm_position(parts[2]), file=parts[3]
            )
        raise fuse.FuseOSError(errno.ENOENT)

    raise fuse.FuseOSError(errno.ENOENT)


POSITION_REGEX = re.compile(r"^\d+\. ")


# In collages, we print directories with position of the release in the collage. When parsing,
# strip it out. Otherwise we will have to handle this parsing in every method.
def rm_position(x: str) -> str:
    return POSITION_REGEX.sub("", x)


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
        "st_mode": (stat.S_IFDIR | 0o755) if mode == "dir" else (stat.S_IFREG | 0o644),
        "st_size": st_size,
        "st_uid": os.getuid(),
        "st_gid": os.getgid(),
        "st_atime": st_atime,
        "st_mtime": st_mtime,
        "st_ctime": st_ctime,
    }


def mount_virtualfs(
    c: Config,
    foreground: bool = False,
    nothreads: bool = False,
    debug: bool = False,
) -> None:
    fuse.FUSE(
        VirtualFS(c),
        str(c.fuse_mount_dir),
        foreground=foreground,
        nothreads=nothreads,
        debug=debug,
    )


def unmount_virtualfs(c: Config) -> None:
    subprocess.run(["umount", str(c.fuse_mount_dir)])
