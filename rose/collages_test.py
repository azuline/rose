import shutil
from typing import Any

import pytest
import tomllib

from rose.cache import connect, update_cache
from rose.cache_test import TEST_COLLAGE_1, TEST_RELEASE_2, TEST_RELEASE_3
from rose.collages import (
    add_release_to_collage,
    create_collage,
    delete_collage,
    delete_release_from_collage,
    dump_collages,
    edit_collage_in_editor,
    rename_collage,
)
from rose.config import Config

# TODO: Fixture for common setup.


def test_delete_release_from_collage(config: Config) -> None:
    # Set up the filesystem that will be updated.
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_RELEASE_3, config.music_source_dir / TEST_RELEASE_3.name)
    shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
    # Bootstrap initial cache.
    update_cache(config)

    delete_release_from_collage(
        config, "Rose Gold", "Carly Rae Jepsen - 1990. I Love Carly [Pop;Dream Pop] {A Cool Label}"
    )

    # Assert file is updated.
    with (config.music_source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
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


def test_collage_lifecycle(config: Config) -> None:
    # Set up the filesystem that will be updated.
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_RELEASE_3, config.music_source_dir / TEST_RELEASE_3.name)
    # Bootstrap initial cache.
    update_cache(config)

    filepath = config.music_source_dir / "!collages" / "Rose Gold.toml"

    # Create collage.
    assert not filepath.exists()
    create_collage(config, "Rose Gold")
    assert filepath.is_file()
    with connect(config) as conn:
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM collages WHERE name = 'Rose Gold')")
        assert cursor.fetchone()[0]

    # Add one release.
    add_release_to_collage(
        config, "Rose Gold", "Carly Rae Jepsen - 1990. I Love Carly [Pop;Dream Pop] {A Cool Label}"
    )
    with filepath.open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["releases"]} == {"ilovecarly"}
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT release_id FROM collages_releases WHERE collage_name = 'Rose Gold'"
        )
        assert {r["release_id"] for r in cursor} == {"ilovecarly"}

    # Add another release.
    add_release_to_collage(
        config, "Rose Gold", "NewJeans - 1990. I Love NewJeans [K-Pop;R&B] {A Cool Label}"
    )
    with (config.music_source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["releases"]} == {"ilovecarly", "ilovenewjeans"}
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT release_id FROM collages_releases WHERE collage_name = 'Rose Gold'"
        )
        assert {r["release_id"] for r in cursor} == {"ilovecarly", "ilovenewjeans"}

    # Delete one release.
    delete_release_from_collage(
        config, "Rose Gold", "NewJeans - 1990. I Love NewJeans [K-Pop;R&B] {A Cool Label}"
    )
    with filepath.open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["releases"]} == {"ilovecarly"}
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT release_id FROM collages_releases WHERE collage_name = 'Rose Gold'"
        )
        assert {r["release_id"] for r in cursor} == {"ilovecarly"}

    # And delete the collage.
    delete_collage(config, "Rose Gold")
    assert not filepath.is_file()
    with connect(config) as conn:
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM collages WHERE name = 'Rose Gold')")
        assert not cursor.fetchone()[0]


def test_collage_add_duplicate(config: Config) -> None:
    shutil.copytree(TEST_RELEASE_3, config.music_source_dir / TEST_RELEASE_3.name)
    update_cache(config)
    create_collage(config, "Rose Gold")
    add_release_to_collage(
        config, "Rose Gold", "NewJeans - 1990. I Love NewJeans [K-Pop;R&B] {A Cool Label}"
    )
    add_release_to_collage(
        config, "Rose Gold", "NewJeans - 1990. I Love NewJeans [K-Pop;R&B] {A Cool Label}"
    )
    with (config.music_source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert len(diskdata["releases"]) == 1
    with connect(config) as conn:
        cursor = conn.execute("SELECT * FROM collages_releases WHERE collage_name = 'Rose Gold'")
        assert len(cursor.fetchall()) == 1


def test_rename_collage(config: Config) -> None:
    shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
    rename_collage(config, "Rose Gold", "Black Pink")

    assert not (config.music_source_dir / "!collages" / "Rose Gold.toml").exists()
    assert (config.music_source_dir / "!collages" / "Black Pink.toml").exists()
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


def test_edit_collages_ordering(monkeypatch: Any, config: Config) -> None:
    # Set up the filesystem that will be updated.
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_RELEASE_3, config.music_source_dir / TEST_RELEASE_3.name)
    shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
    # Bootstrap initial cache.
    update_cache(config)
    filepath = config.music_source_dir / "!collages" / "Rose Gold.toml"

    def mock_edit(x: str) -> str:
        return "\n".join(reversed(x.split("\n")))

    monkeypatch.setattr("rose.collages.click.edit", mock_edit)
    edit_collage_in_editor(config, "Rose Gold")

    with filepath.open("rb") as fp:
        data = tomllib.load(fp)
    assert data["releases"][0]["uuid"] == "ilovenewjeans"
    assert data["releases"][1]["uuid"] == "ilovecarly"


def test_edit_collages_delete_release(monkeypatch: Any, config: Config) -> None:
    # Set up the filesystem that will be updated.
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_RELEASE_3, config.music_source_dir / TEST_RELEASE_3.name)
    shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
    # Bootstrap initial cache.
    update_cache(config)
    filepath = config.music_source_dir / "!collages" / "Rose Gold.toml"

    def mock_edit(x: str) -> str:
        return x.split("\n")[0]

    monkeypatch.setattr("rose.collages.click.edit", mock_edit)
    edit_collage_in_editor(config, "Rose Gold")

    with filepath.open("rb") as fp:
        data = tomllib.load(fp)
    assert len(data["releases"]) == 1
