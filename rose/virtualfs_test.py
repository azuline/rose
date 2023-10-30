import shutil
import subprocess
import time
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import asdict
from multiprocessing import Process
from pathlib import Path

import pytest

from rose.audiotags import AudioTags
from rose.config import Config
from rose.virtualfs import mount_virtualfs, unmount_virtualfs


@contextmanager
def start_virtual_fs(c: Config) -> Iterator[None]:
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
        assert (root / "1. Releases" / "r1" / "01. 01.m4a").is_file()
        assert not (root / "1. Releases" / "r1" / "lala.m4a").exists()
        assert can_read(root / "1. Releases" / "r1" / "01. 01.m4a")

        assert (root / "1. Releases" / "r2" / "cover.jpg").is_file()
        assert can_read(root / "1. Releases" / "r2" / "cover.jpg")
        assert not (root / "1. Releases" / "r1" / "cover.jpg").exists()
        assert not (root / "1. Releases" / "r2" / "cover.png").exists()

        assert (root / "2. Releases - New").is_dir()
        assert (root / "2. Releases - New" / "{NEW} r3").is_dir()
        assert not (root / "2. Releases - New" / "r2").exists()
        assert (root / "2. Releases - New" / "{NEW} r3" / "01. 01.m4a").is_file()
        assert not (root / "2. Releases - New" / "{NEW} r3" / "lalala").exists()

        assert (root / "3. Releases - Recently Added").is_dir()
        assert (root / "3. Releases - Recently Added" / "[0000-01-01] r2").exists()
        assert not (root / "3. Releases - Recently Added" / "r2").exists()
        assert (root / "3. Releases - Recently Added" / "[0000-01-01] r2" / "01. 01.m4a").is_file()
        assert not (root / "3. Releases - Recently Added" / "r2" / "lalala").exists()

        assert (root / "4. Artists").is_dir()
        assert (root / "4. Artists" / "Bass Man").is_dir()
        assert not (root / "4. Artists" / "lalala").exists()
        assert (root / "4. Artists" / "Bass Man" / "r1").is_dir()
        assert not (root / "4. Artists" / "Bass Man" / "lalala").exists()
        assert (root / "4. Artists" / "Bass Man" / "r1" / "01. 01.m4a").is_file()
        assert not (root / "4. Artists" / "Bass Man" / "r1" / "lalala.m4a").exists()
        assert can_read(root / "4. Artists" / "Bass Man" / "r1" / "01. 01.m4a")

        assert (root / "5. Genres").is_dir()
        assert (root / "5. Genres" / "Techno").is_dir()
        assert not (root / "5. Genres" / "lalala").exists()
        assert (root / "5. Genres" / "Techno" / "r1").is_dir()
        assert not (root / "5. Genres" / "Techno" / "lalala").exists()
        assert (root / "5. Genres" / "Techno" / "r1" / "01. 01.m4a").is_file()
        assert not (root / "5. Genres" / "Techno" / "r1" / "lalala.m4a").exists()
        assert can_read(root / "5. Genres" / "Techno" / "r1" / "01. 01.m4a")

        assert (root / "6. Labels").is_dir()
        assert (root / "6. Labels" / "Silk Music").is_dir()
        assert not (root / "6. Labels" / "lalala").exists()
        assert (root / "6. Labels" / "Silk Music" / "r1").is_dir()
        assert not (root / "6. Labels" / "Silk Music" / "lalala").exists()
        assert (root / "6. Labels" / "Silk Music" / "r1" / "01. 01.m4a").is_file()
        assert not (root / "6. Labels" / "Silk Music" / "r1" / "lalala").exists()
        assert can_read(root / "6. Labels" / "Silk Music" / "r1" / "01. 01.m4a")

        assert (root / "7. Collages").is_dir()
        assert (root / "7. Collages" / "Rose Gold").is_dir()
        assert (root / "7. Collages" / "Ruby Red").is_dir()
        assert not (root / "7. Collages" / "lalala").exists()
        assert (root / "7. Collages" / "Rose Gold" / "1. r1").is_dir()
        assert not (root / "7. Collages" / "Rose Gold" / "lalala").exists()
        assert (root / "7. Collages" / "Rose Gold" / "1. r1" / "01. 01.m4a").is_file()
        assert not (root / "7. Collages" / "Rose Gold" / "1. r1" / "lalala").exists()
        assert can_read(root / "7. Collages" / "Rose Gold" / "1. r1" / "01. 01.m4a")

        assert (root / "8. Playlists").is_dir()
        assert (root / "8. Playlists" / "Lala Lisa").is_dir()
        assert (root / "8. Playlists" / "Turtle Rabbit").is_dir()
        assert not (root / "8. Playlists" / "lalala").exists()
        assert (root / "8. Playlists" / "Lala Lisa" / "1. 01.m4a").is_file()
        assert (root / "8. Playlists" / "Lala Lisa" / "cover.jpg").is_file()
        assert not (root / "8. Playlists" / "Lala Lisa" / "lalala").exists()
        assert can_read(root / "8. Playlists" / "Lala Lisa" / "1. 01.m4a")
        assert can_read(root / "8. Playlists" / "Lala Lisa" / "cover.jpg")


