import time
from collections.abc import Iterator
from contextlib import contextmanager
from multiprocessing import Process
from pathlib import Path

import pytest

from rose.config import Config
from rose.virtualfs import mount_virtualfs, unmount_virtualfs


@contextmanager
def startfs(c: Config) -> Iterator[None]:
    p = Process(target=mount_virtualfs, args=[c, True])
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
        assert (root / "Albums").is_dir()
        assert (root / "Albums" / "r1").is_dir()
        assert not (root / "Albums" / "lalala").exists()
        assert (root / "Albums" / "r1" / "01.m4a").is_file()
        assert not (root / "Albums" / "r1" / "lala.m4a").exists()
        assert can_read(root / "Albums" / "r1" / "01.m4a")

        assert (root / "Albums" / "r2" / "cover.jpg").is_file()
        assert can_read(root / "Albums" / "r2" / "cover.jpg")
        assert not (root / "Albums" / "r1" / "cover.jpg").exists()
        assert not (root / "Albums" / "r2" / "cover.png").exists()

        assert (root / "Artists").is_dir()
        assert (root / "Artists" / "Bass Man").is_dir()
        assert not (root / "Artists" / "lalala").exists()
        assert (root / "Artists" / "Bass Man" / "r1").is_dir()
        assert not (root / "Artists" / "Bass Man" / "lalala").exists()
        assert (root / "Artists" / "Bass Man" / "r1" / "01.m4a").is_file()
        assert not (root / "Artists" / "Bass Man" / "r1" / "lalala.m4a").exists()
        assert can_read(root / "Artists" / "Bass Man" / "r1" / "01.m4a")

        assert (root / "Genres").is_dir()
        assert (root / "Genres" / "Techno").is_dir()
        assert not (root / "Genres" / "lalala").exists()
        assert (root / "Genres" / "Techno" / "r1").is_dir()
        assert not (root / "Genres" / "Techno" / "lalala").exists()
        assert (root / "Genres" / "Techno" / "r1" / "01.m4a").is_file()
        assert not (root / "Genres" / "Techno" / "r1" / "lalala.m4a").exists()
        assert can_read(root / "Genres" / "Techno" / "r1" / "01.m4a")

        assert (root / "Labels").is_dir()
        assert (root / "Labels" / "Silk Music").is_dir()
        assert not (root / "Labels" / "lalala").exists()
        assert (root / "Labels" / "Silk Music" / "r1").is_dir()
        assert not (root / "Labels" / "Silk Music" / "lalala").exists()
        assert (root / "Labels" / "Silk Music" / "r1" / "01.m4a").is_file()
        assert not (root / "Labels" / "Silk Music" / "r1" / "lalala").exists()
        assert can_read(root / "Labels" / "Silk Music" / "r1" / "01.m4a")

        assert not (root / "lalala").exists()
