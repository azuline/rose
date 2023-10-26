import contextlib
import copy
import hashlib
import logging
import math
import multiprocessing
import os
import os.path
import re
import sqlite3
import time
import traceback
from collections.abc import Iterator
from dataclasses import asdict, dataclass
from datetime import datetime
from pathlib import Path
from typing import Any, TypeVar

import tomli_w
import tomllib
import uuid6

from rose.artiststr import format_artist_string
from rose.config import Config
from rose.tagger import SUPPORTED_EXTENSIONS, AudioFile

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


def migrate_database(c: Config) -> None:
    """
    "Migrate" the database. If the schema in the database does not match that on disk, then nuke the
    database and recreate it from scratch. Otherwise, no op.

    We can do this because the database is just a read cache. It is not source-of-truth for any of
    its own data.
    """
    with CACHE_SCHEMA_PATH.open("rb") as fp:
        schema_hash = hashlib.sha256(fp.read()).hexdigest()

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
            cursor = conn.execute("SELECT schema_hash, config_hash FROM _schema_hash")
            row = cursor.fetchone()
            if row and row["schema_hash"] == schema_hash and row["config_hash"] == c.hash:
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
              , PRIMARY KEY (schema_hash, config_hash)
            )
            """
        )
        conn.execute(
            "INSERT INTO _schema_hash (schema_hash, config_hash) VALUES (?, ?)",
            (schema_hash, c.hash),
        )


@contextlib.contextmanager
def lock(c: Config, name: str, timeout: float = 1.0) -> Iterator[None]:
    try:
        while True:
            with connect(c) as conn:
                cursor = conn.execute("SELECT MAX(valid_until) FROM locks WHERE name = ?", (name,))
                row = cursor.fetchone()
                if not row or not row[0] or row[0] < time.time():
                    logger.debug(f"Acquiring lock for {name} with timeout {timeout}")
                    valid_until = time.time() + timeout
                    conn.execute(
                        "INSERT INTO locks (name, valid_until) VALUES (?, ?)", (name, valid_until)
                    )
                    break
                sleep = max(0, row[0] - time.time())
                logger.debug(f"Failed to acquire lock for {name}: sleeping for {sleep}")
                time.sleep(sleep)
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
class CachedArtist:
    name: str
    role: str
    # Whether this artist is an aliased name of the artist that the release was actually released
    # under. We include aliased artists in the cached state because it lets us relate releases from
    # the aliased name to the main artist. However, we ignore these artists when computing the
    # virtual dirname and tags.
    alias: bool = False


@dataclass
class CachedRelease:
    id: str
    source_path: Path
    cover_image_path: Path | None
    added_at: str  # ISO8601 timestamp
    datafile_mtime: str
    virtual_dirname: str
    title: str
    releasetype: str
    year: int | None
    new: bool
    multidisc: bool
    genres: list[str]
    labels: list[str]
    artists: list[CachedArtist]
    formatted_artists: str


@dataclass
class CachedTrack:
    id: str
    source_path: Path
    source_mtime: str
    virtual_filename: str
    title: str
    release_id: str
    track_number: str
    disc_number: str
    formatted_release_position: str
    duration_seconds: int

    artists: list[CachedArtist]
    formatted_artists: str


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


VALID_COVER_EXTENSIONS = [".jpg", ".jpeg", ".png"]
VALID_COVER_FILENAMES = [x + y for x in ["cover", "folder", "art"] for y in VALID_COVER_EXTENSIONS]

RELEASE_TYPE_FORMATTER = {
    "album": "Album",
    "single": "Single",
    "ep": "EP",
    "compilation": "Compilation",
    "soundtrack": "Soundtrack",
    "live": "Live",
    "remix": "Remix",
    "djmix": "DJ-Mix",
    "mixtape": "Mixtape",
    "other": "Other",
    "unknown": "Unknown",
}

STORED_DATA_FILE_REGEX = re.compile(r"\.rose\.([^.]+)\.toml")


def update_cache(c: Config, force: bool = False) -> None:
    """
    Update the read cache to match the data for all releases in the music source directory. Delete
    any cached releases that are no longer present on disk.
    """
    update_cache_for_releases(c, None, force)
    update_cache_evict_nonexistent_releases(c)
    update_cache_for_collages(c, None, force)
    update_cache_evict_nonexistent_collages(c)
    update_cache_for_playlists(c, None, force)
    update_cache_evict_nonexistent_playlists(c)


def update_cache_evict_nonexistent_releases(c: Config) -> None:
    logger.info("Evicting cached releases that are not on disk")
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
            logger.info(f"Evicted release {row['source_path']} from cache")


def update_cache_for_releases(
    c: Config,
    # Leave as None to update all releases.
    release_dirs: list[Path] | None = None,
    force: bool = False,
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
    logger.info(f"Refreshing the read cache for {len(release_dirs)} releases")
    logger.debug(f"Refreshing cached data for {', '.join([r.name for r in release_dirs])}")

    # Batch size defaults to equal split across all processes. However, if the number of directories
    # is small, we shrink the # of processes to save on overhead.
    num_proc = c.max_proc
    if len(release_dirs) < c.max_proc * 50:
        num_proc = max(1, math.ceil(len(release_dirs) // 50))
    batch_size = len(release_dirs) // num_proc + 1

    # Track the known virtual dirnames for collision calculation. This needs to be shared across
    # all processes, because we want to compare against the global set of known virtual dirnames.
    manager = multiprocessing.Manager()
    known_virtual_dirnames = manager.dict()
    # Create a queue to propagate exceptions back up to the parent.
    error_queue = manager.Queue()

    with multiprocessing.Pool(processes=c.max_proc) as pool:
        # At 0, no batch. At 1, 1 batch. At 49, 1 batch. At 50, 1 batch. At 51, 2 batches.
        for i in range(0, len(release_dirs), batch_size):
            logger.debug(
                f"Spawning release cache update process for releases [{i}, {i+batch_size})"
            )
            pool.apply_async(
                _update_cache_for_releases_process,
                (c, release_dirs[i : i + batch_size], force, known_virtual_dirnames, error_queue),
            )
        pool.close()
        pool.join()

    if not error_queue.empty():
        etype, tb = error_queue.get()
        raise etype(f"Error in cache update subprocess.\n{tb}")


def _update_cache_for_releases_process(
    c: Config,
    release_dirs: list[Path],
    force: bool,
    known_virtual_dirnames: dict[str, bool],
    error_queue: "multiprocessing.Queue[Any]",
) -> None:
    """General error handling stuff for the cache update subprocess."""
    try:
        return _update_cache_for_releases_executor(c, release_dirs, force, known_virtual_dirnames)
    except Exception as e:
        # Use traceback.format_exc() to get the formatted traceback string
        tb = traceback.format_exc()
        error_queue.put((type(e), tb))


def _update_cache_for_releases_executor(
    c: Config,
    release_dirs: list[Path],
    force: bool,
    known_virtual_dirnames: dict[str, bool],
) -> None:
    """The implementation logic, split out for multiprocessing."""
    # First, call readdir on every release directory. We store the results in a map of
    # Path Basename -> (Release ID if exists, filenames).
    dir_tree: list[tuple[Path, str | None, list[str]]] = []
    release_uuids: list[str] = []
    for rd in release_dirs:
        release_id = None
        files: list[str] = []
        if not rd.is_dir():
            logger.debug(f"Skipping scanning {rd} because it is not a directory")
            continue
        for root, _, fx in os.walk(str(rd)):
            for f in fx:
                if m := STORED_DATA_FILE_REGEX.match(f):
                    release_id = m[1]
                files.append(os.path.join(root, f))
        dir_tree.append((rd.resolve(), release_id, files))
        if release_id is not None:
            release_uuids.append(release_id)

    # Then batch query for all metadata associated with the discovered IDs. This pulls all data into
    # memory for fast access throughout this function. We do this in two passes (and two queries!):
    # 1. Fetch all releases.
    # 2. Fetch all tracks in a single query, and then associates each track with a release.
    # The tracks are stored as a dict of source_path -> Track.
    cached_releases: dict[str, tuple[CachedRelease, dict[str, CachedTrack]]] = {}
    with connect(c) as conn:
        cursor = conn.execute(
            rf"""
            WITH genres AS (
                SELECT
                    release_id
                  , GROUP_CONCAT(genre, ' \\ ') AS genres
                FROM releases_genres
                GROUP BY release_id
            ), labels AS (
                SELECT
                    release_id
                  , GROUP_CONCAT(label, ' \\ ') AS labels
                FROM releases_labels
                GROUP BY release_id
            ), artists AS (
                SELECT
                    release_id
                  , GROUP_CONCAT(artist, ' \\ ') AS names
                  , GROUP_CONCAT(role, ' \\ ') AS roles
                FROM releases_artists
                GROUP BY release_id
            )
            SELECT
                r.id
              , r.source_path
              , r.cover_image_path
              , r.added_at
              , r.datafile_mtime
              , r.virtual_dirname
              , r.title
              , r.release_type
              , r.release_year
              , r.multidisc
              , r.new
              , r.formatted_artists
              , COALESCE(g.genres, '') AS genres
              , COALESCE(l.labels, '') AS labels
              , COALESCE(a.names, '') AS artist_names
              , COALESCE(a.roles, '') AS artist_roles
            FROM releases r
            LEFT JOIN genres g ON g.release_id = r.id
            LEFT JOIN labels l ON l.release_id = r.id
            LEFT JOIN artists a ON a.release_id = r.id
            WHERE r.id IN ({','.join(['?']*len(release_uuids))})
            """,
            release_uuids,
        )
        for row in cursor:
            release_artists: list[CachedArtist] = []
            for n, r in zip(row["artist_names"].split(r" \\ "), row["artist_roles"].split(r" \\ ")):
                if not n:
                    # This can occur if there are no artist names; then we get a single iteration
                    # with empty string.
                    continue
                release_artists.append(CachedArtist(name=n, role=r))
            cached_releases[row["id"]] = (
                CachedRelease(
                    id=row["id"],
                    source_path=Path(row["source_path"]),
                    cover_image_path=Path(row["cover_image_path"])
                    if row["cover_image_path"]
                    else None,
                    added_at=row["added_at"],
                    datafile_mtime=row["datafile_mtime"],
                    virtual_dirname=row["virtual_dirname"],
                    title=row["title"],
                    releasetype=row["release_type"],
                    year=row["release_year"],
                    multidisc=bool(row["multidisc"]),
                    new=bool(row["new"]),
                    genres=row["genres"].split(r" \\ ") if row["genres"] else [],
                    labels=row["labels"].split(r" \\ ") if row["labels"] else [],
                    artists=release_artists,
                    formatted_artists=row["formatted_artists"],
                ),
                {},
            )
            known_virtual_dirnames[row["virtual_dirname"]] = True

        logger.debug(f"Found {len(cached_releases)}/{len(release_dirs)} releases in cache")

        cursor = conn.execute(
            rf"""
            WITH artists AS (
                SELECT
                    track_id
                  , GROUP_CONCAT(artist, ' \\ ') AS names
                  , GROUP_CONCAT(role, ' \\ ') AS roles
                FROM tracks_artists
                GROUP BY track_id
            )
            SELECT
                t.id
              , t.source_path
              , t.source_mtime
              , t.virtual_filename
              , t.title
              , t.release_id
              , t.track_number
              , t.disc_number
              , t.formatted_release_position
              , t.duration_seconds
              , t.formatted_artists
              , COALESCE(a.names, '') AS artist_names
              , COALESCE(a.roles, '') AS artist_roles
            FROM tracks t
            JOIN releases r ON r.id = t.release_id
            LEFT JOIN artists a ON a.track_id = t.id
            WHERE r.id IN ({','.join(['?']*len(release_uuids))})
            """,
            release_uuids,
        )
        num_tracks_found = 0
        for row in cursor:
            track_artists: list[CachedArtist] = []
            for n, r in zip(row["artist_names"].split(r" \\ "), row["artist_roles"].split(r" \\ ")):
                if not n:
                    # This can occur if there are no artist names; then we get a single iteration
                    # with empty string.
                    continue
                track_artists.append(CachedArtist(name=n, role=r))
            cached_releases[row["release_id"]][1][row["source_path"]] = CachedTrack(
                id=row["id"],
                source_path=Path(row["source_path"]),
                source_mtime=row["source_mtime"],
                virtual_filename=row["virtual_filename"],
                title=row["title"],
                release_id=row["release_id"],
                track_number=row["track_number"],
                disc_number=row["disc_number"],
                formatted_release_position=row["formatted_release_position"],
                duration_seconds=row["duration_seconds"],
                artists=track_artists,
                formatted_artists=row["formatted_artists"],
            )
            num_tracks_found += 1

        logger.debug(f"Found {num_tracks_found} tracks in cache")

    # Now iterate over all releases in the source directory. Leverage mtime from stat to determine
    # whether to even check the file tags or not. Compute the necessary database updates and store
    # them in the `upd_` variables. After this loop, we will execute the database updates based on
    # the `upd_` varaibles.
    loop_start = time.time()
    upd_delete_source_paths: list[str] = []
    upd_release_args: list[list[Any]] = []
    upd_release_artist_args: list[list[Any]] = []
    upd_release_genre_args: list[list[Any]] = []
    upd_release_label_args: list[list[Any]] = []
    upd_unknown_cached_tracks_args: list[tuple[str, list[str]]] = []
    upd_track_args: list[list[Any]] = []
    upd_track_artist_args: list[list[Any]] = []
    # The following two variables store updates for a collage's and playlist's description_meta
    # fields. Map of entity id -> dir/filename.
    upd_collage_release_dirnames: dict[str, str] = {}
    upd_playlist_track_filenames: dict[str, str] = {}
    for source_path, preexisting_release_id, files in dir_tree:
        logger.debug(f"Updating release {source_path.name}")
        # Check to see if we should even process the directory. If the directory does not have
        # any tracks, skip it. And if it does not have any tracks, but is in the cache, remove
        # it from the cache.
        for f in files:
            if any(f.lower().endswith(ext) for ext in SUPPORTED_EXTENSIONS):
                break
        else:
            logger.debug(f"Did not find any audio files in release {source_path}, skipping")
            logger.debug(f"Scheduling cache deletion for empty directory release {source_path}")
            upd_delete_source_paths.append(str(source_path))
            continue

        # This value is used to track whether to update the database for this release. If this
        # is False at the end of this loop body, we can save a database update call.
        release_dirty = False

        # Fetch the release from the cache. We will be updating this value on-the-fly, so
        # instantiate to zero values if we do not have a default value.
        try:
            release, cached_tracks = cached_releases[preexisting_release_id or ""]
        except KeyError:
            logger.debug(
                f"First-time unidentified release found at release {source_path}, "
                "writing UUID and new"
            )
            release_dirty = True
            release = CachedRelease(
                id=preexisting_release_id or "",
                source_path=source_path,
                datafile_mtime="",
                cover_image_path=None,
                added_at="",
                virtual_dirname="",
                title="",
                releasetype="",
                year=None,
                new=True,
                multidisc=False,
                genres=[],
                labels=[],
                artists=[],
                formatted_artists="",
            )
            cached_tracks = {}

        # Handle source path change; if it's changed, update the release.
        if source_path != release.source_path:
            logger.debug(f"Source path change detected for release {source_path}, updating")
            release.source_path = source_path
            release_dirty = True

        # The directory does not have a release ID, so create the stored data file.
        if not preexisting_release_id:
            logger.debug(f"Creating new stored data file for release {source_path}")
            stored_release_data = StoredDataFile(
                new=True,
                added_at=datetime.now().astimezone().replace(microsecond=0).isoformat(),
            )
            new_release_id = str(uuid6.uuid7())
            datafile_path = source_path / f".rose.{new_release_id}.toml"
            # No need to lock here, as since the release ID is new, there is no way there is a
            # concurrent writer.
            with datafile_path.open("wb") as fp:
                tomli_w.dump(asdict(stored_release_data), fp)
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
                with lock(c, release_lock_name(preexisting_release_id)):
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
                    # And then write the data back to disk if it changed. This allows us to update
                    # datafiles to contain newer default values.
                    new_resolved_data = asdict(datafile)
                    if new_resolved_data != diskdata:
                        logger.debug(
                            f"Updating values in stored data file for release {source_path}"
                        )
                        with datafile_path.open("wb") as fp:
                            tomli_w.dump(new_resolved_data, fp)

        # Handle cover art change.
        try:
            cover = next(
                Path(f).resolve() for f in files if os.path.basename(f) in VALID_COVER_FILENAMES
            )
        except StopIteration:  # No cover art in directory.
            cover = None
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
        #
        # Note that we do NOT calculate the virtual_filename in this loop, because we need to
        # know whether the release is multidisc to do that. But we only know whether a release
        # is multidisc after having all the track metadata. So we do virtual_dirname calculation
        # in a follow-up loop.
        tracks: list[CachedTrack] = []
        track_ids_to_insert: set[str] = set()
        # This value is set to true if we read an AudioFile and used it to confirm the release
        # tags.
        pulled_release_tags = False
        for f in files:
            if not any(os.path.basename(f).lower().endswith(ext) for ext in SUPPORTED_EXTENSIONS):
                continue
            cached_track = cached_tracks.get(f, None)
            track_mtime = str(os.stat(f).st_mtime)
            # Skip re-read if we can reuse a cached entry.
            if cached_track and track_mtime == cached_track.source_mtime and not force:
                logger.debug(
                    f"Track cache hit (mtime) for {os.path.basename(f)}, reusing cached data"
                )
                tracks.append(cached_track)
                unknown_cached_tracks.remove(f)
                continue

            # Otherwise, read tags from disk and construct a new cached_track.
            logger.debug(f"Track cache miss for {os.path.basename(f)}, reading tags from disk")
            tags = AudioFile.from_file(Path(f))

            # Now that we're here, pull the release tags. We also need them to compute the
            # formatted artist string.
            if not pulled_release_tags:
                pulled_release_tags = True
                release_title = tags.album or "Unknown Release"
                if release_title != release.title:
                    logger.debug(f"Release title change detected for {source_path}, updating")
                    release.title = release_title
                    release_dirty = True

                release_type = tags.release_type
                if release_type != release.releasetype:
                    logger.debug(f"Release type change detected for {source_path}, updating")
                    release.releasetype = release_type
                    release_dirty = True

                if tags.year != release.year:
                    logger.debug(f"Release year change detected for {source_path}, updating")
                    release.year = tags.year
                    release_dirty = True

                if set(tags.genre) != set(release.genres):
                    logger.debug(f"Release genre change detected for {source_path}, updating")
                    release.genres = _uniq(tags.genre)
                    release_dirty = True

                if set(tags.label) != set(release.labels):
                    logger.debug(f"Release label change detected for {source_path}, updating")
                    release.labels = _uniq(tags.label)
                    release_dirty = True

                release_artists = []
                for role, names in asdict(tags.album_artists).items():
                    for name in _uniq(names):
                        release_artists.append(CachedArtist(name=name, role=role))
                        # And also make sure we attach any parent aliases for this artist.
                        for alias in c.artist_aliases_parents_map.get(name, []):
                            release_artists.append(CachedArtist(name=alias, role=role, alias=True))
                if release_artists != release.artists:
                    logger.debug(f"Release artists change detected for {source_path}, updating")
                    release.artists = release_artists
                    release_dirty = True

                release_formatted_artists = format_artist_string(tags.album_artists, release.genres)
                if release_formatted_artists != release.formatted_artists:
                    logger.debug(
                        f"Release formatted artists change detected for {source_path}, updating"
                    )
                    release.formatted_artists = release_formatted_artists
                    release_dirty = True

                # Calculate the release's virtual dirname.
                release_virtual_dirname = release.formatted_artists + " - "
                if release.year:
                    release_virtual_dirname += str(release.year) + ". "
                release_virtual_dirname += release.title
                if release.releasetype not in ["album", "other", "unknown"] and not (
                    release.releasetype == "remix" and "remix" in release.title.lower()
                ):
                    release_virtual_dirname += " - " + RELEASE_TYPE_FORMATTER.get(
                        release.releasetype, release.releasetype.title()
                    )
                if release.genres:
                    release_virtual_dirname += " [" + ";".join(sorted(release.genres)) + "]"
                if release.labels:
                    release_virtual_dirname += " {" + ";".join(sorted(release.labels)) + "}"
                if release.new:
                    release_virtual_dirname = "{NEW} " + release_virtual_dirname
                release_virtual_dirname = _sanitize_filename(release_virtual_dirname)
                # And in case of a name collision, add an extra number at the end. Iterate to
                # find the first unused number.
                original_virtual_dirname = release_virtual_dirname
                collision_no = 2
                while True:
                    if (
                        release.virtual_dirname == release_virtual_dirname
                        or not known_virtual_dirnames.get(release_virtual_dirname, False)
                    ):
                        break
                    logger.debug(
                        "Virtual dirname collision: "
                        f"{release_virtual_dirname=} {known_virtual_dirnames=}"
                    )
                    release_virtual_dirname = f"{original_virtual_dirname} [{collision_no}]"
                    collision_no += 1

                if release_virtual_dirname != release.virtual_dirname:
                    logger.debug(
                        f"Release virtual dirname change detected for {source_path}, updating"
                    )
                    if release.virtual_dirname in known_virtual_dirnames:
                        known_virtual_dirnames[release.virtual_dirname] = False
                    known_virtual_dirnames[release_virtual_dirname] = True
                    release.virtual_dirname = release_virtual_dirname
                    release_dirty = True
                    upd_collage_release_dirnames[release.id] = release.virtual_dirname

            # Here we compute the track ID. We store the track ID on the audio file in order to
            # enable persistence. This does mutate the file!
            #
            # We don't attempt to optimize this write; however, there is not much purpose to doing
            # so, since this occurs once over the lifetime of the track's existence in Rose. We
            # optimize this function because it is called repeatedly upon every metadata edit, but
            # in this case, we skip this code path once an ID is generated.
            track_id = tags.id
            if not track_id:
                with lock(c, release_lock_name(release.id)):
                    track_id = str(uuid6.uuid7())
                    tags.id = track_id
                    tags.flush()

            # And now create the cached track.
            track = CachedTrack(
                id=track_id,
                source_path=Path(f),
                source_mtime=track_mtime,
                virtual_filename="",
                title=tags.title or "Unknown Title",
                release_id=release.id,
                # Remove `.` here because we use `.` to parse out discno/trackno in the virtual
                # filesystem. It should almost never happen, but better to be safe.
                track_number=(tags.track_number or "1").replace(".", ""),
                disc_number=(tags.disc_number or "1").replace(".", ""),
                # This is calculated with the virtual filename.
                formatted_release_position="",
                duration_seconds=tags.duration_sec,
                artists=[],
                formatted_artists=format_artist_string(tags.artists, release.genres),
            )
            tracks.append(track)
            for role, names in asdict(tags.artists).items():
                for name in _uniq(names):
                    track.artists.append(CachedArtist(name=name, role=role))
                    # And also make sure we attach any parent aliases for this artist.
                    for alias in c.artist_aliases_parents_map.get(name, []):
                        track.artists.append(CachedArtist(name=alias, role=role, alias=True))
            track_ids_to_insert.add(track.id)

        # Now calculate whether this release is multidisc, and then assign virtual_filenames and
        # formatted_release_positions for each track that lacks one.
        multidisc = len({t.disc_number for t in tracks}) > 1
        if release.multidisc != multidisc:
            logger.debug(f"Release multidisc change detected for {source_path}, updating")
            release_dirty = True
            release.multidisc = multidisc
        # Use this set to avoid name collisions.
        seen_track_names: set[str] = set()
        for i, t in enumerate(tracks):
            formatted_release_position = ""
            if multidisc and t.disc_number:
                formatted_release_position += f"{t.disc_number:0>2}-"
            if t.track_number:
                formatted_release_position += f"{t.track_number:0>2}"
            if formatted_release_position != t.formatted_release_position:
                logger.debug(
                    f"Track formatted release position change detected for {t.source_path}, "
                    "updating"
                )
                tracks[i].formatted_release_position = formatted_release_position
                track_ids_to_insert.add(t.id)

            virtual_filename = ""
            virtual_filename += f"{t.formatted_artists} - "
            virtual_filename += t.title or "Unknown Title"
            virtual_filename += t.source_path.suffix
            virtual_filename = _sanitize_filename(virtual_filename)
            # And in case of a name collision, add an extra number at the end. Iterate to find
            # the first unused number.
            original_virtual_filename = virtual_filename
            collision_no = 2
            while True:
                if virtual_filename not in seen_track_names:
                    break
                virtual_filename = f"{original_virtual_filename} [{collision_no}]"
                collision_no += 1
            seen_track_names.add(virtual_filename)
            if virtual_filename != t.virtual_filename:
                logger.debug(
                    f"Track virtual filename change detected for {t.source_path}, updating"
                )
                tracks[i].virtual_filename = virtual_filename
                track_ids_to_insert.add(t.id)
                upd_playlist_track_filenames[t.id] = virtual_filename

        # Schedule database executions.
        if unknown_cached_tracks or release_dirty or track_ids_to_insert:
            logger.info(f"Applying cache updates for release {source_path.name}")

        if unknown_cached_tracks:
            logger.debug(f"Deleting {len(unknown_cached_tracks)} unknown tracks from cache")
            upd_unknown_cached_tracks_args.append((release.id, list(unknown_cached_tracks)))

        if release_dirty:
            logger.debug(f"Scheduling upsert for dirty release in database: {source_path}")
            upd_release_args.append(
                [
                    release.id,
                    str(release.source_path),
                    str(release.cover_image_path) if release.cover_image_path else None,
                    release.added_at,
                    release.datafile_mtime,
                    release.virtual_dirname,
                    release.title,
                    release.releasetype,
                    release.year,
                    release.multidisc,
                    release.new,
                    release.formatted_artists,
                ]
            )
            for genre in release.genres:
                upd_release_genre_args.append([release.id, genre, _sanitize_filename(genre)])
            for label in release.labels:
                upd_release_label_args.append([release.id, label, _sanitize_filename(label)])
            for art in release.artists:
                upd_release_artist_args.append(
                    [release.id, art.name, _sanitize_filename(art.name), art.role, art.alias]
                )

        for track in tracks:
            if track.id not in track_ids_to_insert:
                continue
            logger.debug(f"Scheduling upsert for dirty track in database: {track.source_path}")
            upd_track_args.append(
                [
                    track.id,
                    str(track.source_path),
                    track.source_mtime,
                    track.virtual_filename,
                    track.title,
                    track.release_id,
                    track.track_number,
                    track.disc_number,
                    track.formatted_release_position,
                    track.duration_seconds,
                    track.formatted_artists,
                ]
            )
            for art in track.artists:
                upd_track_artist_args.append(
                    [track.id, art.name, _sanitize_filename(art.name), art.role, art.alias]
                )
    logger.debug(f"Release update scheduling loop time {time.time() - loop_start=}")

    exec_start = time.time()
    with connect(c) as conn:
        if upd_delete_source_paths:
            conn.execute(
                f"""
                DELETE FROM releases
                WHERE source_path IN ({','.join(['?']*len(upd_delete_source_paths))})
                """,
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
                  , virtual_dirname
                  , title
                  , release_type
                  , release_year
                  , multidisc
                  , new
                  , formatted_artists
                ) VALUES {",".join(["(?,?,?,?,?,?,?,?,?,?,?,?)"] * len(upd_release_args))}
                ON CONFLICT (id) DO UPDATE SET
                    source_path       = excluded.source_path
                  , cover_image_path  = excluded.cover_image_path
                  , added_at          = excluded.added_at
                  , datafile_mtime    = excluded.datafile_mtime
                  , virtual_dirname   = excluded.virtual_dirname
                  , title             = excluded.title
                  , release_type      = excluded.release_type
                  , release_year      = excluded.release_year
                  , multidisc         = excluded.multidisc
                  , new               = excluded.new
                  , formatted_artists = excluded.formatted_artists
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
                INSERT INTO releases_genres (release_id, genre, genre_sanitized)
                VALUES {",".join(["(?,?,?)"]*len(upd_release_genre_args))}
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
                INSERT INTO releases_labels (release_id, label, label_sanitized)
                VALUES {",".join(["(?,?,?)"]*len(upd_release_label_args))}
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
                INSERT INTO releases_artists (release_id, artist, artist_sanitized, role, alias)
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
                  , virtual_filename
                  , title
                  , release_id
                  , track_number
                  , disc_number
                  , formatted_release_position
                  , duration_seconds
                  , formatted_artists
                )
                VALUES {",".join(["(?,?,?,?,?,?,?,?,?,?,?)"]*len(upd_track_args))}
                ON CONFLICT (id) DO UPDATE SET
                    source_path                = excluded.source_path
                  , source_mtime               = excluded.source_mtime
                  , virtual_filename           = excluded.virtual_filename
                  , title                      = excluded.title
                  , release_id                 = excluded.release_id
                  , track_number               = excluded.track_number
                  , disc_number                = excluded.disc_number
                  , formatted_release_position = excluded.formatted_release_position
                  , duration_seconds           = excluded.duration_seconds
                  , formatted_artists          = excluded.formatted_artists
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
                INSERT INTO tracks_artists (track_id, artist, artist_sanitized, role, alias)
                VALUES {",".join(["(?,?,?,?,?)"]*len(upd_track_artist_args))}
                """,
                _flatten(upd_track_artist_args),
            )
        if upd_collage_release_dirnames:
            cursor = conn.execute(
                f"""
                SELECT DISTINCT collage_name FROM collages_releases
                WHERE release_id IN ({','.join(['?'] * len(upd_collage_release_dirnames))})
                ORDER BY collage_name
                """,
                list(upd_collage_release_dirnames.keys()),
            )
            collages = [row["collage_name"] for row in cursor]
            if collages:
                # Because we force the update, the collage will query for the new dirnames and
                # update the files.
                update_cache_for_collages(c, collages, force=True)
        if upd_playlist_track_filenames:
            cursor = conn.execute(
                f"""
                SELECT DISTINCT playlist_name FROM playlists_tracks
                WHERE track_id IN ({','.join(['?'] * len(upd_playlist_track_filenames))})
                ORDER BY playlist_name
                """,
                list(upd_playlist_track_filenames.keys()),
            )
            playlists = [row["playlist_name"] for row in cursor]
            if playlists:
                # Because we force update, the playlist will query for the new filenames and update
                # the files.
                update_cache_for_playlists(c, playlists, force=True)
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
    logger.info(f"Refreshing the read cache for {len(files)} collages")

    cached_collages: dict[str, CachedCollage] = {}
    with connect(c) as conn:
        cursor = conn.execute(
            r"""
            SELECT
                c.name
              , c.source_mtime
              , COALESCE(GROUP_CONCAT(cr.release_id, ' \\ '), '') AS release_ids
            FROM collages c
            LEFT JOIN collages_releases cr ON cr.collage_name = c.name
            GROUP BY c.name
            """,
        )
        for row in cursor:
            cached_collages[row["name"]] = CachedCollage(
                name=row["name"],
                source_mtime=row["source_mtime"],
                release_ids=row["release_ids"].split(r" \\ ") if row["release_ids"] else [],
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

                # Filter out releases that no longer exist.
                for rls in releases:
                    if rls["uuid"] not in existing_release_ids:
                        logger.info(
                            f"Removing nonexistent release {rls['description_meta']} "
                            f"from collage {cached_collage.name}"
                        )
                releases = [rls for rls in releases if rls["uuid"] in existing_release_ids]
                cached_collage.release_ids = [r["uuid"] for r in releases]
                logger.debug(f"Found {len(cached_collage.release_ids)} release(s) in {source_path}")

                # Update the description_metas.
                cursor = conn.execute(
                    f"""
                    SELECT id, virtual_dirname
                    FROM releases WHERE id IN ({','.join(['?'] * len(releases))})
                    """,
                    cached_collage.release_ids,
                )
                desc_map = {r["id"]: r["virtual_dirname"] for r in cursor}
                for i, rls in enumerate(releases):
                    releases[i]["description_meta"] = desc_map[rls["uuid"]]

                # Update the collage on disk if we have changed information.
                if releases != original_releases:
                    logger.info(f"Updating release descriptions for {cached_collage.name}")
                    data["releases"] = releases
                    with source_path.open("wb") as fp:
                        tomli_w.dump(data, fp)
                    cached_collage.source_mtime = str(os.stat(source_path).st_mtime)

                logger.info(f"Applying cache updates for collage {cached_collage.name}")
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
                for position, rid in enumerate(cached_collage.release_ids):
                    args.extend([cached_collage.name, rid, position + 1])
                if args:
                    conn.execute(
                        f"""
                        INSERT INTO collages_releases (collage_name, release_id, position)
                        VALUES {','.join(['(?, ?, ?)'] * len(cached_collage.release_ids))}
                        """,
                        args,
                    )

    logger.debug(f"Collage update loop time {time.time() - loop_start=}")


def update_cache_evict_nonexistent_collages(c: Config) -> None:
    logger.info("Evicting cached collages that are not on disk")
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
            logger.info(f"Evicted collage {row['name']} from cache")


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
    for f in os.scandir(str(playlist_dir)):
        path = Path(f.path)
        if path.suffix != ".toml":
            continue
        if not path.is_file():
            logger.debug(f"Skipping processing playlist {path.name} because it is not a file")
            continue
        if playlist_names is None or path.stem in playlist_names:
            files.append((path.resolve(), path.stem, f))
    logger.info(f"Refreshing the read cache for {len(files)} playlists")

    cached_playlists: dict[str, CachedPlaylist] = {}
    with connect(c) as conn:
        cursor = conn.execute(
            r"""
            SELECT
                p.name
              , p.source_mtime
              , p.cover_path
              , COALESCE(GROUP_CONCAT(pt.track_id, ' \\ '), '') AS track_ids
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
                track_ids=row["track_ids"].split(r" \\ ") if row["track_ids"] else [],
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
                for ext in VALID_COVER_EXTENSIONS:
                    cover_path = source_path.with_suffix(ext)
                    if cover_path.is_file():
                        cached_playlist.cover_path = cover_path
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

                # Filter out tracks that no longer exist.
                for trk in tracks:
                    if trk["uuid"] not in existing_track_ids:
                        logger.info(
                            f"Removing nonexistent track {trk['description_meta']} "
                            f"from playlist {cached_playlist.name}"
                        )
                tracks = [trk for trk in tracks if trk["uuid"] in existing_track_ids]
                cached_playlist.track_ids = [r["uuid"] for r in tracks]
                logger.debug(f"Found {len(cached_playlist.track_ids)} track(s) in {source_path}")

                # Update the description_metas.
                cursor = conn.execute(
                    f"""
                    SELECT id, virtual_filename
                    FROM tracks WHERE id IN ({','.join(['?'] * len(tracks))})
                    """,
                    cached_playlist.track_ids,
                )
                desc_map = {r["id"]: r["virtual_filename"] for r in cursor}
                for i, trk in enumerate(tracks):
                    tracks[i]["description_meta"] = desc_map[trk["uuid"]]

                # Update the playlist on disk if we have changed information.
                if tracks != original_tracks:
                    logger.info(f"Updating track descriptions for {cached_playlist.name}")
                    data["tracks"] = tracks
                    with source_path.open("wb") as fp:
                        tomli_w.dump(data, fp)
                    cached_playlist.source_mtime = str(os.stat(source_path).st_mtime)

                logger.info(f"Applying cache updates for playlist {cached_playlist.name}")
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
                for position, rid in enumerate(cached_playlist.track_ids):
                    args.extend([cached_playlist.name, rid, position + 1])
                if args:
                    conn.execute(
                        f"""
                        INSERT INTO playlists_tracks (playlist_name, track_id, position)
                        VALUES {','.join(['(?, ?, ?)'] * len(cached_playlist.track_ids))}
                        """,
                        args,
                    )

    logger.debug(f"playlist update loop time {time.time() - loop_start=}")


def update_cache_evict_nonexistent_playlists(c: Config) -> None:
    logger.info("Evicting cached playlists that are not on disk")
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
            logger.info(f"Evicted playlist {row['name']} from cache")


def list_releases(
    c: Config,
    sanitized_artist_filter: str | None = None,
    sanitized_genre_filter: str | None = None,
    sanitized_label_filter: str | None = None,
    new: bool | None = None,
) -> Iterator[CachedRelease]:
    with connect(c) as conn:
        query = r"""
            WITH genres AS (
                SELECT
                    release_id
                  , GROUP_CONCAT(genre, ' \\ ') AS genres
                FROM (SELECT * FROM releases_genres ORDER BY genre)
                GROUP BY release_id
            ), labels AS (
                SELECT
                    release_id
                  , GROUP_CONCAT(label, ' \\ ') AS labels
                FROM (SELECT * FROM releases_labels ORDER BY label)
                GROUP BY release_id
            ), artists AS (
                SELECT
                    release_id
                  , GROUP_CONCAT(artist, ' \\ ') AS names
                  , GROUP_CONCAT(role, ' \\ ') AS roles
                FROM (SELECT * FROM releases_artists ORDER BY artist, role)
                GROUP BY release_id
            )
            SELECT
                r.id
              , r.source_path
              , r.cover_image_path
              , r.added_at
              , r.datafile_mtime
              , r.virtual_dirname
              , r.title
              , r.release_type
              , r.release_year
              , r.multidisc
              , r.new
              , r.formatted_artists
              , COALESCE(g.genres, '') AS genres
              , COALESCE(l.labels, '') AS labels
              , COALESCE(a.names, '') AS artist_names
              , COALESCE(a.roles, '') AS artist_roles
            FROM releases r
            LEFT JOIN genres g ON g.release_id = r.id
            LEFT JOIN labels l ON l.release_id = r.id
            LEFT JOIN artists a ON a.release_id = r.id
            WHERE 1=1
        """
        args: list[str | bool] = []
        if sanitized_artist_filter:
            query += """
                AND EXISTS (
                    SELECT * FROM releases_artists
                    WHERE release_id = r.id AND artist_sanitized = ?
                )
            """
            args.append(sanitized_artist_filter)
        if sanitized_genre_filter:
            query += """
                AND EXISTS (
                    SELECT * FROM releases_genres
                    WHERE release_id = r.id AND genre_sanitized = ?
                )
            """
            args.append(sanitized_genre_filter)
        if sanitized_label_filter:
            query += """
                AND EXISTS (
                    SELECT * FROM releases_labels
                    WHERE release_id = r.id AND label_sanitized = ?
                )
            """
            args.append(sanitized_label_filter)
        if new is not None:
            query += "AND r.new = ?"
            args.append(new)
        query += " ORDER BY r.source_path"

        cursor = conn.execute(query, args)
        for row in cursor:
            artists: list[CachedArtist] = []
            for n, r in zip(row["artist_names"].split(r" \\ "), row["artist_roles"].split(r" \\ ")):
                if not n:
                    # This can occur if there are no artist names; then we get a single iteration
                    # with empty string.
                    continue
                artists.append(CachedArtist(name=n, role=r))
            yield CachedRelease(
                id=row["id"],
                source_path=Path(row["source_path"]),
                cover_image_path=Path(row["cover_image_path"]) if row["cover_image_path"] else None,
                added_at=row["added_at"],
                datafile_mtime=row["datafile_mtime"],
                virtual_dirname=row["virtual_dirname"],
                title=row["title"],
                releasetype=row["release_type"],
                year=row["release_year"],
                multidisc=bool(row["multidisc"]),
                new=bool(row["new"]),
                genres=row["genres"].split(r" \\ ") if row["genres"] else [],
                labels=row["labels"].split(r" \\ ") if row["labels"] else [],
                artists=artists,
                formatted_artists=row["formatted_artists"],
            )


def get_release(
    c: Config,
    release_id_or_virtual_dirname: str,
) -> tuple[CachedRelease, list[CachedTrack]] | None:
    with connect(c) as conn:
        cursor = conn.execute(
            r"""
            WITH genres AS (
                SELECT
                    release_id
                  , GROUP_CONCAT(genre, ' \\ ') AS genres
                FROM (SELECT * FROM releases_genres ORDER BY genre)
                GROUP BY release_id
            ), labels AS (
                SELECT
                    release_id
                  , GROUP_CONCAT(label, ' \\ ') AS labels
                FROM (SELECT * FROM releases_labels ORDER BY label)
                GROUP BY release_id
            ), artists AS (
                SELECT
                    release_id
                  , GROUP_CONCAT(artist, ' \\ ') AS names
                  , GROUP_CONCAT(role, ' \\ ') AS roles
                FROM (SELECT * FROM releases_artists ORDER BY artist, role)
                GROUP BY release_id
            )
            SELECT
                r.id
              , r.source_path
              , r.cover_image_path
              , r.added_at
              , r.datafile_mtime
              , r.virtual_dirname
              , r.title
              , r.release_type
              , r.release_year
              , r.multidisc
              , r.new
              , r.formatted_artists
              , COALESCE(g.genres, '') AS genres
              , COALESCE(l.labels, '') AS labels
              , COALESCE(a.names, '') AS artist_names
              , COALESCE(a.roles, '') AS artist_roles
            FROM releases r
            LEFT JOIN genres g ON g.release_id = r.id
            LEFT JOIN labels l ON l.release_id = r.id
            LEFT JOIN artists a ON a.release_id = r.id
            WHERE r.id = ? or r.virtual_dirname = ?
            """,
            (release_id_or_virtual_dirname, release_id_or_virtual_dirname),
        )
        row = cursor.fetchone()
        if not row:
            return None
        rartists: list[CachedArtist] = []
        for n, r in zip(row["artist_names"].split(r" \\ "), row["artist_roles"].split(r" \\ ")):
            if not n:
                # This can occur if there are no artist names; then we get a single iteration
                # with empty string.
                continue
            rartists.append(CachedArtist(name=n, role=r))
        release = CachedRelease(
            id=row["id"],
            source_path=Path(row["source_path"]),
            cover_image_path=Path(row["cover_image_path"]) if row["cover_image_path"] else None,
            added_at=row["added_at"],
            datafile_mtime=row["datafile_mtime"],
            virtual_dirname=row["virtual_dirname"],
            title=row["title"],
            releasetype=row["release_type"],
            year=row["release_year"],
            multidisc=bool(row["multidisc"]),
            new=bool(row["new"]),
            genres=row["genres"].split(r" \\ ") if row["genres"] else [],
            labels=row["labels"].split(r" \\ ") if row["labels"] else [],
            artists=rartists,
            formatted_artists=row["formatted_artists"],
        )

        tracks: list[CachedTrack] = []
        cursor = conn.execute(
            r"""
            WITH artists AS (
                SELECT
                    track_id
                  , GROUP_CONCAT(artist, ' \\ ') AS names
                  , GROUP_CONCAT(role, ' \\ ') AS roles
                FROM (SELECT * FROM tracks_artists ORDER BY artist, role)
                GROUP BY track_id
            )
            SELECT
                t.id
              , t.source_path
              , t.source_mtime
              , t.virtual_filename
              , t.title
              , t.release_id
              , t.track_number
              , t.disc_number
              , t.formatted_release_position
              , t.duration_seconds
              , t.formatted_artists
              , COALESCE(a.names, '') AS artist_names
              , COALESCE(a.roles, '') AS artist_roles
            FROM tracks t
            JOIN releases r ON r.id = t.release_id
            LEFT JOIN artists a ON a.track_id = t.id
            WHERE r.id = ? OR r.virtual_dirname = ?
            ORDER BY t.disc_number, t.track_number
            """,
            (release_id_or_virtual_dirname, release_id_or_virtual_dirname),
        )
        for row in cursor:
            tartists: list[CachedArtist] = []
            for n, r in zip(row["artist_names"].split(r" \\ "), row["artist_roles"].split(r" \\ ")):
                if not n:
                    # This can occur if there are no artist names; then we get a single iteration
                    # with empty string.
                    continue
                tartists.append(CachedArtist(name=n, role=r))
            tracks.append(
                CachedTrack(
                    id=row["id"],
                    source_path=Path(row["source_path"]),
                    source_mtime=row["source_mtime"],
                    virtual_filename=row["virtual_filename"],
                    title=row["title"],
                    release_id=row["release_id"],
                    track_number=row["track_number"],
                    disc_number=row["disc_number"],
                    formatted_release_position=row["formatted_release_position"],
                    duration_seconds=row["duration_seconds"],
                    formatted_artists=row["formatted_artists"],
                    artists=tartists,
                )
            )

    return (release, tracks)


def get_release_id_from_virtual_dirname(c: Config, release_virtual_dirname: str) -> str | None:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT id FROM releases WHERE virtual_dirname = ?",
            (release_virtual_dirname,),
        )
        if row := cursor.fetchone():
            assert isinstance(row["id"], str)
            return row["id"]
    return None


def get_release_virtual_dirname_from_id(c: Config, uuid: str) -> str | None:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT virtual_dirname FROM releases WHERE id = ?",
            (uuid,),
        )
        if row := cursor.fetchone():
            assert isinstance(row["virtual_dirname"], str)
            return row["virtual_dirname"]
    return None


def get_release_source_path_from_id(c: Config, uuid: str) -> Path | None:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT source_path FROM releases WHERE id = ?",
            (uuid,),
        )
        if row := cursor.fetchone():
            return Path(row["source_path"])
    return None


def get_track_filename(c: Config, uuid: str) -> str | None:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT virtual_filename FROM tracks WHERE id = ?",
            (uuid,),
        )
        if row := cursor.fetchone():
            assert isinstance(row["virtual_filename"], str)
            return row["virtual_filename"]
    return None


def list_artists(c: Config) -> Iterator[tuple[str, str]]:
    with connect(c) as conn:
        cursor = conn.execute("SELECT DISTINCT artist, artist_sanitized FROM releases_artists")
        for row in cursor:
            yield row["artist"], row["artist_sanitized"]


def list_genres(c: Config) -> Iterator[tuple[str, str]]:
    with connect(c) as conn:
        cursor = conn.execute("SELECT DISTINCT genre, genre_sanitized FROM releases_genres")
        for row in cursor:
            yield row["genre"], row["genre_sanitized"]


def list_labels(c: Config) -> Iterator[tuple[str, str]]:
    with connect(c) as conn:
        cursor = conn.execute("SELECT DISTINCT label, label_sanitized FROM releases_labels")
        for row in cursor:
            yield row["label"], row["label_sanitized"]


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
            r"""
            WITH artists AS (
                SELECT
                    track_id
                  , GROUP_CONCAT(artist, ' \\ ') AS names
                  , GROUP_CONCAT(role, ' \\ ') AS roles
                FROM (SELECT * FROM tracks_artists ORDER BY artist, role)
                GROUP BY track_id
            )
            SELECT
                t.id
              , t.source_path
              , t.source_mtime
              , t.virtual_filename
              , t.title
              , t.release_id
              , t.track_number
              , t.disc_number
              , t.formatted_release_position
              , t.duration_seconds
              , t.formatted_artists
              , COALESCE(a.names, '') AS artist_names
              , COALESCE(a.roles, '') AS artist_roles
            FROM tracks t
            JOIN playlists_tracks pt ON pt.track_id = t.id
            LEFT JOIN artists a ON a.track_id = t.id
            WHERE pt.playlist_name = ?
            ORDER BY pt.position ASC
            """,
            (playlist_name,),
        )
        tracks: list[CachedTrack] = []
        for row in cursor:
            tartists: list[CachedArtist] = []
            for n, r in zip(row["artist_names"].split(r" \\ "), row["artist_roles"].split(r" \\ ")):
                if not n:
                    # This can occur if there are no artist names; then we get a single iteration
                    # with empty string.
                    continue
                tartists.append(CachedArtist(name=n, role=r))
            playlist.track_ids.append(row["id"])
            tracks.append(
                CachedTrack(
                    id=row["id"],
                    source_path=Path(row["source_path"]),
                    source_mtime=row["source_mtime"],
                    virtual_filename=row["virtual_filename"],
                    title=row["title"],
                    release_id=row["release_id"],
                    track_number=row["track_number"],
                    disc_number=row["disc_number"],
                    formatted_release_position=row["formatted_release_position"],
                    duration_seconds=row["duration_seconds"],
                    formatted_artists=row["formatted_artists"],
                    artists=tartists,
                )
            )

    return playlist, tracks


def list_collages(c: Config) -> Iterator[str]:
    with connect(c) as conn:
        cursor = conn.execute("SELECT DISTINCT name FROM collages")
        for row in cursor:
            yield row["name"]


def list_collage_releases(c: Config, collage_name: str) -> Iterator[tuple[int, str, Path]]:
    """Returns tuples of (position, release_virtual_dirname, release_source_path)."""
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT cr.position, r.virtual_dirname, r.source_path
            FROM collages_releases cr
            JOIN releases r ON r.id = cr.release_id
            WHERE cr.collage_name = ?
            ORDER BY cr.position
            """,
            (collage_name,),
        )
        for row in cursor:
            yield (row["position"], row["virtual_dirname"], Path(row["source_path"]))


def release_exists(c: Config, virtual_dirname: str) -> Path | None:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT source_path FROM releases WHERE virtual_dirname = ?",
            (virtual_dirname,),
        )
        if row := cursor.fetchone():
            return Path(row["source_path"])
        return None


