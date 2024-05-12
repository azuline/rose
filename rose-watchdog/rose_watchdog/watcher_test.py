import shutil
import time
from collections.abc import Iterator
from contextlib import contextmanager
from multiprocessing import Process

from rose import Config
from rose.cache import connect

from conftest import TEST_COLLAGE_1, TEST_PLAYLIST_1, TEST_RELEASE_2, TEST_RELEASE_3, retry_for_sec
from rose_watchdog.watcher import start_watchdog


@contextmanager
def start_watcher(c: Config) -> Iterator[None]:
    process = Process(target=start_watchdog, args=[c])
    try:
        process.start()
        time.sleep(0.05)
        yield
    finally:
        process.terminate()


def test_watchdog_events(config: Config) -> None:
    src = config.music_source_dir
    with start_watcher(config):
        # Create release.
        shutil.copytree(TEST_RELEASE_2, src / TEST_RELEASE_2.name)
        for _ in retry_for_sec(2):
            with connect(config) as conn:
                cursor = conn.execute("SELECT id FROM releases")
                if {r["id"] for r in cursor.fetchall()} == {"ilovecarly"}:
                    break
        else:
            raise AssertionError("Failed to find release ID in cache.")

        # Create another release.
        shutil.copytree(TEST_RELEASE_3, src / TEST_RELEASE_3.name)
        for _ in retry_for_sec(2):
            with connect(config) as conn:
                cursor = conn.execute("SELECT id FROM releases")
                if {r["id"] for r in cursor.fetchall()} == {"ilovecarly", "ilovenewjeans"}:
                    break
        else:
            raise AssertionError("Failed to find second release ID in cache.")

        # Create collage.
        shutil.copytree(TEST_COLLAGE_1, src / "!collages")
        for _ in retry_for_sec(2):
            with connect(config) as conn:
                cursor = conn.execute("SELECT name FROM collages")
                if {r["name"] for r in cursor.fetchall()} != {"Rose Gold"}:
                    continue
                cursor = conn.execute("SELECT release_id FROM collages_releases")
                if {r["release_id"] for r in cursor.fetchall()} != {"ilovecarly", "ilovenewjeans"}:
                    continue
                break
        else:
            raise AssertionError("Failed to find collage in cache.")

        # Create playlist.
        shutil.copytree(TEST_PLAYLIST_1, src / "!playlists")
        for _ in retry_for_sec(2):
            with connect(config) as conn:
                cursor = conn.execute("SELECT name FROM playlists")
                if {r["name"] for r in cursor.fetchall()} != {"Lala Lisa"}:
                    continue
                cursor = conn.execute("SELECT track_id FROM playlists_tracks")
                if {r["track_id"] for r in cursor.fetchall()} != {"iloveloona", "ilovetwice"}:
                    continue
                break
        else:
            raise AssertionError("Failed to find release in cache.")

        # Create/rename/delete random files; check that they don't interfere with rest of the test.
        (src / "hi.nfo").touch()
        (src / "hi.nfo").rename(src / "!collages" / "bye.haha")
        (src / "!collages" / "bye.haha").rename(src / "!playlists" / "bye.haha")
        (src / "!playlists" / "bye.haha").unlink()

        # Delete release.
        shutil.rmtree(src / TEST_RELEASE_2.name)
        for _ in retry_for_sec(2):
            with connect(config) as conn:
                cursor = conn.execute("SELECT id FROM releases")
                if {r["id"] for r in cursor.fetchall()} != {"ilovenewjeans"}:
                    continue
                break
        else:
            raise AssertionError("Failed to see release deletion in cache.")

        # Rename release.
        (src / TEST_RELEASE_3.name).rename(src / "lalala")
        time.sleep(0.5)
        for _ in retry_for_sec(2):
            with connect(config) as conn:
                cursor = conn.execute("SELECT id, source_path FROM releases")
                rows = cursor.fetchall()
                if len(rows) != 1:
                    continue
                row = rows[0]
                if row["id"] != "ilovenewjeans":
                    continue
                if row["source_path"] != str(src / "lalala"):
                    continue
                break
        else:
            raise AssertionError("Failed to see release deletion in cache.")

        # Rename collage.
        (src / "!collages" / "Rose Gold.toml").rename(src / "!collages" / "Black Pink.toml")
        for _ in retry_for_sec(2):
            with connect(config) as conn:
                cursor = conn.execute("SELECT name FROM collages")
                if {r["name"] for r in cursor.fetchall()} != {"Black Pink"}:
                    continue
                cursor = conn.execute("SELECT release_id FROM collages_releases")
                if {r["release_id"] for r in cursor.fetchall()} != {"ilovecarly", "ilovenewjeans"}:
                    continue
                break
        else:
            raise AssertionError("Failed to see collage rename in cache.")

        # Delete collage.
        (src / "!collages" / "Black Pink.toml").unlink()
        for _ in retry_for_sec(2):
            with connect(config) as conn:
                cursor = conn.execute("SELECT COUNT(*) FROM collages")
                if cursor.fetchone()[0] == 0:
                    break
        else:
            raise AssertionError("Failed to see collage deletion in cache.")

        # Rename playlist.
        (src / "!playlists" / "Lala Lisa.toml").rename(src / "!playlists" / "Turtle Rabbit.toml")
        for _ in retry_for_sec(2):
            with connect(config) as conn:
                cursor = conn.execute("SELECT name FROM playlists")
                if {r["name"] for r in cursor.fetchall()} == {"Turtle Rabbit"}:
                    break
        else:
            raise AssertionError("Failed to see playlist rename in cache.")

        # Delete playlist.
        (src / "!playlists" / "Turtle Rabbit.toml").unlink()
        for _ in retry_for_sec(2):
            with connect(config) as conn:
                cursor = conn.execute("SELECT COUNT(*) FROM playlists")
                if cursor.fetchone()[0] == 0:
                    break
        else:
            raise AssertionError("Failed to see playlist deletion in cache.")