def test_virtual_filesystem_write_files(
    config: Config,
    source_dir: Path,  # noqa: ARG001
) -> None:
    """Assert that 1. we can write files and 2. cache updates in response."""
    root = config.fuse_mount_dir
    path = (
        root
        / "1. Releases"
        / "{NEW} BLACKPINK - 1990. I Love Blackpink [K-Pop;Pop]"
        / "01. BLACKPINK - Track 1.m4a"
    )
    with start_virtual_fs(config):
        # Write!
        af = AudioTags.from_file(path)
        assert af.title == "Track 1"
        af.title = "Hahahaha!!"
        af.flush()
        # Read! File should have been renamed post-cache update.
        assert not path.exists()
        path = path.with_name("01. BLACKPINK - Hahahaha!!.m4a")
        assert path.is_file()
        af = AudioTags.from_file(path)
        assert af.title == "Hahahaha!!"


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
        subprocess.run(
            [
                "cp",
                "-rp",
                str(root / "1. Releases" / "r1"),
                str(root / "7. Collages" / "New Jeans" / "r1"),
            ],
            check=True,
        )
        assert (root / "7. Collages" / "New Jeans" / "r1").is_dir()
        assert (root / "7. Collages" / "New Jeans" / "r1" / "01. 01.m4a").is_file()
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


@pytest.mark.usefixtures("seeded_cache")
def test_virtual_filesystem_add_collage_release_prefix_stripping(config: Config) -> None:
    """Test that we can add a release from the esoteric views to a collage, regardless of prefix."""
    root = config.fuse_mount_dir

    dirs = [
        root / "1. Releases" / "r1",
        root / "3. Releases - Recently Added" / "[0000-00-01] r1",
        root / "7. Collages" / "Rose Gold" / "1. r1",
    ]

    with start_virtual_fs(config):
        for d in dirs:
            shutil.copytree(d, root / "7. Collages" / "Ruby Red" / "r1")
            (root / "7. Collages" / "Ruby Red" / "r1").rmdir()


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
        # Use `cp -p` to test the ghost files behavior. A pure copy file will succeed, because it
        # stops after the release. However, cp -p also attempts to set some attributes on the moved
        # file, which fails if we immediately vanish the file post-release, which the naive
        # implementation does.
        subprocess.run(
            [
                "cp",
                "-p",
                str(release_dir / filename),
                str(root / "8. Playlists" / "New Jeans" / filename),
            ],
            check=True,
        )
        # Assert that we can see the attributes of the ghost file.
        assert (root / "8. Playlists" / "New Jeans" / filename).is_file()
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


