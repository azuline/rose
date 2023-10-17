import shutil

import tomllib

from rose.cache import connect, update_cache
from rose.cache_test import TEST_COLLAGE_1, TEST_RELEASE_2, TEST_RELEASE_3
from rose.collages import (
    add_release_to_collage,
    create_collage,
    delete_collage,
    delete_release_from_collage,
)
from rose.config import Config


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
        config,
        "Rose Gold",
        "Carly Rae Jepsen - 1990. I Love Carly [Pop;Dream Pop] {A Cool Label}",
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
    add_release_to_collage(config, "Rose Gold", "NewJeans - 1990. I Love NewJeans [K-Pop;R&B]")
    with (config.music_source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["releases"]} == {"ilovecarly", "ilovenewjeans"}
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT release_id FROM collages_releases WHERE collage_name = 'Rose Gold'"
        )
        assert {r["release_id"] for r in cursor} == {"ilovecarly", "ilovenewjeans"}

    # Delete one release.
    delete_release_from_collage(config, "Rose Gold", "NewJeans - 1990. I Love NewJeans [K-Pop;R&B]")
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
