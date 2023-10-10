from dataclasses import dataclass
from pathlib import Path


@dataclass
class CachedArtist:
    name: str
    role: str


@dataclass
class CachedRelease:
    id: str
    source_path: Path
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
    trackno: str
    discno: str
    duration_sec: int

    artists: list[CachedArtist]
