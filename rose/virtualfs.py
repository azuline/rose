import contextlib
import errno
import logging
import os
import re
import stat
import subprocess
import time
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
    get_release,
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
from rose.config import Config
from rose.releases import ReleaseDoesNotExistError, delete_release, toggle_release_new

logger = logging.getLogger(__name__)


# IDK how to get coverage on this thing.
class VirtualFS(fuse.Operations):  # type: ignore
    def __init__(self, config: Config):
        self.config = config
        self.hide_artists_set = set(config.fuse_hide_artists)
        self.hide_genres_set = set(config.fuse_hide_genres)
        self.hide_labels_set = set(config.fuse_hide_labels)
        # We cache some items for getattr for performance reasons--after a ls, getattr is serially
        # called for each item in the directory, and sequential 1k SQLite reads is quite slow in any
        # universe. So whenever we have a readdir, we do a batch read and populate the getattr
        # cache. The getattr cache is valid for only 1 second, which prevents stale results from
        # being read from it.
        #
        # The dict is a map of paths to (timestamp, mkstat_args). The timestamp should be checked
        # upon access. If the cache entry is valid, mkstat should be called with the provided args.
        self.getattr_cache: dict[str, tuple[float, Any]] = {}
        super().__init__()

    def getattr(self, path: str, fh: int) -> dict[str, Any]:
        logger.debug(f"Received getattr for {path=} {fh=}")

        with contextlib.suppress(KeyError):
            ts, mkstat_args = self.getattr_cache[path]
            if time.time() - ts < 1.0:
                logger.debug(f"Returning cached getattr result for {path=}")
                return mkstat(*mkstat_args)

        logger.debug(f"Handling uncached getattr for {path=}")
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

        # Outside of yielding the strings, we also populate the getattr cache here. See the comment
        # in __init__ for documentation.

        if p.view == "Root":
            yield from [
                "1. Releases",
                "2. Releases - New",
                "3. Releases - Recently Added",
                "4. Artists",
                "5. Genres",
                "6. Labels",
                "7. Collages",
            ]
        elif p.release:
            cachedata = get_release(self.config, p.release)
            if not cachedata:
                raise fuse.FuseOSError(errno.ENOENT) from None
            release, tracks = cachedata
            for track in tracks:
                yield track.virtual_filename
                self.getattr_cache[path + "/" + track.virtual_filename] = (
                    time.time(),
                    ("file", track.source_path),
                )
            if release.cover_image_path:
                yield release.cover_image_path.name
                self.getattr_cache[path + "/" + release.cover_image_path.name] = (
                    time.time(),
                    ("file", release.cover_image_path),
                )
        elif p.artist or p.genre or p.label or p.view == "Releases" or p.view == "New":
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
                new=True if p.view == "New" else None,
            ):
                yield release.virtual_dirname
                self.getattr_cache[path + "/" + release.virtual_dirname] = (
                    time.time(),
                    ("dir", release.source_path),
                )
        elif p.view == "Recently Added":
            for release in list_releases(self.config):
                dirname = f"[{release.added_at[:10]}] {release.virtual_dirname}"
                yield dirname
                self.getattr_cache[path + "/" + dirname] = (
                    time.time(),
                    ("dir", release.source_path),
                )
        elif p.view == "Artists":
            for artist, sanitized_artist in list_artists(self.config):
                if artist in self.hide_artists_set:
                    continue
                yield sanitized_artist
                self.getattr_cache[path + "/" + sanitized_artist] = (time.time(), ("dir",))
        elif p.view == "Genres":
            for genre, sanitized_genre in list_genres(self.config):
                if genre in self.hide_genres_set:
                    continue
                yield sanitized_genre
                self.getattr_cache[path + "/" + sanitized_genre] = (time.time(), ("dir",))
        elif p.view == "Labels":
            for label, sanitized_label in list_labels(self.config):
                if label in self.hide_labels_set:
                    continue
                yield sanitized_label
                self.getattr_cache[path + "/" + sanitized_label] = (time.time(), ("dir",))
        elif p.view == "Collages" and p.collage:
            releases = list(list_collage_releases(self.config, p.collage))
            pad_size = max(len(str(r[0])) for r in releases)
            for idx, virtual_dirname, source_dir in releases:
                v = f"{str(idx).zfill(pad_size)}. {virtual_dirname}"
                yield v
                self.getattr_cache[path + "/" + v] = (time.time(), ("dir", source_dir))
        elif p.view == "Collages":
            # Don't need to sanitize because the collage names come from filenames.
            for collage in list_collages(self.config):
                yield collage
                self.getattr_cache[path + "/" + collage] = (time.time(), ("dir",))
        else:
            raise fuse.FuseOSError(errno.ENOENT)

    def open(self, path: str, flags: int) -> int:
        logger.debug(f"Received open for {path=} {flags=}")
        p = parse_virtual_path(path)
        logger.debug(f"Parsed open path as {p}")

        if p.release and p.file:
            cachedata = get_release(self.config, p.release)
            if cachedata:
                release, tracks = cachedata
                if release.cover_image_path and p.file == release.cover_image_path.name:
                    return os.open(str(release.cover_image_path), flags)
                for track in tracks:
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
                cachedata = get_release(self.config, p.release)
                if cachedata:
                    release, tracks = cachedata
                    if release.cover_image_path and p.file == release.cover_image_path.name:
                        os.truncate(str(release.cover_image_path), length)
                    for track in tracks:
                        if track.virtual_filename == p.file:
                            return os.truncate(str(track.source_path), length)

    def release(self, path: str, fh: int) -> None:
        logger.debug(f"Received release for {path=} {fh=}")
        os.close(fh)

    def mkdir(self, path: str, mode: int) -> None:
        logger.debug(f"Received mkdir for {path=} {mode=}")
        p = parse_virtual_path(path)
        logger.debug(f"Parsed mkdir path as {p}")

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
        logger.debug(f"Parsed rmdir path as {p}")

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
        logger.debug(f"Parsed rename old path as {op}")
        np = parse_virtual_path(new)
        logger.debug(f"Parsed rename new path as {np}")

        # Possible actions:
        # 1. Rename a collage.
        # 2. Toggle a release's new status.
        if (
            (op.release and np.release)
            and op.release.removeprefix("{NEW} ") == np.release.removeprefix("{NEW} ")
            and (not op.file and not np.file)
        ):
            if op.release.startswith("{NEW} ") != np.release.startswith("{NEW} "):
                toggle_release_new(self.config, op.release)
            else:
                raise fuse.FuseOSError(errno.EACCES)
        elif op.view == "Collages" and np.view == "Collages":
            if (
                (op.collage and np.collage)
                and op.collage != np.collage
                and (not op.release and not np.release)
            ):
                rename_collage(self.config, op.collage, np.collage)
            else:
                raise fuse.FuseOSError(errno.EACCES)
        else:
            raise fuse.FuseOSError(errno.EACCES)
        # TODO: Consider allowing renaming artist/genre/label here?

    # Unimplemented:
    # - readlink
    # - mknod
    # - unlink
    # - symlink
    # - link
    # - opendir
    # - releasedir
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
    view: Literal[
        "Root",
        "Releases",
        "Artists",
        "Genres",
        "Labels",
        "Collages",
        "New",
        "Recently Added",
    ] | None
    artist: str | None = None
    genre: str | None = None
    label: str | None = None
    collage: str | None = None
    release: str | None = None
    file: str | None = None


