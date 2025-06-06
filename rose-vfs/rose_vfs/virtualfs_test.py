import dataclasses
import shutil
import subprocess
import time
from collections.abc import Iterator
from contextlib import contextmanager
from multiprocessing import Process
from pathlib import Path

import pytest
from rose import AudioTags, Config

from conftest import retry_for_sec
from rose_vfs.virtualfs import ALL_TRACKS, mount_virtualfs, unmount_virtualfs

R1_VNAME = "Techno Man & Bass Man - 2023. Release 1"
R2_VNAME = "Violin Woman (feat. Conductor Woman) - 2021. Release 2 [NEW]"
R3_VNAME = "Unknown Artists - 2021. Release 3"
R4_VNAME = "Unknown Artists - 2021. Release 4"


@contextmanager
def start_virtual_fs(c: Config) -> Iterator[None]:
    p = Process(target=mount_virtualfs, args=[c, True])
    try:
        p.start()
        # Takes >1 second to mount with MacFUSE, ~100ms on Linux.
        start = time.time()
        while not list(c.vfs.mount_dir.iterdir()):
            diff = time.time() - start
            assert diff < 2, "timed out waiting for vfs to mount"
            time.sleep(0.05)
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

    root = config.vfs.mount_dir
    with start_virtual_fs(config):
        # fmt: off
        assert not (root / "lalala").exists()

        assert (root / "1. Releases").is_dir()
        assert (root / "1. Releases" / R1_VNAME).is_dir()
        assert not (root / "1. Releases" / "lalala").exists()
        assert (root / "1. Releases" / R1_VNAME / "01. Track 1.m4a").is_file()
        assert not (root / "1. Releases" / R1_VNAME / "lala.m4a").exists()
        assert can_read(root / "1. Releases" / R1_VNAME / "01. Track 1.m4a")

        assert (root / "1. Releases" / R2_VNAME / "cover.jpg").is_file()
        assert can_read(root / "1. Releases" / R2_VNAME / "cover.jpg")
        assert not (root / "1. Releases" / R1_VNAME / "cover.jpg").exists()
        assert not (root / "1. Releases" / R2_VNAME / "cover.png").exists()

        assert (root / "1. Releases" / R2_VNAME / ".rose.r2.toml").is_file()
        assert can_read(root / "1. Releases" / R2_VNAME / ".rose.r2.toml")

        assert (root / "1. Releases - New").is_dir()
        assert (root / "1. Releases - New" / R2_VNAME).is_dir()
        assert not (root / "1. Releases - New" / R3_VNAME).exists()
        assert (root / "1. Releases - New" / R2_VNAME / "01. Track 1 (feat. Conductor Woman).m4a").is_file()
        assert not (root / "1. Releases - New" / R2_VNAME / "lalala").exists()

        assert (root / "1. Releases - Added On").is_dir()
        assert (root / "1. Releases - Added On" / f"[0000-01-01] {R2_VNAME}").exists()
        assert not (root / "1. Releases - Added On" / R2_VNAME).exists()
        assert (root / "1. Releases - Added On" / f"[0000-01-01] {R2_VNAME}" / "01. Track 1 (feat. Conductor Woman).m4a").is_file()
        assert not (root / "1. Releases - Added On" / R2_VNAME / "lalala").exists()

        assert (root / "1. Releases - Released On").is_dir()
        assert (root / "1. Releases - Released On" / f"[2019] {R2_VNAME}").exists()
        assert not (root / "1. Releases - Released On" / R2_VNAME).exists()
        assert (root / "1. Releases - Released On" / f"[2019] {R2_VNAME}" / "01. Track 1 (feat. Conductor Woman).m4a").is_file()
        assert not (root / "1. Releases - Released On" / R2_VNAME / "lalala").exists()

        assert (root / "2. Artists").is_dir()
        assert (root / "2. Artists" / "Bass Man").is_dir()
        assert not (root / "2. Artists" / "lalala").exists()
        assert (root / "2. Artists" / "Bass Man" / R1_VNAME).is_dir()
        assert not (root / "2. Artists" / "Bass Man" / "lalala").exists()
        assert (root / "2. Artists" / "Bass Man" / R1_VNAME / "01. Track 1.m4a").is_file()
        assert not (root / "2. Artists" / "Bass Man" / R1_VNAME / "lalala.m4a").exists()
        assert can_read(root / "2. Artists" / "Bass Man" / R1_VNAME / "01. Track 1.m4a")

        assert (root / "3. Genres").is_dir()
        assert (root / "3. Genres" / "Techno").is_dir()
        assert not (root / "3. Genres" / "lalala").exists()
        assert (root / "3. Genres" / "Techno" / R1_VNAME).is_dir()
        assert not (root / "3. Genres" / "Techno" / "lalala").exists()
        assert (root / "3. Genres" / "Techno" / R1_VNAME / "01. Track 1.m4a").is_file()
        assert not (root / "3. Genres" / "Techno" / R1_VNAME / "lalala.m4a").exists()
        assert can_read(root / "3. Genres" / "Techno" / R1_VNAME / "01. Track 1.m4a")

        assert (root / "4. Descriptors").is_dir()
        assert (root / "4. Descriptors" / "Warm").is_dir()
        assert not (root / "4. Descriptors" / "lalala").exists()
        assert (root / "4. Descriptors" / "Warm" / R1_VNAME).is_dir()
        assert not (root / "4. Descriptors" / "Warm" / "lalala").exists()
        assert (root / "4. Descriptors" / "Warm" / R1_VNAME / "01. Track 1.m4a").is_file()
        assert not (root / "4. Descriptors" / "Warm" / R1_VNAME / "lalala.m4a").exists()
        assert can_read(root / "4. Descriptors" / "Warm" / R1_VNAME / "01. Track 1.m4a")

        assert (root / "5. Labels").is_dir()
        assert (root / "5. Labels" / "Silk Music").is_dir()
        assert not (root / "5. Labels" / "lalala").exists()
        assert (root / "5. Labels" / "Silk Music" / R1_VNAME).is_dir()
        assert not (root / "5. Labels" / "Silk Music" / "lalala").exists()
        assert (root / "5. Labels" / "Silk Music" / R1_VNAME / "01. Track 1.m4a").is_file()
        assert not (root / "5. Labels" / "Silk Music" / R1_VNAME / "lalala").exists()
        assert can_read(root / "5. Labels" / "Silk Music" / R1_VNAME / "01. Track 1.m4a")

        assert (root / "6. Loose Tracks").is_dir()
        assert (root / "6. Loose Tracks" / f"{R4_VNAME}").exists()
        for vname in [R1_VNAME, R2_VNAME, R3_VNAME]:
            assert not (root / "6. Loose Tracks" / vname).exists()
        assert (root / "6. Loose Tracks" / f"{R4_VNAME}" / "01. Track 1.m4a").is_file()
        assert not (root / "6. Loose Tracks" / R4_VNAME / "lalala").exists()

        assert (root / "7. Collages").is_dir()
        assert (root / "7. Collages" / "Rose Gold").is_dir()
        assert (root / "7. Collages" / "Ruby Red").is_dir()
        assert not (root / "7. Collages" / "lalala").exists()
        assert (root / "7. Collages" / "Rose Gold" / f"1. {R1_VNAME}").is_dir()
        assert not (root / "7. Collages" / "Rose Gold" / "lalala").exists()
        assert (root / "7. Collages" / "Rose Gold" / f"1. {R1_VNAME}" / "01. Track 1.m4a").is_file()
        assert not (root / "7. Collages" / "Rose Gold" / f"1. {R1_VNAME}" / "lalala").exists()
        assert can_read(root / "7. Collages" / "Rose Gold" / f"1. {R1_VNAME}" / "01. Track 1.m4a")

        assert (root / "8. Playlists").is_dir()
        assert (root / "8. Playlists" / "Lala Lisa").is_dir()
        assert (root / "8. Playlists" / "Turtle Rabbit").is_dir()
        assert not (root / "8. Playlists" / "lalala").exists()
        assert (root / "8. Playlists" / "Lala Lisa" / "1. Techno Man & Bass Man - Track 1.m4a").is_file()
        assert (root / "8. Playlists" / "Lala Lisa" / "cover.jpg").is_file()
        assert not (root / "8. Playlists" / "Turtle Rabbit" / "1. Techno Man & Bass Man - Track 1.m4a").is_file()
        assert not (root / "8. Playlists" / "Lala Lisa" / "lalala").exists()
        assert can_read(root / "8. Playlists" / "Lala Lisa" / "1. Techno Man & Bass Man - Track 1.m4a")
        assert can_read(root / "8. Playlists" / "Lala Lisa" / "cover.jpg")
        # fmt: on