def test_virtual_filesystem_set_release_cover_art(
    config: Config,
    source_dir: Path,  # noqa: ARG001
) -> None:
    root = config.fuse_mount_dir
    release_dir = root / "1. Releases" / "{NEW} BLACKPINK - 1990. I Love Blackpink [K-Pop;Pop]"
    with start_virtual_fs(config):
        assert not (release_dir / "cover.jpg").is_file()
        # First write.
        with (release_dir / "folder.jpg").open("w") as fp:
            fp.write("hi")
        assert (release_dir / "cover.jpg").is_file()
        with (release_dir / "cover.jpg").open("r") as fp:
            assert fp.read() == "hi"

        # Second write to same filename.
        with (release_dir / "cover.jpg").open("w") as fp:
            fp.write("hi")
        with (release_dir / "cover.jpg").open("r") as fp:
            assert fp.read() == "hi"

        # Third write to different filename.
        with (release_dir / "front.png").open("w") as fp:
            fp.write("hi")
        assert (release_dir / "cover.png").is_file()
        with (release_dir / "cover.png").open("r") as fp:
            assert fp.read() == "hi"
        # Because of ghost writes, getattr succeeds, so we shouldn't check exists().
        assert "cover.jpg" not in [f.name for f in release_dir.iterdir()]


def test_virtual_filesystem_set_playlist_cover_art(
    config: Config,
    source_dir: Path,  # noqa: ARG001
) -> None:
    root = config.fuse_mount_dir
    playlist_dir = root / "8. Playlists" / "Lala Lisa"
    with start_virtual_fs(config):
        assert (playlist_dir / "cover.jpg").is_file()
        # First write.
        with (playlist_dir / "folder.jpg").open("w") as fp:
            fp.write("hi")
        assert (playlist_dir / "cover.jpg").is_file()
        with (playlist_dir / "cover.jpg").open("r") as fp:
            assert fp.read() == "hi"

        # Second write to same filename.
        with (playlist_dir / "cover.jpg").open("w") as fp:
            fp.write("hi")
        with (playlist_dir / "cover.jpg").open("r") as fp:
            assert fp.read() == "hi"

        # Third write to different filename.
        with (playlist_dir / "front.png").open("w") as fp:
            fp.write("hi")
        assert (playlist_dir / "cover.png").is_file()
        with (playlist_dir / "cover.png").open("r") as fp:
            assert fp.read() == "hi"
        # Because of ghost writes, getattr succeeds, so we shouldn't check exists().
        assert "cover.jpg" not in [f.name for f in playlist_dir.iterdir()]


def test_virtual_filesystem_delete_release(config: Config, source_dir: Path) -> None:
    dirname = "NewJeans - 1990. I Love NewJeans [K-Pop;R&B]"
    root = config.fuse_mount_dir
    with start_virtual_fs(config):
        # Fix: If we return EACCES from unlink, then `rm -r` fails despite `rmdir` succeeding. Thus
        # we no-op if we cannot unlink a file. And we test the real tool we want to use in
        # production.
        subprocess.run(["rm", "-r", str(root / "1. Releases" / dirname)], check=True)
        assert not (root / "1. Releases" / dirname).exists()
        assert not (root / "1. Releases" / f"{{NEW}} {dirname}").is_dir()
        assert not (source_dir / "Test Release 3").exists()


def test_virtual_filesystem_read_from_deleted_file(config: Config, source_dir: Path) -> None:
    """
    Properly catch system errors that occur due an out of date cache. Though many can occur, we
    won't test for them all. However, we've wrapped all calls to RoseLogicalCore in OSError ->
    FUSEError translations.
    """
    source_path = source_dir / "Test Release 1" / "01.m4a"
    fuse_path = (
        config.fuse_mount_dir
        / "1. Releases"
        / "{NEW} BLACKPINK - 1990. I Love Blackpink [K-Pop;Pop]"
        / "01. BLACKPINK - Track 1.m4a"
    )

    source_path.unlink()
    with start_virtual_fs(config):
        with pytest.raises(OSError):  # noqa: PT011
            fuse_path.open("rb")
        # Assert that the virtual fs did not crash. It needs some time to propagate the crash.
        time.sleep(0.05)
        assert (config.fuse_mount_dir / "1. Releases").is_dir()


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