# In collages, we print directories with position of the release in the collage. When parsing,
# strip it out. Otherwise we will have to handle this parsing in every method.
POSITION_REGEX = re.compile(r"^\d+\. ")
# In recently added, we print the date that the release was added to the library. When parsing,
# strip it out.
ADDED_AT_REGEX = re.compile(r"^\[[\d-]{10}\] ")


def parse_virtual_path(path: str) -> ParsedPath:
    parts = path.split("/")[1:]  # First part is always empty string.

    if len(parts) == 1 and parts[0] == "":
        return ParsedPath(view="Root")

    if parts[0] == "1. Releases":
        if len(parts) == 1:
            return ParsedPath(view="Releases")
        if len(parts) == 2:
            return ParsedPath(view="Releases", release=parts[1])
        if len(parts) == 3:
            return ParsedPath(view="Releases", release=parts[1], file=parts[2])
        raise fuse.FuseOSError(errno.ENOENT)

    if parts[0] == "2. Releases - New":
        if len(parts) == 1:
            return ParsedPath(view="New")
        if len(parts) == 2 and parts[1].startswith("{NEW} "):
            return ParsedPath(view="New", release=parts[1])
        if len(parts) == 3 and parts[1].startswith("{NEW} "):
            return ParsedPath(view="New", release=parts[1], file=parts[2])
        raise fuse.FuseOSError(errno.ENOENT)

    if parts[0] == "3. Releases - Recently Added":
        if len(parts) == 1:
            return ParsedPath(view="Recently Added")
        if len(parts) == 2 and ADDED_AT_REGEX.match(parts[1]):
            return ParsedPath(view="Recently Added", release=ADDED_AT_REGEX.sub("", parts[1]))
        if len(parts) == 3 and ADDED_AT_REGEX.match(parts[1]):
            return ParsedPath(
                view="Recently Added",
                release=ADDED_AT_REGEX.sub("", parts[1]),
                file=parts[2],
            )
        raise fuse.FuseOSError(errno.ENOENT)

    if parts[0] == "4. Artists":
        if len(parts) == 1:
            return ParsedPath(view="Artists")
        if len(parts) == 2:
            return ParsedPath(view="Artists", artist=parts[1])
        if len(parts) == 3:
            return ParsedPath(view="Artists", artist=parts[1], release=parts[2])
        if len(parts) == 4:
            return ParsedPath(view="Artists", artist=parts[1], release=parts[2], file=parts[3])
        raise fuse.FuseOSError(errno.ENOENT)

    if parts[0] == "5. Genres":
        if len(parts) == 1:
            return ParsedPath(view="Genres")
        if len(parts) == 2:
            return ParsedPath(view="Genres", genre=parts[1])
        if len(parts) == 3:
            return ParsedPath(view="Genres", genre=parts[1], release=parts[2])
        if len(parts) == 4:
            return ParsedPath(view="Genres", genre=parts[1], release=parts[2], file=parts[3])
        raise fuse.FuseOSError(errno.ENOENT)

    if parts[0] == "6. Labels":
        if len(parts) == 1:
            return ParsedPath(view="Labels")
        if len(parts) == 2:
            return ParsedPath(view="Labels", label=parts[1])
        if len(parts) == 3:
            return ParsedPath(view="Labels", label=parts[1], release=parts[2])
        if len(parts) == 4:
            return ParsedPath(view="Labels", label=parts[1], release=parts[2], file=parts[3])
        raise fuse.FuseOSError(errno.ENOENT)

    if parts[0] == "7. Collages":
        if len(parts) == 1:
            return ParsedPath(view="Collages")
        if len(parts) == 2:
            return ParsedPath(view="Collages", collage=parts[1])
        if len(parts) == 3:
            return ParsedPath(
                view="Collages", collage=parts[1], release=POSITION_REGEX.sub("", parts[2])
            )
        if len(parts) == 4:
            return ParsedPath(
                view="Collages",
                collage=parts[1],
                release=POSITION_REGEX.sub("", parts[2]),
                file=parts[3],
            )
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
