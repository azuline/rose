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
from collections.abc import Iterator
from dataclasses import dataclass
from datetime import datetime
from hashlib import sha256
from pathlib import Path
from typing import Any, TypeVar

import tomli_w
import tomllib
import uuid6

from rose.audiotags import SUPPORTED_AUDIO_EXTENSIONS, AudioTags
from rose.common import VERSION, Artist, ArtistMapping, sanitize_filename, uniq
from rose.config import Config
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


@dataclass
class CachedRelease:
    id: str
    source_path: Path
    cover_image_path: Path | None
    added_at: str  # ISO8601 timestamp
    datafile_mtime: str
    title: str
    releasetype: str
    year: int | None
    new: bool
    multidisc: bool
    genres: list[str]
    labels: list[str]
    artists: ArtistMapping

    def dump(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "source_path": str(self.source_path.resolve()),
            "cover_image_path": str(self.cover_image_path.resolve())
            if self.cover_image_path
            else None,
            "added_at": self.added_at,
            "title": self.title,
            "releasetype": self.releasetype,
            "year": self.year,
            "new": self.new,
            "genres": self.genres,
            "labels": self.labels,
            "artists": self.artists.dump(),
        }


@dataclass
class CachedTrack:
    id: str
    source_path: Path
    source_mtime: str
    title: str
    release_id: str
    tracknumber: str
    discnumber: str
    duration_seconds: int

    artists: ArtistMapping

    # Stored on here for virtual path generation; not inserted into the database.
    release_multidisc: bool

    def dump(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "source_path": str(self.source_path.resolve()),
            "title": self.title,
            "release_id": self.release_id,
            "tracknumber": self.tracknumber,
            "discnumber": self.discnumber,
            "duration_seconds": self.duration_seconds,
            "artists": self.artists.dump(),
        }


@dataclass
class CachedCollage:
    name: str
    source_mtime: str
    release_ids: list[str]


@dataclass
class CachedPlaylist:
    name: str
    source_mtime: str
    cover_path: Path | None
    track_ids: list[str]


