"""
End-to-end tests over the CLI. Unlike the other tests in this package, these drive the top-level
`music` command group with a real configuration file and a real source directory, so they exercise
config parsing, cache invalidation, argument resolution, and the delegated library call together.

The tests are intentionally brief: each one walks through a workflow and spot-checks the result.
The exhaustive behavioral tests live next to the library functions in music-py.
"""

import json
import shutil
import subprocess
import sys
import time
from collections.abc import Callable
from pathlib import Path
from typing import Any

import pytest
from click.testing import CliRunner, Result

from conftest import TEST_COLLAGE_1, TEST_PLAYLIST_1, TEST_RELEASE_1, TEST_RELEASE_2, TEST_RELEASE_3, TESTDATA
from music_cli.cli import cli

Music = Callable[..., Result]


@pytest.fixture
def config_path(isolated_dir: Path) -> Path:
    source_dir = isolated_dir / "source"
    source_dir.mkdir()
    shutil.copytree(TEST_RELEASE_1, source_dir / TEST_RELEASE_1.name)
    shutil.copytree(TEST_RELEASE_2, source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_RELEASE_3, source_dir / TEST_RELEASE_3.name)
    (isolated_dir / "mount").mkdir()

    path = isolated_dir / "config.toml"
    with path.open("w") as fp:
        fp.write(
            f"""\
music_source_dir = "{source_dir}"
cache_dir = "{isolated_dir / "cache"}"
vfs.mount_dir = "{isolated_dir / "mount"}"
"""
        )
    return path


@pytest.fixture
def music(config_path: Path) -> Music:
    """Invoke the CLI as a user would: `music("releases", "print-all")`. Asserts success by default."""

    def invoke(*args: str, exit_code: int = 0) -> Result:
        res = CliRunner().invoke(cli, ["--config", str(config_path), *args])
        assert res.exit_code == exit_code, f"music {' '.join(args)} exited {res.exit_code}:\n{res.output}"
        return res

    invoke("cache", "update")
    return invoke


def _json(res: Result) -> Any:
    return json.loads(res.stdout)


def _release(music: Music, title: str) -> dict[str, Any]:
    [release] = [r for r in _json(music("releases", "print-all")) if r["releasetitle"] == title]
    assert isinstance(release, dict)
    return release


def test_entrypoint(config_path: Path) -> None:
    """Run the real `python -m music_cli` entrypoint in a subprocess."""
    cmd = [sys.executable, "-m", "music_cli", "--config", str(config_path)]

    res = subprocess.run([*cmd, "version"], capture_output=True, text=True)
    assert res.returncode == 0
    assert res.stdout.strip()

    # Expected errors are printed without a traceback and exit with status code 1.
    res = subprocess.run([*cmd, "releases", "print", "lalala"], capture_output=True, text=True)
    assert res.returncode == 1
    assert "InvalidReleaseArgError" in res.stdout
    assert "Traceback" not in res.stderr

    # A missing config file is also an expected error.
    res = subprocess.run([*cmd[:-1], "/nonexistent.toml", "version"], capture_output=True, text=True)
    assert res.returncode == 1
    assert "ConfigNotFoundError" in res.stdout


