"""
The cache module encapsulates the read cache and exposes handles for working with the read cache. It
also exposes a locking mechanism that uses the read cache for synchronization.

The SQLite database is considered part of the cache, and so this module encapsulates the SQLite
database too. Though we cheap out a bit, so all the tests freely read from the SQLite database. No
budget!

The read cache is crucial to Rose. See `docs/CACHE_MAINTENANCE.md` for more information.

We consider a few problems in the cache update, whose solutions contribute to
the overall complexity of the cache update sequence:

1. **Arbitrary renames:** Files and directories can be arbitrarily renamed in between cache scans.
   We solve for these renames by writing [Stable Identifiers](#release-track-identifiers) to disk.
   For performance, however, a track update ends up as a delete followed by an insert with the
   just-deleted ID.
2. **In-progress directory creation:** We may come across a directory while it is in the process of
   being created. For example, due to `cp -r`. Unless --force is passed, we skip directories that
   lack a `.rose.{uuid}.toml` file, yet have a `Release ID` written syncthing synchronization.
3. **Performance:** We want to minimize file accesses, so we cache heavily and batch operations
   together. This creates a lot of intermediate state that we accumulate throughout the cache
   update.
"""

from __future__ import annotations

import contextlib
import copy
import dataclasses
import hashlib
import json
import logging
import math
import multiprocessing
import os
import os.path
import re
import sqlite3
import time
from collections import Counter, defaultdict
from collections.abc import Iterator
from dataclasses import dataclass
from datetime import datetime
from hashlib import sha256
from pathlib import Path
from typing import Any, TypeVar

import tomli_w
import tomllib
import uuid6

from rose.audiotags import SUPPORTED_AUDIO_EXTENSIONS, AudioTags, RoseDate
from rose.common import (
    VERSION,
    Artist,
    ArtistMapping,
    sanitize_dirname,
    sanitize_filename,
    sha256_dataclass,
    uniq,
)
from rose.config import Config
from rose.genre_hierarchy import TRANSIENT_CHILD_GENRES, TRANSIENT_PARENT_GENRES
from rose.templates import artistsfmt, eval_release_template, eval_track_template

logger = logging.getLogger(__name__)

T = TypeVar("T")

CACHE_SCHEMA_PATH = Path(__file__).resolve().parent / "cache.sql"


@contextlib.contextmanager
def connect(c: Config) -> Iterator[sqlite3.Connection]:
    conn = sqlite3.connect(
        c.cache_database_path,
        detect_types=sqlite3.PARSE_DECLTYPES,
        isolation_level=None,
        timeout=15.0,
    )
    try:
        conn.row_factory = sqlite3.Row
        conn.execute("PRAGMA foreign_keys=ON")
        conn.execute("PRAGMA journal_mode=WAL")
        yield conn
    finally:
        if conn:
            conn.close()


