import time
from contextlib import contextmanager
from multiprocessing import Process
from pathlib import Path
from typing import Iterator

import pytest

from rose.foundation.conf import Config
from rose.virtualfs import mount_virtualfs, unmount_virtualfs


@contextmanager
def startfs(c: Config) -> Iterator[None]:
    p = Process(target=mount_virtualfs, args=[c, ["-f"]])
    try:
        p.start()
        time.sleep(0.1)
        yield
        unmount_virtualfs(c)
        p.join(timeout=1)
    finally:
        if p.exitcode is None:  # pragma: no cover
            p.terminate()


@pytest.mark.usefixtures("seeded_cache")
def test_virtual_filesystem(config: Config) -> None:
    def can_read(p: Path) -> bool:
        with p.open("rb") as fp:
            return fp.read(256) != b"\x00" * 256

    with startfs(config):
        root = config.fuse_mount_dir
        assert (root / "albums").is_dir()
        assert (root / "albums" / "r1").is_dir()
        assert not (root / "albums" / "lalala").exists()
        assert (root / "albums" / "r1" / "01.m4a").is_file()
        assert not (root / "albums" / "r1" / "lala.m4a").exists()
        assert can_read(root / "albums" / "r1" / "01.m4a")

        assert (root / "artists").is_dir()
        assert (root / "artists" / "Bass Man").is_dir()
        assert not (root / "artists" / "lalala").exists()
        assert (root / "artists" / "Bass Man" / "r1").is_dir()
        assert not (root / "artists" / "Bass Man" / "lalala").exists()
        assert (root / "artists" / "Bass Man" / "r1" / "01.m4a").is_file()
        assert not (root / "artists" / "Bass Man" / "r1" / "lalala.m4a").exists()
        assert can_read(root / "artists" / "Bass Man" / "r1" / "01.m4a")

        assert (root / "genres").is_dir()
        assert (root / "genres" / "Techno").is_dir()
        assert not (root / "genres" / "lalala").exists()
        assert (root / "genres" / "Techno" / "r1").is_dir()
        assert not (root / "genres" / "Techno" / "lalala").exists()
        assert (root / "genres" / "Techno" / "r1" / "01.m4a").is_file()
        assert not (root / "genres" / "Techno" / "r1" / "lalala.m4a").exists()
        assert can_read(root / "genres" / "Techno" / "r1" / "01.m4a")

        assert (root / "labels").is_dir()
        assert (root / "labels" / "Silk Music").is_dir()
        assert not (root / "labels" / "lalala").exists()
        assert (root / "labels" / "Silk Music" / "r1").is_dir()
        assert not (root / "labels" / "Silk Music" / "lalala").exists()
        assert (root / "labels" / "Silk Music" / "r1" / "01.m4a").is_file()
        assert not (root / "labels" / "Silk Music" / "r1" / "lalala").exists()
        assert can_read(root / "labels" / "Silk Music" / "r1" / "01.m4a")

        assert not (root / "lalala").exists()