@pytest.mark.usefixtures("seeded_cache")
def test_virtual_filesystem_reads_all_tracks(config: Config) -> None:
    def can_read(p: Path) -> bool:
        with p.open("rb") as fp:
            return fp.read(256) != b"\\x00" * 256

    r1_track = "Techno Man & Bass Man - 2023. Release 1 - Track 1.m4a"
    r2_track = "Violin Woman (feat. Conductor Woman) - 2021. Release 2 - Track 1.m4a"
    r4_track = "Unknown Artists - 2021. Release 4 - Track 1.m4a"

    root = config.vfs.mount_dir
    with start_virtual_fs(config):
        # fmt: off

        assert (root / "1. Releases" / ALL_TRACKS).is_dir()
        assert (root / "1. Releases" / ALL_TRACKS / r1_track).is_file()
        assert (root / "1. Releases" / ALL_TRACKS / r4_track).is_file()
        assert can_read(root / "1. Releases" / ALL_TRACKS / r1_track)

        assert (root / "1. Releases - New" / ALL_TRACKS).is_dir()
        assert (root / "1. Releases - New" / ALL_TRACKS / r2_track).is_file()
        assert can_read(root / "1. Releases - New" / ALL_TRACKS / r2_track)

        assert (root / "1. Releases - Released On" / ALL_TRACKS).is_dir()
        assert (root / "1. Releases - Released On" / ALL_TRACKS / r1_track).is_file()
        assert can_read(root / "1. Releases - Released On" / ALL_TRACKS / r1_track)

        assert (root / "1. Releases - Added On" / ALL_TRACKS).is_dir()
        assert (root / "1. Releases - Added On" / ALL_TRACKS / r1_track).is_file()
        assert can_read(root / "1. Releases - Added On" / ALL_TRACKS / r1_track)

        assert (root / "2. Artists" / "Bass Man" / ALL_TRACKS).is_dir()
        assert (root / "2. Artists" / "Bass Man" / ALL_TRACKS / r1_track).is_file()
        assert can_read(root / "2. Artists" / "Bass Man" / ALL_TRACKS / r1_track)

        assert (root / "3. Genres" / "Techno" / ALL_TRACKS).is_dir()
        assert (root / "3. Genres" / "Techno" / ALL_TRACKS / r1_track).is_file()
        assert can_read(root / "3. Genres" / "Techno" / ALL_TRACKS / r1_track)

        assert (root / "4. Descriptors" / "Warm" / ALL_TRACKS).is_dir()
        assert (root / "4. Descriptors" / "Warm" / ALL_TRACKS / r1_track).is_file()
        assert can_read(root / "4. Descriptors" / "Warm" / ALL_TRACKS / r1_track)

        assert (root / "5. Labels" / "Silk Music" / ALL_TRACKS).is_dir()
        assert (root / "5. Labels" / "Silk Music" / ALL_TRACKS / r1_track).is_file()
        assert can_read(root / "5. Labels" / "Silk Music" / ALL_TRACKS / r1_track)

        assert (root / "6. Loose Tracks" / ALL_TRACKS).is_dir()
        assert (root / "6. Loose Tracks" / ALL_TRACKS / r4_track).is_file()
        assert not (root / "6. Loose Tracks" / ALL_TRACKS / r1_track).is_file()
        assert can_read(root / "6. Loose Tracks" / ALL_TRACKS / r4_track)

        assert (root / "7. Collages" / "Rose Gold" / ALL_TRACKS).is_dir()
        assert (root / "7. Collages" / "Rose Gold" / ALL_TRACKS / r1_track).is_file()
        assert can_read(root / "7. Collages" / "Rose Gold" / ALL_TRACKS / r1_track)
        # fmt: on


