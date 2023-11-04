"""
The common module is our ugly grab bag of miscellaneous things. Though a fully generalized common
module is _typically_ a bad idea, we have few enough things in it that it's OK for now.
"""

import re
import uuid
from pathlib import Path
from typing import TypeVar

with (Path(__file__).parent / ".version").open("r") as fp:
    VERSION = fp.read().strip()

T = TypeVar("T")


class RoseError(Exception):
    pass


class RoseExpectedError(RoseError):
    """These errors are printed without traceback."""

    pass


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
    return ILLEGAL_FS_CHARS_REGEX.sub("_", x)