def maybe_invalidate_cache_database(c: Config) -> None:
    """
    "Migrate" the database. If the schema in the database does not match that on disk, then nuke the
    database and recreate it from scratch. Otherwise, no op.

    We can do this because the database is just a read cache. It is not source-of-truth for any of
    its own data.
    """
    with CACHE_SCHEMA_PATH.open("rb") as fp:
        schema_hash = hashlib.sha256(fp.read()).hexdigest()

    # Hash a subset of the config fields to use as the cache hash, which invalidates the cache on
    # change. These are the fields that affect cache population. Invalidating the cache on config
    # change ensures that the cache is consistent with the config.
    config_hash_fields = {
        "music_source_dir": str(c.music_source_dir),
        "cache_dir": str(c.cache_dir),
        "cover_art_stems": c.cover_art_stems,
        "valid_art_exts": c.valid_art_exts,
        "ignore_release_directories": c.ignore_release_directories,
    }
    config_hash = sha256(json.dumps(config_hash_fields).encode()).hexdigest()

    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT EXISTS(
                SELECT * FROM sqlite_master
                WHERE type = 'table' AND name = '_schema_hash'
            )
            """
        )
        if cursor.fetchone()[0]:
            cursor = conn.execute("SELECT schema_hash, config_hash, version FROM _schema_hash")
            row = cursor.fetchone()
            if (
                row
                and row["schema_hash"] == schema_hash
                and row["config_hash"] == config_hash
                and row["version"] == VERSION
            ):
                # Everything matches! Exit!
                return

    c.cache_database_path.unlink(missing_ok=True)
    with connect(c) as conn:
        with CACHE_SCHEMA_PATH.open("r") as fp:
            conn.executescript(fp.read())
        conn.execute(
            """
            CREATE TABLE _schema_hash (
                schema_hash TEXT
              , config_hash TEXT
              , version TEXT
              , PRIMARY KEY (schema_hash, config_hash, version)
            )
            """
        )
        conn.execute(
            "INSERT INTO _schema_hash (schema_hash, config_hash, version) VALUES (?, ?, ?)",
            (schema_hash, config_hash, VERSION),
        )


@contextlib.contextmanager
def lock(c: Config, name: str, timeout: float = 1.0) -> Iterator[None]:
    try:
        while True:
            with connect(c) as conn:
                cursor = conn.execute("SELECT MAX(valid_until) FROM locks WHERE name = ?", (name,))
                row = cursor.fetchone()
                # If a lock exists, sleep until the lock is available. All locks should be very
                # short lived, so this shouldn't be a big performance penalty.
                if row and row[0] and row[0] > time.time():
                    sleep = max(0, row[0] - time.time())
                    logger.debug(f"Failed to acquire lock for {name}: sleeping for {sleep}")
                    time.sleep(sleep)
                    continue
                logger.debug(f"Attempting to acquire lock for {name} with timeout {timeout}")
                valid_until = time.time() + timeout
                try:
                    conn.execute(
                        "INSERT INTO locks (name, valid_until) VALUES (?, ?)", (name, valid_until)
                    )
                except sqlite3.IntegrityError as e:
                    logger.debug(f"Failed to acquire lock for {name}, trying again: {e}")
                    continue
                logger.debug(
                    f"Successfully acquired lock for {name} with timeout {timeout} "
                    f"until {valid_until}"
                )
                break
        yield
    finally:
        logger.debug(f"Releasing lock {name}")
        with connect(c) as conn:
            conn.execute("DELETE FROM locks WHERE name = ?", (name,))


def release_lock_name(release_id: str) -> str:
    return f"release-{release_id}"


def collage_lock_name(collage_name: str) -> str:
    return f"collage-{collage_name}"


def playlist_lock_name(playlist_name: str) -> str:
    return f"playlist-{playlist_name}"


@dataclass(slots=True)
class CachedRelease:
    id: str
    source_path: Path
    cover_image_path: Path | None
    added_at: str  # ISO8601 timestamp
    datafile_mtime: str
    releasetitle: str
    releasetype: str
    releasedate: RoseDate | None
    originaldate: RoseDate | None
    compositiondate: RoseDate | None
    edition: str | None
    catalognumber: str | None
    new: bool
    disctotal: int
    genres: list[str]
    parent_genres: list[str]
    secondary_genres: list[str]
    parent_secondary_genres: list[str]
    descriptors: list[str]
    labels: list[str]
    releaseartists: ArtistMapping
    metahash: str

    @classmethod
    def from_view(cls, c: Config, row: dict[str, Any], aliases: bool = True) -> CachedRelease:
        secondary_genres = _split(row["secondary_genres"]) if row["secondary_genres"] else []
        genres = _split(row["genres"]) if row["genres"] else []
        return CachedRelease(
            id=row["id"],
            source_path=Path(row["source_path"]),
            cover_image_path=Path(row["cover_image_path"]) if row["cover_image_path"] else None,
            added_at=row["added_at"],
            datafile_mtime=row["datafile_mtime"],
            releasetitle=row["releasetitle"],
            releasetype=row["releasetype"],
            releasedate=RoseDate.parse(row["releasedate"]),
            originaldate=RoseDate.parse(row["originaldate"]),
            compositiondate=RoseDate.parse(row["compositiondate"]),
            catalognumber=row["catalognumber"],
            edition=row["edition"],
            disctotal=row["disctotal"],
            new=bool(row["new"]),
            genres=genres,
            secondary_genres=secondary_genres,
            parent_genres=_get_parent_genres(genres),
            parent_secondary_genres=_get_parent_genres(secondary_genres),
            descriptors=_split(row["descriptors"]) if row["descriptors"] else [],
            labels=_split(row["labels"]) if row["labels"] else [],
            releaseartists=_unpack_artists(
                c, row["releaseartist_names"], row["releaseartist_roles"], aliases=aliases
            ),
            metahash=row["metahash"],
        )

    def dump(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "source_path": str(self.source_path.resolve()),
            "cover_image_path": str(self.cover_image_path.resolve())
            if self.cover_image_path
            else None,
            "added_at": self.added_at,
            "releasetitle": self.releasetitle,
            "releasetype": self.releasetype,
            "releasedate": str(self.releasedate) if self.releasedate else None,
            "originaldate": str(self.originaldate) if self.originaldate else None,
            "compositiondate": str(self.compositiondate) if self.compositiondate else None,
            "catalognumber": self.catalognumber,
            "edition": self.edition,
            "new": self.new,
            "disctotal": self.disctotal,
            "genres": self.genres,
            "parent_genres": self.parent_genres,
            "secondary_genres": self.secondary_genres,
            "parent_secondary_genres": self.parent_secondary_genres,
            "descriptors": self.descriptors,
            "labels": self.labels,
            "releaseartists": self.releaseartists.dump(),
        }


@dataclass(slots=True)
class CachedTrack:
    id: str
    source_path: Path
    source_mtime: str
    tracktitle: str
    tracknumber: str
    tracktotal: int
    discnumber: str
    duration_seconds: int
    trackartists: ArtistMapping
    metahash: str

    release: CachedRelease

    @classmethod
    def from_view(
        cls,
        c: Config,
        row: dict[str, Any],
        release: CachedRelease,
        aliases: bool = True,
    ) -> CachedTrack:
        return CachedTrack(
            id=row["id"],
            source_path=Path(row["source_path"]),
            source_mtime=row["source_mtime"],
            tracktitle=row["tracktitle"],
            tracknumber=row["tracknumber"],
            tracktotal=row["tracktotal"],
            discnumber=row["discnumber"],
            duration_seconds=row["duration_seconds"],
            trackartists=_unpack_artists(
                c,
                row["trackartist_names"],
                row["trackartist_roles"],
                aliases=aliases,
            ),
            metahash=row["metahash"],
            release=release,
        )

    def dump(self, with_release_info: bool = True) -> dict[str, Any]:
        r = {
            "id": self.id,
            "source_path": str(self.source_path.resolve()),
            "tracktitle": self.tracktitle,
            "tracknumber": self.tracknumber,
            "tracktotal": self.tracktotal,
            "discnumber": self.discnumber,
            "duration_seconds": self.duration_seconds,
            "trackartists": self.trackartists.dump(),
        }
        if with_release_info:
            r.update(
                {
                    "release_id": self.release.id,
                    "added_at": self.release.added_at,
                    "releasetitle": self.release.releasetitle,
                    "releasetype": self.release.releasetype,
                    "disctotal": self.release.disctotal,
                    "releasedate": str(self.release.releasedate)
                    if self.release.releasedate
                    else None,
                    "originaldate": str(self.release.originaldate)
                    if self.release.originaldate
                    else None,
                    "compositiondate": str(self.release.compositiondate)
                    if self.release.compositiondate
                    else None,
                    "catalognumber": self.release.catalognumber,
                    "edition": self.release.edition,
                    "new": self.release.new,
                    "genres": self.release.genres,
                    "parent_genres": self.release.parent_genres,
                    "secondary_genres": self.release.secondary_genres,
                    "parent_secondary_genres": self.release.parent_secondary_genres,
                    "descriptors": self.release.descriptors,
                    "labels": self.release.labels,
                    "releaseartists": self.release.releaseartists.dump(),
                }
            )
        return r


@dataclass(slots=True)
class CachedCollage:
    name: str
    source_mtime: str
    release_ids: list[str]


@dataclass(slots=True)
class CachedPlaylist:
    name: str
    source_mtime: str
    cover_path: Path | None
    track_ids: list[str]


@dataclass(slots=True)
class StoredDataFile:
    new: bool
    added_at: str  # ISO8601 timestamp


STORED_DATA_FILE_REGEX = re.compile(r"\.rose\.([^.]+)\.toml")


def update_cache(
    c: Config,
    force: bool = False,
    # For testing.
    force_multiprocessing: bool = False,
) -> None:
    """
    Update the read cache to match the data for all releases in the music source directory. Delete
    any cached releases that are no longer present on disk.
    """
    update_cache_for_releases(c, None, force, force_multiprocessing=force_multiprocessing)
    update_cache_evict_nonexistent_releases(c)
    update_cache_for_collages(c, None, force)
    update_cache_evict_nonexistent_collages(c)
    update_cache_for_playlists(c, None, force)
    update_cache_evict_nonexistent_playlists(c)


def update_cache_evict_nonexistent_releases(c: Config) -> None:
    logger.debug("Evicting cached releases that are not on disk")
    dirs = [Path(d.path).resolve() for d in os.scandir(c.music_source_dir) if d.is_dir()]
    with connect(c) as conn:
        cursor = conn.execute(
            f"""
            DELETE FROM releases
            WHERE source_path NOT IN ({",".join(["?"] * len(dirs))})
            RETURNING source_path
            """,
            [str(d) for d in dirs],
        )
        for row in cursor:
            logger.info(f"Evicted missing release {row['source_path']} from cache")


def update_cache_for_releases(
    c: Config,
    # Leave as None to update all releases.
    release_dirs: list[Path] | None = None,
    force: bool = False,
    # For testing.
    force_multiprocessing: bool = False,
) -> None:
    """
    Update the read cache to match the data for any passed-in releases. If a directory lacks a
    .rose.{uuid}.toml datafile, create the datafile for the release and set it to the initial state.

    This is a hot path and is thus performance-optimized. The bottleneck is disk accesses, so we
    structure this function in order to minimize them. We solely read files that have changed since
    last run and batch writes together. We trade higher memory for reduced disk accesses.
    Concretely, we:

    1. Execute one big SQL query at the start to fetch the relevant previous caches.
    2. Skip reading a file's data if the mtime has not changed since the previous cache update.
    3. Batch SQLite write operations to the end of this function, and only execute a SQLite upsert
       if the read data differs from the previous caches.

    We also shard the directories across multiple processes and execute them simultaneously.
    """
    release_dirs = release_dirs or [
        Path(d.path) for d in os.scandir(c.music_source_dir) if d.is_dir()
    ]
    release_dirs = [
        d
        for d in release_dirs
        if d.name != "!collages"
        and d.name != "!playlists"
        and d.name not in c.ignore_release_directories
    ]
    if not release_dirs:
        logger.debug("No-Op: No whitelisted releases passed into update_cache_for_releases")
        return
    logger.debug(f"Refreshing the read cache for {len(release_dirs)} releases")
    if len(release_dirs) < 10:
        logger.debug(f"Refreshing cached data for {', '.join([r.name for r in release_dirs])}")

    # If the number of releases changed is less than 50; do not bother with all that multiprocessing
    # gunk: instead, directly call the executor.
    #
    # This has an added benefit of not spawning processes from the virtual filesystem and watchdog
    # processes, as those processes always update the cache for one release at a time and are
    # multithreaded. Starting other processes from threads is bad!
    if not force_multiprocessing and len(release_dirs) < 50:
        logger.debug(
            f"Running cache update executor in same process because {len(release_dirs)=} < 50"
        )
        _update_cache_for_releases_executor(c, release_dirs, force)
        return

    # Batch size defaults to equal split across all processes. However, if the number of directories
    # is small, we shrink the # of processes to save on overhead.
    num_proc = c.max_proc
    if len(release_dirs) < c.max_proc * 50:
        num_proc = max(1, math.ceil(len(release_dirs) // 50))
    batch_size = len(release_dirs) // num_proc + 1

    manager = multiprocessing.Manager()
    # Have each process propagate the collages and playlists it wants to update back upwards. We
    # will dispatch the force updater only once in the main process, instead of many times in each
    # process.
    collages_to_force_update = manager.list()
    playlists_to_force_update = manager.list()

    errors: list[BaseException] = []

    logger.debug("Creating multiprocessing pool to parallelize cache executors.")
    with multiprocessing.Pool(processes=c.max_proc) as pool:
        # At 0, no batch. At 1, 1 batch. At 49, 1 batch. At 50, 1 batch. At 51, 2 batches.
        for i in range(0, len(release_dirs), batch_size):
            logger.debug(
                f"Spawning release cache update process for releases [{i}, {i+batch_size})"
            )
            pool.apply_async(
                _update_cache_for_releases_executor,
                (
                    c,
                    release_dirs[i : i + batch_size],
                    force,
                    collages_to_force_update,
                    playlists_to_force_update,
                ),
                error_callback=lambda e: errors.append(e),
            )
        pool.close()
        pool.join()

    if errors:
        raise ExceptionGroup("Exception occurred in cache update subprocesses", errors)  # type: ignore

    if collages_to_force_update:
        update_cache_for_collages(c, uniq(list(collages_to_force_update)), force=True)
    if playlists_to_force_update:
        update_cache_for_playlists(c, uniq(list(playlists_to_force_update)), force=True)


def _update_cache_for_releases_executor(
    c: Config,
    release_dirs: list[Path],
    force: bool,
    # If these are not None, we will store the collages and playlists to update in here instead of
    # invoking the update functions directly. If these are None, we will not put anything in them
    # and instead invoke update_cache_for_{collages,playlists} directly. This is a Bad Pattern, but
    # good enough.
    collages_to_force_update_receiver: list[str] | None = None,
    playlists_to_force_update_receiver: list[str] | None = None,
) -> None:
    """The implementation logic, split out for multiprocessing."""
    # First, call readdir on every release directory. We store the results in a map of
    # Path Basename -> (Release ID if exists, filenames).
    dir_scan_start = time.time()
    dir_tree: list[tuple[Path, str | None, list[Path]]] = []
    release_uuids: list[str] = []
    for rd in release_dirs:
        release_id = None
        files: list[Path] = []
        if not rd.is_dir():
            logger.debug(f"Skipping scanning {rd} because it is not a directory")
            continue
        for root, _, subfiles in os.walk(str(rd)):
            for sf in subfiles:
                if m := STORED_DATA_FILE_REGEX.match(sf):
                    release_id = m[1]
                files.append(Path(root) / sf)
        dir_tree.append((rd.resolve(), release_id, files))
        if release_id is not None:
            release_uuids.append(release_id)
    logger.debug(f"Release update source dir scan time {time.time() - dir_scan_start=}")

    cache_read_start = time.time()
    # Then batch query for all metadata associated with the discovered IDs. This pulls all data into
    # memory for fast access throughout this function. We do this in two passes (and two queries!):
    # 1. Fetch all releases.
    # 2. Fetch all tracks in a single query, and then associates each track with a release.
    # The tracks are stored as a dict of source_path -> Track.
    cached_releases: dict[str, tuple[CachedRelease, dict[str, CachedTrack]]] = {}
    with connect(c) as conn:
        cursor = conn.execute(
            rf"""
            SELECT *
            FROM releases_view
            WHERE id IN ({','.join(['?']*len(release_uuids))})
            """,
            release_uuids,
        )
        for row in cursor:
            cached_releases[row["id"]] = (CachedRelease.from_view(c, row, aliases=False), {})

        logger.debug(f"Found {len(cached_releases)}/{len(release_dirs)} releases in cache")

        cursor = conn.execute(
            rf"""
            SELECT *
            FROM tracks_view
            WHERE release_id IN ({','.join(['?']*len(release_uuids))})
            """,
            release_uuids,
        )
        num_tracks_found = 0
        for row in cursor:
            cached_releases[row["release_id"]][1][row["source_path"]] = CachedTrack.from_view(
                c,
                row,
                cached_releases[row["release_id"]][0],
                aliases=False,
            )
            num_tracks_found += 1
        logger.debug(f"Found {num_tracks_found} tracks in cache")
    logger.debug(f"Release update cache read time {time.time() - cache_read_start=}")

    # Now iterate over all releases in the source directory. Leverage mtime from stat to determine
    # whether to even check the file tags or not. Compute the necessary database updates and store
    # them in the `upd_` variables. After this loop, we will execute the database updates based on
    # the `upd_` varaibles.
    loop_start = time.time()
    upd_delete_source_paths: list[str] = []
    upd_release_args: list[list[Any]] = []
    upd_release_ids: list[str] = []
    upd_release_artist_args: list[list[Any]] = []
    upd_release_genre_args: list[list[Any]] = []
    upd_release_secondary_genre_args: list[list[Any]] = []
    upd_release_descriptor_args: list[list[Any]] = []
    upd_release_label_args: list[list[Any]] = []
    upd_unknown_cached_tracks_args: list[tuple[str, list[str]]] = []
    upd_track_args: list[list[Any]] = []
    upd_track_ids: list[str] = []
    upd_track_artist_args: list[list[Any]] = []
    for source_path, preexisting_release_id, files in dir_tree:
        logger.debug(f"Scanning release {source_path.name}")
        # Check to see if we should even process the directory. If the directory does not have
        # any tracks, skip it. And if it does not have any tracks, but is in the cache, remove
        # it from the cache.
        first_audio_file: Path | None = None
        for f in files:
            if f.suffix.lower() in SUPPORTED_AUDIO_EXTENSIONS:
                first_audio_file = f
                break
        else:
            logger.debug(f"Did not find any audio files in release {source_path}, skipping")
            logger.debug(f"Scheduling cache deletion for empty directory release {source_path}")
            upd_delete_source_paths.append(str(source_path))
            continue
        assert first_audio_file is not None

        # This value is used to track whether to update the database for this release. If this
        # is False at the end of this loop body, we can save a database update call.
        release_dirty = False

        # Fetch the release from the cache. We will be updating this value on-the-fly, so
        # instantiate to zero values if we do not have a default value.
        try:
            release, cached_tracks = cached_releases[preexisting_release_id or ""]
        except KeyError:
            logger.debug(
                f"First-time unidentified release found at release {source_path}, writing UUID and new"
            )
            release_dirty = True
            release = CachedRelease(
                id=preexisting_release_id or "",
                source_path=source_path,
                datafile_mtime="",
                cover_image_path=None,
                added_at="",
                releasetitle="",
                releasetype="",
                releasedate=None,
                originaldate=None,
                compositiondate=None,
                catalognumber=None,
                edition=None,
                new=True,
                disctotal=0,
                genres=[],
                parent_genres=[],
                secondary_genres=[],
                parent_secondary_genres=[],
                descriptors=[],
                labels=[],
                releaseartists=ArtistMapping(),
                metahash="",
            )
            cached_tracks = {}

        # Handle source path change; if it's changed, update the release.
        if source_path != release.source_path:
            logger.debug(f"Source path change detected for release {source_path}, updating")
            release.source_path = source_path
            release_dirty = True

        # The directory does not have a release ID, so create the stored data file. Also, in case
        # the directory changes mid-scan, wrap this in an error handler.
        try:
            if not preexisting_release_id:
                # However, skip this directory for a special case. Because directory copying/movement is
                # not atomic, we may read a directory in a in-progres creation state. If:
                #
                # 1. The directory lacks a `.rose.{uuid}.toml` file, but the files have Rose IDs,
                # 2. And the directory mtime is less than 3 seconds ago,
                #
                # We consider the directory to be in a in-progress creation state. And so we do not
                # process the directory at this time.
                release_id_from_first_file = None
                with contextlib.suppress(Exception):
                    release_id_from_first_file = AudioTags.from_file(first_audio_file).release_id
                if release_id_from_first_file and not force:
                    logger.warning(
                        f"No-Op: Skipping release at {source_path}: files in release already have "
                        f"release_id {release_id_from_first_file}, but .rose.{{uuid}}.toml is missing, "
                        "is another tool in the middle of writing the directory? Run with --force to "
                        "recreate .rose.{uuid}.toml"
                    )
                    continue

                logger.debug(f"Creating new stored data file for release {source_path}")
                stored_release_data = StoredDataFile(
                    new=True,
                    added_at=datetime.now().astimezone().replace(microsecond=0).isoformat(),
                )
                # Preserve the release ID already present the first file if we can.
                new_release_id = release_id_from_first_file or str(uuid6.uuid7())
                datafile_path = source_path / f".rose.{new_release_id}.toml"
                # No need to lock here, as since the release ID is new, there is no way there is a
                # concurrent writer.
                with datafile_path.open("wb") as fp:
                    tomli_w.dump(dataclasses.asdict(stored_release_data), fp)
                release.id = new_release_id
                release.new = stored_release_data.new
                release.added_at = stored_release_data.added_at
                release.datafile_mtime = str(os.stat(datafile_path).st_mtime)
                release_dirty = True
            else:
                # Otherwise, check to see if the mtime changed from what we know. If it has, read
                # from the datafile.
                datafile_path = source_path / f".rose.{preexisting_release_id}.toml"
                datafile_mtime = str(os.stat(datafile_path).st_mtime)
                if datafile_mtime != release.datafile_mtime or force:
                    logger.debug(f"Datafile changed for release {source_path}, updating")
                    release_dirty = True
                    release.datafile_mtime = datafile_mtime
                    # For performance reasons (!!), don't acquire a lock here. However, acquire a lock
                    # if we are to write to the file. We won't worry about lost writes here.
                    with datafile_path.open("rb") as fp:
                        diskdata = tomllib.load(fp)
                    datafile = StoredDataFile(
                        new=diskdata.get("new", True),
                        added_at=diskdata.get(
                            "added_at",
                            datetime.now().astimezone().replace(microsecond=0).isoformat(),
                        ),
                    )
                    release.new = datafile.new
                    release.added_at = datafile.added_at
                    new_resolved_data = dataclasses.asdict(datafile)
                    logger.debug(f"Updating values in stored data file for release {source_path}")
                    if new_resolved_data != diskdata:
                        # And then write the data back to disk if it changed. This allows us to update
                        # datafiles to contain newer default values.
                        lockname = release_lock_name(preexisting_release_id)
                        with lock(c, lockname), datafile_path.open("wb") as fp:
                            tomli_w.dump(new_resolved_data, fp)
        except FileNotFoundError:
            logger.warning(f"Skipping update on {source_path}: directory no longer exists")
            continue

        # Handle cover art change.
        cover = None
        for f in files:
            if f.name.lower() in c.valid_cover_arts:
                cover = f
                break
        if cover != release.cover_image_path:
            logger.debug(f"Cover art file for release {source_path} updated to path {cover}")
            release.cover_image_path = cover
            release_dirty = True

        # Now we'll switch over to processing some of the tracks. We need track metadata in
        # order to calculate some fields of the release, so we'll first compute the valid set of
        # CachedTracks, and then we will finalize the release and execute any required database
        # operations for the release and tracks.

        # We want to know which cached tracks are no longer on disk. By the end of the following
        # loop, this set should only contain the such tracks, which will be deleted in the
        # database execution handling step.
        unknown_cached_tracks: set[str] = set(cached_tracks.keys())
        # Next, we will construct the list of tracks that are on the release. We will also
        # leverage mtimes and such to avoid unnecessary recomputations. If a release has changed
        # and should be updated in the database, we add its ID to track_ids_to_insert, which
        # will be used in the database execution step.
        tracks: list[CachedTrack] = []
        track_ids_to_insert: set[str] = set()
        # This value is set to true if we read an AudioTags and used it to confirm the release
        # tags.
        pulled_release_tags = False
        totals_ctr: dict[str, int] = Counter()
        for f in files:
            if f.suffix.lower() not in SUPPORTED_AUDIO_EXTENSIONS:
                continue

            cached_track = cached_tracks.get(str(f), None)
            with contextlib.suppress(KeyError):
                unknown_cached_tracks.remove(str(f))

            try:
                track_mtime = str(os.stat(f).st_mtime)
                # Skip re-read if we can reuse a cached entry.
                if cached_track and track_mtime == cached_track.source_mtime and not force:
                    logger.debug(
                        f"Track cache hit (mtime) for {os.path.basename(f)}, reusing cached data"
                    )
                    tracks.append(cached_track)
                    totals_ctr[cached_track.discnumber] += 1
                    continue

                # Otherwise, read tags from disk and construct a new cached_track.
                logger.debug(f"Track cache miss for {os.path.basename(f)}, reading tags from disk")
                tags = AudioTags.from_file(Path(f))
            except FileNotFoundError:
                logger.warning(
                    f"Skipping track update for {os.path.basename(f)}: file no longer exists"
                )
                continue

            # Now that we're here, pull the release tags. We also need them to compute the
            # formatted artist string.
            if not pulled_release_tags:
                pulled_release_tags = True
                release_title = tags.releasetitle or "Unknown Release"
                if release_title != release.releasetitle:
                    logger.debug(f"Release title change detected for {source_path}, updating")
                    release.releasetitle = release_title
                    release_dirty = True

                releasetype = tags.releasetype
                if releasetype != release.releasetype:
                    logger.debug(f"Release type change detected for {source_path}, updating")
                    release.releasetype = releasetype
                    release_dirty = True

                if tags.releasedate != release.releasedate:
                    logger.debug(f"Release year change detected for {source_path}, updating")
                    release.releasedate = tags.releasedate
                    release_dirty = True

                if tags.originaldate != release.originaldate:
                    logger.debug(
                        f"Release original year change detected for {source_path}, updating"
                    )
                    release.originaldate = tags.originaldate
                    release_dirty = True

                if tags.compositiondate != release.compositiondate:
                    logger.debug(
                        f"Release composition year change detected for {source_path}, updating"
                    )
                    release.compositiondate = tags.compositiondate
                    release_dirty = True

                if tags.edition != release.edition:
                    logger.debug(f"Release edition change detected for {source_path}, updating")
                    release.edition = tags.edition
                    release_dirty = True

                if tags.catalognumber != release.catalognumber:
                    logger.debug(
                        f"Release catalog number change detected for {source_path}, updating"
                    )
                    release.catalognumber = tags.catalognumber
                    release_dirty = True

                if tags.genre != release.genres:
                    logger.debug(f"Release genre change detected for {source_path}, updating")
                    release.genres = uniq(tags.genre)
                    release_dirty = True

                if tags.secondarygenre != release.secondary_genres:
                    logger.debug(
                        f"Release secondary genre change detected for {source_path}, updating"
                    )
                    release.secondary_genres = uniq(tags.secondarygenre)
                    release_dirty = True

                if tags.descriptor != release.descriptors:
                    logger.debug(f"Release descriptor change detected for {source_path}, updating")
                    release.descriptors = uniq(tags.descriptor)
                    release_dirty = True

                if tags.label != release.labels:
                    logger.debug(f"Release label change detected for {source_path}, updating")
                    release.labels = uniq(tags.label)
                    release_dirty = True

                if tags.releaseartists != release.releaseartists:
                    logger.debug(f"Release artists change detected for {source_path}, updating")
                    release.releaseartists = tags.releaseartists
                    release_dirty = True

            # Here we compute the track ID. We store the track ID on the audio file in order to
            # enable persistence. This does mutate the file!
            #
            # We don't attempt to optimize this write; however, there is not much purpose to doing
            # so, since this occurs once over the lifetime of the track's existence in Rose. We
            # optimize this function because it is called repeatedly upon every metadata edit, but
            # in this case, we skip this code path once an ID is generated.
            #
            # We also write the release ID to the tags. This is not needed in normal operations
            # (since we have .rose.{uuid}.toml!), but provides a layer of defense in situations like
            # a directory being written file-by-file and being processed in a half-written state.
            track_id = tags.id
            if not track_id or not tags.release_id or tags.release_id != release.id:
                # This is our first time reading this track in the system, so no cocurrent processes
                # should be reading/writing this file. We can avoid locking. And If we have two
                # concurrent first-time cache updates, other places will have issues too.
                tags.id = tags.id or str(uuid6.uuid7())
                tags.release_id = release.id
                try:
                    tags.flush()
                    # And refresh the mtime because we've just written to the file.
                    track_id = tags.id
                    track_mtime = str(os.stat(f).st_mtime)
                except FileNotFoundError:
                    logger.warning(
                        f"Skipping track update for {os.path.basename(f)}: file no longer exists"
                    )
                    continue

            # And now create the cached track.
            track = CachedTrack(
                id=track_id,
                source_path=Path(f),
                source_mtime=track_mtime,
                tracktitle=tags.tracktitle or "Unknown Title",
                # Remove `.` here because we use `.` to parse out discno/trackno in the virtual
                # filesystem. It should almost never happen, but better to be safe. We set the
                # totals on all tracks the end of the loop.
                tracknumber=(tags.tracknumber or "1").replace(".", ""),
                tracktotal=tags.tracktotal or 1,
                discnumber=(tags.discnumber or "1").replace(".", ""),
                # This is calculated with the virtual filename.
                duration_seconds=tags.duration_sec,
                trackartists=tags.trackartists,
                metahash="",
                release=release,
            )
            tracks.append(track)
            track_ids_to_insert.add(track.id)
            totals_ctr[track.discnumber] += 1

        # Now set the tracktotals and disctotals.
        disctotal = len(totals_ctr)
        if release.disctotal != disctotal:
            logger.debug(f"Release disctotal change detected for {release.source_path}, updating")
            release_dirty = True
            release.disctotal = disctotal
        for track in tracks:
            tracktotal = totals_ctr[track.discnumber]
            assert tracktotal != 0, "This track isn't in the counter, impossible!"
            if tracktotal != track.tracktotal:
                logger.debug(f"Track tracktotal change detected for {track.source_path}, updating")
                track.tracktotal = tracktotal
                track_ids_to_insert.add(track.id)

        # And now perform directory/file renames if configured.
        if c.rename_source_files:
            if release_dirty:
                wanted_dirname = eval_release_template(c.path_templates.source.release, release)
                wanted_dirname = sanitize_dirname(c, wanted_dirname, True)
                # Iterate until we've either:
                # 1. Realized that the name of the source path matches the desired dirname (which we
                #    may not realize immediately if there are name conflicts).
                # 2. Or renamed the source directory to match our desired name.
                original_wanted_dirname = wanted_dirname
                collision_no = 2
                while wanted_dirname != release.source_path.name:
                    new_source_path = release.source_path.with_name(wanted_dirname)
                    # If there is a collision, bump the collision counter and retry.
                    if new_source_path.exists():
                        new_max_len = c.max_filename_bytes - (3 + len(str(collision_no)))
                        wanted_dirname = f"{original_wanted_dirname[:new_max_len]} [{collision_no}]"
                        collision_no += 1
                        continue
                    # If no collision, rename the directory.
                    old_source_path = release.source_path
                    old_source_path.rename(new_source_path)
                    logger.info(
                        f"Renamed source release directory {old_source_path.name} to {new_source_path.name}"
                    )
                    release.source_path = new_source_path
                    # Update the cached cover image path.
                    if release.cover_image_path:
                        coverlocalpath = str(release.cover_image_path).removeprefix(
                            f"{old_source_path}/"
                        )
                        release.cover_image_path = release.source_path / coverlocalpath
                    # Update the cached track paths and schedule them for database insertions.
                    for track in tracks:
                        tracklocalpath = str(track.source_path).removeprefix(f"{old_source_path}/")
                        track.source_path = release.source_path / tracklocalpath
                        track.source_mtime = str(os.stat(track.source_path).st_mtime)
                        track_ids_to_insert.add(track.id)
            for track in [t for t in tracks if t.id in track_ids_to_insert]:
                wanted_filename = eval_track_template(c.path_templates.source.track, track)
                wanted_filename = sanitize_filename(c, wanted_filename, True)
                # And repeat a similar process to the release rename handling. Except: we can have
                # arbitrarily nested files here, so we need to compare more than the name.
                original_wanted_stem = Path(wanted_filename).stem
                original_wanted_suffix = Path(wanted_filename).suffix
                collision_no = 2
                while (
                    relpath := str(track.source_path).removeprefix(f"{release.source_path}/")
                ) and wanted_filename != relpath:
                    new_source_path = release.source_path / wanted_filename
                    if new_source_path.exists():
                        new_max_len = c.max_filename_bytes - (
                            3 + len(str(collision_no)) + len(original_wanted_suffix)
                        )
                        wanted_filename = f"{original_wanted_stem[:new_max_len]} [{collision_no}]{original_wanted_suffix}"
                        collision_no += 1
                        continue
                    old_source_path = track.source_path
                    old_source_path.rename(new_source_path)
                    track.source_path = new_source_path
                    track.source_mtime = str(os.stat(track.source_path).st_mtime)
                    logger.info(
                        f"Renamed source file {release.source_path.name}/{relpath} to {release.source_path.name}/{wanted_filename}"
                    )
                    # And clean out any empty directories post-rename.
                    while relpath := os.path.dirname(relpath):
                        relppp = release.source_path / relpath
                        if relppp.is_dir() and not list(relppp.iterdir()):
                            relppp.rmdir()

        # Schedule database executions.
        if unknown_cached_tracks or release_dirty or track_ids_to_insert:
            logger.info(f"Updating cache for release {release.source_path.name}")

        if unknown_cached_tracks:
            logger.debug(f"Deleting {len(unknown_cached_tracks)} unknown tracks from cache")
            upd_unknown_cached_tracks_args.append((release.id, list(unknown_cached_tracks)))

        if release_dirty:
            logger.debug(f"Scheduling upsert for dirty release in database: {release.source_path}")
            upd_release_args.append(
                [
                    release.id,
                    str(release.source_path),
                    str(release.cover_image_path) if release.cover_image_path else None,
                    release.added_at,
                    release.datafile_mtime,
                    release.releasetitle,
                    release.releasetype,
                    str(release.releasedate) if release.releasedate else None,
                    str(release.originaldate) if release.originaldate else None,
                    str(release.compositiondate) if release.compositiondate else None,
                    release.edition,
                    release.catalognumber,
                    release.disctotal,
                    release.new,
                    sha256_dataclass(release),
                ]
            )
            upd_release_ids.append(release.id)
            for pos, genre in enumerate(release.genres):
                upd_release_genre_args.append([release.id, genre, pos])
            for pos, genre in enumerate(release.secondary_genres):
                upd_release_secondary_genre_args.append([release.id, genre, pos])
            for pos, desc in enumerate(release.descriptors):
                upd_release_descriptor_args.append([release.id, desc, pos])
            for pos, label in enumerate(release.labels):
                upd_release_label_args.append([release.id, label, pos])
            pos = 0
            for role, artists in release.releaseartists.items():
                for art in artists:
                    upd_release_artist_args.append([release.id, art.name, role, pos])
                    pos += 1

        if track_ids_to_insert:
            for track in tracks:
                if track.id not in track_ids_to_insert:
                    continue
                logger.debug(f"Scheduling upsert for dirty track in database: {track.source_path}")
                upd_track_args.append(
                    [
                        track.id,
                        str(track.source_path),
                        track.source_mtime,
                        track.tracktitle,
                        track.release.id,
                        track.tracknumber,
                        track.tracktotal,
                        track.discnumber,
                        track.duration_seconds,
                        sha256_dataclass(track),
                    ]
                )
                upd_track_ids.append(track.id)
                pos = 0
                for role, artists in track.trackartists.items():
                    for art in artists:
                        upd_track_artist_args.append([track.id, art.name, role, pos])
                        pos += 1
    logger.debug(f"Release update scheduling loop time {time.time() - loop_start=}")

    exec_start = time.time()
    # During execution, identify the collages and playlists to update afterwards. We will invoke an
    # update for those collages and playlists with force=True after updating the release tables.
    update_collages = None
    update_playlists = None
    with connect(c) as conn:
        if upd_delete_source_paths:
            conn.execute(
                f"DELETE FROM releases WHERE source_path IN ({','.join(['?']*len(upd_delete_source_paths))})",
                upd_delete_source_paths,
            )
        if upd_unknown_cached_tracks_args:
            query = "DELETE FROM tracks WHERE false"
            args: list[Any] = []
            for release_id, utrks in upd_unknown_cached_tracks_args:
                query += f" OR (release_id = ? AND source_path IN ({','.join(['?']*len(utrks))}))"
                args.extend([release_id, *utrks])
            conn.execute(query, args)
        if upd_release_args:
            # The OR REPLACE handles source_path conflicts. The ON CONFLICT handles normal updates.
            conn.execute(
                f"""
                INSERT OR REPLACE INTO releases (
                    id
                  , source_path
                  , cover_image_path
                  , added_at
                  , datafile_mtime
                  , title
                  , releasetype
                  , releasedate
                  , originaldate
                  , compositiondate
                  , edition
                  , catalognumber
                  , disctotal
                  , new
                  , metahash
                ) VALUES {",".join(["(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"] * len(upd_release_args))}
                ON CONFLICT (id) DO UPDATE SET
                    source_path      = excluded.source_path
                  , cover_image_path = excluded.cover_image_path
                  , added_at         = excluded.added_at
                  , datafile_mtime   = excluded.datafile_mtime
                  , title            = excluded.title
                  , releasetype      = excluded.releasetype
                  , releasedate      = excluded.releasedate
                  , originaldate     = excluded.originaldate
                  , compositiondate  = excluded.compositiondate
                  , edition          = excluded.edition
                  , catalognumber    = excluded.catalognumber
                  , disctotal        = excluded.disctotal
                  , new              = excluded.new
                  , metahash         = excluded.metahash
               """,
                _flatten(upd_release_args),
            )
            conn.execute(
                f"""
                DELETE FROM releases_genres
                WHERE release_id IN ({",".join(["?"]*len(upd_release_args))})
                """,
                [a[0] for a in upd_release_args],
            )
            if upd_release_genre_args:
                conn.execute(
                    f"""
                    INSERT INTO releases_genres (release_id, genre, position)
                    VALUES {",".join(["(?,?,?)"]*len(upd_release_genre_args))}
                    """,
                    _flatten(upd_release_genre_args),
                )
            conn.execute(
                f"""
                DELETE FROM releases_secondary_genres
                WHERE release_id IN ({",".join(["?"]*len(upd_release_args))})
                """,
                [a[0] for a in upd_release_args],
            )
            if upd_release_secondary_genre_args:
                conn.execute(
                    f"""
                    INSERT INTO releases_secondary_genres (release_id, genre, position)
                    VALUES {",".join(["(?,?,?)"]*len(upd_release_secondary_genre_args))}
                    """,
                    _flatten(upd_release_secondary_genre_args),
                )
            conn.execute(
                f"""
                DELETE FROM releases_descriptors
                WHERE release_id IN ({",".join(["?"]*len(upd_release_args))})
                """,
                [a[0] for a in upd_release_args],
            )
            if upd_release_descriptor_args:
                conn.execute(
                    f"""
                    INSERT INTO releases_descriptors (release_id, descriptor, position)
                    VALUES {",".join(["(?,?,?)"]*len(upd_release_descriptor_args))}
                    """,
                    _flatten(upd_release_descriptor_args),
                )
            conn.execute(
                f"""
                DELETE FROM releases_labels
                WHERE release_id IN ({",".join(["?"]*len(upd_release_args))})
                """,
                [a[0] for a in upd_release_args],
            )
            if upd_release_label_args:
                conn.execute(
                    f"""
                    INSERT INTO releases_labels (release_id, label, position)
                    VALUES {",".join(["(?,?,?)"]*len(upd_release_label_args))}
                    """,
                    _flatten(upd_release_label_args),
                )
            conn.execute(
                f"""
                DELETE FROM releases_artists
                WHERE release_id IN ({",".join(["?"]*len(upd_release_args))})
                """,
                [a[0] for a in upd_release_args],
            )
            if upd_release_artist_args:
                conn.execute(
                    f"""
                    INSERT INTO releases_artists (release_id, artist, role, position)
                    VALUES {",".join(["(?,?,?,?)"]*len(upd_release_artist_args))}
                    """,
                    _flatten(upd_release_artist_args),
                )
        if upd_track_args:
            # The OR REPLACE handles source_path conflicts. The ON CONFLICT handles normal updates.
            conn.execute(
                f"""
                INSERT OR REPLACE INTO tracks (
                    id
                  , source_path
                  , source_mtime
                  , title
                  , release_id
                  , tracknumber
                  , tracktotal
                  , discnumber
                  , duration_seconds
                  , metahash
                )
                VALUES {",".join(["(?,?,?,?,?,?,?,?,?,?)"]*len(upd_track_args))}
                ON CONFLICT (id) DO UPDATE SET
                    source_path                = excluded.source_path
                  , source_mtime               = excluded.source_mtime
                  , title                      = excluded.title
                  , release_id                 = excluded.release_id
                  , tracknumber                = excluded.tracknumber
                  , tracktotal                 = excluded.tracktotal
                  , discnumber                 = excluded.discnumber
                  , duration_seconds           = excluded.duration_seconds
                  , metahash                   = excluded.metahash
                """,
                _flatten(upd_track_args),
            )
        if upd_track_artist_args:
            conn.execute(
                f"""
                DELETE FROM tracks_artists
                WHERE track_id IN ({",".join(["?"]*len(upd_track_artist_args))})
                """,
                [a[0] for a in upd_track_artist_args],
            )
            conn.execute(
                f"""
                INSERT INTO tracks_artists (track_id, artist, role, position)
                VALUES {",".join(["(?,?,?,?)"]*len(upd_track_artist_args))}
                """,
                _flatten(upd_track_artist_args),
            )
        # And update the full text search engine here for any tracks and releases that have been
        # affected. Note that we do not worry about cleaning out deleted releases and tracks from
        # the full text search engine, since we join against tracks at the use site, which filters
        # out deleted tracks/releases from the full text search engine. Furthermore, the cache is
        # full-nuked often enough that there should not be much space waste.
        if upd_release_ids or upd_track_ids:
            conn.execute(
                f"""
                DELETE FROM rules_engine_fts WHERE rowid IN (
                    SELECT t.rowid
                    FROM tracks t
                    JOIN releases r ON r.id = t.release_id
                    WHERE t.id IN ({",".join(["?"]*len(upd_track_ids))})
                       OR r.id IN ({",".join(["?"]*len(upd_release_ids))})
               )
                """,
                [*upd_track_ids, *upd_release_ids],
            )
            # That cool section breaker shuriken character is our multi-value delimiter and how we
            # force-match strict prefix/suffix.
            conn.create_function("process_string_for_fts", 1, process_string_for_fts)
            conn.execute(
                f"""
                INSERT INTO rules_engine_fts (
                    rowid
                  , tracktitle
                  , tracknumber
                  , tracktotal
                  , discnumber
                  , disctotal
                  , releasetitle
                  , releasedate
                  , originaldate
                  , compositiondate
                  , edition
                  , catalognumber
                  , releasetype
                  , genre
                  , secondarygenre
                  , descriptor
                  , label
                  , releaseartist
                  , trackartist
                )
                SELECT
                    t.rowid
                  , process_string_for_fts(t.title) AS tracktitle
                  , process_string_for_fts(t.tracknumber) AS tracknumber
                  , process_string_for_fts(t.tracktotal) AS tracknumber
                  , process_string_for_fts(t.discnumber) AS discnumber
                  , process_string_for_fts(r.disctotal) AS discnumber
                  , process_string_for_fts(r.title) AS releasetitle
                  , process_string_for_fts(r.releasedate) AS releasedate
                  , process_string_for_fts(r.originaldate) AS originaldate
                  , process_string_for_fts(r.compositiondate) AS compositiondate
                  , process_string_for_fts(r.edition) AS edition
                  , process_string_for_fts(r.catalognumber) AS catalognumber
                  , process_string_for_fts(r.releasetype) AS releasetype
                  , process_string_for_fts(COALESCE(GROUP_CONCAT(rg.genre, ' '), '')) AS genre
                  , process_string_for_fts(COALESCE(GROUP_CONCAT(rs.genre, ' '), '')) AS secondarygenre
                  , process_string_for_fts(COALESCE(GROUP_CONCAT(rd.descriptor, ' '), '')) AS descriptor
                  , process_string_for_fts(COALESCE(GROUP_CONCAT(rl.label, ' '), '')) AS label
                  , process_string_for_fts(COALESCE(GROUP_CONCAT(ra.artist, ' '), '')) AS releaseartist
                  , process_string_for_fts(COALESCE(GROUP_CONCAT(ta.artist, ' '), '')) AS trackartist
                FROM tracks t
                JOIN releases r ON r.id = t.release_id
                LEFT JOIN releases_genres rg ON rg.release_id = r.id
                LEFT JOIN releases_secondary_genres rs ON rs.release_id = r.id
                LEFT JOIN releases_descriptors rd ON rd.release_id = r.id
                LEFT JOIN releases_labels rl ON rl.release_id = r.id
                LEFT JOIN releases_artists ra ON ra.release_id = r.id
                LEFT JOIN tracks_artists ta ON ta.track_id = t.id
                WHERE t.id IN ({",".join(["?"]*len(upd_track_ids))})
                   OR r.id IN ({",".join(["?"]*len(upd_release_ids))})
                GROUP BY t.id
                """,
                [*upd_track_ids, *upd_release_ids],
            )

        # Schedule collage/playlist updates in order to update description_meta. We simply update
        # collages and playlists if any of their members have changed--we do not try to be precise
        # here, as the update is very cheap. The point here is to avoid running the collage/playlist
        # update in the No Op case, not to optimize the invalidation case.
        if upd_release_ids:
            cursor = conn.execute(
                f"""
                SELECT DISTINCT cr.collage_name
                FROM collages_releases cr
                JOIN releases r ON r.id = cr.release_id
                WHERE cr.release_id IN ({','.join(['?'] * len(upd_release_ids))})
                ORDER BY cr.collage_name
                """,
                upd_release_ids,
            )
            update_collages = [row["collage_name"] for row in cursor]
        if upd_track_ids:
            cursor = conn.execute(
                f"""
                SELECT DISTINCT pt.playlist_name
                FROM playlists_tracks pt
                JOIN tracks t ON t.id = pt.track_id
                WHERE pt.track_id IN ({','.join(['?'] * len(upd_track_ids))})
                ORDER BY pt.playlist_name
                """,
                upd_track_ids,
            )
            update_playlists = [row["playlist_name"] for row in cursor]

    if update_collages:
        if collages_to_force_update_receiver is not None:
            collages_to_force_update_receiver.extend(update_collages)
        else:
            update_cache_for_collages(c, update_collages, force=True)
    if update_playlists:
        if playlists_to_force_update_receiver is not None:
            playlists_to_force_update_receiver.extend(update_playlists)
        else:
            update_cache_for_playlists(c, update_playlists, force=True)

    logger.debug(f"Database execution loop time {time.time() - exec_start=}")


def update_cache_for_collages(
    c: Config,
    # Leave as None to update all collages.
    collage_names: list[str] | None = None,
    force: bool = False,
) -> None:
    """
    Update the read cache to match the data for all stored collages.

    This is performance-optimized in a similar way to the update releases function. We:

    1. Execute one big SQL query at the start to fetch the relevant previous caches.
    2. Skip reading a file's data if the mtime has not changed since the previous cache update.
    3. Only execute a SQLite upsert if the read data differ from the previous caches.

    However, we do not batch writes to the end of the function, nor do we process the collages in
    parallel. This is because we should have far fewer collages than releases.
    """
    collage_dir = c.music_source_dir / "!collages"
    collage_dir.mkdir(exist_ok=True)

    files: list[tuple[Path, str, os.DirEntry[str]]] = []
    for f in os.scandir(str(collage_dir)):
        path = Path(f.path)
        if path.suffix != ".toml":
            continue
        if not path.is_file():
            logger.debug(f"Skipping processing collage {path.name} because it is not a file")
            continue
        if collage_names is None or path.stem in collage_names:
            files.append((path.resolve(), path.stem, f))
    logger.debug(f"Refreshing the read cache for {len(files)} collages")

    cached_collages: dict[str, CachedCollage] = {}
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT
                c.name
              , c.source_mtime
              , COALESCE(GROUP_CONCAT(cr.release_id, ' ¬ '), '') AS release_ids
            FROM collages c
            LEFT JOIN collages_releases cr ON cr.collage_name = c.name
            GROUP BY c.name
            """,
        )
        for row in cursor:
            cached_collages[row["name"]] = CachedCollage(
                name=row["name"],
                source_mtime=row["source_mtime"],
                release_ids=_split(row["release_ids"]) if row["release_ids"] else [],
            )

        # We want to validate that all release IDs exist before we write them. In order to do that,
        # we need to know which releases exist.
        cursor = conn.execute("SELECT id FROM releases")
        existing_release_ids = {row["id"] for row in cursor}

    loop_start = time.time()
    with connect(c) as conn:
        for source_path, name, f in files:
            try:
                cached_collage = cached_collages[name]
            except KeyError:
                logger.debug(f"First-time unidentified collage found at {source_path}")
                cached_collage = CachedCollage(
                    name=name,
                    source_mtime="",
                    release_ids=[],
                )

            try:
                source_mtime = str(f.stat().st_mtime)
            except FileNotFoundError:
                # Collage was deleted... continue without doing anything. It will be cleaned up by
                # the eviction function.
                continue
            if source_mtime == cached_collage.source_mtime and not force:
                logger.debug(f"Collage cache hit (mtime) for {source_path}, reusing cached data")
                continue

            logger.debug(f"Collage cache miss (mtime) for {source_path}, reading data from disk")
            cached_collage.source_mtime = source_mtime

            with lock(c, collage_lock_name(name)):
                with source_path.open("rb") as fp:
                    data = tomllib.load(fp)
                original_releases = data.get("releases", [])
                releases = copy.deepcopy(original_releases)

                # Update the markings for releases that no longer exist. We will flag releases as
                # missing/not-missing here, so that if they are re-added (maybe it was a temporary
                # disappearance)? they are recovered in the collage.
                for rls in releases:
                    if not rls.get("missing", False) and rls["uuid"] not in existing_release_ids:
                        logger.warning(
                            f"Marking missing release {rls['description_meta']} as missing in collage {cached_collage.name}"
                        )
                        rls["missing"] = True
                    elif rls.get("missing", False) and rls["uuid"] in existing_release_ids:
                        logger.info(
                            f"Missing release {rls['description_meta']} in collage {cached_collage.name} found: removing missing flag"
                        )
                        del rls["missing"]

                cached_collage.release_ids = [r["uuid"] for r in releases]
                logger.debug(
                    f"Found {len(cached_collage.release_ids)} release(s) (including missing) in {source_path}"
                )

                # Update the description_metas.
                desc_map: dict[str, str] = {}
                cursor = conn.execute(
                    f"""
                    SELECT id, releasetitle, releasedate, releaseartist_names, releaseartist_roles FROM releases_view
                    WHERE id IN ({','.join(['?']*len(releases))})
                    """,
                    cached_collage.release_ids,
                )
                for row in cursor:
                    desc_map[row["id"]] = calculate_release_logtext(
                        title=row["releasetitle"],
                        releasedate=RoseDate.parse(row["releasedate"]),
                        artists=_unpack_artists(
                            c, row["releaseartist_names"], row["releaseartist_roles"]
                        ),
                    )
                for i, rls in enumerate(releases):
                    with contextlib.suppress(KeyError):
                        releases[i]["description_meta"] = desc_map[rls["uuid"]]
                    if rls.get("missing", False) and not releases[i]["description_meta"].endswith(
                        " {MISSING}"
                    ):
                        releases[i]["description_meta"] += " {MISSING}"

                # Update the collage on disk if we have changed information.
                if releases != original_releases:
                    logger.debug(f"Updating release descriptions for {cached_collage.name}")
                    data["releases"] = releases
                    with source_path.open("wb") as fp:
                        tomli_w.dump(data, fp)
                    cached_collage.source_mtime = str(os.stat(source_path).st_mtime)

                logger.info(f"Updating cache for collage {cached_collage.name}")
                conn.execute(
                    """
                    INSERT INTO collages (name, source_mtime) VALUES (?, ?)
                    ON CONFLICT (name) DO UPDATE SET source_mtime = excluded.source_mtime
                    """,
                    (cached_collage.name, cached_collage.source_mtime),
                )
                conn.execute(
                    "DELETE FROM collages_releases WHERE collage_name = ?",
                    (cached_collage.name,),
                )
                args: list[Any] = []
                for position, rls in enumerate(releases):
                    args.extend(
                        [cached_collage.name, rls["uuid"], position + 1, rls.get("missing", False)]
                    )
                if args:
                    conn.execute(
                        f"""
                        INSERT INTO collages_releases (collage_name, release_id, position, missing)
                        VALUES {','.join(['(?, ?, ?, ?)'] * len(releases))}
                        """,
                        args,
                    )

    logger.debug(f"Collage update loop time {time.time() - loop_start=}")