def test_virtual_filesystem_write_files(
    config: Config,
    source_dir: Path,  # noqa: ARG001
) -> None:
    """Assert that 1. we can write files and 2. cache updates in response."""
    root = config.vfs.mount_dir
    path = root / "1. Releases" / "BLACKPINK - 1990. I Love Blackpink [NEW]" / "01. Track 1.m4a"
    with start_virtual_fs(config):
        # Write!
        af = AudioTags.from_file(path)
        assert af.tracktitle == "Track 1"
        af.tracktitle = "Hahahaha!!"
        af.flush(config)
        # Read! File should have been renamed post-cache update. exists() for the old file will
        # resolve because of the "legacy file resolution" grace period, but the old file should no
        # longer appear in readdir.
        assert path not in set(path.parent.iterdir())
        path = path.with_name("01. Hahahaha!!.m4a")
        assert path.is_file()
        af = AudioTags.from_file(path)
        assert af.tracktitle == "Hahahaha!!"


@pytest.mark.usefixtures("seeded_cache")
def test_virtual_filesystem_collage_actions(config: Config) -> None:
    root = config.vfs.mount_dir
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
        collage_dir = root / "7. Collages" / "New Jeans"
        subprocess.run(
            [
                "cp",
                "-rp",
                str(root / "1. Releases" / R1_VNAME),
                str(collage_dir / f"1. {R1_VNAME}"),
            ],
            check=True,
        )
        assert (collage_dir / f"1. {R1_VNAME}").is_dir()
        assert (collage_dir / f"1. {R1_VNAME}" / "01. Track 1.m4a").is_file()
        with (src / "!collages" / "New Jeans.toml").open("r") as fp:
            assert "r1" in fp.read()
        # Delete release from collage.
        (collage_dir / f"1. {R1_VNAME}").rmdir()
        assert (collage_dir / f"1. {R1_VNAME}").exists()
        with (src / "!collages" / "New Jeans.toml").open("r") as fp:
            assert "r1" not in fp.read()
        # Delete collage.
        collage_dir.rmdir()
        assert not (src / "!collages" / "New Jeans.toml").exists()