def test_releases(music: Music, isolated_dir: Path) -> None:
    releases = _json(music("releases", "print-all"))
    assert {r["releasetitle"] for r in releases} == {"I Love Blackpink", "I Love NewJeans", "I Love Carly"}
    assert len(_json(music("releases", "print-all", "releasetitle:NewJeans"))) == 1

    # Releases can be addressed by both UUID and source path.
    release = _release(music, "I Love Blackpink")
    rid, rpath = release["id"], release["source_path"]
    assert _json(music("releases", "print", rid)) == _json(music("releases", "print", rpath))

    assert release["new"]
    music("releases", "toggle-new", rpath)
    assert not _json(music("releases", "print", rid))["new"]

    assert not release["favorite"]
    music("releases", "toggle-favorite", rid)
    assert _json(music("releases", "print", rid))["favorite"]

    music("releases", "set-rating", rid, "80")
    assert _json(music("releases", "print", rid))["rating"] == 80
    music("releases", "set-rating", rid, "--clear")
    assert _json(music("releases", "print", rid))["rating"] is None
    music("releases", "set-rating", rid, exit_code=2)

    cover = isolated_dir / "art.jpg"
    cover.touch()
    music("releases", "set-cover-release", rid, str(cover))
    assert _json(music("releases", "print", rid))["cover_image_path"] == str(Path(rpath) / "cover.jpg")
    music("releases", "delete-cover-release", rid)
    assert _json(music("releases", "print", rid))["cover_image_path"] is None

    music("releases", "run-rule", "--dry-run", rid, "releasetitle/replace:Dry Run")
    assert _json(music("releases", "print", rid))["releasetitle"] == "I Love Blackpink"
    music("releases", "run-rule", "--yes", rid, "releasetitle/replace:I Love Rosé")
    release = _json(music("releases", "print", rid))
    assert release["releasetitle"] == "I Love Rosé"

    music("releases", "create-single", release["tracks"][0]["source_path"])
    assert len(_json(music("releases", "print-all", "releasetype:single"))) == 1

    music("releases", "delete", rid)
    assert not Path(rpath).exists()
    assert len(_json(music("releases", "print-all"))) == 3


def test_tracks(music: Music) -> None:
    tracks = _json(music("tracks", "print-all"))
    assert len(tracks) == 6
    assert len(_json(music("tracks", "print-all", "tracktitle:^Track 1$"))) == 3

    # Tracks can be addressed by both UUID and source path.
    track = tracks[0]
    tid, tpath = track["id"], track["source_path"]
    assert _json(music("tracks", "print", tid)) == _json(music("tracks", "print", tpath))

    music("tracks", "run-rule", "--yes", tpath, "tracktitle/replace:Whistle")
    assert _json(music("tracks", "print", tid))["tracktitle"] == "Whistle"


def test_collages(music: Music) -> None:
    release = _release(music, "I Love Blackpink")

    music("collages", "create", "Rose Gold")
    music("collages", "add-release", "Rose Gold", release["source_path"])
    collage = _json(music("collages", "print", "Rose Gold"))
    assert [r["id"] for r in collage["releases"]] == [release["id"]]

    music("collages", "rename", "Rose Gold", "Black Pink")
    assert [c["name"] for c in _json(music("collages", "print-all"))] == ["Black Pink"]

    music("collages", "remove-release", "Black Pink", release["id"])
    assert _json(music("collages", "print", "Black Pink"))["releases"] == []

    music("collages", "delete", "Black Pink")
    assert _json(music("collages", "print-all")) == []


def test_playlists(music: Music, isolated_dir: Path) -> None:
    track = _json(music("tracks", "print-all"))[0]

    music("playlists", "create", "Lala Lisa")
    music("playlists", "add-track", "Lala Lisa", track["source_path"])
    playlist = _json(music("playlists", "print", "Lala Lisa"))
    assert [t["id"] for t in playlist["tracks"]] == [track["id"]]

    cover = isolated_dir / "art.jpg"
    cover.touch()
    music("playlists", "set-cover", "Lala Lisa", str(cover))
    assert _json(music("playlists", "print", "Lala Lisa"))["cover_image_path"] is not None
    music("playlists", "delete-cover", "Lala Lisa")
    assert _json(music("playlists", "print", "Lala Lisa"))["cover_image_path"] is None

    music("playlists", "rename", "Lala Lisa", "Turtle Rabbit")
    assert [p["name"] for p in _json(music("playlists", "print-all"))] == ["Turtle Rabbit"]

    music("playlists", "remove-track", "Turtle Rabbit", track["id"])
    assert _json(music("playlists", "print", "Turtle Rabbit"))["tracks"] == []

    music("playlists", "delete", "Turtle Rabbit")
    assert _json(music("playlists", "print-all")) == []