@dataclass
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
) -> None:  # pragma: no cover
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
            SELECT
                id
              , source_path
              , cover_image_path
              , added_at
              , datafile_mtime
              , title
              , releasetype
              , year
              , multidisc
              , new
              , genres
              , labels
              , artist_names
              , artist_roles
            FROM releases_view
            WHERE id IN ({','.join(['?']*len(release_uuids))})
            """,
            release_uuids,
        )
        for row in cursor:
            cached_releases[row["id"]] = (
                CachedRelease(
                    id=row["id"],
                    source_path=Path(row["source_path"]),
                    cover_image_path=Path(row["cover_image_path"])
                    if row["cover_image_path"]
                    else None,
                    added_at=row["added_at"],
                    datafile_mtime=row["datafile_mtime"],
                    title=row["title"],
                    releasetype=row["releasetype"],
                    year=row["year"],
                    multidisc=bool(row["multidisc"]),
                    new=bool(row["new"]),
                    genres=_split(row["genres"]),
                    labels=_split(row["labels"]),
                    artists=_unpack_artists(
                        c, row["artist_names"], row["artist_roles"], aliases=False
                    ),
                ),
                {},
            )

        logger.debug(f"Found {len(cached_releases)}/{len(release_dirs)} releases in cache")

        cursor = conn.execute(
            rf"""
            SELECT
                t.id
              , t.source_path
              , t.source_mtime
              , t.title
              , t.release_id
              , t.tracknumber
              , t.discnumber
              , t.duration_seconds
              , t.artist_names
              , t.artist_roles
            FROM tracks_view t
            JOIN releases r ON r.id = t.release_id
            WHERE r.id IN ({','.join(['?']*len(release_uuids))})
            """,
            release_uuids,
        )
        num_tracks_found = 0
        for row in cursor:
            cached_releases[row["release_id"]][1][row["source_path"]] = CachedTrack(
                id=row["id"],
                source_path=Path(row["source_path"]),
                source_mtime=row["source_mtime"],
                title=row["title"],
                release_id=row["release_id"],
                tracknumber=row["tracknumber"],
                discnumber=row["discnumber"],
                duration_seconds=row["duration_seconds"],
                artists=_unpack_artists(c, row["artist_names"], row["artist_roles"], aliases=False),
                release_multidisc=cached_releases[row["release_id"]][0].multidisc,
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
                title="",
                releasetype="",
                year=None,
                new=True,
                multidisc=False,
                genres=[],
                labels=[],
                artists=ArtistMapping(),
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
                release_title = tags.album or "Unknown Release"
                if release_title != release.title:
                    logger.debug(f"Release title change detected for {source_path}, updating")
                    release.title = release_title
                    release_dirty = True

                releasetype = tags.releasetype
                if releasetype != release.releasetype:
                    logger.debug(f"Release type change detected for {source_path}, updating")
                    release.releasetype = releasetype
                    release_dirty = True

                if tags.year != release.year:
                    logger.debug(f"Release year change detected for {source_path}, updating")
                    release.year = tags.year
                    release_dirty = True

                if set(tags.genre) != set(release.genres):
                    logger.debug(f"Release genre change detected for {source_path}, updating")
                    release.genres = uniq(tags.genre)
                    release_dirty = True

                if set(tags.label) != set(release.labels):
                    logger.debug(f"Release label change detected for {source_path}, updating")
                    release.labels = uniq(tags.label)
                    release_dirty = True

                if tags.albumartists != release.artists:
                    logger.debug(f"Release artists change detected for {source_path}, updating")
                    release.artists = tags.albumartists
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
                title=tags.title or "Unknown Title",
                release_id=release.id,
                # Remove `.` here because we use `.` to parse out discno/trackno in the virtual
                # filesystem. It should almost never happen, but better to be safe.
                tracknumber=(tags.tracknumber or "1").replace(".", ""),
                discnumber=(tags.discnumber or "1").replace(".", ""),
                # This is calculated with the virtual filename.
                duration_seconds=tags.duration_sec,
                artists=tags.trackartists,
                # We set this later.
                release_multidisc=False,
            )
            tracks.append(track)
            track_ids_to_insert.add(track.id)

        # Now calculate whether this release is multidisc. Only recompute this if any tracks have
        # changed. Otherwise, save CPU cycles.
        if track_ids_to_insert or unknown_cached_tracks:
            multidisc = len({t.discnumber for t in tracks}) > 1
            for t in tracks:
                t.release_multidisc = multidisc
            if release.multidisc != multidisc:
                logger.debug(f"Release multidisc change detected for {source_path}, updating")
                release_dirty = True
                release.multidisc = multidisc

        # And now perform directory/file renames if configured.
        if c.rename_source_files:
            if release_dirty:
                wanted_dirname = eval_release_template(c.path_templates.source.release, release)
                wanted_dirname = sanitize_filename(wanted_dirname)
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
                        wanted_dirname = f"{original_wanted_dirname} [{collision_no}]"
                        collision_no += 1
                        continue
                    # If no collision, rename the directory.
                    old_source_path = release.source_path
                    old_source_path.rename(new_source_path)
                    logger.info(
                        f"Renamed source release directory {old_source_path.name} to {new_source_path.name}"
                    )
                    release.source_path = new_source_path
                    # Update the track paths and schedule them for database insertions.
                    for track in tracks:
                        tracklocalpath = str(track.source_path).removeprefix(f"{old_source_path}/")
                        track.source_path = release.source_path / tracklocalpath
                        track.source_mtime = str(os.stat(track.source_path).st_mtime)
                        track_ids_to_insert.add(track.id)
            for track in [t for t in tracks if t.id in track_ids_to_insert]:
                wanted_filename = eval_track_template(c.path_templates.source.track, track)
                wanted_filename = sanitize_filename(wanted_filename)
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
                        wanted_filename = (
                            f"{original_wanted_stem} [{collision_no}]{original_wanted_suffix}"
                        )
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
                    release.title,
                    release.releasetype,
                    release.year,
                    release.multidisc,
                    release.new,
                ]
            )
            upd_release_ids.append(release.id)
            for pos, genre in enumerate(release.genres):
                upd_release_genre_args.append([release.id, genre, sanitize_filename(genre), pos])
            for pos, label in enumerate(release.labels):
                upd_release_label_args.append([release.id, label, sanitize_filename(label), pos])
            pos = 0
            for role, artists in release.artists.items():
                for art in artists:
                    upd_release_artist_args.append(
                        [release.id, art.name, sanitize_filename(art.name), role, pos]
                    )
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
                        track.title,
                        track.release_id,
                        track.tracknumber,
                        track.discnumber,
                        track.duration_seconds,
                    ]
                )
                upd_track_ids.append(track.id)
                pos = 0
                for role, artists in track.artists.items():
                    for art in artists:
                        upd_track_artist_args.append(
                            [track.id, art.name, sanitize_filename(art.name), role, pos]
                        )
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
            conn.execute(
                f"""
                INSERT INTO releases (
                    id
                  , source_path
                  , cover_image_path
                  , added_at
                  , datafile_mtime
                  , title
                  , releasetype
                  , year
                  , multidisc
                  , new
                ) VALUES {",".join(["(?,?,?,?,?,?,?,?,?,?)"] * len(upd_release_args))}
                ON CONFLICT (id) DO UPDATE SET
                    source_path      = excluded.source_path
                  , cover_image_path = excluded.cover_image_path
                  , added_at         = excluded.added_at
                  , datafile_mtime   = excluded.datafile_mtime
                  , title            = excluded.title
                  , releasetype      = excluded.releasetype
                  , year             = excluded.year
                  , multidisc        = excluded.multidisc
                  , new              = excluded.new
                """,
                _flatten(upd_release_args),
            )
        if upd_release_genre_args:
            conn.execute(
                f"""
                DELETE FROM releases_genres
                WHERE release_id IN ({",".join(["?"]*len(upd_release_genre_args))})
                """,
                [a[0] for a in upd_release_genre_args],
            )
            conn.execute(
                f"""
                INSERT INTO releases_genres (release_id, genre, genre_sanitized, position)
                VALUES {",".join(["(?,?,?,?)"]*len(upd_release_genre_args))}
                """,
                _flatten(upd_release_genre_args),
            )
        if upd_release_label_args:
            conn.execute(
                f"""
                DELETE FROM releases_labels
                WHERE release_id IN ({",".join(["?"]*len(upd_release_label_args))})
                """,
                [a[0] for a in upd_release_label_args],
            )
            conn.execute(
                f"""
                INSERT INTO releases_labels (release_id, label, label_sanitized, position)
                VALUES {",".join(["(?,?,?,?)"]*len(upd_release_label_args))}
                """,
                _flatten(upd_release_label_args),
            )
        if upd_release_artist_args:
            conn.execute(
                f"""
                DELETE FROM releases_artists
                WHERE release_id IN ({",".join(["?"]*len(upd_release_artist_args))})
                """,
                [a[0] for a in upd_release_artist_args],
            )
            conn.execute(
                f"""
                INSERT INTO releases_artists (release_id, artist, artist_sanitized, role, position)
                VALUES {",".join(["(?,?,?,?,?)"]*len(upd_release_artist_args))}
                """,
                _flatten(upd_release_artist_args),
            )
        if upd_track_args:
            conn.execute(
                f"""
                INSERT INTO tracks (
                    id
                  , source_path
                  , source_mtime
                  , title
                  , release_id
                  , tracknumber
                  , discnumber
                  , duration_seconds
                )
                VALUES {",".join(["(?,?,?,?,?,?,?,?)"]*len(upd_track_args))}
                ON CONFLICT (id) DO UPDATE SET
                    source_path                = excluded.source_path
                  , source_mtime               = excluded.source_mtime
                  , title                      = excluded.title
                  , release_id                 = excluded.release_id
                  , tracknumber               = excluded.tracknumber
                  , discnumber                = excluded.discnumber
                  , duration_seconds           = excluded.duration_seconds
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
                INSERT INTO tracks_artists (track_id, artist, artist_sanitized, role, position)
                VALUES {",".join(["(?,?,?,?,?)"]*len(upd_track_artist_args))}
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
                  , discnumber
                  , albumtitle
                  , year
                  , releasetype
                  , genre
                  , label
                  , albumartist
                  , trackartist
                )
                SELECT
                    t.rowid
                  , process_string_for_fts(t.title) AS tracktitle
                  , process_string_for_fts(t.tracknumber) AS tracknumber
                  , process_string_for_fts(t.discnumber) AS discnumber
                  , process_string_for_fts(r.title) AS albumtitle
                  , process_string_for_fts(r.year) AS year
                  , process_string_for_fts(r.releasetype) AS releasetype
                  , process_string_for_fts(COALESCE(GROUP_CONCAT(rg.genre, ' '), '')) AS genre
                  , process_string_for_fts(COALESCE(GROUP_CONCAT(rl.label, ' '), '')) AS label
                  , process_string_for_fts(COALESCE(GROUP_CONCAT(ra.artist, ' '), '')) AS albumartist
                  , process_string_for_fts(COALESCE(GROUP_CONCAT(ta.artist, ' '), '')) AS trackartist
                FROM tracks t
                JOIN releases r ON r.id = t.release_id
                LEFT JOIN releases_genres rg ON rg.release_id = r.id
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
                list(upd_track_ids),
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
                    SELECT id, title, year, artist_names, artist_roles FROM releases_view
                    WHERE id IN ({','.join(['?']*len(releases))})
                    """,
                    cached_collage.release_ids,
                )
                for row in cursor:
                    desc_map[row["id"]] = calculate_release_logtext(
                        title=row["title"],
                        year=row["year"],
                        artists=_unpack_artists(c, row["artist_names"], row["artist_roles"]),
                    )
                for i, rls in enumerate(releases):
                    with contextlib.suppress(KeyError):
                        releases[i]["description_meta"] = desc_map[rls["uuid"]]
                    if rls.get("missing", False):
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
                    SELECT id, title, source_path, artist_names, artist_roles FROM tracks_view
                    WHERE id IN ({','.join(['?']*len(tracks))})
                    """,
                    cached_playlist.track_ids,
                )
                for row in cursor:
                    desc_map[row["id"]] = calculate_track_logtext(
                        title=row["title"],
                        artists=_unpack_artists(c, row["artist_names"], row["artist_roles"]),
                        suffix=Path(row["source_path"]).suffix,
                    )
                for i, trk in enumerate(tracks):
                    with contextlib.suppress(KeyError):
                        tracks[i]["description_meta"] = desc_map[trk["uuid"]]
                    if trk.get("missing", False):
                        tracks[i]["description_meta"] += " {MISSING}"

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


