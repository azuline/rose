import binascii
import hashlib
import logging
import os
import random
import sqlite3
import time
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import asdict, dataclass
from pathlib import Path

import tomli_w
import tomllib
import uuid6

from rose.artiststr import format_artist_string
from rose.config import Config
from rose.sanitize import sanitize_filename
from rose.tagger import AudioFile

logger = logging.getLogger(__name__)

CACHE_SCHEMA_PATH = Path(__file__).resolve().parent / "cache.sql"


@contextmanager
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


@contextmanager
def transaction(conn: sqlite3.Connection) -> Iterator[sqlite3.Connection]:
    """
    A simple context wrapper for a database transaction. If connection is null,
    a new connection is created.
    """
    tx_log_id = binascii.b2a_hex(random.randbytes(8)).decode()
    start_time = time.time()

    # If we're already in a transaction, don't create a nested transaction.
    if conn.in_transaction:
        logger.debug(f"Transaction {tx_log_id}. Starting nested transaction, NoOp.")
        yield conn
        logger.debug(
            f"Transaction {tx_log_id}. End of nested transaction. "
            f"Duration: {time.time() - start_time}."
        )
        return

    logger.debug(f"Transaction {tx_log_id}. Starting transaction from conn.")
    with conn:
        # We BEGIN IMMEDIATE to avoid deadlocks, which pisses the hell out of me because no one's
        # documenting this properly and SQLite just dies without respecting the timeout and without
        # a reasonable error message. Absurd.
        # - https://sqlite.org/forum/forumpost/a3db6dbff1cd1d5d
        conn.execute("BEGIN IMMEDIATE")
        yield conn
        logger.debug(
            f"Transaction {tx_log_id}. End of transaction from conn. "
            f"Duration: {time.time() - start_time}."
        )


def migrate_database(c: Config) -> None:
    """
    "Migrate" the database. If the schema in the database does not match that on disk, then nuke the
    database and recreate it from scratch. Otherwise, no op.

    We can do this because the database is just a read cache. It is not source-of-truth for any of
    its own data.
    """
    with CACHE_SCHEMA_PATH.open("rb") as fp:
        latest_schema_hash = hashlib.sha256(fp.read()).hexdigest()

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
            cursor = conn.execute("SELECT value FROM _schema_hash")
            if (row := cursor.fetchone()) and row[0] == latest_schema_hash:
                # Everything matches! Exit!
                return

    c.cache_database_path.unlink(missing_ok=True)
    with connect(c) as conn:
        with CACHE_SCHEMA_PATH.open("r") as fp:
            conn.executescript(fp.read())
        conn.execute("CREATE TABLE _schema_hash (value TEXT PRIMARY KEY)")
        conn.execute("INSERT INTO _schema_hash (value) VALUES (?)", (latest_schema_hash,))


@dataclass
class CachedArtist:
    name: str
    role: str


@dataclass
class CachedRelease:
    id: str
    source_path: Path
    cover_image_path: Path | None
    virtual_dirname: str
    title: str
    release_type: str
    release_year: int | None
    new: bool
    genres: list[str]
    labels: list[str]
    artists: list[CachedArtist]


@dataclass
class CachedTrack:
    id: str
    source_path: Path
    virtual_filename: str
    title: str
    release_id: str
    track_number: str
    disc_number: str
    duration_seconds: int

    artists: list[CachedArtist]


VALID_COVER_FILENAMES = [
    stem + ext for stem in ["cover", "folder", "art"] for ext in [".jpg", ".jpeg", ".png"]
]

SUPPORTED_EXTENSIONS = [
    ".mp3",
    ".m4a",
    ".ogg",
    ".opus",
    ".flac",
]

SUPPORTED_RELEASE_TYPES = [
    "album",
    "single",
    "ep",
    "compilation",
    "soundtrack",
    "live",
    "remix",
    "djmix",
    "mixtape",
    "other",
    "unknown",
]


def update_cache_for_all_releases(c: Config) -> None:
    """
    Process and update the cache for all releases. Delete any nonexistent releases.
    """
    dirs = [Path(d.path).resolve() for d in os.scandir(c.music_source_dir) if d.is_dir()]
    logger.info(f"Found {len(dirs)} releases to update")
    for d in dirs:
        update_cache_for_release(c, d)
    logger.info("Deleting cached releases that are not on disk")
    with connect(c) as conn:
        conn.execute(
            f"""
            DELETE FROM releases
            WHERE source_path NOT IN ({",".join(["?"] * len(dirs))})
            """,
            [str(d) for d in dirs],
        )


