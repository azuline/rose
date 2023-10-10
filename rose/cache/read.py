from pathlib import Path
from typing import Iterator

from rose.cache.database import connect
from rose.cache.dataclasses import CachedArtist, CachedRelease
from rose.foundation.conf import Config


def list_releases(c: Config) -> Iterator[CachedRelease]:
    with connect(c) as conn:
        cursor = conn.execute(
            r"""
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
            """
        )
        for row in cursor:
            artists: list[CachedArtist] = []
            for n, r in zip(row["artist_names"].split(r" \\ "), row["artist_roles"].split(r" \\ ")):
                artists.append(CachedArtist(name=n, role=r))
            yield CachedRelease(
                id=row["id"],
                source_path=Path(row["source_path"]),
                virtual_dirname=row["virtual_dirname"],
                title=row["title"],
                release_type=row["release_type"],
                release_year=row["release_year"],
                new=bool(row["new"]),
                genres=row["genres"].split(r" \\ "),
                labels=row["labels"].split(r" \\ "),
                artists=artists,
            )


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


def get_release(c: Config, virtual_dirname: str) -> CachedRelease | None:
    with connect(c) as conn:
        cursor = conn.execute(
            r"""
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
            WHERE r.virtual_dirname = ?
            """,
            (virtual_dirname,),
        )
        row = cursor.fetchone()
        if not row:
            return None
        artists: list[CachedArtist] = []
        for n, r in zip(row["artist_names"].split(r" \\ "), row["artist_roles"].split(r" \\ ")):
            artists.append(CachedArtist(name=n, role=r))
        return CachedRelease(
            id=row["id"],
            source_path=Path(row["source_path"]),
            virtual_dirname=row["virtual_dirname"],
            title=row["title"],
            release_type=row["release_type"],
            release_year=row["release_year"],
            new=bool(row["new"]),
            genres=row["genres"].split(r" \\ "),
            labels=row["labels"].split(r" \\ "),
            artists=artists,
        )


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