def update_cache_evict_nonexistent_collages(c: Config) -> None:
    logger.debug("Evicting cached collages that are not on disk")
    collage_names: list[str] = []
    for f in os.scandir(c.music_source_dir / "!collages"):
        p = Path(f.path)
        if p.is_file() and p.suffix == ".toml":
            collage_names.append(p.stem)

    with connect(c) as conn:
        cursor = conn.execute(
            f"""
            DELETE FROM collages
            WHERE name NOT IN ({",".join(["?"] * len(collage_names))})
            RETURNING name
            """,
            collage_names,
        )
        for row in cursor:
            logger.info(f"Evicted missing collage {row['name']} from cache")


def update_cache_for_playlists(
    c: Config,
    # Leave as None to update all playlists.
    playlist_names: list[str] | None = None,
    force: bool = False,
) -> None:
    """
    Update the read cache to match the data for all stored playlists.

    This is performance-optimized in a similar way to the update releases function. We:

    1. Execute one big SQL query at the start to fetch the relevant previous caches.
    2. Skip reading a file's data if the mtime has not changed since the previous cache update.
    3. Only execute a SQLite upsert if the read data differ from the previous caches.

    However, we do not batch writes to the end of the function, nor do we process the playlists in
    parallel. This is because we should have far fewer playlists than releases.
    """
    playlist_dir = c.music_source_dir / "!playlists"
    playlist_dir.mkdir(exist_ok=True)

    files: list[tuple[Path, str, os.DirEntry[str]]] = []
    all_files_in_dir: list[Path] = []
    for f in os.scandir(str(playlist_dir)):
        path = Path(f.path)
        all_files_in_dir.append(path)
        if path.suffix != ".toml":
            continue
        if not path.is_file():
            logger.debug(f"Skipping processing playlist {path.name} because it is not a file")
            continue
        if playlist_names is None or path.stem in playlist_names:
            files.append((path.resolve(), path.stem, f))
    logger.debug(f"Refreshing the read cache for {len(files)} playlists")

    cached_playlists: dict[str, CachedPlaylist] = {}
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT
                p.name
              , p.source_mtime
              , p.cover_path
              , COALESCE(GROUP_CONCAT(pt.track_id, ' ¬ '), '') AS track_ids
            FROM playlists p
            LEFT JOIN playlists_tracks pt ON pt.playlist_name = p.name
            GROUP BY p.name
            """,
        )
        for row in cursor:
            cached_playlists[row["name"]] = CachedPlaylist(
                name=row["name"],
                source_mtime=row["source_mtime"],
                cover_path=Path(row["cover_path"]) if row["cover_path"] else None,
                track_ids=_split(row["track_ids"]) if row["track_ids"] else [],
            )

        # We want to validate that all track IDs exist before we write them. In order to do that,
        # we need to know which tracks exist.
        cursor = conn.execute("SELECT id FROM tracks")
        existing_track_ids = {row["id"] for row in cursor}

    loop_start = time.time()
    with connect(c) as conn:
        for source_path, name, f in files:
            try:
                cached_playlist = cached_playlists[name]
            except KeyError:
                logger.debug(f"First-time unidentified playlist found at {source_path}")
                cached_playlist = CachedPlaylist(
                    name=name,
                    source_mtime="",
                    cover_path=None,
                    track_ids=[],
                )

            # We do a quick scan for the playlist's cover art here. We always do this check, as it
            # amounts to ~4 getattrs. If a change is detected, we ignore the mtime optimization and
            # always update the database.
            dirty = False
            if cached_playlist.cover_path and not cached_playlist.cover_path.is_file():
                cached_playlist.cover_path = None
                dirty = True
            if not cached_playlist.cover_path:
                for potential_art_file in all_files_in_dir:
                    if (
                        potential_art_file.stem == name
                        and potential_art_file.suffix.lower().lstrip(".") in c.valid_art_exts
                    ):
                        cached_playlist.cover_path = potential_art_file.resolve()
                        dirty = True
                        break

            try:
                source_mtime = str(f.stat().st_mtime)
            except FileNotFoundError:
                # Playlist was deleted... continue without doing anything. It will be cleaned up by
                # the eviction function.
                continue
            if source_mtime == cached_playlist.source_mtime and not force and not dirty:
                logger.debug(f"playlist cache hit (mtime) for {source_path}, reusing cached data")
                continue

            logger.debug(
                f"playlist cache miss (mtime/{dirty=}) for {source_path}, reading data from disk"
            )
            cached_playlist.source_mtime = source_mtime

            with lock(c, playlist_lock_name(name)):
                with source_path.open("rb") as fp:
                    data = tomllib.load(fp)
                original_tracks = data.get("tracks", [])
                tracks = copy.deepcopy(original_tracks)

                # Update the markings for tracks that no longer exist. We will flag tracks as
                # missing/not-missing here, so that if they are re-added (maybe it was a temporary
                # disappearance)? they are recovered in the playlist.
                for trk in tracks:
                    if not trk.get("missing", False) and trk["uuid"] not in existing_track_ids:
                        logger.warning(
                            f"Marking missing track {trk['description_meta']} as missing in playlist {cached_playlist.name}"
                        )
                        trk["missing"] = True
                    elif trk.get("missing", False) and trk["uuid"] in existing_track_ids:
                        logger.info(
                            f"Missing trk {trk['description_meta']} in playlist {cached_playlist.name} found: removing missing flag"
                        )
                        del trk["missing"]

                cached_playlist.track_ids = [t["uuid"] for t in tracks]
                logger.debug(
                    f"Found {len(cached_playlist.track_ids)} track(s) (including missing) in {source_path}"
                )

                # Update the description_metas.
                desc_map: dict[str, str] = {}
                cursor = conn.execute(
                    f"""
                    SELECT
                        t.id
                      , t.tracktitle
                      , t.source_path
                      , t.trackartist_names
                      , t.trackartist_roles
                      , r.releasedate
                    FROM tracks_view t
                    JOIN releases_view r ON r.id = t.release_id
                    WHERE t.id IN ({','.join(['?']*len(tracks))})
                    """,
                    cached_playlist.track_ids,
                )
                for row in cursor:
                    desc_map[row["id"]] = calculate_track_logtext(
                        title=row["tracktitle"],
                        artists=_unpack_artists(
                            c, row["trackartist_names"], row["trackartist_roles"]
                        ),
                        releasedate=RoseDate.parse(row["releasedate"]),
                        suffix=Path(row["source_path"]).suffix,
                    )
                for trk in tracks:
                    with contextlib.suppress(KeyError):
                        trk["description_meta"] = desc_map[trk["uuid"]]
                    if trk.get("missing", False) and not trk["description_meta"].endswith(
                        " {MISSING}"
                    ):
                        trk["description_meta"] += " {MISSING}"

                # Update the playlist on disk if we have changed information.
                if tracks != original_tracks:
                    logger.debug(f"Updating track descriptions for {cached_playlist.name}")
                    data["tracks"] = tracks
                    with source_path.open("wb") as fp:
                        tomli_w.dump(data, fp)
                    cached_playlist.source_mtime = str(os.stat(source_path).st_mtime)

                logger.info(f"Updating cache for playlist {cached_playlist.name}")
                conn.execute(
                    """
                    INSERT INTO playlists (name, source_mtime, cover_path) VALUES (?, ?, ?)
                    ON CONFLICT (name) DO UPDATE SET
                        source_mtime = excluded.source_mtime
                      , cover_path = excluded.cover_path
                    """,
                    (
                        cached_playlist.name,
                        cached_playlist.source_mtime,
                        str(cached_playlist.cover_path) if cached_playlist.cover_path else None,
                    ),
                )
                conn.execute(
                    "DELETE FROM playlists_tracks WHERE playlist_name = ?",
                    (cached_playlist.name,),
                )
                args: list[Any] = []
                for position, trk in enumerate(tracks):
                    args.extend(
                        [cached_playlist.name, trk["uuid"], position + 1, trk.get("missing", False)]
                    )
                if args:
                    conn.execute(
                        f"""
                        INSERT INTO playlists_tracks (playlist_name, track_id, position, missing)
                        VALUES {','.join(['(?, ?, ?, ?)'] * len(tracks))}
                        """,
                        args,
                    )

    logger.debug(f"playlist update loop time {time.time() - loop_start=}")


def update_cache_evict_nonexistent_playlists(c: Config) -> None:
    logger.debug("Evicting cached playlists that are not on disk")
    playlist_names: list[str] = []
    for f in os.scandir(c.music_source_dir / "!playlists"):
        p = Path(f.path)
        if p.is_file() and p.suffix == ".toml":
            playlist_names.append(p.stem)

    with connect(c) as conn:
        cursor = conn.execute(
            f"""
            DELETE FROM playlists
            WHERE name NOT IN ({",".join(["?"] * len(playlist_names))})
            RETURNING name
            """,
            playlist_names,
        )
        for row in cursor:
            logger.info(f"Evicted missing playlist {row['name']} from cache")


def list_releases_delete_this(
    c: Config,
    artist_filter: str | None = None,
    genre_filter: str | None = None,
    descriptor_filter: str | None = None,
    label_filter: str | None = None,
    new: bool | None = None,
) -> list[CachedRelease]:
    with connect(c) as conn:
        query = "SELECT * FROM releases_view WHERE 1=1"
        args: list[str | bool] = []
        if artist_filter:
            artists: list[str] = [artist_filter]
            for alias in _get_all_artist_aliases(c, artist_filter):
                artists.append(alias)
            query += f"""
                AND EXISTS (
                    SELECT * FROM releases_artists
                    WHERE release_id = id AND artist IN ({','.join(['?']*len(artists))})
                )
            """
            args.extend(artists)
        if genre_filter:
            genres = [genre_filter]
            genres.extend(TRANSIENT_CHILD_GENRES.get(genre_filter, []))
            query += f"""
                AND (
                    EXISTS (
                        SELECT * FROM releases_genres
                        WHERE release_id = id AND genre IN ({",".join(["?"]*len(genres))})
                    )
                    OR EXISTS (
                        SELECT * FROM releases_secondary_genres
                        WHERE release_id = id AND genre IN ({",".join(["?"]*len(genres))})
                    )
                )
            """
            args.extend(genres)
            args.extend(genres)
        if descriptor_filter:
            query += """
                AND EXISTS (
                    SELECT * FROM releases_descriptors
                    WHERE release_id = id AND descriptor = ?
                )
            """
            args.append(descriptor_filter)
        if label_filter:
            query += """
                AND EXISTS (
                    SELECT * FROM releases_labels
                    WHERE release_id = id AND label = ?
                )
            """
            args.append(label_filter)
        if new is not None:
            query += " AND new = ?"
            args.append(new)
        query += " ORDER BY source_path"

        cursor = conn.execute(query, args)
        releases: list[CachedRelease] = []
        for row in cursor:
            releases.append(CachedRelease.from_view(c, row))
        return releases


def list_releases(c: Config, release_ids: list[str] | None = None) -> list[CachedRelease]:
    """Fetch data associated with given release IDs. Pass None to fetch all."""
    query = "SELECT * FROM releases_view"
    args = []
    if release_ids is not None:
        query += f" WHERE id IN ({','.join(['?']*len(release_ids))})"
        args = release_ids
    query += " ORDER BY source_path"
    with connect(c) as conn:
        cursor = conn.execute(query, args)
        releases: list[CachedRelease] = []
        for row in cursor:
            releases.append(CachedRelease.from_view(c, row))
        return releases


def get_release(c: Config, release_id: str) -> CachedRelease | None:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT * FROM releases_view WHERE id = ?",
            (release_id,),
        )
        row = cursor.fetchone()
        if not row:
            return None
        return CachedRelease.from_view(c, row)


def get_release_logtext(c: Config, release_id: str) -> str | None:
    """Get a human-readable identifier for a release suitable for logging."""
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT releasetitle, releasedate, releaseartist_names, releaseartist_roles FROM releases_view WHERE id = ?",
            (release_id,),
        )
        row = cursor.fetchone()
        if not row:
            return None
        return calculate_release_logtext(
            title=row["releasetitle"],
            releasedate=RoseDate.parse(row["releasedate"]),
            artists=_unpack_artists(c, row["releaseartist_names"], row["releaseartist_roles"]),
        )


def calculate_release_logtext(
    title: str,
    releasedate: RoseDate | None,
    artists: ArtistMapping,
) -> str:
    logtext = f"{artistsfmt(artists)} - "
    if releasedate:
        logtext += f"{releasedate.year}. "
    logtext += title
    return logtext


def list_tracks(c: Config, track_ids: list[str] | None = None) -> list[CachedTrack]:
    """Fetch data associated with given track IDs. Pass None to fetch all."""
    query = "SELECT * FROM tracks_view"
    args = []
    if track_ids is not None:
        query += f" WHERE id IN ({','.join(['?']*len(track_ids))})"
        args = track_ids
    query += " ORDER BY source_path"
    with connect(c) as conn:
        cursor = conn.execute(query, args)
        trackrows = cursor.fetchall()

        release_ids = [r["release_id"] for r in trackrows]
        cursor = conn.execute(
            f"""
            SELECT *
            FROM releases_view
            WHERE id IN ({','.join(['?']*len(release_ids))})
            """,
            release_ids,
        )
        releases_map: dict[str, CachedRelease] = {}
        for row in cursor:
            releases_map[row["id"]] = CachedRelease.from_view(c, row)

        rval = []
        for row in trackrows:
            rval.append(CachedTrack.from_view(c, row, releases_map[row["release_id"]]))
        return rval


def get_track(c: Config, uuid: str) -> CachedTrack | None:
    with connect(c) as conn:
        cursor = conn.execute("SELECT * FROM tracks_view WHERE id = ?", (uuid,))
        trackrow = cursor.fetchone()
        if not trackrow:
            return None
        cursor = conn.execute("SELECT * FROM releases_view WHERE id = ?", (trackrow["release_id"],))
        release = CachedRelease.from_view(c, cursor.fetchone())
        return CachedTrack.from_view(c, trackrow, release)


def get_tracks_associated_with_release(
    c: Config,
    release: CachedRelease,
) -> list[CachedTrack]:
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT *
            FROM tracks_view
            WHERE release_id = ?
            ORDER BY release_id, FORMAT('%4d.%4d', discnumber, tracknumber)
            """,
            (release.id,),
        )
        rval = []
        for row in cursor:
            rval.append(CachedTrack.from_view(c, row, release))
        return rval