@pytest.mark.usefixtures("seeded_cache")
def test_virtual_filesystem_add_collage_release_with_any_dirname(config: Config) -> None:
    """Test that we can add a release from the esoteric views to a collage, regardless of directory name."""
    root = config.vfs.mount_dir

    with start_virtual_fs(config):
        shutil.copytree(
            root / "1. Releases" / R1_VNAME,
            root / "7. Collages" / "Ruby Red" / "LALA HAHA",
        )
        # fmt: off
        assert (root / "7. Collages" / "Ruby Red" / f"1. {R1_VNAME}").is_dir()
        assert (root / "7. Collages" / "Ruby Red" / f"1. {R1_VNAME}" / ".rose.r1.toml").is_file()
        # fmt: on


def test_virtual_filesystem_playlist_actions(
    config: Config,
    source_dir: Path,  # noqa: ARG001
) -> None:
    root = config.vfs.mount_dir
    src = config.music_source_dir

    release_dir = root / "1. Releases" / "BLACKPINK - 1990. I Love Blackpink [NEW]"
    filename = "01. Track 1.m4a"

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
            assert "BLACKPINK - Track 1" in fp.read()
        # Delete track from playlist.
        (root / "8. Playlists" / "New Jeans" / "1. BLACKPINK - Track 1.m4a").unlink()
        assert not (root / "8. Playlists" / "New Jeans" / "1. BLACKPINK - Track 1.m4a").exists()
        with (src / "!playlists" / "New Jeans.toml").open("r") as fp:
            assert "BLACKPINK - Track 1" not in fp.read()
        # Delete playlist.
        (root / "8. Playlists" / "New Jeans").rmdir()
        assert not (src / "!playlists" / "New Jeans.toml").exists()