def update_cache_for_release(c: Config, release_dir: Path) -> None:
    """
    Given a release's directory, update the cache entry based on the release's metadata. If this is
    a new release or track, update the directory and file names to include the UUIDs.

    Returns the new release_dir if a rename occurred; otherwise, returns the same release_dir.
    """
    logger.info(f"Refreshing cached data for {release_dir.name}")
    with connect(c) as conn, transaction(conn) as conn:
        # The release will be updated based on the album tags of the first track.
        release: CachedRelease | None = None
        # But first, parse the release_id from the directory name. If the directory name does not
        # contain a release_id, generate one and rename the directory.
        stored_release_data = _read_stored_data_file(release_dir)
        if not stored_release_data:
            stored_release_data = _create_stored_data_file(release_dir)

        # Fetch all track tags from disk.
        track_tags: list[tuple[os.DirEntry[str], AudioFile]] = []
        for f in os.scandir(release_dir):
            # Skip non-music files.
            if any(f.name.endswith(ext) for ext in SUPPORTED_EXTENSIONS):
                track_tags.append((f, AudioFile.from_file(Path(f.path))))

        # Calculate whether this is a multidisc release or not. This will affect the virtual
        # filename formatting.
        multidisc = len({t.disc_number for _, t in track_tags}) > 1

        for f, tags in track_tags:
            # If this is the first track, upsert the release.
            if release is None:
                logger.debug("Upserting release from first track's tags")

                # Compute the album's visual directory name.
                virtual_dirname = format_artist_string(tags.album_artists, tags.genre) + " - "
                if tags.year:
                    virtual_dirname += str(tags.year) + ". "
                virtual_dirname += tags.album or "Unknown Release"
                if (
                    tags.release_type
                    and tags.release_type.lower() in SUPPORTED_RELEASE_TYPES
                    and tags.release_type not in ["album", "unknown"]
                ):
                    virtual_dirname += " - " + tags.release_type.title()
                if tags.genre:
                    virtual_dirname += " [" + ";".join(tags.genre) + "]"
                if tags.label:
                    virtual_dirname += " {" + ";".join(tags.label) + "}"
                virtual_dirname = sanitize_filename(virtual_dirname)
                # And in case of a name collision, add an extra number at the end. Iterate to find
                # the first unused number.
                original_virtual_dirname = virtual_dirname
                collision_no = 1
                while True:
                    collision_no += 1
                    cursor = conn.execute(
                        """
                        SELECT EXISTS(
                            SELECT * FROM releases WHERE virtual_dirname = ? AND id <> ?
                        )
                        """,
                        (virtual_dirname, stored_release_data.uuid),
                    )
                    if not cursor.fetchone()[0]:
                        break
                    virtual_dirname = f"{original_virtual_dirname} [{collision_no}]"

                # Search for cover art.
                cover_image_path = None
                for cn in VALID_COVER_FILENAMES:
                    p = release_dir / cn
                    if p.is_file():
                        cover_image_path = p.resolve()
                        break

                # Construct the cached release.
                release = CachedRelease(
                    id=stored_release_data.uuid,
                    source_path=release_dir.resolve(),
                    cover_image_path=cover_image_path,
                    virtual_dirname=virtual_dirname,
                    title=tags.album or "Unknown Release",
                    release_type=(
                        tags.release_type.lower()
                        if tags.release_type
                        and tags.release_type.lower() in SUPPORTED_RELEASE_TYPES
                        else "unknown"
                    ),
                    release_year=tags.year,
                    new=True,
                    genres=tags.genre,
                    labels=tags.label,
                    artists=[],
                )
                for role, names in asdict(tags.album_artists).items():
                    for name in names:
                        release.artists.append(CachedArtist(name=name, role=role))

                # Upsert the release.
                conn.execute(
                    """
                    INSERT INTO releases
                    (id, source_path, cover_image_path, virtual_dirname, title, release_type,
                     release_year, new)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                    ON CONFLICT (id) DO UPDATE SET
                        source_path = ?,
                        cover_image_path = ?,
                        virtual_dirname = ?,
                        title = ?,
                        release_type = ?,
                        release_year = ?,
                        new = ?
                    """,
                    (
                        release.id,
                        str(release.source_path),
                        str(release.cover_image_path),
                        release.virtual_dirname,
                        release.title,
                        release.release_type,
                        release.release_year,
                        release.new,
                        str(release.source_path),
                        str(release.cover_image_path),
                        release.virtual_dirname,
                        release.title,
                        release.release_type,
                        release.release_year,
                        release.new,
                    ),
                )
                for genre in release.genres:
                    conn.execute(
                        """
                        INSERT INTO releases_genres (release_id, genre, genre_sanitized)
                        VALUES (?, ?, ?)
                        ON CONFLICT (release_id, genre) DO NOTHING
                        """,
                        (release.id, genre, sanitize_filename(genre)),
                    )
                for label in release.labels:
                    conn.execute(
                        """
                        INSERT INTO releases_labels (release_id, label, label_sanitized)
                        VALUES (?, ?, ?)
                        ON CONFLICT (release_id, label) DO NOTHING
                        """,
                        (release.id, label, sanitize_filename(label)),
                    )
                for art in release.artists:
                    conn.execute(
                        """
                        INSERT INTO releases_artists (release_id, artist, artist_sanitized, role)
                        VALUES (?, ?, ?, ?)
                        ON CONFLICT (release_id, artist) DO UPDATE SET role = ?
                        """,
                        (release.id, art.name, sanitize_filename(art.name), art.role, art.role),
                    )

            # Now process the track. Release is guaranteed to exist here.
            filepath = Path(f.path)

            # Track ID is transient with the cache; we don't put it in any persistent stores.
            track_id = str(uuid6.uuid7())

            virtual_filename = ""
            if multidisc and tags.disc_number:
                virtual_filename += f"{tags.disc_number:0>2}-"
            if tags.track_number:
                virtual_filename += f"{tags.track_number:0>2}. "
            virtual_filename += tags.title or "Unknown Title"
            if tags.release_type in ["compilation", "soundtrack", "remix", "djmix", "mixtape"]:
                virtual_filename += " (by " + format_artist_string(tags.artists, tags.genre) + ")"
            virtual_filename += filepath.suffix
            virtual_filename = sanitize_filename(virtual_filename)
            # And in case of a name collision, add an extra number at the end. Iterate to find
            # the first unused number.
            original_virtual_filename = virtual_filename
            collision_no = 1
            while True:
                collision_no += 1
                cursor = conn.execute(
                    """
                    SELECT EXISTS(
                        SELECT * FROM tracks WHERE virtual_filename = ? AND id <> ?
                    )
                    """,
                    (virtual_filename, track_id),
                )
                if not cursor.fetchone()[0]:
                    break
                virtual_filename = f"{original_virtual_filename} [{collision_no}]"

            track = CachedTrack(
                id=track_id,
                source_path=filepath,
                virtual_filename=virtual_filename,
                title=tags.title or "Unknown Title",
                release_id=release.id,
                track_number=tags.track_number or "1",
                disc_number=tags.disc_number or "1",
                duration_seconds=tags.duration_sec,
                artists=[],
            )
            for role, names in asdict(tags.artists).items():
                for name in names:
                    track.artists.append(CachedArtist(name=name, role=role))
            conn.execute(
                """
                INSERT INTO tracks
                (id, source_path, virtual_filename, title, release_id,
                 track_number, disc_number, duration_seconds)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT (id) DO UPDATE SET
                    source_path = ?,
                    virtual_filename = ?,
                    title = ?,
                    release_id = ?,
                    track_number = ?,
                    disc_number = ?,
                    duration_seconds = ?
                """,
                (
                    track.id,
                    str(track.source_path),
                    track.virtual_filename,
                    track.title,
                    track.release_id,
                    track.track_number,
                    track.disc_number,
                    track.duration_seconds,
                    str(track.source_path),
                    track.virtual_filename,
                    track.title,
                    track.release_id,
                    track.track_number,
                    track.disc_number,
                    track.duration_seconds,
                ),
            )
            for art in track.artists:
                conn.execute(
                    """
                    INSERT INTO tracks_artists (track_id, artist, artist_sanitized, role)
                    VALUES (?, ?, ?, ?)
                    ON CONFLICT (track_id, artist) DO UPDATE SET role = ?
                    """,
                    (track.id, art.name, sanitize_filename(art.name), art.role, art.role),
                )


