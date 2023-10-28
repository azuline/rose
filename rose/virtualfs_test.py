import shutil
import subprocess
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
def start_virtual_fs(c: Config) -> Iterator[None]:
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
    with start_virtual_fs(config):
        assert not (root / "lalala").exists()

        assert (root / "1. Releases").is_dir()
        assert (root / "1. Releases" / "r1").is_dir()
        assert not (root / "1. Releases" / "lalala").exists()
        assert (root / "1. Releases" / "r1" / "01.m4a").is_file()
        assert not (root / "1. Releases" / "r1" / "lala.m4a").exists()
        assert can_read(root / "1. Releases" / "r1" / "01.m4a")

        assert (root / "1. Releases" / "r2" / "cover.jpg").is_file()
        assert can_read(root / "1. Releases" / "r2" / "cover.jpg")
        assert not (root / "1. Releases" / "r1" / "cover.jpg").exists()
        assert not (root / "1. Releases" / "r2" / "cover.png").exists()

        assert (root / "2. Releases - New").is_dir()
        assert (root / "2. Releases - New" / "{NEW} r3").is_dir()
        assert not (root / "2. Releases - New" / "r2").exists()
        assert (root / "2. Releases - New" / "{NEW} r3" / "01.m4a").is_file()
        assert not (root / "2. Releases - New" / "{NEW} r3" / "lalala").exists()

        assert (root / "3. Releases - Recently Added").is_dir()
        assert (root / "3. Releases - Recently Added" / "[0000-01-01] r2").exists()
        assert not (root / "3. Releases - Recently Added" / "r2").exists()
        assert (root / "3. Releases - Recently Added" / "[0000-01-01] r2" / "01.m4a").is_file()
        assert not (root / "3. Releases - Recently Added" / "r2" / "lalala").exists()

        assert (root / "4. Artists").is_dir()
        assert (root / "4. Artists" / "Bass Man").is_dir()
        assert not (root / "4. Artists" / "lalala").exists()
        assert (root / "4. Artists" / "Bass Man" / "r1").is_dir()
        assert not (root / "4. Artists" / "Bass Man" / "lalala").exists()
        assert (root / "4. Artists" / "Bass Man" / "r1" / "01.m4a").is_file()
        assert not (root / "4. Artists" / "Bass Man" / "r1" / "lalala.m4a").exists()
        assert can_read(root / "4. Artists" / "Bass Man" / "r1" / "01.m4a")

        assert (root / "5. Genres").is_dir()
        assert (root / "5. Genres" / "Techno").is_dir()
        assert not (root / "5. Genres" / "lalala").exists()
        assert (root / "5. Genres" / "Techno" / "r1").is_dir()
        assert not (root / "5. Genres" / "Techno" / "lalala").exists()
        assert (root / "5. Genres" / "Techno" / "r1" / "01.m4a").is_file()
        assert not (root / "5. Genres" / "Techno" / "r1" / "lalala.m4a").exists()
        assert can_read(root / "5. Genres" / "Techno" / "r1" / "01.m4a")

        assert (root / "6. Labels").is_dir()
        assert (root / "6. Labels" / "Silk Music").is_dir()
        assert not (root / "6. Labels" / "lalala").exists()
        assert (root / "6. Labels" / "Silk Music" / "r1").is_dir()
        assert not (root / "6. Labels" / "Silk Music" / "lalala").exists()
        assert (root / "6. Labels" / "Silk Music" / "r1" / "01.m4a").is_file()
        assert not (root / "6. Labels" / "Silk Music" / "r1" / "lalala").exists()
        assert can_read(root / "6. Labels" / "Silk Music" / "r1" / "01.m4a")

        assert (root / "7. Collages").is_dir()
        assert (root / "7. Collages" / "Rose Gold").is_dir()
        assert (root / "7. Collages" / "Ruby Red").is_dir()
        assert not (root / "7. Collages" / "lalala").exists()
        assert (root / "7. Collages" / "Rose Gold" / "1. r1").is_dir()
        assert not (root / "7. Collages" / "Rose Gold" / "lalala").exists()
        assert (root / "7. Collages" / "Rose Gold" / "1. r1" / "01.m4a").is_file()
        assert not (root / "7. Collages" / "Rose Gold" / "1. r1" / "lalala").exists()
        assert can_read(root / "7. Collages" / "Rose Gold" / "1. r1" / "01.m4a")

        assert (root / "8. Playlists").is_dir()
        assert (root / "8. Playlists" / "Lala Lisa").is_dir()
        assert (root / "8. Playlists" / "Turtle Rabbit").is_dir()
        assert not (root / "8. Playlists" / "lalala").exists()
        assert (root / "8. Playlists" / "Lala Lisa" / "1. 01.m4a").is_file()
        assert (root / "8. Playlists" / "Lala Lisa" / "cover.jpg").is_file()
        assert not (root / "8. Playlists" / "Lala Lisa" / "lalala").exists()
        assert can_read(root / "8. Playlists" / "Lala Lisa" / "1. 01.m4a")
        assert can_read(root / "8. Playlists" / "Lala Lisa" / "cover.jpg")