def get_tracks_associated_with_releases(
    c: Config,
    releases: list[CachedRelease],
) -> list[tuple[CachedRelease, list[CachedTrack]]]:
    releases_map = {r.id: r for r in releases}
    tracks_map: dict[str, list[CachedTrack]] = defaultdict(list)
    with connect(c) as conn:
        cursor = conn.execute(
            f"""
            SELECT *
            FROM tracks_view
            WHERE release_id IN ({','.join(['?']*len(releases))})
            ORDER BY release_id, FORMAT('%4d.%4d', discnumber, tracknumber)
            """,
            [r.id for r in releases],
        )
        for row in cursor:
            tracks_map[row["release_id"]].append(
                CachedTrack.from_view(c, row, releases_map[row["release_id"]])
            )

    rval = []
    for release in releases:
        tracks = tracks_map[release.id]
        rval.append((release, tracks))
    return rval


def get_path_of_track_in_release(
    c: Config,
    track_id: str,
    release_id: str,
) -> Path | None:
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT source_path
            FROM tracks
            WHERE id = ? AND release_id = ?
            """,
            (
                track_id,
                release_id,
            ),
        )
        row = cursor.fetchone()
        if row:
            return Path(row["source_path"])
        return None


def get_path_of_track_in_playlist(
    c: Config,
    track_id: str,
    playlist_name: str,
) -> Path | None:
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT t.source_path
            FROM tracks t
            JOIN playlists_tracks pt ON pt.track_id = t.id AND pt.playlist_name = ?
            WHERE t.id = ?
            """,
            (
                playlist_name,
                track_id,
            ),
        )
        row = cursor.fetchone()
        if row:
            return Path(row["source_path"])
        return None


