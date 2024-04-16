"""
The common module is our ugly grab bag of common toys. Though a fully generalized common module is
_typically_ a bad idea, we have few enough things in it that it's OK for now.
"""

import dataclasses
import hashlib
import os.path
import re
from collections.abc import Iterator
from pathlib import Path
from typing import Any, TypeVar

with (Path(__file__).parent / ".version").open("r") as fp:
    VERSION = fp.read().strip()

T = TypeVar("T")


class RoseError(Exception):
    pass


class RoseExpectedError(RoseError):
    """These errors are printed without traceback."""

    pass


@dataclasses.dataclass
class Artist:
    name: str
    alias: bool = False

    def __hash__(self) -> int:
        return hash((self.name, self.alias))


@dataclasses.dataclass
class ArtistMapping:
    main: list[Artist] = dataclasses.field(default_factory=list)
    guest: list[Artist] = dataclasses.field(default_factory=list)
    remixer: list[Artist] = dataclasses.field(default_factory=list)
    producer: list[Artist] = dataclasses.field(default_factory=list)
    composer: list[Artist] = dataclasses.field(default_factory=list)
    conductor: list[Artist] = dataclasses.field(default_factory=list)
    djmixer: list[Artist] = dataclasses.field(default_factory=list)

    @property
    def all(self) -> list[Artist]:
        return uniq(
            self.main
            + self.guest
            + self.remixer
            + self.producer
            + self.composer
            + self.conductor
            + self.djmixer
        )

    def dump(self) -> dict[str, Any]:
        return dataclasses.asdict(self)

    def items(self) -> Iterator[tuple[str, list[Artist]]]:
        yield "main", self.main
        yield "guest", self.guest
        yield "remixer", self.remixer
        yield "producer", self.producer
        yield "composer", self.composer
        yield "conductor", self.conductor
        yield "djmixer", self.djmixer


def uniq(xs: list[T]) -> list[T]:
    rv: list[T] = []
    seen: set[T] = set()
    for x in xs:
        if x not in seen:
            rv.append(x)
            seen.add(x)
    return rv


ILLEGAL_FS_CHARS_REGEX = re.compile(r'[:\?<>\\*\|"\/]+')


def sanitize_dirname(name: str, enforce_maxlen: bool) -> str:
    """
    Replace illegal characters and truncate. We have 255 bytes in ext4, and we truncate to 240 in
    order to leave room for any collision numbers.

    enforce_maxlen is for host filesystems, which are sometimes subject to length constraints (e.g.
    ext4).
    """
    name = ILLEGAL_FS_CHARS_REGEX.sub("_", name)
    if enforce_maxlen:
        name = name.encode("utf-8")[:240].decode("utf-8", "ignore")
    return name


def sanitize_filename(name: str, enforce_maxlen: bool) -> str:
    """Same as sanitize dirname, except we preserve file extension."""
    name = ILLEGAL_FS_CHARS_REGEX.sub("_", name)
    if enforce_maxlen:
        # Preserve the extension.
        stem, ext = os.path.splitext(name)
        # But ignore if the extension is longer than 6 characters; that means it's probably bullshit.
        if len(ext.encode()) > 6:
            stem = name
            ext = ""
        stem = stem.encode("utf-8")[:240].decode("utf-8", "ignore")
        name = stem + ext
    return name


def sha256_dataclass(dc: Any) -> str:
    hasher = hashlib.sha256()
    _rec_sha256_dataclass(hasher, dc)
    return hasher.hexdigest()


def _rec_sha256_dataclass(hasher: Any, value: Any) -> None:
    if dataclasses.is_dataclass(value):
        for field in sorted(value.__dataclass_fields__):  # Sort the fields for consistent order
            _rec_sha256_dataclass(hasher, getattr(value, field))
    elif isinstance(value, list):
        for item in value:
            _rec_sha256_dataclass(hasher, item)
    elif isinstance(value, dict):
        for k, v in sorted(value.items()):  # Sort the keys for consistent order
            hasher.update(str(k).encode())
            _rec_sha256_dataclass(hasher, v)
    else:
        hasher.update(str(value).encode())