def list_releases(
    c: Config,
    sanitized_artist_filter: str | None = None,
    sanitized_genre_filter: str | None = None,
    sanitized_label_filter: str | None = None,
    new: bool | None = None,
) -> Iterator[CachedRelease]:
    with connect(c) as conn:
        query = """
            SELECT
                id
              , source_path
              , cover_image_path
              , added_at
              , datafile_mtime
              , title
              , releasetype
              , year
              , multidisc
              , new
              , genres
              , labels
              , artist_names
              , artist_roles
            FROM releases_view
            WHERE 1=1
        """
        args: list[str | bool] = []
        if sanitized_artist_filter:
            sanitized_artists: list[str] = [sanitized_artist_filter]
            for alias in c.sanitized_artist_aliases_map.get(sanitized_artist_filter, []):
                sanitized_artists.append(alias)
            query += f"""
                AND EXISTS (
                    SELECT * FROM releases_artists
                    WHERE release_id = id AND artist_sanitized IN ({','.join(['?']*len(sanitized_artists))})
                )
            """
            args.extend(sanitized_artists)
        if sanitized_genre_filter:
            query += """
                AND EXISTS (
                    SELECT * FROM releases_genres
                    WHERE release_id = id AND genre_sanitized = ?
                )
            """
            args.append(sanitized_genre_filter)
        if sanitized_label_filter:
            query += """
                AND EXISTS (
                    SELECT * FROM releases_labels
                    WHERE release_id = id AND label_sanitized = ?
                )
            """
            args.append(sanitized_label_filter)
        if new is not None:
            query += "AND new = ?"
            args.append(new)
        query += " ORDER BY source_path"

        cursor = conn.execute(query, args)
        for row in cursor:
            yield CachedRelease(
                id=row["id"],
                source_path=Path(row["source_path"]),
                cover_image_path=Path(row["cover_image_path"]) if row["cover_image_path"] else None,
                added_at=row["added_at"],
                datafile_mtime=row["datafile_mtime"],
                title=row["title"],
                releasetype=row["releasetype"],
                year=row["year"],
                multidisc=bool(row["multidisc"]),
                new=bool(row["new"]),
                genres=_split(row["genres"]),
                labels=_split(row["labels"]),
                artists=_unpack_artists(c, row["artist_names"], row["artist_roles"]),
            )