STORED_DATA_FILE_NAME = ".rose.toml"


@dataclass
class StoredDataFile:
    uuid: str
    new: bool


def _read_stored_data_file(path: Path) -> StoredDataFile | None:
    for f in path.iterdir():
        if f.name == STORED_DATA_FILE_NAME:
            logger.debug(f"Found stored data file for {path}")
            with f.open("rb") as fp:
                diskdata = tomllib.load(fp)
            datafile = StoredDataFile(
                uuid=diskdata.get("uuid", str(uuid6.uuid7())),
                new=diskdata.get("new", True),
            )
            resolveddata = asdict(datafile)
            if resolveddata != diskdata:
                logger.debug(f"Setting new default values in stored data file for {path}")
                with f.open("wb") as fp:
                    tomli_w.dump(resolveddata, fp)
            return datafile
    return None


def _create_stored_data_file(path: Path) -> StoredDataFile:
    logger.debug(f"Creating stored data file for {path}")
    data = StoredDataFile(
        uuid=str(uuid6.uuid7()),
        new=True,
    )
    with (path / ".rose.toml").open("wb") as fp:
        tomli_w.dump(asdict(data), fp)
    return data


def list_releases(
    c: Config,
    sanitized_artist_filter: str | None = None,
    sanitized_genre_filter: str | None = None,
    sanitized_label_filter: str | None = None,
) -> Iterator[CachedRelease]:
    with connect(c) as conn:
        query = r"""
            WITH genres AS (
                SELECT
                    release_id,
                    GROUP_CONCAT(genre, ' \\ ') AS genres
                FROM releases_genres
                GROUP BY release_id
            ), labels AS (
                SELECT
                    release_id,
                    GROUP_CONCAT(label, ' \\ ') AS labels
                FROM releases_labels
                GROUP BY release_id
            ), artists AS (
                SELECT
                    release_id,
                    GROUP_CONCAT(artist, ' \\ ') AS names,
                    GROUP_CONCAT(role, ' \\ ') AS roles
                FROM releases_artists
                GROUP BY release_id
            )
            SELECT
                r.id
              , r.source_path
              , r.cover_image_path
              , r.virtual_dirname
              , r.title
              , r.release_type
              , r.release_year
              , r.new
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
        args: list[str] = []
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

        cursor = conn.execute(query, args)
        for row in cursor:
            artists: list[CachedArtist] = []
            for n, r in zip(row["artist_names"].split(r" \\ "), row["artist_roles"].split(r" \\ ")):
                artists.append(CachedArtist(name=n, role=r))
            yield CachedRelease(
                id=row["id"],
                source_path=Path(row["source_path"]),
                cover_image_path=Path(row["cover_image_path"]) if row["cover_image_path"] else None,
                virtual_dirname=row["virtual_dirname"],
                title=row["title"],
                release_type=row["release_type"],
                release_year=row["release_year"],
                new=bool(row["new"]),
                genres=row["genres"].split(r" \\ "),
                labels=row["labels"].split(r" \\ "),
                artists=artists,
            )


@dataclass
class ReleaseFiles:
    tracks: list[CachedTrack]
    cover: Path | None


def get_release_files(c: Config, release_virtual_dirname: str) -> ReleaseFiles:
    rf = ReleaseFiles(tracks=[], cover=None)

    with connect(c) as conn:
        cursor = conn.execute(
            r"""
            WITH artists AS (
                SELECT
                    track_id,
                    GROUP_CONCAT(artist, ' \\ ') AS names,
                    GROUP_CONCAT(role, ' \\ ') AS roles
                FROM tracks_artists
                GROUP BY track_id
            )
            SELECT
                t.id
              , t.source_path
              , t.virtual_filename
              , t.title
              , t.release_id
              , t.track_number
              , t.disc_number
              , t.duration_seconds
              , COALESCE(a.names, '') AS artist_names
              , COALESCE(a.roles, '') AS artist_roles
            FROM tracks t
            JOIN releases r ON r.id = t.release_id
            LEFT JOIN artists a ON a.track_id = t.id
            WHERE r.virtual_dirname = ?
            """,
            (release_virtual_dirname,),
        )
        for row in cursor:
            artists: list[CachedArtist] = []
            for n, r in zip(row["artist_names"].split(r" \\ "), row["artist_roles"].split(r" \\ ")):
                artists.append(CachedArtist(name=n, role=r))
            rf.tracks.append(
                CachedTrack(
                    id=row["id"],
                    source_path=Path(row["source_path"]),
                    virtual_filename=row["virtual_filename"],
                    title=row["title"],
                    release_id=row["release_id"],
                    track_number=row["track_number"],
                    disc_number=row["disc_number"],
                    duration_seconds=row["duration_seconds"],
                    artists=artists,
                )
            )

        cursor = conn.execute(
            "SELECT cover_image_path FROM releases WHERE virtual_dirname = ?",
            (release_virtual_dirname,),
        )
        if (row := cursor.fetchone()) and row["cover_image_path"]:
            rf.cover = Path(row["cover_image_path"])

    return rf


def list_artists(c: Config) -> Iterator[str]:
    with connect(c) as conn:
        cursor = conn.execute("SELECT DISTINCT artist FROM releases_artists")
        for row in cursor:
            yield row["artist"]


def list_genres(c: Config) -> Iterator[str]:
    with connect(c) as conn:
        cursor = conn.execute("SELECT DISTINCT genre FROM releases_genres")
        for row in cursor:
            yield row["genre"]


def list_labels(c: Config) -> Iterator[str]:
    with connect(c) as conn:
        cursor = conn.execute("SELECT DISTINCT label FROM releases_labels")
        for row in cursor:
            yield row["label"]


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
    c: Config, release_virtual_dirname: str, track_virtual_filename: str
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
