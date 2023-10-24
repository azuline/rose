import uuid


class RoseError(Exception):
    pass


def valid_uuid(x: str) -> bool:
    try:
        uuid.UUID(x)
        return True
    except ValueError:
        return False
