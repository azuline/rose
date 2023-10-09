import logging
import sqlite3
from collections.abc import Iterator
from contextlib import contextmanager

import yoyo

from rose.foundation.conf import MIGRATIONS_PATH, Config

logger = logging.getLogger(__name__)


@contextmanager
def connect(c: Config) -> Iterator[sqlite3.Connection]:
    conn = connect_fn(c)
    try:
        yield conn
    finally:
        conn.close()


def connect_fn(c: Config) -> sqlite3.Connection:
    """Non-context manager version of connect."""
    conn = sqlite3.connect(
        c.cache_database_path,
        detect_types=sqlite3.PARSE_DECLTYPES,
        isolation_level=None,
        timeout=15.0,
    )

    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA foreign_keys=ON")
    conn.execute("PRAGMA journal_mode=WAL")

    return conn


def migrate_database(c: Config) -> None:
    db_backend = yoyo.get_backend(f"sqlite:///{c.cache_database_path}")
    db_migrations = yoyo.read_migrations(str(MIGRATIONS_PATH))

    logger.debug("Applying database migrations")
    with db_backend.lock():
        db_backend.apply_migrations(db_backend.to_apply(db_migrations))
