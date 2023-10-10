import os
import re
from dataclasses import asdict, dataclass
from pathlib import Path

import uuid6

from rose.cache.database import connect
from rose.foundation.conf import Config
from rose.tagger import AudioFile

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


@dataclass
class CachedRelease:
    id: str
    source_path: Path
    title: str
    release_type: str
    release_year: int | None
    new: bool


@dataclass
class CachedTrack:
    id: str
    source_path: Path
    source_mtime: int
    title: str
    release_id: str
    trackno: str
    discno: str
    duration_sec: int


@dataclass
class CachedArtist:
    id: str
    name: str


def process_release(c: Config, release_dir: Path) -> None:
    """
    Given a release's directory, update the cache entry based on the release's metadata.
    If this is a new release or track, update the directory and file names to include the UUIDs.
    """
    with connect(c) as conn:
        # The release will be updated based on the album tags of the first track.
        release: CachedRelease | None = None
        # But first, parse the release_id from the directory name. If the directory name does not
        # contain a release_id, generate one and rename the directory.
        release_id = _parse_uuid_from_path(release_dir)
        if not release_id:
            release_id = str(uuid6.uuid7())
            release_dir = _rename_with_uuid(release_dir, release_id)

        for f in os.scandir(release_dir):
            # Skip non-music files.
            if not any(f.name.endswith(ext) for ext in SUPPORTED_EXTENSIONS):
                continue

            tags = AudioFile.from_file(Path(f.path))
            # If this is the first track, upsert the release.
            if release is None:
                release = CachedRelease(
                    id=release_id,
                    source_path=release_dir,
                    title=tags.album or "Unknown Release",
                    release_type=(
                        tags.release_type
                        if tags.release_type in SUPPORTED_RELEASE_TYPES
                        else "unknown"
                    ),
                    release_year=tags.year,
                    new=True,
                )
                conn.execute(
                    """
                    INSERT INTO releases
                    (id, source_path, title, release_type, release_year, new)
                    VALUES (?, ?, ?, ?, ?, ?)
                    ON CONFLICT (id) DO UPDATE SET
                        source_path = ?,
                        title = ?,
                        release_type = ?,
                        release_year = ?,
                        new = ?
                    """,
                    (
                        release.id,
                        str(release.source_path),
                        release.title,
                        release.release_type,
                        release.release_year,
                        release.new,
                        str(release.source_path),
                        release.title,
                        release.release_type,
                        release.release_year,
                        release.new,
                    ),
                )
                for genre in tags.genre:
                    conn.execute(
                        """
                        INSERT INTO releases_genres (release_id, genre) VALUES (?, ?)
                        ON CONFLICT (release_id, genre) DO NOTHING
                        """,
                        (release.id, genre),
                    )
                for role, names in asdict(tags.album_artists).items():
                    for name in names:
                        conn.execute(
                            """
                            INSERT INTO releases_artists (release_id, artist, role)
                            VALUES (?, ?, ?)
                            ON CONFLICT (release_id, artist) DO UPDATE SET role = ?
                            """,
                            (release.id, name, role, role),
                        )

            # Now process the track. Release is guaranteed to exist here.
            filepath = Path(f.path)
            track_id = _parse_uuid_from_path(filepath)
            if not track_id:
                track_id = str(uuid6.uuid7())
                filepath = _rename_with_uuid(filepath, track_id)
            track = CachedTrack(
                id=track_id,
                source_path=filepath,
                source_mtime=int(f.stat().st_mtime),
                title=tags.title or "Unknown Title",
                release_id=release.id,
                trackno=tags.track_number or "1",
                discno=tags.disc_number or "1",
                duration_sec=tags.duration_sec,
            )
            conn.execute(
                """
                INSERT INTO tracks
                (id, source_path, source_mtime, title, release_id, track_number, disc_number,
                 duration_seconds)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT (id) DO UPDATE SET
                    source_path = ?,
                    source_mtime = ?,
                    title = ?,
                    release_id = ?,
                    track_number = ?,
                    disc_number = ?,
                    duration_seconds = ?
                """,
                (
                    track.id,
                    str(track.source_path),
                    track.source_mtime,
                    track.title,
                    track.release_id,
                    track.trackno,
                    track.discno,
                    track.duration_sec,
                    str(track.source_path),
                    track.source_mtime,
                    track.title,
                    track.release_id,
                    track.trackno,
                    track.discno,
                    track.duration_sec,
                ),
            )
            for role, names in asdict(tags.artists).items():
                for name in names:
                    conn.execute(
                        """
                        INSERT INTO tracks_artists (track_id, artist, role)
                        VALUES (?, ?, ?)
                        ON CONFLICT (track_id, artist) DO UPDATE SET role = ?
                        """,
                        (track.id, name, role, role),
                    )


def _parse_uuid_from_path(path: Path) -> str | None:
    if m := re.search(r"\{id=([^\]]+)\}$", path.stem):
        return m[1]
    return None


def _rename_with_uuid(src: Path, uuid: str) -> Path:
    new_stem = src.stem + f" {{id={uuid}}}"
    dst = src.with_stem(new_stem)
    src.rename(dst)
    return dst