def list_releases_with_tracks(
    c: Config,
    release_ids: str,
) -> list[tuple[CachedRelease, list[CachedTrack]]]:
    rval: list[tuple[CachedRelease, list[CachedTrack]]] = []
    tracksmap: dict[str, tuple[CachedRelease, list[CachedTrack]]] = {}
    with connect(c) as conn:
        cursor = conn.execute(
            f"""
            SELECT
                id
              , source_path
              , cover_image_path
              , added_at
              , datafile_mtime
              , title
              , releasetype
              , year
              , multidisc
              , new
              , genres
              , labels
              , artist_names
              , artist_roles
            FROM releases_view
            WHERE id IN ({','.join(['?']*len(release_ids))})
            """,
            release_ids,
        )
        for row in cursor:
            release = CachedRelease(
                id=row["id"],
                source_path=Path(row["source_path"]),
                cover_image_path=Path(row["cover_image_path"]) if row["cover_image_path"] else None,
                added_at=row["added_at"],
                datafile_mtime=row["datafile_mtime"],
                title=row["title"],
                releasetype=row["releasetype"],
                year=row["year"],
                multidisc=bool(row["multidisc"]),
                new=bool(row["new"]),
                genres=_split(row["genres"]),
                labels=_split(row["labels"]),
                artists=_unpack_artists(c, row["artist_names"], row["artist_roles"]),
            )
            tracks: list[CachedTrack] = []
            tup = (release, tracks)
            rval.append(tup)
            tracksmap[release.id] = tup

        cursor = conn.execute(
            f"""
            SELECT
                id
              , release_id
              , source_path
              , source_mtime
              , title
              , release_id
              , tracknumber
              , discnumber
              , duration_seconds
              , artist_names
              , artist_roles
            FROM tracks_view
            WHERE release_id IN ({','.join(['?']*len(release_ids))})
            ORDER BY release_id, FORMAT('%4d.%4d', discnumber, tracknumber)
            """,
            release_ids,
        )
        for row in cursor:
            tracksmap[row["release_id"]][1].append(
                CachedTrack(
                    id=row["id"],
                    source_path=Path(row["source_path"]),
                    source_mtime=row["source_mtime"],
                    title=row["title"],
                    release_id=row["release_id"],
                    tracknumber=row["tracknumber"],
                    discnumber=row["discnumber"],
                    duration_seconds=row["duration_seconds"],
                    artists=_unpack_artists(c, row["artist_names"], row["artist_roles"]),
                    release_multidisc=tracksmap[row["release_id"]][0].multidisc,
                )
            )

    return rval


