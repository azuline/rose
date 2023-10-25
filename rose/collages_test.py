from pathlib import Path
from typing import Any

import pytest
import tomllib

from rose.cache import connect
from rose.collages import (
    add_release_to_collage,
    create_collage,
    delete_collage,
    dump_collages,
    edit_collage_in_editor,
    remove_release_from_collage,
    rename_collage,
)
from rose.config import Config


def test_remove_release_from_collage(config: Config, source_dir: Path) -> None:
    remove_release_from_collage(
        config, "Rose Gold", "Carly Rae Jepsen - 1990. I Love Carly [Pop;Dream Pop] {A Cool Label}"
    )

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
    add_release_to_collage(
        config, "All Eyes", "Carly Rae Jepsen - 1990. I Love Carly [Pop;Dream Pop] {A Cool Label}"
    )
    with filepath.open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["releases"]} == {"ilovecarly"}
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT release_id FROM collages_releases WHERE collage_name = 'All Eyes'"
        )
        assert {r["release_id"] for r in cursor} == {"ilovecarly"}

    # Add another release.
    add_release_to_collage(
        config, "All Eyes", "NewJeans - 1990. I Love NewJeans [K-Pop;R&B] {A Cool Label}"
    )
    with (source_dir / "!collages" / "All Eyes.toml").open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["releases"]} == {"ilovecarly", "ilovenewjeans"}
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT release_id FROM collages_releases WHERE collage_name = 'All Eyes'"
        )
        assert {r["release_id"] for r in cursor} == {"ilovecarly", "ilovenewjeans"}

    # Delete one release.
    remove_release_from_collage(
        config, "All Eyes", "NewJeans - 1990. I Love NewJeans [K-Pop;R&B] {A Cool Label}"
    )
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
    add_release_to_collage(
        config, "All Eyes", "NewJeans - 1990. I Love NewJeans [K-Pop;R&B] {A Cool Label}"
    )
    add_release_to_collage(
        config, "All Eyes", "NewJeans - 1990. I Love NewJeans [K-Pop;R&B] {A Cool Label}"
    )
    with (source_dir / "!collages" / "All Eyes.toml").open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert len(diskdata["releases"]) == 1
    with connect(config) as conn:
        cursor = conn.execute("SELECT * FROM collages_releases WHERE collage_name = 'All Eyes'")
        assert len(cursor.fetchall()) == 1


def test_rename_collage(config: Config, source_dir: Path) -> None:
    rename_collage(config, "Rose Gold", "Black Pink")
    assert not (source_dir / "!collages" / "Rose Gold.toml").exists()
    assert (source_dir / "!collages" / "Black Pink.toml").exists()
    with connect(config) as conn:
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM collages WHERE name = 'Black Pink')")
        assert cursor.fetchone()[0]
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM collages WHERE name = 'Rose Gold')")
        assert not cursor.fetchone()[0]


@pytest.mark.usefixtures("seeded_cache")
def test_dump_collages(config: Config) -> None:
    out = dump_collages(config)
    # fmt: off
    assert out == '{"Rose Gold": [{"position": 0, "release": "r1"}, {"position": 1, "release": "r2"}], "Ruby Red": []}' # noqa: E501
    # fmt: on


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