def track_exists(
    c: Config,
    release_virtual_dirname: str,
    track_virtual_filename: str,
) -> Path | None:
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT t.source_path
            FROM tracks t
            JOIN releases r ON t.release_id = r.id
            WHERE r.virtual_dirname = ? AND t.virtual_filename = ?
            """,
            (
                release_virtual_dirname,
                track_virtual_filename,
            ),
        )
        if row := cursor.fetchone():
            return Path(row["source_path"])
        return None


def cover_exists(c: Config, release_virtual_dirname: str, cover_name: str) -> Path | None:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT cover_image_path FROM releases r WHERE r.virtual_dirname = ?",
            (release_virtual_dirname,),
        )
        if (row := cursor.fetchone()) and row["cover_image_path"]:
            p = Path(row["cover_image_path"])
            if p.name == cover_name:
                return p
        return None


def artist_exists(c: Config, artist_sanitized: str) -> bool:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT EXISTS(SELECT * FROM releases_artists WHERE artist_sanitized = ?)",
            (artist_sanitized,),
        )
        return bool(cursor.fetchone()[0])


def genre_exists(c: Config, genre_sanitized: str) -> bool:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT EXISTS(SELECT * FROM releases_genres WHERE genre_sanitized = ?)",
            (genre_sanitized,),
        )
        return bool(cursor.fetchone()[0])


def label_exists(c: Config, label_sanitized: str) -> bool:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT EXISTS(SELECT * FROM releases_labels WHERE label_sanitized = ?)",
            (label_sanitized,),
        )
        return bool(cursor.fetchone()[0])


def collage_exists(c: Config, name: str) -> bool:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT EXISTS(SELECT * FROM collages WHERE name = ?)",
            (name,),
        )
        return bool(cursor.fetchone()[0])


def playlist_exists(c: Config, name: str) -> bool:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT EXISTS(SELECT * FROM playlists WHERE name = ?)",
            (name,),
        )
        return bool(cursor.fetchone()[0])


ILLEGAL_FS_CHARS_REGEX = re.compile(r'[:\?<>\\*\|"\/]+')


def _sanitize_filename(x: str) -> str:
    return ILLEGAL_FS_CHARS_REGEX.sub("_", x)


def _flatten(xxs: list[list[T]]) -> list[T]:
    xs: list[T] = []
    for group in xxs:
        xs.extend(group)
    return xs


def _uniq(xs: list[T]) -> list[T]:
    rv: list[T] = []
    seen: set[T] = set()
    for x in xs:
        if x not in seen:
            rv.append(x)
            seen.add(x)
    return rv