def get_track_logtext(c: Config, track_id: str) -> str | None:
    """Get a human-readable identifier for a track suitable for logging."""
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT
                t.tracktitle
              , t.source_path
              , t.trackartist_names
              , t.trackartist_roles
              , r.releasedate
            FROM tracks_view t
            JOIN releases_view r ON r.id = t.release_id
            WHERE t.id = ?
            """,
            (track_id,),
        )
        row = cursor.fetchone()
        if not row:
            return None
        return calculate_track_logtext(
            title=row["tracktitle"],
            artists=_unpack_artists(c, row["trackartist_names"], row["trackartist_roles"]),
            releasedate=RoseDate.parse(row["releasedate"]),
            suffix=Path(row["source_path"]).suffix,
        )


def calculate_track_logtext(
    title: str,
    artists: ArtistMapping,
    releasedate: RoseDate | None,
    suffix: str,
) -> str:
    rval = f"{artistsfmt(artists)} - {title or 'Unknown Title'}"
    if releasedate:
        rval += f" [{releasedate.year}]"
    rval += suffix
    return rval


def list_playlists(c: Config) -> list[str]:
    with connect(c) as conn:
        cursor = conn.execute("SELECT DISTINCT name FROM playlists")
        return [r["name"] for r in cursor]


def get_playlist(c: Config, playlist_name: str) -> tuple[CachedPlaylist, list[CachedTrack]] | None:
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT
                name
              , source_mtime
              , cover_path
            FROM playlists
            WHERE name = ?
            """,
            (playlist_name,),
        )
        row = cursor.fetchone()
        if not row:
            return None
        playlist = CachedPlaylist(
            name=row["name"],
            source_mtime=row["source_mtime"],
            cover_path=Path(row["cover_path"]) if row["cover_path"] else None,
            # Accumulated below when we query the tracks.
            track_ids=[],
        )

        cursor = conn.execute(
            """
            SELECT t.*
            FROM tracks_view t
            JOIN playlists_tracks pt ON pt.track_id = t.id
            WHERE pt.playlist_name = ? AND NOT pt.missing
            ORDER BY pt.position ASC
            """,
            (playlist_name,),
        )
        trackrows = cursor.fetchall()

        release_ids = [r["release_id"] for r in trackrows]
        cursor = conn.execute(
            f"""
            SELECT *
            FROM releases_view
            WHERE id IN ({','.join(['?']*len(release_ids))})
            """,
            release_ids,
        )
        releases_map: dict[str, CachedRelease] = {}
        for row in cursor:
            releases_map[row["id"]] = CachedRelease.from_view(c, row)

        tracks: list[CachedTrack] = []
        for row in trackrows:
            playlist.track_ids.append(row["id"])
            tracks.append(CachedTrack.from_view(c, row, releases_map[row["release_id"]]))

    return playlist, tracks


