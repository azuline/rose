import json
from pathlib import Path
from typing import Any

import pytest
import tomllib

from rose.cache import connect, update_cache
from rose.collages import (
    add_release_to_collage,
    create_collage,
    delete_collage,
    dump_collage,
    dump_collages,
    edit_collage_in_editor,
    remove_release_from_collage,
    rename_collage,
)
from rose.config import Config


def test_remove_release_from_collage(config: Config, source_dir: Path) -> None:
    remove_release_from_collage(config, "Rose Gold", "ilovecarly")

    # Assert file is updated.
    with (source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
        diskdata = tomllib.load(fp)
    assert len(diskdata["releases"]) == 1
    assert diskdata["releases"][0]["uuid"] == "ilovenewjeans"

    # Assert cache is updated.
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT release_id FROM collages_releases WHERE collage_name = 'Rose Gold'"
        )
        ids = [r["release_id"] for r in cursor]
        assert ids == ["ilovenewjeans"]


def test_collage_lifecycle(config: Config, source_dir: Path) -> None:
    filepath = source_dir / "!collages" / "All Eyes.toml"

    # Create collage.
    assert not filepath.exists()
    create_collage(config, "All Eyes")
    assert filepath.is_file()
    with connect(config) as conn:
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM collages WHERE name = 'All Eyes')")
        assert cursor.fetchone()[0]

    # Add one release.
    add_release_to_collage(config, "All Eyes", "ilovecarly")
    with filepath.open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["releases"]} == {"ilovecarly"}
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT release_id FROM collages_releases WHERE collage_name = 'All Eyes'"
        )
        assert {r["release_id"] for r in cursor} == {"ilovecarly"}

    # Add another release.
    add_release_to_collage(config, "All Eyes", "ilovenewjeans")
    with (source_dir / "!collages" / "All Eyes.toml").open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["releases"]} == {"ilovecarly", "ilovenewjeans"}
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT release_id FROM collages_releases WHERE collage_name = 'All Eyes'"
        )
        assert {r["release_id"] for r in cursor} == {"ilovecarly", "ilovenewjeans"}

    # Delete one release.
    remove_release_from_collage(config, "All Eyes", "ilovenewjeans")
    with filepath.open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["releases"]} == {"ilovecarly"}
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT release_id FROM collages_releases WHERE collage_name = 'All Eyes'"
        )
        assert {r["release_id"] for r in cursor} == {"ilovecarly"}

    # And delete the collage.
    delete_collage(config, "All Eyes")
    assert not filepath.is_file()
    with connect(config) as conn:
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM collages WHERE name = 'All Eyes')")
        assert not cursor.fetchone()[0]


def test_collage_add_duplicate(config: Config, source_dir: Path) -> None:
    create_collage(config, "All Eyes")
    add_release_to_collage(config, "All Eyes", "ilovenewjeans")
    add_release_to_collage(config, "All Eyes", "ilovenewjeans")
    with (source_dir / "!collages" / "All Eyes.toml").open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert len(diskdata["releases"]) == 1
    with connect(config) as conn:
        cursor = conn.execute("SELECT * FROM collages_releases WHERE collage_name = 'All Eyes'")
        assert len(cursor.fetchall()) == 1


def test_rename_collage(config: Config, source_dir: Path) -> None:
    # And check that auxiliary files were renamed. Create an aux .txt file here.
    (source_dir / "!collages" / "Rose Gold.txt").touch()

    rename_collage(config, "Rose Gold", "Black Pink")
    assert not (source_dir / "!collages" / "Rose Gold.toml").exists()
    assert not (source_dir / "!collages" / "Rose Gold.txt").exists()
    assert (source_dir / "!collages" / "Black Pink.toml").exists()
    assert (source_dir / "!collages" / "Black Pink.txt").exists()

    with connect(config) as conn:
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM collages WHERE name = 'Black Pink')")
        assert cursor.fetchone()[0]
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM collages WHERE name = 'Rose Gold')")
        assert not cursor.fetchone()[0]


@pytest.mark.usefixtures("seeded_cache")
def test_dump_collage(config: Config) -> None:
    out = dump_collage(config, "Rose Gold")
    assert json.loads(out) == {
        "name": "Rose Gold",
        "releases": [
            {
                "position": 1,
                "id": "r1",
                "source_path": f"{config.music_source_dir}/r1",
                "cover_image_path": None,
                "added_at": "0000-01-01T00:00:00+00:00",
                "releasetitle": "Release 1",
                "releasetype": "album",
                "year": 2023,
                "new": False,
                "disctotal": 1,
                "genres": ["Techno", "Deep House"],
                "labels": ["Silk Music"],
                "releaseartists": {
                    "main": [
                        {"name": "Techno Man", "alias": False},
                        {"name": "Bass Man", "alias": False},
                    ],
                    "guest": [],
                    "remixer": [],
                    "producer": [],
                    "composer": [],
                    "djmixer": [],
                },
            },
            {
                "position": 2,
                "id": "r2",
                "source_path": f"{config.music_source_dir}/r2",
                "cover_image_path": f"{config.music_source_dir}/r2/cover.jpg",
                "added_at": "0000-01-01T00:00:00+00:00",
                "releasetitle": "Release 2",
                "releasetype": "album",
                "year": 2021,
                "new": False,
                "disctotal": 1,
                "genres": ["Classical"],
                "labels": ["Native State"],
                "releaseartists": {
                    "main": [{"name": "Violin Woman", "alias": False}],
                    "guest": [{"name": "Conductor Woman", "alias": False}],
                    "remixer": [],
                    "producer": [],
                    "composer": [],
                    "djmixer": [],
                },
            },
        ],
    }


