"""
The common module is our ugly grab bag of common toys. Though a fully generalized common module is
_typically_ a bad idea, we have few enough things in it that it's OK for now.
"""

import dataclasses
import re
import uuid
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
        return hash(f"{self.name}+{self.alias}")


@dataclasses.dataclass
class ArtistMapping:
    main: list[Artist] = dataclasses.field(default_factory=list)
    guest: list[Artist] = dataclasses.field(default_factory=list)
    remixer: list[Artist] = dataclasses.field(default_factory=list)
    producer: list[Artist] = dataclasses.field(default_factory=list)
    composer: list[Artist] = dataclasses.field(default_factory=list)
    djmixer: list[Artist] = dataclasses.field(default_factory=list)

    @property
    def all(self) -> list[Artist]:
        return uniq(
            self.main + self.guest + self.remixer + self.producer + self.composer + self.djmixer
        )

    def dump(self) -> dict[str, Any]:
        return dataclasses.asdict(self)

    def items(self) -> Iterator[tuple[str, list[Artist]]]:
        yield "main", self.main
        yield "guest", self.guest
        yield "remixer", self.remixer
        yield "producer", self.producer
        yield "composer", self.composer
        yield "djmixer", self.djmixer


def valid_uuid(x: str) -> bool:
    try:
        uuid.UUID(x)
        return True
    except ValueError:
        return False


def uniq(xs: list[T]) -> list[T]:
    rv: list[T] = []
    seen: set[T] = set()
    for x in xs:
        if x not in seen:
            rv.append(x)
            seen.add(x)
    return rv


ILLEGAL_FS_CHARS_REGEX = re.compile(r'[:\?<>\\*\|"\/]+')


def sanitize_filename(x: str) -> str:
    """
    Replace illegal characters and truncate. We have 255 bytes in ext4, and we truncate to 240 in
    order to leave room for any collision numbers.
    """
    x = ILLEGAL_FS_CHARS_REGEX.sub("_", x)
    return x.encode("utf-8")[:240].decode("utf-8", "ignore")