def playlist_exists(c: Config, playlist_name: str) -> bool:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT EXISTS(SELECT * FROM playlists WHERE name = ?)",
            (playlist_name,),
        )
        return bool(cursor.fetchone()[0])


def get_playlist_cover_path(c: Config, playlist_name: str) -> Path | None:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT cover_path FROM playlists WHERE name = ?",
            (playlist_name,),
        )
        row = cursor.fetchone()
        if row and row["cover_path"]:
            return Path(row["cover_path"])
        return None


def list_collages(c: Config) -> list[str]:
    with connect(c) as conn:
        cursor = conn.execute("SELECT DISTINCT name FROM collages")
        return [r["name"] for r in cursor]


def get_collage(c: Config, collage_name: str) -> tuple[CachedCollage, list[CachedRelease]] | None:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT name, source_mtime FROM collages WHERE name = ?",
            (collage_name,),
        )
        row = cursor.fetchone()
        if not row:
            return None
        collage = CachedCollage(
            name=row["name"],
            source_mtime=row["source_mtime"],
            # Accumulated below when we query the releases.
            release_ids=[],
        )
        cursor = conn.execute(
            """
            SELECT r.*
            FROM releases_view r
            JOIN collages_releases cr ON cr.release_id = r.id
            WHERE cr.collage_name = ? AND NOT cr.missing
            ORDER BY cr.position ASC
            """,
            (collage_name,),
        )
        releases: list[CachedRelease] = []
        for row in cursor:
            collage.release_ids.append(row["id"])
            releases.append(CachedRelease.from_view(c, row))

    return (collage, releases)


