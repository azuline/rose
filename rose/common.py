import re
import uuid


class RoseError(Exception):
    pass


ILLEGAL_FS_CHARS_REGEX = re.compile(r'[:\?<>\\*\|"\/]+')


def sanitize_filename(x: str) -> str:
    return ILLEGAL_FS_CHARS_REGEX.sub("_", x)


def valid_uuid(x: str) -> bool:
    try:
        uuid.UUID(x)
        return True
    except ValueError:
        return False