@pytest.mark.usefixtures("seeded_cache")
def test_virtual_filesystem_write_files(config: Config) -> None:
    root = config.fuse_mount_dir
    with start_virtual_fs(config):
        with (root / "1. Releases" / "r1" / "01.m4a").open("w") as fp:
            fp.write("abc")
        with (root / "1. Releases" / "r1" / "01.m4a").open("r") as fp:
            assert fp.read() == "abc"
        with pytest.raises(OSError):  # noqa: PT011
            (root / "1. Releases" / "r1" / "lalala").open("w")


@pytest.mark.usefixtures("seeded_cache")
def test_virtual_filesystem_collage_actions(config: Config) -> None:
    root = config.fuse_mount_dir
    src = config.music_source_dir

    with start_virtual_fs(config):
        # Create collage.
        (root / "7. Collages" / "New Tee").mkdir(parents=True)
        assert (src / "!collages" / "New Tee.toml").is_file()
        # Rename collage.
        (root / "7. Collages" / "New Tee").rename(root / "7. Collages" / "New Jeans")
        assert (src / "!collages" / "New Jeans.toml").is_file()
        assert not (src / "!collages" / "New Tee.toml").exists()
        # Add release to collage.
        shutil.copytree(root / "1. Releases" / "r1", root / "7. Collages" / "New Jeans" / "r1")
        assert (root / "7. Collages" / "New Jeans" / "r1").is_dir()
        assert (root / "7. Collages" / "New Jeans" / "r1" / "01.m4a").is_file()
        with (src / "!collages" / "New Jeans.toml").open("r") as fp:
            assert "r1" in fp.read()
        # Delete release from collage.
        (root / "7. Collages" / "New Jeans" / "r1").rmdir()
        assert not (root / "7. Collages" / "New Jeans" / "r1").exists()
        with (src / "!collages" / "New Jeans.toml").open("r") as fp:
            assert "r1" not in fp.read()
        # Delete collage.
        (root / "7. Collages" / "New Jeans").rmdir()
        assert not (src / "!collages" / "New Jeans.toml").exists()


def test_virtual_filesystem_playlist_actions(
    config: Config,
    source_dir: Path,  # noqa: ARG001
) -> None:
    root = config.fuse_mount_dir
    src = config.music_source_dir

    release_dir = root / "1. Releases" / "{NEW} BLACKPINK - 1990. I Love Blackpink [K-Pop;Pop]"
    filename = "01. BLACKPINK - Track 1.m4a"

    with start_virtual_fs(config):
        # Create playlist.
        (root / "8. Playlists" / "New Tee").mkdir(parents=True)
        assert (src / "!playlists" / "New Tee.toml").is_file()
        # Rename playlist.
        (root / "8. Playlists" / "New Tee").rename(root / "8. Playlists" / "New Jeans")
        assert (src / "!playlists" / "New Jeans.toml").is_file()
        assert not (src / "!playlists" / "New Tee.toml").exists()
        # Add track to playlist.
        shutil.copyfile(release_dir / filename, root / "8. Playlists" / "New Jeans" / filename)
        assert (root / "8. Playlists" / "New Jeans" / "1. BLACKPINK - Track 1.m4a").is_file()
        with (src / "!playlists" / "New Jeans.toml").open("r") as fp:
            assert "BLACKPINK - Track 1.m4a" in fp.read()
        # Delete track from playlist.
        (root / "8. Playlists" / "New Jeans" / "1. BLACKPINK - Track 1.m4a").unlink()
        assert not (root / "8. Playlists" / "New Jeans" / "1. BLACKPINK - Track 1.m4a").exists()
        with (src / "!playlists" / "New Jeans.toml").open("r") as fp:
            assert "BLACKPINK - Track 1.m4a" not in fp.read()
        # Delete playlist.
        (root / "8. Playlists" / "New Jeans").rmdir()
        assert not (src / "!playlists" / "New Jeans.toml").exists()