def collage_exists(c: Config, collage_name: str) -> bool:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT EXISTS(SELECT * FROM collages WHERE name = ?)",
            (collage_name,),
        )
        return bool(cursor.fetchone()[0])


def list_artists(c: Config) -> list[str]:
    with connect(c) as conn:
        cursor = conn.execute("SELECT DISTINCT artist FROM releases_artists")
        return [row["artist"] for row in cursor]


def artist_exists(c: Config, artist: str) -> bool:
    args: list[str] = [artist]
    for alias in _get_all_artist_aliases(c, artist):
        args.append(alias)
    with connect(c) as conn:
        cursor = conn.execute(
            f"""
            SELECT EXISTS(
                SELECT * FROM releases_artists
                WHERE artist IN ({','.join(['?']*len(args))})
            )
            """,
            args,
        )
        return bool(cursor.fetchone()[0])


@dataclass(frozen=True)
class GenreEntry:
    genre: str
    only_new_releases: bool


def list_genres(c: Config) -> list[GenreEntry]:
    with connect(c) as conn:
        query = """
            SELECT rg.genre, MIN(r.id) AS has_non_new_release
            FROM releases_genres rg
            LEFT JOIN releases r ON r.id = rg.release_id AND NOT r.new
            GROUP BY rg.genre
        """
        cursor = conn.execute(query)
        rval: dict[str, bool] = {}
        for row in cursor:
            rval[row["genre"]] = row["has_non_new_release"] is None
            for g in TRANSIENT_PARENT_GENRES.get(row["genre"], []):
                # We are accumulating here whether any release of this genre is not-new. Thus, if a
                # past iteration had a not-new release, make sure the accumulator stays false. And
                # if we have a not-new release this time, set it false. Otherwise, keep it true.
                rval[g] = not (rval.get(g) is False or row["has_non_new_release"] is not None)
        return [GenreEntry(genre=k, only_new_releases=v) for k, v in rval.items()]