def test_virtual_filesystem_release_cover_art_actions(
    config: Config,
    source_dir: Path,  # noqa: ARG001
) -> None:
    root = config.vfs.mount_dir
    release_dir = root / "1. Releases" / "BLACKPINK - 1990. I Love Blackpink [NEW]"
    with start_virtual_fs(config):
        assert not (release_dir / "cover.jpg").is_file()
        # First write.
        with (release_dir / "folder.jpg").open("w") as fp:
            fp.write("hi")
        for _ in retry_for_sec(0.2):
            if not (release_dir / "cover.jpg").is_file():
                continue
            with (release_dir / "cover.jpg").open("r") as fp:
                if fp.read() != "hi":
                    continue
            break

        # Second write to same filename.
        with (release_dir / "cover.jpg").open("w") as fp:
            fp.write("hi")
        for _ in retry_for_sec(0.2):
            with (release_dir / "cover.jpg").open("r") as fp:
                if fp.read() == "hi":
                    break

        # Third write to different filename.
        with (release_dir / "front.png").open("w") as fp:
            fp.write("hi")
        for _ in retry_for_sec(0.2):
            if not (release_dir / "cover.png").is_file():
                continue
            with (release_dir / "cover.png").open("r") as fp:
                if fp.read() != "hi":
                    continue
            # Because of ghost writes, getattr succeeds, so we shouldn't check exists().
            if "cover.jpg" not in [f.name for f in release_dir.iterdir()]:
                continue
            break


def test_virtual_filesystem_playlist_cover_art_actions(
    config: Config,
    source_dir: Path,  # noqa: ARG001
) -> None:
    root = config.vfs.mount_dir
    playlist_dir = root / "8. Playlists" / "Lala Lisa"
    with start_virtual_fs(config):
        assert (playlist_dir / "cover.jpg").is_file()
        # First write.
        with (playlist_dir / "folder.jpg").open("w") as fp:
            fp.write("hi")
        for _ in retry_for_sec(0.2):
            if not (playlist_dir / "cover.jpg").is_file():
                continue
            with (playlist_dir / "cover.jpg").open("r") as fp:
                if fp.read() != "hi":
                    continue
            break

        # Second write to same filename.
        with (playlist_dir / "cover.jpg").open("w") as fp:
            fp.write("bye")
        for _ in retry_for_sec(0.2):
            with (playlist_dir / "cover.jpg").open("r") as fp:
                if fp.read() == "bye":
                    break

        # Third write to different filename.
        with (playlist_dir / "front.png").open("w") as fp:
            fp.write("hi")
        for _ in retry_for_sec(0.2):
            if not (playlist_dir / "cover.png").is_file():
                continue
            with (playlist_dir / "cover.png").open("r") as fp:
                if fp.read() != "hi":
                    continue
            # Because of ghost writes, getattr succeeds, so we shouldn't check exists().
            if not "cover.jpg" not in [f.name for f in playlist_dir.iterdir()]:
                continue
            break

        # Now delete the cover art.
        (playlist_dir / "cover.png").unlink()
        for _ in retry_for_sec(0.2):
            if not (playlist_dir / "cover.png").exists():
                break


def test_virtual_filesystem_delete_release(config: Config, source_dir: Path) -> None:
    dirname = "NewJeans - 1990. I Love NewJeans"
    root = config.vfs.mount_dir
    with start_virtual_fs(config):
        # Fix: If we return EACCES from unlink, then `rm -r` fails despite `rmdir` succeeding. Thus
        # we no-op if we cannot unlink a file. And we test the real tool we want to use in
        # production.
        subprocess.run(["rm", "-r", str(root / "1. Releases" / dirname)], check=True)
        assert not (root / "1. Releases" / dirname).exists()
        assert not (root / "1. Releases" / f"{dirname} [NEW]").is_dir()
        assert not (source_dir / "Test Release 3").exists()


