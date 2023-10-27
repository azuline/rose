import uuid
from pathlib import Path

with (Path(__file__).parent / ".version").open("r") as fp:
    VERSION = fp.read().strip()


class RoseError(Exception):
    pass


def valid_uuid(x: str) -> bool:
    try:
        uuid.UUID(x)
        return True
    except ValueError:
        return False
