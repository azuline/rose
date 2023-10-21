import shutil
import time
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import asdict
from multiprocessing import Process
from pathlib import Path

import pytest

from rose.config import Config
from rose.virtualfs import mount_virtualfs, unmount_virtualfs


@contextmanager
def startfs(c: Config) -> Iterator[None]:
    p = Process(target=mount_virtualfs, args=[c, True, True, False])
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
def test_virtual_filesystem_reads(config: Config) -> None:
    def can_read(p: Path) -> bool:
        with p.open("rb") as fp:
            return fp.read(256) != b"\x00" * 256

    root = config.fuse_mount_dir
    with startfs(config):
        assert not (root / "lalala").exists()

        assert (root / "Releases").is_dir()
        assert (root / "Releases" / "r1").is_dir()
        assert not (root / "Releases" / "lalala").exists()
        assert (root / "Releases" / "r1" / "01.m4a").is_file()
        assert not (root / "Releases" / "r1" / "lala.m4a").exists()
        assert can_read(root / "Releases" / "r1" / "01.m4a")

        assert (root / "Releases" / "r2" / "cover.jpg").is_file()
        assert can_read(root / "Releases" / "r2" / "cover.jpg")
        assert not (root / "Releases" / "r1" / "cover.jpg").exists()
        assert not (root / "Releases" / "r2" / "cover.png").exists()

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

        assert (root / "Collages").is_dir()
        assert (root / "Collages" / "Rose Gold").is_dir()
        assert (root / "Collages" / "Ruby Red").is_dir()
        assert not (root / "Collages" / "lalala").exists()
        assert (root / "Collages" / "Rose Gold" / "1. r1").is_dir()
        assert not (root / "Collages" / "Rose Gold" / "lalala").exists()
        assert (root / "Collages" / "Rose Gold" / "1. r1" / "01.m4a").is_file()
        assert not (root / "Collages" / "Rose Gold" / "1. r1" / "lalala").exists()
        assert can_read(root / "Collages" / "Rose Gold" / "1. r1" / "01.m4a")

        assert (root / "New").is_dir()
        assert (root / "New" / "{NEW} r3").is_dir()
        assert not (root / "New" / "r2").exists()
        assert (root / "New" / "{NEW} r3" / "01.m4a").is_file()
        assert not (root / "New" / "{NEW} r3" / "lalala").exists()


@pytest.mark.usefixtures("seeded_cache")
def test_virtual_filesystem_write_files(config: Config) -> None:
    root = config.fuse_mount_dir
    with startfs(config):
        with (root / "Releases" / "r1" / "01.m4a").open("w") as fp:
            fp.write("abc")
        with (root / "Releases" / "r1" / "01.m4a").open("r") as fp:
            assert fp.read() == "abc"
        with pytest.raises(OSError):  # noqa: PT011
            (root / "Releases" / "r1" / "lalala").open("w")


@pytest.mark.usefixtures("seeded_cache")
def test_virtual_filesystem_collage_actions(config: Config) -> None:
    root = config.fuse_mount_dir
    src = config.music_source_dir

    with startfs(config):
        # Create collage.
        (root / "Collages" / "New Tee").mkdir(parents=True)
        assert (src / "!collages" / "New Tee.toml").is_file()
        # Rename collage.
        (root / "Collages" / "New Tee").rename(root / "Collages" / "New Jeans")
        assert (src / "!collages" / "New Jeans.toml").is_file()
        assert not (src / "!collages" / "New Tee.toml").exists()
        # Add release to collage.
        shutil.copytree(root / "Releases" / "r1", root / "Collages" / "New Jeans" / "r1")
        assert (root / "Collages" / "New Jeans" / "r1").is_dir()
        assert (root / "Collages" / "New Jeans" / "r1" / "01.m4a").is_file()
        with (src / "!collages" / "New Jeans.toml").open("r") as fp:
            assert "r1" in fp.read()
        # Delete release from collage.
        (root / "Collages" / "New Jeans" / "r1").rmdir()
        assert not (root / "Collages" / "New Jeans" / "r1").exists()
        with (src / "!collages" / "New Jeans.toml").open("r") as fp:
            assert "r1" not in fp.read()
        # Delete collage.
        (root / "Collages" / "New Jeans").rmdir()
        assert not (src / "!collages" / "New Jeans.toml").exists()


def test_virtual_filesystem_toggle_new(config: Config, source_dir: Path) -> None:  # noqa: ARG001
    dirname = "NewJeans - 1990. I Love NewJeans [K-Pop;R&B] {A Cool Label}"
    root = config.fuse_mount_dir
    with startfs(config):
        (root / "Releases" / dirname).rename(root / "Releases" / f"{{NEW}} {dirname}")
        assert (root / "Releases" / f"{{NEW}} {dirname}").is_dir()
        assert not (root / "Releases" / dirname).exists()
        (root / "Releases" / f"{{NEW}} {dirname}").rename(root / "Releases" / dirname)
        assert (root / "Releases" / dirname).is_dir()
        assert not (root / "Releases" / f"{{NEW}} {dirname}").exists()
        with pytest.raises(OSError):  # noqa: PT011
            (root / "Releases" / dirname).rename(root / "Releases" / "lalala")


@pytest.mark.usefixtures("seeded_cache")
def test_virtual_filesystem_hide_values(config: Config) -> None:
    new_config = Config(
        **{
            **asdict(config),
            "fuse_hide_artists": ["Bass Man"],
            "fuse_hide_genres": ["Techno"],
            "fuse_hide_labels": ["Silk Music"],
        },
    )
    root = config.fuse_mount_dir
    with startfs(new_config):
        assert not (root / "Artists" / "Bass Man").exists()
        assert not (root / "Genres" / "Techno").exists()
        assert not (root / "Labels" / "Silk Music").exists()