def list_tracks(c: Config, track_ids: list[str]) -> list[CachedTrack]:
    rval: list[CachedTrack] = []
    with connect(c) as conn:
        cursor = conn.execute(
            f"""
            SELECT
                t.id
              , t.release_id
              , t.source_path
              , t.source_mtime
              , t.title
              , t.release_id
              , t.tracknumber
              , t.discnumber
              , t.duration_seconds
              , t.artist_names
              , t.artist_roles
              , r.multidisc
            FROM tracks_view t
            JOIN releases r ON r.id = t.release_id
            WHERE t.id IN ({','.join(['?']*len(track_ids))})
            ORDER BY r.source_path, FORMAT('%4d.%4d', t.discnumber, t.tracknumber)
            """,
            track_ids,
        )
        for row in cursor:
            rval.append(
                CachedTrack(
                    id=row["id"],
                    source_path=Path(row["source_path"]),
                    source_mtime=row["source_mtime"],
                    title=row["title"],
                    release_id=row["release_id"],
                    tracknumber=row["tracknumber"],
                    discnumber=row["discnumber"],
                    duration_seconds=row["duration_seconds"],
                    artists=_unpack_artists(c, row["artist_names"], row["artist_roles"]),
                    release_multidisc=bool(row["multidisc"]),
                )
            )

    return rval