def test_virtual_filesystem_read_from_deleted_file(config: Config, source_dir: Path) -> None:
    """
    Properly catch system errors that occur due an out of date cache. Though many can occur, we
    won't test for them all. However, we've wrapped all calls to RoseLogicalCore in OSError ->
    FUSEError translations.
    """
    source_path = source_dir / "Test Release 1" / "01.m4a"
    fuse_path = config.vfs.mount_dir / "1. Releases" / "BLACKPINK - 1990. I Love Blackpink [NEW]" / "01. Track 1.m4a"

    source_path.unlink()
    with start_virtual_fs(config):
        with pytest.raises(OSError):  # noqa: PT011
            fuse_path.open("rb")
        # Assert that the virtual fs did not crash. It needs some time to propagate the crash.
        time.sleep(0.05)
        assert (config.vfs.mount_dir / "1. Releases").is_dir()


@pytest.mark.usefixtures("seeded_cache")
def test_virtual_filesystem_blacklist(config: Config) -> None:
    new_config = dataclasses.replace(
        config,
        vfs=dataclasses.replace(
            config.vfs,
            artists_blacklist=["Bass Man"],
            genres_blacklist=["Techno"],
            descriptors_blacklist=["Warm"],
            labels_blacklist=["Silk Music"],
        ),
    )
    root = config.vfs.mount_dir
    with start_virtual_fs(new_config):
        assert (root / "2. Artists" / "Techno Man").is_dir()
        assert (root / "3. Genres" / "Deep House").is_dir()
        assert (root / "4. Descriptors" / "Hot").exists()
        assert (root / "5. Labels" / "Native State").is_dir()
        assert not (root / "2. Artists" / "Bass Man").exists()
        assert not (root / "3. Genres" / "Techno").exists()
        assert not (root / "4. Descriptors" / "Warm").is_dir()
        assert not (root / "5. Labels" / "Silk Music").exists()


@pytest.mark.usefixtures("seeded_cache")
def test_virtual_filesystem_whitelist(config: Config) -> None:
    new_config = dataclasses.replace(
        config,
        vfs=dataclasses.replace(
            config.vfs,
            artists_whitelist=["Bass Man"],
            genres_whitelist=["Techno"],
            descriptors_whitelist=["Warm"],
            labels_whitelist=["Silk Music"],
        ),
    )
    root = config.vfs.mount_dir
    with start_virtual_fs(new_config):
        assert not (root / "2. Artists" / "Techno Man").exists()
        assert not (root / "3. Genres" / "Deep House").exists()
        assert not (root / "4. Descriptors" / "Hot").exists()
        assert not (root / "5. Labels" / "Native State").exists()
        assert (root / "2. Artists" / "Bass Man").is_dir()
        assert (root / "3. Genres" / "Techno").is_dir()
        assert (root / "4. Descriptors" / "Warm").is_dir()
        assert (root / "5. Labels" / "Silk Music").is_dir()


@pytest.mark.usefixtures("seeded_cache")
def test_virtual_filesystem_hide_new_release_classifiers(config: Config) -> None:
    new_config = dataclasses.replace(
        config,
        vfs=dataclasses.replace(
            config.vfs,
            hide_genres_with_only_new_releases=True,
            hide_descriptors_with_only_new_releases=True,
            hide_labels_with_only_new_releases=True,
        ),
    )
    root = config.vfs.mount_dir
    with start_virtual_fs(new_config):
        assert not (root / "3. Genres" / "Modern Classical").exists()
        assert not (root / "4. Descriptors" / "Wet").exists()
        assert not (root / "5. Labels" / "Native State").exists()
        assert (root / "3. Genres" / "Deep House").is_dir()
        assert (root / "3. Genres" / "House").is_dir()
        assert (root / "4. Descriptors" / "Warm").is_dir()
        assert (root / "5. Labels" / "Silk Music").is_dir()