def genre_exists(c: Config, genre: str) -> bool:
    with connect(c) as conn:
        args = [genre]
        args.extend(TRANSIENT_CHILD_GENRES.get(genre, []))
        cursor = conn.execute(
            f"SELECT EXISTS(SELECT * FROM releases_genres WHERE genre IN ({','.join(['?']*len(args))}))",
            args,
        )
        return bool(cursor.fetchone()[0])


@dataclass(frozen=True)
class DescriptorEntry:
    descriptor: str
    only_new_releases: bool


def list_descriptors(c: Config) -> list[DescriptorEntry]:
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT rg.descriptor, MIN(r.id) AS has_non_new_release
            FROM releases_descriptors rg
            LEFT JOIN releases r ON r.id = rg.release_id AND NOT r.new
            GROUP BY rg.descriptor
            """
        )
        return [
            DescriptorEntry(
                descriptor=row["descriptor"],
                only_new_releases=row["has_non_new_release"] is None,
            )
            for row in cursor
        ]


def descriptor_exists(c: Config, descriptor: str) -> bool:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT EXISTS(SELECT * FROM releases_descriptors WHERE descriptor = ?)",
            (descriptor,),
        )
        return bool(cursor.fetchone()[0])


@dataclass(frozen=True)
class LabelEntry:
    label: str
    only_new_releases: bool


def list_labels(c: Config) -> list[LabelEntry]:
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT rg.label, MIN(r.id) AS has_non_new_release
            FROM releases_labels rg
            LEFT JOIN releases r ON r.id = rg.release_id AND NOT r.new
            GROUP BY rg.label
            """
        )
        return [
            LabelEntry(label=row["label"], only_new_releases=row["has_non_new_release"] is None)
            for row in cursor
        ]


def label_exists(c: Config, label: str) -> bool:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT EXISTS(SELECT * FROM releases_labels WHERE label = ?)",
            (label,),
        )
        return bool(cursor.fetchone()[0])


def _split(xs: str) -> list[str]:
    """Split the stringly-encoded arrays from the database by the sentinel character."""
    if not xs:
        return []
    return xs.split(" ¬ ")


def _unpack_artists(
    c: Config,
    names: str,
    roles: str,
    *,
    aliases: bool = True,
) -> ArtistMapping:
    mapping = ArtistMapping()
    seen: set[tuple[str, str]] = set()
    for name, role in _unpack(names, roles):
        role_artists: list[Artist] = getattr(mapping, role)
        role_artists.append(Artist(name=name, alias=False))
        seen.add((name, role))
        if not aliases:
            continue

        # Get all immediate and transitive artist aliases.
        unvisited: set[str] = {name}
        while unvisited:
            cur = unvisited.pop()
            for alias in c.artist_aliases_parents_map.get(cur, []):
                if (alias, role) not in seen:
                    role_artists.append(Artist(name=alias, alias=True))
                    seen.add((alias, role))
                    unvisited.add(alias)
    return mapping


def _get_all_artist_aliases(c: Config, x: str) -> list[str]:
    """Includes transitive aliases."""
    aliases: set[str] = set()
    unvisited: set[str] = {x}
    while unvisited:
        cur = unvisited.pop()
        if cur in aliases:
            continue
        aliases.add(cur)
        unvisited.update(c.artist_aliases_map.get(cur, []))
    return list(aliases)


def _get_parent_genres(genres: list[str]) -> list[str]:
    rval: set[str] = set()
    for g in genres:
        rval.update(TRANSIENT_PARENT_GENRES.get(g, []))
    return sorted(rval)


def _flatten(xxs: list[list[T]]) -> list[T]:
    xs: list[T] = []
    for group in xxs:
        xs.extend(group)
    return xs


def _unpack(*xxs: str) -> Iterator[tuple[str, ...]]:
    """
    Unpack an arbitrary number of strings, each of which is a " ¬ "-delimited list in actuality,
    but encoded as a string. This " ¬ "-delimited list-as-a-string is the convention we use to
    return arrayed data from a SQL query without introducing additional disk accesses.

    As a concrete example:

        >>> _unpack("Rose ¬ Lisa ¬ Jisoo ¬ Jennie", "vocal ¬ dance ¬ visual ¬ vocal")
        [("Rose", "vocal"), ("Lisa", "dance"), ("Jisoo", "visual"), ("Jennie", "vocal")]
    """
    # If the strings are empty, then split will resolve to `[""]`. But we don't want to loop over an
    # empty string, so we specially exit if we hit that case.
    if all(not xs for xs in xxs):
        return []
    yield from zip(*[_split(xs) for xs in xxs])


def process_string_for_fts(x: str) -> str:
    # In order to have performant substring search, we use FTS and hack it such that every character
    # is a token. We use "¬" as our separator character, hoping that it is not used in any metadata.
    return "¬".join(str(x)) if x else x