def get_release(c: Config, release_id: str) -> tuple[CachedRelease, list[CachedTrack]] | None:
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT
                id
              , source_path
              , cover_image_path
              , added_at
              , datafile_mtime
              , title
              , releasetype
              , year
              , multidisc
              , new
              , genres
              , labels
              , artist_names
              , artist_roles
            FROM releases_view
            WHERE id = ?
            """,
            (release_id,),
        )
        row = cursor.fetchone()
        if not row:
            return None
        release = CachedRelease(
            id=row["id"],
            source_path=Path(row["source_path"]),
            cover_image_path=Path(row["cover_image_path"]) if row["cover_image_path"] else None,
            added_at=row["added_at"],
            datafile_mtime=row["datafile_mtime"],
            title=row["title"],
            releasetype=row["releasetype"],
            year=row["year"],
            multidisc=bool(row["multidisc"]),
            new=bool(row["new"]),
            genres=_split(row["genres"]) if row["genres"] else [],
            labels=_split(row["labels"]) if row["labels"] else [],
            artists=_unpack_artists(c, row["artist_names"], row["artist_roles"]),
        )

        tracks: list[CachedTrack] = []
        cursor = conn.execute(
            """
            SELECT
                t.id
              , t.source_path
              , t.source_mtime
              , t.title
              , t.release_id
              , t.tracknumber
              , t.discnumber
              , t.duration_seconds
              , t.artist_names
              , t.artist_roles
            FROM tracks_view t
            JOIN releases r ON r.id = t.release_id
            WHERE r.id = ?
            ORDER BY FORMAT('%4d.%4d', t.discnumber, t.tracknumber)
            """,
            (release_id,),
        )
        for row in cursor:
            tracks.append(
                CachedTrack(
                    id=row["id"],
                    source_path=Path(row["source_path"]),
                    source_mtime=row["source_mtime"],
                    title=row["title"],
                    release_id=row["release_id"],
                    tracknumber=row["tracknumber"],
                    discnumber=row["discnumber"],
                    duration_seconds=row["duration_seconds"],
                    artists=_unpack_artists(c, row["artist_names"], row["artist_roles"]),
                    release_multidisc=release.multidisc,
                )
            )

    return (release, tracks)


def get_release_logtext(c: Config, release_id: str) -> str | None:
    """Get a human-readable identifier for a release suitable for logging."""
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT title, year, artist_names, artist_roles FROM releases_view WHERE id = ?",
            (release_id,),
        )
        row = cursor.fetchone()
        if not row:
            return None
        return calculate_release_logtext(
            title=row["title"],
            year=row["year"],
            artists=_unpack_artists(c, row["artist_names"], row["artist_roles"]),
        )


def calculate_release_logtext(
    title: str,
    year: int | None,
    artists: ArtistMapping,
) -> str:
    logtext = f"{artistsfmt(artists)} - "
    if year:
        logtext += f"{year}. "
    logtext += title
    return logtext


def get_release_source_path(c: Config, uuid: str) -> Path | None:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT source_path FROM releases WHERE id = ?",
            (uuid,),
        )
        if row := cursor.fetchone():
            return Path(row["source_path"])
        return None


def get_release_source_paths(c: Config, uuids: list[str]) -> list[Path]:
    with connect(c) as conn:
        cursor = conn.execute(
            f"SELECT source_path FROM releases WHERE id IN ({','.join(['?']*len(uuids))})",
            uuids,
        )
        return [Path(r["source_path"]) for r in cursor]


def get_track(c: Config, uuid: str) -> CachedTrack | None:
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT
                id
              , source_path
              , source_mtime
              , title
              , release_id
              , tracknumber
              , discnumber
              , duration_seconds
              , multidisc
              , artist_names
              , artist_roles
            FROM tracks_view
            WHERE id = ?
            """,
            (uuid,),
        )
        row = cursor.fetchone()
        if not row:
            return None
        return CachedTrack(
            id=row["id"],
            source_path=Path(row["source_path"]),
            source_mtime=row["source_mtime"],
            title=row["title"],
            release_id=row["release_id"],
            tracknumber=row["tracknumber"],
            discnumber=row["discnumber"],
            duration_seconds=row["duration_seconds"],
            artists=_unpack_artists(c, row["artist_names"], row["artist_roles"]),
            release_multidisc=row["multidisc"],
        )