@pytest.mark.usefixtures("seeded_cache")
def test_dump_collages(config: Config) -> None:
    out = dump_collages(config)
    assert json.loads(out) == [
        {
            "name": "Rose Gold",
            "releases": [
                {
                    "position": 1,
                    "id": "r1",
                    "source_path": f"{config.music_source_dir}/r1",
                    "cover_image_path": None,
                    "added_at": "0000-01-01T00:00:00+00:00",
                    "releasetitle": "Release 1",
                    "releasetype": "album",
                    "year": 2023,
                    "new": False,
                    "disctotal": 1,
                    "genres": ["Techno", "Deep House"],
                    "labels": ["Silk Music"],
                    "releaseartists": {
                        "main": [
                            {"name": "Techno Man", "alias": False},
                            {"name": "Bass Man", "alias": False},
                        ],
                        "guest": [],
                        "remixer": [],
                        "producer": [],
                        "composer": [],
                        "djmixer": [],
                    },
                },
                {
                    "position": 2,
                    "id": "r2",
                    "source_path": f"{config.music_source_dir}/r2",
                    "cover_image_path": f"{config.music_source_dir}/r2/cover.jpg",
                    "added_at": "0000-01-01T00:00:00+00:00",
                    "releasetitle": "Release 2",
                    "releasetype": "album",
                    "year": 2021,
                    "new": False,
                    "disctotal": 1,
                    "genres": ["Classical"],
                    "labels": ["Native State"],
                    "releaseartists": {
                        "main": [{"name": "Violin Woman", "alias": False}],
                        "guest": [{"name": "Conductor Woman", "alias": False}],
                        "remixer": [],
                        "producer": [],
                        "composer": [],
                        "djmixer": [],
                    },
                },
            ],
        },
        {"name": "Ruby Red", "releases": []},
    ]


def test_edit_collages_ordering(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    filepath = source_dir / "!collages" / "Rose Gold.toml"
    monkeypatch.setattr("rose.collages.click.edit", lambda x: "\n".join(reversed(x.split("\n"))))
    edit_collage_in_editor(config, "Rose Gold")

    with filepath.open("rb") as fp:
        data = tomllib.load(fp)
    assert data["releases"][0]["uuid"] == "ilovenewjeans"
    assert data["releases"][1]["uuid"] == "ilovecarly"


def test_edit_collages_remove_release(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    filepath = source_dir / "!collages" / "Rose Gold.toml"
    monkeypatch.setattr("rose.collages.click.edit", lambda x: x.split("\n")[0])
    edit_collage_in_editor(config, "Rose Gold")

    with filepath.open("rb") as fp:
        data = tomllib.load(fp)
    assert len(data["releases"]) == 1


def test_collage_handle_missing_release(config: Config, source_dir: Path) -> None:
    """Test that the lifecycle of the collage remains unimpeded despite a missing release."""
    filepath = source_dir / "!collages" / "Black Pink.toml"
    with filepath.open("w") as fp:
        fp.write(
            """\
[[releases]]
uuid = "ilovecarly"
description_meta = "lalala"
[[releases]]
uuid = "ghost"
description_meta = "lalala {MISSING}"
missing = true
"""
        )
    update_cache(config)

    # Assert that adding another release works.
    add_release_to_collage(config, "Black Pink", "ilovenewjeans")
    with (source_dir / "!collages" / "Black Pink.toml").open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["releases"]} == {"ghost", "ilovecarly", "ilovenewjeans"}
        assert next(r for r in diskdata["releases"] if r["uuid"] == "ghost")["missing"]
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT release_id FROM collages_releases WHERE collage_name = 'Black Pink'"
        )
        assert {r["release_id"] for r in cursor} == {"ghost", "ilovecarly", "ilovenewjeans"}

    # Delete that release.
    remove_release_from_collage(config, "Black Pink", "ilovenewjeans")
    with filepath.open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["releases"]} == {"ghost", "ilovecarly"}
        assert next(r for r in diskdata["releases"] if r["uuid"] == "ghost")["missing"]
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT release_id FROM collages_releases WHERE collage_name = 'Black Pink'"
        )
        assert {r["release_id"] for r in cursor} == {"ghost", "ilovecarly"}

    # And delete the collage.
    delete_collage(config, "Black Pink")
    assert not filepath.is_file()
    with connect(config) as conn:
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM collages WHERE name = 'Black Pink')")
        assert not cursor.fetchone()[0]
