import logging
import os
import re
from dataclasses import asdict
from pathlib import Path

import uuid6

from rose.cache.database import connect, transaction
from rose.cache.dataclasses import CachedArtist, CachedRelease, CachedTrack
from rose.foundation.conf import Config
from rose.tagger import ArtistTags, AudioFile
from rose.virtualfs.sanitize import sanitize_filename

logger = logging.getLogger(__name__)

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

ID_REGEX = re.compile(r"\{id=([^\}]+)\}$")


def update_cache_for_all_releases(c: Config) -> None:
    """
    Process and update the cache for all releases. Delete any nonexistent releases.
    """
    dirs = [Path(d.path).resolve() for d in os.scandir(c.music_source_dir) if d.is_dir()]
    logger.info(f"Found {len(dirs)} releases to update")
    for i, d in enumerate(dirs):
        dirs[i] = update_cache_for_release(c, d)
    logger.info("Deleting cached releases that are not on disk")
    with connect(c) as conn:
        conn.execute(
            f"""
            DELETE FROM releases
            WHERE source_path NOT IN ({",".join(["?"] * len(dirs))})
            """,
            [str(d) for d in dirs],
        )


def update_cache_for_release(c: Config, release_dir: Path) -> Path:
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
        release_id = _parse_uuid_from_path(release_dir)
        if not release_id:
            release_id = str(uuid6.uuid7())
            logger.debug(f"Assigning id={release_id} to release {release_dir.name}")
            release_dir = _rename_with_uuid(release_dir, release_id)

        for f in os.scandir(release_dir):
            # Skip non-music files.
            if not any(f.name.endswith(ext) for ext in SUPPORTED_EXTENSIONS):
                continue

            tags = AudioFile.from_file(Path(f.path))
            # If this is the first track, upsert the release.
            if release is None:
                logger.debug("Upserting release from first track's tags")

                virtual_dirname = _format_artists(tags.album_artists) + " - "
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

                release = CachedRelease(
                    id=release_id,
                    source_path=release_dir.resolve(),
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

                conn.execute(
                    """
                    INSERT INTO releases
                    (id, source_path, virtual_dirname, title, release_type, release_year, new)
                    VALUES (?, ?, ?, ?, ?, ?, ?)
                    ON CONFLICT (id) DO UPDATE SET
                        source_path = ?,
                        virtual_dirname = ?,
                        title = ?,
                        release_type = ?,
                        release_year = ?,
                        new = ?
                    """,
                    (
                        release.id,
                        str(release.source_path),
                        release.virtual_dirname,
                        release.title,
                        release.release_type,
                        release.release_year,
                        release.new,
                        str(release.source_path),
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
                        INSERT INTO releases_genres (release_id, genre) VALUES (?, ?)
                        ON CONFLICT (release_id, genre) DO NOTHING
                        """,
                        (release.id, genre),
                    )
                for label in release.labels:
                    conn.execute(
                        """
                        INSERT INTO releases_labels (release_id, label) VALUES (?, ?)
                        ON CONFLICT (release_id, label) DO NOTHING
                        """,
                        (release.id, label),
                    )
                for art in release.artists:
                    conn.execute(
                        """
                        INSERT INTO releases_artists (release_id, artist, role)
                        VALUES (?, ?, ?)
                        ON CONFLICT (release_id, artist) DO UPDATE SET role = ?
                        """,
                        (release.id, art.name, art.role, art.role),
                    )

            # Now process the track. Release is guaranteed to exist here.
            filepath = Path(f.path)

            track_id = _parse_uuid_from_path(filepath)
            if not track_id:
                track_id = str(uuid6.uuid7())
                logger.debug(f"Assigning id={release_id} to track {filepath.name}")
                filepath = _rename_with_uuid(filepath, track_id)

            virtual_filename = ""
            if tags.disc_number:
                virtual_filename += f"{tags.disc_number:0>2}-"
            if tags.track_number:
                virtual_filename += f"{tags.track_number:0>2}. "
            virtual_filename += tags.title or "Unknown Title"
            virtual_filename += f" [{tags.duration_sec // 60}：{tags.duration_sec % 60:02d}]"
            if tags.artists != tags.album_artists:
                virtual_filename += " (by " + _format_artists(tags.artists) + ")"
            virtual_filename = sanitize_filename(virtual_filename)

            track = CachedTrack(
                id=track_id,
                source_path=filepath,
                virtual_filename=virtual_filename,
                title=tags.title or "Unknown Title",
                release_id=release.id,
                trackno=tags.track_number or "1",
                discno=tags.disc_number or "1",
                duration_sec=tags.duration_sec,
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
                    track.trackno,
                    track.discno,
                    track.duration_sec,
                    str(track.source_path),
                    track.virtual_filename,
                    track.title,
                    track.release_id,
                    track.trackno,
                    track.discno,
                    track.duration_sec,
                ),
            )
            for art in track.artists:
                conn.execute(
                    """
                    INSERT INTO tracks_artists (track_id, artist, role) VALUES (?, ?, ?)
                    ON CONFLICT (track_id, artist) DO UPDATE SET role = ?
                    """,
                    (track.id, art.name, art.role, art.role),
                )

    return release_dir


def _parse_uuid_from_path(path: Path) -> str | None:
    if m := ID_REGEX.search(path.name if path.is_dir() else path.stem):
        return m[1]
    return None


def _rename_with_uuid(src: Path, uuid: str) -> Path:
    if src.is_dir():
        dst = src.with_name(src.name + f" {{id={uuid}}}")
    else:
        dst = src.with_stem(src.stem + f" {{id={uuid}}}")
    return src.rename(dst)


def _format_artists(a: ArtistTags) -> str:
    r = ";".join(a.producer + a.main + a.remixer)
    if a.composer:
        r = ";".join(a.composer) + " performed by. " + r
    if a.djmixer:
        r = ";".join(a.djmixer) + " pres. " + r
    if a.guest:
        r += " feat. " + ";".join(a.guest)
    return r
