import shutil
from pathlib import Path
from typing import Any

import pytest
import tomllib

from conftest import TEST_RELEASE_1
from rose.cache import connect, update_cache
from rose.config import Config
from rose.playlists import (
    add_track_to_playlist,
    create_playlist,
    delete_playlist,
    dump_playlists,
    edit_playlist_in_editor,
    remove_track_from_playlist,
    rename_playlist,
)


def test_remove_track_from_playlist(config: Config, source_dir: Path) -> None:
    remove_track_from_playlist(config, "Lala Lisa", "iloveloona")

    # Assert file is updated.
    with (source_dir / "!playlists" / "Lala Lisa.toml").open("rb") as fp:
        diskdata = tomllib.load(fp)
    assert len(diskdata["tracks"]) == 1
    assert diskdata["tracks"][0]["uuid"] == "ilovetwice"

    # Assert cache is updated.
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT track_id FROM playlists_tracks WHERE playlist_name = 'Lala Lisa'"
        )
        ids = [r["track_id"] for r in cursor]
        assert ids == ["ilovetwice"]


def test_playlist_lifecycle(config: Config, source_dir: Path) -> None:
    filepath = source_dir / "!playlists" / "You & Me.toml"

    # Create playlist.
    assert not filepath.exists()
    create_playlist(config, "You & Me")
    assert filepath.is_file()
    with connect(config) as conn:
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM playlists WHERE name = 'You & Me')")
        assert cursor.fetchone()[0]

    # Add one track.
    add_track_to_playlist(config, "You & Me", "iloveloona")
    with filepath.open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["tracks"]} == {"iloveloona"}
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'"
        )
        assert {r["track_id"] for r in cursor} == {"iloveloona"}

    # Add another track.
    add_track_to_playlist(config, "You & Me", "ilovetwice")
    with (source_dir / "!playlists" / "You & Me.toml").open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["tracks"]} == {"iloveloona", "ilovetwice"}
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'"
        )
        assert {r["track_id"] for r in cursor} == {"iloveloona", "ilovetwice"}

    # Delete one track.
    remove_track_from_playlist(config, "You & Me", "ilovetwice")
    with filepath.open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["tracks"]} == {"iloveloona"}
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'"
        )
        assert {r["track_id"] for r in cursor} == {"iloveloona"}

    # And delete the playlist.
    delete_playlist(config, "You & Me")
    assert not filepath.is_file()
    with connect(config) as conn:
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM playlists WHERE name = 'You & Me')")
        assert not cursor.fetchone()[0]


def test_playlist_add_duplicate(config: Config, source_dir: Path) -> None:
    create_playlist(config, "You & Me")
    add_track_to_playlist(config, "You & Me", "ilovetwice")
    add_track_to_playlist(config, "You & Me", "ilovetwice")
    with (source_dir / "!playlists" / "You & Me.toml").open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert len(diskdata["tracks"]) == 1
    with connect(config) as conn:
        cursor = conn.execute("SELECT * FROM playlists_tracks WHERE playlist_name = 'You & Me'")
        assert len(cursor.fetchall()) == 1


def test_rename_playlist(config: Config, source_dir: Path) -> None:
    # And check that auxiliary files were renamed. Create an aux cover art here.
    (source_dir / "!playlists" / "Lala Lisa.jpg").touch()

    rename_playlist(config, "Lala Lisa", "Turtle Rabbit")
    assert not (source_dir / "!playlists" / "Lala Lisa.toml").exists()
    assert not (source_dir / "!playlists" / "Lala Lisa.jpg").exists()
    assert (source_dir / "!playlists" / "Turtle Rabbit.toml").exists()
    assert (source_dir / "!playlists" / "Turtle Rabbit.jpg").exists()

    with connect(config) as conn:
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM playlists WHERE name = 'Turtle Rabbit')")
        assert cursor.fetchone()[0]
        cursor = conn.execute("SELECT cover_path FROM playlists WHERE name = 'Turtle Rabbit'")
        assert Path(cursor.fetchone()[0]) == source_dir / "!playlists" / "Turtle Rabbit.jpg"
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM playlists WHERE name = 'Lala Lisa')")
        assert not cursor.fetchone()[0]


@pytest.mark.usefixtures("seeded_cache")
def test_dump_playlists(config: Config) -> None:
    out = dump_playlists(config)
    # fmt: off
    assert out == '{"Lala Lisa": [{"position": 1, "track_id": "t1", "track_filename": "01.m4a"}, {"position": 2, "track_id": "t3", "track_filename": "01.m4a"}], "Turtle Rabbit": []}' # noqa: E501
    # fmt: on


def test_edit_playlists_ordering(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    filepath = source_dir / "!playlists" / "Lala Lisa.toml"
    monkeypatch.setattr("rose.playlists.click.edit", lambda x: "\n".join(reversed(x.split("\n"))))
    edit_playlist_in_editor(config, "Lala Lisa")

    with filepath.open("rb") as fp:
        data = tomllib.load(fp)
    assert data["tracks"][0]["uuid"] == "ilovetwice"
    assert data["tracks"][1]["uuid"] == "iloveloona"


def test_edit_playlists_remove_track(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    filepath = source_dir / "!playlists" / "Lala Lisa.toml"
    monkeypatch.setattr("rose.playlists.click.edit", lambda x: x.split("\n")[0])
    edit_playlist_in_editor(config, "Lala Lisa")

    with filepath.open("rb") as fp:
        data = tomllib.load(fp)
    assert len(data["tracks"]) == 1


def test_edit_playlists_duplicate_track_name(monkeypatch: Any, config: Config) -> None:
    """
    When there are duplicate virtual filenames, we append UUID. Check that it works by asserting on
    the seen text and checking that reversing the order works.
    """
    # Generate conflicting virtual tracknames by having two copies of a release in the library.
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / "a")
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / "b")
    update_cache(config)

    with connect(config) as conn:
        # Get the first track of each release.
        cursor = conn.execute("SELECT id FROM tracks WHERE source_path LIKE '%01.m4a'")
        track_ids = [r["id"] for r in cursor]
        assert len(track_ids) == 2

    create_playlist(config, "You & Me")
    for tid in track_ids:
        add_track_to_playlist(config, "You & Me", tid)

    seen = ""

    def editfn(x: str) -> str:
        nonlocal seen
        seen = x
        return "\n".join(reversed(x.split("\n")))

    monkeypatch.setattr("rose.playlists.click.edit", editfn)
    edit_playlist_in_editor(config, "You & Me")

    assert seen == "\n".join([f"BLACKPINK - Track 1.m4a [{tid}]" for tid in track_ids])

    with (config.music_source_dir / "!playlists" / "You & Me.toml").open("rb") as fp:
        data = tomllib.load(fp)
    assert data["tracks"][0]["uuid"] == track_ids[1]
    assert data["tracks"][1]["uuid"] == track_ids[0]
