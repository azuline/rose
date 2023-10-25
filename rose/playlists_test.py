from pathlib import Path
from typing import Any

import pytest
import tomllib

from rose.cache import connect
from rose.config import Config
from rose.playlists import (
    add_track_to_playlist,
    create_playlist,
    delete_playlist,
    delete_track_from_playlist,
    dump_playlists,
    edit_playlist_in_editor,
    rename_playlist,
)


def test_delete_track_from_playlist(config: Config, source_dir: Path) -> None:
    delete_track_from_playlist(config, "Lala Lisa", "iloveloona")

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
    delete_track_from_playlist(config, "You & Me", "ilovetwice")
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
    rename_playlist(config, "Lala Lisa", "Turtle Rabbit")
    assert not (source_dir / "!playlists" / "Lala Lisa.toml").exists()
    assert (source_dir / "!playlists" / "Turtle Rabbit.toml").exists()
    with connect(config) as conn:
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM playlists WHERE name = 'Turtle Rabbit')")
        assert cursor.fetchone()[0]
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM playlists WHERE name = 'Lala Lisa')")
        assert not cursor.fetchone()[0]


@pytest.mark.usefixtures("seeded_cache")
def test_dump_playlists(config: Config) -> None:
    out = dump_playlists(config)
    # fmt: off
    assert out == '{"Lala Lisa": [{"position": 0, "track": "01.m4a"}, {"position": 1, "track": "01.m4a"}], "Turtle Rabbit": []}' # noqa: E501
    # fmt: on


def test_edit_playlists_ordering(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    filepath = source_dir / "!playlists" / "Lala Lisa.toml"
    monkeypatch.setattr("rose.playlists.click.edit", lambda x: "\n".join(reversed(x.split("\n"))))
    edit_playlist_in_editor(config, "Lala Lisa")

    with filepath.open("rb") as fp:
        data = tomllib.load(fp)
    assert data["tracks"][0]["uuid"] == "ilovetwice"
    assert data["tracks"][1]["uuid"] == "iloveloona"


def test_edit_playlists_delete_track(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    filepath = source_dir / "!playlists" / "Lala Lisa.toml"
    monkeypatch.setattr("rose.playlists.click.edit", lambda x: x.split("\n")[0])
    edit_playlist_in_editor(config, "Lala Lisa")

    with filepath.open("rb") as fp:
        data = tomllib.load(fp)
    assert len(data["tracks"]) == 1