def get_track_logtext(c: Config, track_id: str) -> str | None:
    """Get a human-readable identifier for a track suitable for logging."""
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT title, source_path, artist_names, artist_roles FROM tracks_view WHERE id = ?",
            (track_id,),
        )
        row = cursor.fetchone()
        if not row:
            return None
        return calculate_track_logtext(
            title=row["title"],
            artists=_unpack_artists(c, row["artist_names"], row["artist_roles"]),
            suffix=Path(row["source_path"]).suffix,
        )


def calculate_track_logtext(title: str, artists: ArtistMapping, suffix: str) -> str:
    return f"{artistsfmt(artists)} - {title or 'Unknown Title'}{suffix}"


def list_playlists(c: Config) -> Iterator[str]:
    with connect(c) as conn:
        cursor = conn.execute("SELECT DISTINCT name FROM playlists")
        for row in cursor:
            yield row["name"]


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
            SELECT
                t.id
              , t.source_path
              , t.source_mtime
              , t.title
              , t.release_id
              , t.tracknumber
              , t.discnumber
              , t.duration_seconds
              , r.multidisc
              , t.artist_names
              , t.artist_roles
            FROM tracks_view t
            JOIN releases r ON r.id = t.release_id
            JOIN playlists_tracks pt ON pt.track_id = t.id
            WHERE pt.playlist_name = ? AND NOT pt.missing
            ORDER BY pt.position ASC
            """,
            (playlist_name,),
        )
        tracks: list[CachedTrack] = []
        for row in cursor:
            playlist.track_ids.append(row["id"])
            tracks.append(
                CachedTrack(
                    id=row["id"],
                    source_path=Path(row["source_path"]),
                    source_mtime=row["source_mtime"],
                    title=row["title"],
                    release_id=row["release_id"],
                    tracknumber=row["tracknumber"],
                    discnumber=row["discnumber"],
                    duration_seconds=row["duration_seconds"],
                    artists=_unpack_artists(c, row["artist_names"], row["artist_roles"]),
                    release_multidisc=row["multidisc"],
                )
            )

    return playlist, tracks


def list_collages(c: Config) -> Iterator[str]:
    with connect(c) as conn:
        cursor = conn.execute("SELECT DISTINCT name FROM collages")
        for row in cursor:
            yield row["name"]


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
            SELECT
                r.id
              , r.source_path
              , r.cover_image_path
              , r.added_at
              , r.datafile_mtime
              , r.title
              , r.releasetype
              , r.year
              , r.multidisc
              , r.new
              , r.genres
              , r.labels
              , r.artist_names
              , r.artist_roles
            FROM releases_view r
            JOIN collages_releases cr ON cr.release_id = r.id
            WHERE cr.collage_name = ?
            ORDER BY cr.position ASC
            """,
            (collage_name,),
        )
        releases: list[CachedRelease] = []
        for row in cursor:
            collage.release_ids.append(row["id"])
            releases.append(
                CachedRelease(
                    id=row["id"],
                    source_path=Path(row["source_path"]),
                    cover_image_path=Path(row["cover_image_path"])
                    if row["cover_image_path"]
                    else None,
                    added_at=row["added_at"],
                    datafile_mtime=row["datafile_mtime"],
                    title=row["title"],
                    releasetype=row["releasetype"],
                    year=row["year"],
                    multidisc=bool(row["multidisc"]),
                    new=bool(row["new"]),
                    genres=_split(row["genres"]) if row["genres"] else [],
                    labels=_split(row["labels"]) if row["labels"] else [],
                    artists=_unpack_artists(c, row["artist_names"], row["artist_roles"]),
                )
            )

    return (collage, releases)


