import hashlib

from rose.cache.database import connect, migrate_database
from rose.foundation.conf import SCHEMA_PATH, Config


def test_schema(config: Config) -> None:
    # Test that the schema successfully bootstraps.
    with SCHEMA_PATH.open("rb") as fp:
        latest_schema_hash = hashlib.sha256(fp.read()).hexdigest()
    migrate_database(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT value FROM _schema_hash")
        assert cursor.fetchone()[0] == latest_schema_hash


def test_migration(config: Config) -> None:
    # Test that "migrating" the database correctly migrates it.
    config.cache_database_path.unlink()
    with connect(config) as conn:
        conn.execute("CREATE TABLE _schema_hash (value TEXT PRIMARY KEY)")
        conn.execute("INSERT INTO _schema_hash (value) VALUES ('haha')")

    with SCHEMA_PATH.open("rb") as fp:
        latest_schema_hash = hashlib.sha256(fp.read()).hexdigest()
    migrate_database(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT value FROM _schema_hash")
        assert cursor.fetchone()[0] == latest_schema_hash
        cursor = conn.execute("SELECT COUNT(*) FROM _schema_hash")
        assert cursor.fetchone()[0] == 1
