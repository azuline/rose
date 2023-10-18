import shutil
import time
from collections.abc import Iterator
from contextlib import contextmanager

from conftest import TEST_COLLAGE_1, TEST_RELEASE_2, TEST_RELEASE_3
from rose.cache import connect
from rose.config import Config
from rose.watcher import create_watchdog_observer


@contextmanager
def start_watcher(c: Config) -> Iterator[None]:
    observer = create_watchdog_observer(c)
    try:
        observer.start()  # type: ignore
        time.sleep(0.05)
        yield
    finally:
        observer.stop()  # type: ignore


def test_watchdog_events(config: Config) -> None:
    src = config.music_source_dir
    with start_watcher(config):
        # Create release.
        shutil.copytree(TEST_RELEASE_2, src / TEST_RELEASE_2.name)
        time.sleep(0.05)
        with connect(config) as conn:
            cursor = conn.execute("SELECT id FROM releases")
            assert {r["id"] for r in cursor.fetchall()} == {"ilovecarly"}

        # Create another release.
        shutil.copytree(TEST_RELEASE_3, src / TEST_RELEASE_3.name)
        time.sleep(0.05)
        with connect(config) as conn:
            cursor = conn.execute("SELECT id FROM releases")
            assert {r["id"] for r in cursor.fetchall()} == {"ilovecarly", "ilovenewjeans"}

        # Create collage.
        shutil.copytree(TEST_COLLAGE_1, src / "!collages")
        time.sleep(0.05)
        with connect(config) as conn:
            cursor = conn.execute("SELECT name FROM collages")
            assert {r["name"] for r in cursor.fetchall()} == {"Rose Gold"}
            cursor = conn.execute("SELECT release_id FROM collages_releases")
            assert {r["release_id"] for r in cursor.fetchall()} == {"ilovecarly", "ilovenewjeans"}

        # Create/rename/delete random files; check that they don't interfere with rest of the test.
        (src / "hi.nfo").touch()
        (src / "hi.nfo").rename(src / "!collages" / "bye.haha")
        (src / "!collages" / "bye.haha").unlink()

        # Delete release.
        shutil.rmtree(src / TEST_RELEASE_3.name)
        time.sleep(0.05)
        with connect(config) as conn:
            cursor = conn.execute("SELECT id FROM releases")
            assert {r["id"] for r in cursor.fetchall()} == {"ilovecarly"}
            cursor = conn.execute("SELECT release_id FROM collages_releases")
            assert {r["release_id"] for r in cursor.fetchall()} == {"ilovecarly"}

        # Rename release.
        (src / TEST_RELEASE_2.name).rename(src / "lalala")
        time.sleep(0.05)
        with connect(config) as conn:
            cursor = conn.execute("SELECT id, source_path FROM releases")
            rows = cursor.fetchall()
            assert len(rows) == 1
            row = rows[0]
            assert row["id"] == "ilovecarly"
            assert row["source_path"] == str(src / "lalala")

        # Rename collage.
        (src / "!collages" / "Rose Gold.toml").rename(src / "!collages" / "Black Pink.toml")
        time.sleep(0.05)
        with connect(config) as conn:
            cursor = conn.execute("SELECT name FROM collages")
            assert {r["name"] for r in cursor.fetchall()} == {"Black Pink"}
            cursor = conn.execute("SELECT release_id FROM collages_releases")
            assert {r["release_id"] for r in cursor.fetchall()} == {"ilovecarly"}

        # Delete collage.
        (src / "!collages" / "Black Pink.toml").unlink()
        time.sleep(0.05)
        with connect(config) as conn:
            cursor = conn.execute("SELECT COUNT(*) FROM collages")
            assert cursor.fetchone()[0] == 0