def list_artists(c: Config) -> Iterator[tuple[str, str]]:
    with connect(c) as conn:
        cursor = conn.execute("SELECT DISTINCT artist, artist_sanitized FROM releases_artists")
        for row in cursor:
            yield row["artist"], row["artist_sanitized"]


def artist_exists(c: Config, artist_sanitized: str) -> bool:
    args: list[str] = [artist_sanitized]
    for alias in c.sanitized_artist_aliases_map.get(artist_sanitized, []):
        args.append(alias)
    with connect(c) as conn:
        cursor = conn.execute(
            f"""
            SELECT EXISTS(
                SELECT * FROM releases_artists
                WHERE artist_sanitized IN ({','.join(['?']*len(args))})
            )
            """,
            args,
        )
        return bool(cursor.fetchone()[0])


def list_genres(c: Config) -> Iterator[tuple[str, str]]:
    with connect(c) as conn:
        cursor = conn.execute("SELECT DISTINCT genre, genre_sanitized FROM releases_genres")
        for row in cursor:
            yield row["genre"], row["genre_sanitized"]


def genre_exists(c: Config, genre_sanitized: str) -> bool:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT EXISTS(SELECT * FROM releases_genres WHERE genre_sanitized = ?)",
            (genre_sanitized,),
        )
        return bool(cursor.fetchone()[0])


def list_labels(c: Config) -> Iterator[tuple[str, str]]:
    with connect(c) as conn:
        cursor = conn.execute("SELECT DISTINCT label, label_sanitized FROM releases_labels")
        for row in cursor:
            yield row["label"], row["label_sanitized"]


def label_exists(c: Config, label_sanitized: str) -> bool:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT EXISTS(SELECT * FROM releases_labels WHERE label_sanitized = ?)",
            (label_sanitized,),
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
    for name, role in _unpack(names, roles):
        role_artists: list[Artist] = getattr(mapping, role)
        role_artists.append(Artist(name=name, alias=False))
        seen: set[str] = {name}
        if aliases:
            for alias in c.artist_aliases_parents_map.get(name, []):
                if alias not in seen:
                    role_artists.append(Artist(name=alias, alias=True))
                    seen.add(alias)
    return mapping


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
        return
    yield from zip(*[_split(xs) for xs in xxs])


def process_string_for_fts(x: str) -> str:
    # In order to have performant substring search, we use FTS and hack it such that every character
    # is a token. We use "¬" as our separator character, hoping that it is not used in any metadata.
    return "¬".join(str(x)) if x else x
