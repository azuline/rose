import binascii
import hashlib
import logging
import random
import sqlite3
import time
from collections.abc import Iterator
from contextlib import contextmanager

from rose.foundation.conf import SCHEMA_PATH, Config

logger = logging.getLogger(__name__)


@contextmanager
def connect(c: Config) -> Iterator[sqlite3.Connection]:
    conn = connect_fn(c)
    try:
        yield conn
    finally:
        conn.close()


@contextmanager
def transaction(conn: sqlite3.Connection) -> Iterator[sqlite3.Connection]:
    """
    A simple context wrapper for a database transaction. If connection is null,
    a new connection is created.
    """
    tx_log_id = binascii.b2a_hex(random.randbytes(8)).decode()
    start_time = time.time()

    # If we're already in a transaction, don't create a nested transaction.
    if conn.in_transaction:
        logger.debug(f"Transaction {tx_log_id}. Starting nested transaction, NoOp.")
        yield conn
        logger.debug(
            f"Transaction {tx_log_id}. End of nested transaction. "
            f"Duration: {time.time() - start_time}."
        )
        return

    logger.debug(f"Transaction {tx_log_id}. Starting transaction from conn.")
    with conn:
        # We BEGIN IMMEDIATE to avoid deadlocks, which pisses the hell out of me because no one's
        # documenting this properly and SQLite just dies without respecting the timeout and without
        # a reasonable error message. Absurd.
        # - https://sqlite.org/forum/forumpost/a3db6dbff1cd1d5d
        conn.execute("BEGIN IMMEDIATE")
        yield conn
        logger.debug(
            f"Transaction {tx_log_id}. End of transaction from conn. "
            f"Duration: {time.time() - start_time}."
        )


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
    """
    "Migrate" the database. If the schema in the database does not match that on disk, then nuke the
    database and recreate it from scratch. Otherwise, no op.

    We can do this because the database is just a read cache. It is not source-of-truth for any of
    its own data.
    """
    with SCHEMA_PATH.open("rb") as fp:
        latest_schema_hash = hashlib.sha256(fp.read()).hexdigest()

    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT EXISTS(
                SELECT * FROM sqlite_master
                WHERE type = 'table' AND name = '_schema_hash'
            )
            """
        )
        if cursor.fetchone()[0]:
            cursor = conn.execute("SELECT value FROM _schema_hash")
            if (row := cursor.fetchone()) and row[0] == latest_schema_hash:
                # Everything matches! Exit!
                return

    c.cache_database_path.unlink(missing_ok=True)
    with connect(c) as conn:
        with SCHEMA_PATH.open("r") as fp:
            conn.executescript(fp.read())
        conn.execute("CREATE TABLE _schema_hash (value TEXT PRIMARY KEY)")
        conn.execute("INSERT INTO _schema_hash (value) VALUES (?)", (latest_schema_hash,))