@pytest.mark.parametrize("entity", ["artist", "genre", "label", "descriptor"])
def test_print_tag_entities(music: Music, entity: str) -> None:
    release = _release(music, "I Love Blackpink")
    music("releases", "run-rule", "--yes", release["id"], "label/add:YG", "descriptor/add:Loud")

    names = [x["name"] for x in _json(music(f"{entity}s", "print-all"))]
    assert names
    printed = _json(music(f"{entity}s", "print", names[0]))
    assert printed["name"] == names[0]
    assert printed["roles"]["main"] if entity == "artist" else printed["releases"]


def test_rules(music: Music, config_path: Path) -> None:
    music("rules", "run", "--dry-run", "releasetitle:Blackpink", "genre/add:Dry Run")
    assert "Dry Run" not in _release(music, "I Love Blackpink")["genres"]

    music("rules", "run", "--yes", "releasetitle:Blackpink", "genre/add:K-Pop Girl Group")
    assert "K-Pop Girl Group" in _release(music, "I Love Blackpink")["genres"]

    # Without --yes, the rule waits for confirmation. Decline it.
    res = CliRunner().invoke(
        cli,
        ["--config", str(config_path), "rules", "run", "releasetitle:Blackpink", "genre/add:Declined"],
        input="n\n",
    )
    assert res.exit_code == 0
    assert "Declined" not in _release(music, "I Love Blackpink")["genres"]

    # Stored rules are read from the configuration file.
    with config_path.open("a") as fp:
        fp.write('\n[[stored_metadata_rules]]\nmatcher = "releasetitle:NewJeans"\nactions = ["genre/add:Stored"]\n')
    music("rules", "run-stored", "--yes")
    assert "Stored" in _release(music, "I Love NewJeans")["genres"]
    assert "Stored" not in _release(music, "I Love Blackpink")["genres"]


def test_cache_update_picks_up_source_changes(music: Music, isolated_dir: Path) -> None:
    source_dir = isolated_dir / "source"
    shutil.copytree(TEST_COLLAGE_1, source_dir / "!collages", dirs_exist_ok=True)
    shutil.copytree(TEST_PLAYLIST_1, source_dir / "!playlists", dirs_exist_ok=True)
    shutil.copytree(TESTDATA / "Tagger", source_dir / "Tagger")
    assert len(_json(music("releases", "print-all"))) == 3

    music("cache", "update")
    assert len(_json(music("releases", "print-all"))) == 4
    assert [c["name"] for c in _json(music("collages", "print-all"))] == ["Rose Gold"]
    assert [p["name"] for p in _json(music("playlists", "print-all"))] == ["Lala Lisa"]

    music("cache", "update", "--force")
    assert len(_json(music("releases", "print-all"))) == 4


def test_config_preview_templates(music: Music) -> None:
    res = music("config", "preview-templates")
    assert "Source Directory - Release" in res.output


def test_fs_mount_unmount(config_path: Path, isolated_dir: Path) -> None:
    from music_vfs.virtualfs_test import _macfuse_available

    if sys.platform == "darwin" and not _macfuse_available():
        pytest.skip("macFUSE kernel extension is not loaded; cannot mount a FUSE filesystem")

    mount_dir = isolated_dir / "mount"
    cmd = [sys.executable, "-m", "music_cli", "--config", str(config_path)]
    # The mount command daemonizes, so the foreground process returns immediately.
    subprocess.run([*cmd, "fs", "mount"], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        # Mounting triggers a cache update in the background; wait for the releases to show up.
        start = time.time()
        while not (release_dirs := sorted((mount_dir / "1. Releases").glob("*I Love NewJeans*"))):
            assert time.time() - start < 10, "timed out waiting for vfs to mount and cache to update"
            time.sleep(0.05)
        # Releases can be addressed by their path in the virtual filesystem.
        res = subprocess.run([*cmd, "releases", "print", str(release_dirs[0])], capture_output=True, text=True)
        assert res.returncode == 0, res.stdout
        assert json.loads(res.stdout)["releasetitle"] == "I Love NewJeans"
    finally:
        subprocess.run([*cmd, "fs", "unmount"], check=True)
    start = time.time()
    while list(mount_dir.iterdir()):
        assert time.time() - start < 10, "timed out waiting for vfs to unmount"
        time.sleep(0.05)