def test_virtual_filesystem_delete_release(config: Config, source_dir: Path) -> None:
    dirname = "NewJeans - 1990. I Love NewJeans [K-Pop;R&B]"
    root = config.fuse_mount_dir
    with start_virtual_fs(config):
        # Fix: If we return EACCES from unlink, then `rm -r` fails despite `rmdir` succeeding. Thus
        # we no-op if we cannot unlink a file. And we test the real tool we want to use in
        # production.
        subprocess.run(["rm", "-r", str(root / "1. Releases" / dirname)], check=True)
        assert not (root / "1. Releases" / f"{{NEW}} {dirname}").is_dir()
        assert not (root / "1. Releases" / dirname).exists()
        assert not (source_dir / "Test Release 3").exists()


def test_virtual_filesystem_toggle_new(config: Config, source_dir: Path) -> None:  # noqa: ARG001
    dirname = "NewJeans - 1990. I Love NewJeans [K-Pop;R&B]"
    root = config.fuse_mount_dir
    with start_virtual_fs(config):
        (root / "1. Releases" / dirname).rename(root / "1. Releases" / f"{{NEW}} {dirname}")
        assert (root / "1. Releases" / f"{{NEW}} {dirname}").is_dir()
        assert not (root / "1. Releases" / dirname).exists()
        (root / "1. Releases" / f"{{NEW}} {dirname}").rename(root / "1. Releases" / dirname)
        assert (root / "1. Releases" / dirname).is_dir()
        assert not (root / "1. Releases" / f"{{NEW}} {dirname}").exists()
        with pytest.raises(OSError):  # noqa: PT011
            (root / "1. Releases" / dirname).rename(root / "1. Releases" / "lalala")


@pytest.mark.usefixtures("seeded_cache")
def test_virtual_filesystem_blacklist(config: Config) -> None:
    new_config = Config(
        **{
            **asdict(config),
            "fuse_artists_blacklist": ["Bass Man"],
            "fuse_genres_blacklist": ["Techno"],
            "fuse_labels_blacklist": ["Silk Music"],
        },
    )
    root = config.fuse_mount_dir
    with start_virtual_fs(new_config):
        assert (root / "4. Artists" / "Techno Man").is_dir()
        assert (root / "5. Genres" / "Deep House").is_dir()
        assert (root / "6. Labels" / "Native State").is_dir()
        assert not (root / "4. Artists" / "Bass Man").exists()
        assert not (root / "5. Genres" / "Techno").exists()
        assert not (root / "6. Labels" / "Silk Music").exists()


@pytest.mark.usefixtures("seeded_cache")
def test_virtual_filesystem_whitelist(config: Config) -> None:
    new_config = Config(
        **{
            **asdict(config),
            "fuse_artists_whitelist": ["Bass Man"],
            "fuse_genres_whitelist": ["Techno"],
            "fuse_labels_whitelist": ["Silk Music"],
        },
    )
    root = config.fuse_mount_dir
    with start_virtual_fs(new_config):
        assert not (root / "4. Artists" / "Techno Man").exists()
        assert not (root / "5. Genres" / "Deep House").exists()
        assert not (root / "6. Labels" / "Native State").exists()
        assert (root / "4. Artists" / "Bass Man").is_dir()
        assert (root / "5. Genres" / "Techno").is_dir()
        assert (root / "6. Labels" / "Silk Music").is_dir()
