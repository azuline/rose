from pathlib import Path
from typing import Iterator

from rose.cache.database import connect
from rose.cache.dataclasses import CachedArtist, CachedRelease
from rose.foundation.conf import Config


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


def release_exists(c: Config, virtual_dirname: str) -> bool:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT EXISTS(SELECT * FROM releases WHERE virtual_dirname = ?)",
            (virtual_dirname,),
        )
        return bool(cursor.fetchone()[0])


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