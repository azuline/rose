"""
End-to-end tests over the CLI. Unlike the other tests in this package, these drive the top-level
`rose` command group with a real configuration file and a real source directory, so they exercise
config parsing, cache invalidation, argument resolution, and the delegated library call together.

The tests are intentionally brief: each one walks through a workflow and spot-checks the result.
The exhaustive behavioral tests live next to the library functions in rose-py.
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
from rose_cli.cli import cli

Rose = Callable[..., Result]


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
def rose(config_path: Path) -> Rose:
    """Invoke the CLI as a user would: `rose("releases", "print-all")`. Asserts success by default."""

    def invoke(*args: str, exit_code: int = 0) -> Result:
        res = CliRunner().invoke(cli, ["--config", str(config_path), *args])
        assert res.exit_code == exit_code, f"rose {' '.join(args)} exited {res.exit_code}:\n{res.output}"
        return res

    invoke("cache", "update")
    return invoke


def _json(res: Result) -> Any:
    return json.loads(res.stdout)


def _release(rose: Rose, title: str) -> dict[str, Any]:
    [release] = [r for r in _json(rose("releases", "print-all")) if r["releasetitle"] == title]
    assert isinstance(release, dict)
    return release


def test_entrypoint(config_path: Path) -> None:
    """Run the real `python -m rose_cli` entrypoint in a subprocess."""
    cmd = [sys.executable, "-m", "rose_cli", "--config", str(config_path)]

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


def test_releases(rose: Rose, isolated_dir: Path) -> None:
    releases = _json(rose("releases", "print-all"))
    assert {r["releasetitle"] for r in releases} == {"I Love Blackpink", "I Love NewJeans", "I Love Carly"}
    assert len(_json(rose("releases", "print-all", "releasetitle:NewJeans"))) == 1

    # Releases can be addressed by both UUID and source path.
    release = _release(rose, "I Love Blackpink")
    rid, rpath = release["id"], release["source_path"]
    assert _json(rose("releases", "print", rid)) == _json(rose("releases", "print", rpath))

    assert release["new"]
    rose("releases", "toggle-new", rpath)
    assert not _json(rose("releases", "print", rid))["new"]

    assert not release["favorite"]
    rose("releases", "toggle-favorite", rid)
    assert _json(rose("releases", "print", rid))["favorite"]

    rose("releases", "set-rating", rid, "80")
    assert _json(rose("releases", "print", rid))["rating"] == 80
    rose("releases", "set-rating", rid, "--clear")
    assert _json(rose("releases", "print", rid))["rating"] is None
    rose("releases", "set-rating", rid, exit_code=2)

    cover = isolated_dir / "art.jpg"
    cover.touch()
    rose("releases", "set-cover-release", rid, str(cover))
    assert _json(rose("releases", "print", rid))["cover_image_path"] == str(Path(rpath) / "cover.jpg")
    rose("releases", "delete-cover-release", rid)
    assert _json(rose("releases", "print", rid))["cover_image_path"] is None

    rose("releases", "run-rule", "--dry-run", rid, "releasetitle/replace:Dry Run")
    assert _json(rose("releases", "print", rid))["releasetitle"] == "I Love Blackpink"
    rose("releases", "run-rule", "--yes", rid, "releasetitle/replace:I Love Rosé")
    release = _json(rose("releases", "print", rid))
    assert release["releasetitle"] == "I Love Rosé"

    rose("releases", "create-single", release["tracks"][0]["source_path"])
    assert len(_json(rose("releases", "print-all", "releasetype:single"))) == 1

    # Renames happen only on an explicit reflush.
    assert Path(rpath).name == TEST_RELEASE_1.name
    rose("releases", "reflush", "--rename", rid)
    rpath = _json(rose("releases", "print", rid))["source_path"]
    assert Path(rpath).name == "BLACKPINK - 1990. I Love Rose [FAVORITE]"
    assert Path(rpath).is_dir()

    rose("releases", "delete", rid)
    assert not Path(rpath).exists()
    assert len(_json(rose("releases", "print-all"))) == 3


def test_tracks(rose: Rose) -> None:
    tracks = _json(rose("tracks", "print-all"))
    assert len(tracks) == 6
    assert len(_json(rose("tracks", "print-all", "tracktitle:^Track 1$"))) == 3

    # Tracks can be addressed by both UUID and source path.
    track = tracks[0]
    tid, tpath = track["id"], track["source_path"]
    assert _json(rose("tracks", "print", tid)) == _json(rose("tracks", "print", tpath))

    rose("tracks", "run-rule", "--yes", tpath, "tracktitle/replace:Whistle")
    assert _json(rose("tracks", "print", tid))["tracktitle"] == "Whistle"


def test_collages(rose: Rose) -> None:
    release = _release(rose, "I Love Blackpink")

    rose("collages", "create", "Rose Gold")
    rose("collages", "add-release", "Rose Gold", release["source_path"])
    collage = _json(rose("collages", "print", "Rose Gold"))
    assert [r["id"] for r in collage["releases"]] == [release["id"]]

    rose("collages", "rename", "Rose Gold", "Black Pink")
    assert [c["name"] for c in _json(rose("collages", "print-all"))] == ["Black Pink"]

    rose("collages", "remove-release", "Black Pink", release["id"])
    assert _json(rose("collages", "print", "Black Pink"))["releases"] == []

    rose("collages", "delete", "Black Pink")
    assert _json(rose("collages", "print-all")) == []


def test_playlists(rose: Rose, isolated_dir: Path) -> None:
    track = _json(rose("tracks", "print-all"))[0]

    rose("playlists", "create", "Lala Lisa")
    rose("playlists", "add-track", "Lala Lisa", track["source_path"])
    playlist = _json(rose("playlists", "print", "Lala Lisa"))
    assert [t["id"] for t in playlist["tracks"]] == [track["id"]]

    cover = isolated_dir / "art.jpg"
    cover.touch()
    rose("playlists", "set-cover", "Lala Lisa", str(cover))
    assert _json(rose("playlists", "print", "Lala Lisa"))["cover_image_path"] is not None
    rose("playlists", "delete-cover", "Lala Lisa")
    assert _json(rose("playlists", "print", "Lala Lisa"))["cover_image_path"] is None

    rose("playlists", "rename", "Lala Lisa", "Turtle Rabbit")
    assert [p["name"] for p in _json(rose("playlists", "print-all"))] == ["Turtle Rabbit"]

    rose("playlists", "remove-track", "Turtle Rabbit", track["id"])
    assert _json(rose("playlists", "print", "Turtle Rabbit"))["tracks"] == []

    rose("playlists", "delete", "Turtle Rabbit")
    assert _json(rose("playlists", "print-all")) == []


@pytest.mark.parametrize("entity", ["artist", "genre", "label", "descriptor"])
def test_print_tag_entities(rose: Rose, entity: str) -> None:
    release = _release(rose, "I Love Blackpink")
    rose("releases", "run-rule", "--yes", release["id"], "label/add:YG", "descriptor/add:Loud")

    names = [x["name"] for x in _json(rose(f"{entity}s", "print-all"))]
    assert names
    printed = _json(rose(f"{entity}s", "print", names[0]))
    assert printed["name"] == names[0]
    assert printed["roles"]["main"] if entity == "artist" else printed["releases"]


def test_rules(rose: Rose, config_path: Path) -> None:
    rose("rules", "run", "--dry-run", "releasetitle:Blackpink", "genre/add:Dry Run")
    assert "Dry Run" not in _release(rose, "I Love Blackpink")["genres"]

    rose("rules", "run", "--yes", "releasetitle:Blackpink", "genre/add:K-Pop Girl Group")
    assert "K-Pop Girl Group" in _release(rose, "I Love Blackpink")["genres"]

    # Without --yes, the rule waits for confirmation. Decline it.
    res = CliRunner().invoke(
        cli,
        ["--config", str(config_path), "rules", "run", "releasetitle:Blackpink", "genre/add:Declined"],
        input="n\n",
    )
    assert res.exit_code == 0
    assert "Declined" not in _release(rose, "I Love Blackpink")["genres"]

    # Stored rules are read from the configuration file.
    with config_path.open("a") as fp:
        fp.write('\n[[stored_metadata_rules]]\nmatcher = "releasetitle:NewJeans"\nactions = ["genre/add:Stored"]\n')
    rose("rules", "run-stored", "--yes")
    assert "Stored" in _release(rose, "I Love NewJeans")["genres"]
    assert "Stored" not in _release(rose, "I Love Blackpink")["genres"]


def test_cache_update_picks_up_source_changes(rose: Rose, isolated_dir: Path) -> None:
    source_dir = isolated_dir / "source"
    shutil.copytree(TEST_COLLAGE_1, source_dir / "!collages", dirs_exist_ok=True)
    shutil.copytree(TEST_PLAYLIST_1, source_dir / "!playlists", dirs_exist_ok=True)
    shutil.copytree(TESTDATA / "Tagger", source_dir / "Tagger")
    assert len(_json(rose("releases", "print-all"))) == 3

    rose("cache", "update")
    assert len(_json(rose("releases", "print-all"))) == 4
    assert [c["name"] for c in _json(rose("collages", "print-all"))] == ["Rose Gold"]
    assert [p["name"] for p in _json(rose("playlists", "print-all"))] == ["Lala Lisa"]

    rose("cache", "update", "--force")
    assert len(_json(rose("releases", "print-all"))) == 4


def test_config_preview_templates(rose: Rose) -> None:
    res = rose("config", "preview-templates")
    assert "Source Directory - Release" in res.output


def test_fs_mount_unmount(config_path: Path, isolated_dir: Path) -> None:
    from rose_vfs.virtualfs_test import _macfuse_available

    if sys.platform == "darwin" and not _macfuse_available():
        pytest.skip("macFUSE kernel extension is not loaded; cannot mount a FUSE filesystem")

    mount_dir = isolated_dir / "mount"
    cmd = [sys.executable, "-m", "rose_cli", "--config", str(config_path)]
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
