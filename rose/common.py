"""
The common module is our ugly grab bag of miscellaneous things. Though a fully generalized common
module is _typically_ a bad idea, we have few enough things in it that it's OK for now.
"""

import uuid
from pathlib import Path

with (Path(__file__).parent / ".version").open("r") as fp:
    VERSION = fp.read().strip()


class RoseError(Exception):
    pass


class InvalidCoverArtFileError(RoseError):
    pass


def valid_uuid(x: str) -> bool:
    try:
        uuid.UUID(x)
        return True
    except ValueError:
        return False
