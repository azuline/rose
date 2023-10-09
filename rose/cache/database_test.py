import sqlite3
from pathlib import Path

import yoyo

from conftest import freeze_database_time
from rose.cache.database import migrate_database
from rose.foundation.conf import MIGRATIONS_PATH, Config


def test_run_database_migrations(config: Config) -> None:
    migrate_database(config)
    assert config.cache_database_path.exists()

    with sqlite3.connect(str(config.cache_database_path)) as conn:
        freeze_database_time(conn)
        cursor = conn.execute("SELECT 1 FROM _yoyo_version")
        assert len(cursor.fetchall()) > 0


def test_migrations(isolated_dir: Path) -> None:
    """
    Test that, for each migration, the up -> down -> up path doesn't
    cause an error. Basically, ladder our way up through the migration
    chain.
    """
    backend = yoyo.get_backend(f"sqlite:///{isolated_dir / 'db.sqlite3'}")
    migrations = yoyo.read_migrations(str(MIGRATIONS_PATH))

    assert len(migrations) > 0
    for mig in migrations:
        backend.apply_one(mig)
        backend.rollback_one(mig)
        backend.apply_one(mig)
