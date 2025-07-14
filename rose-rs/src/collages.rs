"""
The collages module provides functions for interacting with collages.
"""

import logging
import tomllib
from pathlib import Path
from typing import Any

import click
import tomli_w
from send2trash import send2trash

from rose.cache import (
    collage_lock_name,
    get_release_logtext,
    lock,
    update_cache_evict_nonexistent_collages,
    update_cache_for_collages,
)
from rose.common import RoseExpectedError
from rose.config import Config
from rose.releases import ReleaseDoesNotExistError

logger = logging.getLogger(__name__)


class DescriptionMismatchError(RoseExpectedError):
    pass


class CollageDoesNotExistError(RoseExpectedError):
    pass


class CollageAlreadyExistsError(RoseExpectedError):
    pass


def create_collage(c: Config, name: str) -> None:
    (c.music_source_dir / "!collages").mkdir(parents=True, exist_ok=True)
    path = collage_path(c, name)
    with lock(c, collage_lock_name(name)):
        if path.exists():
            raise CollageAlreadyExistsError(f"Collage {name} already exists")
        path.touch()
    logger.info(f"Created collage {name} in source directory")
    update_cache_for_collages(c, [name], force=True)


def delete_collage(c: Config, name: str) -> None:
    path = collage_path(c, name)
    with lock(c, collage_lock_name(name)):
        if not path.exists():
            raise CollageDoesNotExistError(f"Collage {name} does not exist")
        send2trash(path)
    logger.info(f"Deleted collage {name} from source directory")
    update_cache_evict_nonexistent_collages(c)


def rename_collage(c: Config, old_name: str, new_name: str) -> None:
    old_path = collage_path(c, old_name)
    new_path = collage_path(c, new_name)
    with lock(c, collage_lock_name(old_name)), lock(c, collage_lock_name(new_name)):
        if not old_path.exists():
            raise CollageDoesNotExistError(f"Collage {old_name} does not exist")
        if new_path.exists():
            raise CollageAlreadyExistsError(f"Collage {new_name} already exists")
        old_path.rename(new_path)
        # And also rename all files with the same stem (e.g. cover arts).
        for old_adjacent_file in (c.music_source_dir / "!collages").iterdir():
            if old_adjacent_file.stem != old_path.stem:
                continue
            new_adjacent_file = old_adjacent_file.with_name(new_path.stem + old_adjacent_file.suffix)
            if new_adjacent_file.exists():
                continue
            old_adjacent_file.rename(new_adjacent_file)
            logger.debug("Renaming collage-adjacent file {old_adjacent_file} to {new_adjacent_file}")
    logger.info(f"Renamed collage {old_name} to {new_name}")
    update_cache_for_collages(c, [new_name], force=True)
    update_cache_evict_nonexistent_collages(c)


def remove_release_from_collage(c: Config, collage_name: str, release_id: str) -> None:
    release_logtext = get_release_logtext(c, release_id)
    if not release_logtext:
        raise ReleaseDoesNotExistError(f"Release {release_id} does not exist")

    path = collage_path(c, collage_name)
    if not path.exists():
        raise CollageDoesNotExistError(f"Collage {collage_name} does not exist")
    with lock(c, collage_lock_name(collage_name)):
        with path.open("rb") as fp:
            data = tomllib.load(fp)
        old_releases = data.get("releases", [])
        releases_new = [r for r in old_releases if r["uuid"] != release_id]
        if old_releases == releases_new:
            logger.info(f"No-Op: Release {release_logtext} not in collage {collage_name}")
            return
        data["releases"] = releases_new
        with path.open("wb") as fp:
            tomli_w.dump(data, fp)
    logger.info(f"Removed release {release_logtext} from collage {collage_name}")
    update_cache_for_collages(c, [collage_name], force=True)


def add_release_to_collage(
    c: Config,
    collage_name: str,
    release_id: str,
) -> None:
    release_logtext = get_release_logtext(c, release_id)
    if not release_logtext:
        raise ReleaseDoesNotExistError(f"Release {release_id} does not exist")

    path = collage_path(c, collage_name)
    if not path.exists():
        raise CollageDoesNotExistError(f"Collage {collage_name} does not exist")

    with lock(c, collage_lock_name(collage_name)):
        with path.open("rb") as fp:
            data = tomllib.load(fp)
        data["releases"] = data.get("releases", [])
        # Check to see if release is already in the collage. If so, no op. We don't support
        # duplicate collage entries.
        for r in data["releases"]:
            if r["uuid"] == release_id:
                logger.info(f"No-Op: Release {release_logtext} already in collage {collage_name}")
                return
        data["releases"].append({"uuid": release_id, "description_meta": release_logtext})
        with path.open("wb") as fp:
            tomli_w.dump(data, fp)
    logger.info(f"Added release {release_logtext} to collage {collage_name}")
    update_cache_for_collages(c, [collage_name], force=True)


def edit_collage_in_editor(c: Config, collage_name: str) -> None:
    path = collage_path(c, collage_name)
    if not path.exists():
        raise CollageDoesNotExistError(f"Collage {collage_name} does not exist")
    with lock(c, collage_lock_name(collage_name), timeout=60.0):
        with path.open("rb") as fp:
            data = tomllib.load(fp)
        raw_releases = data.get("releases", [])
        edited_release_descriptions = click.edit("\n".join([r["description_meta"] for r in raw_releases]))
        if edited_release_descriptions is None:
            logger.info("Aborting: metadata file not submitted.")
            return
        uuid_mapping = {r["description_meta"]: r["uuid"] for r in raw_releases}

        edited_releases: list[dict[str, Any]] = []
        for desc in edited_release_descriptions.strip().split("\n"):
            try:
                uuid = uuid_mapping[desc]
            except KeyError as e:
                raise DescriptionMismatchError(
                    f"Release {desc} does not match a known release in the collage. " "Was the line edited?"
                ) from e
            edited_releases.append({"uuid": uuid, "description_meta": desc})
        data["releases"] = edited_releases

        with path.open("wb") as fp:
            tomli_w.dump(data, fp)
    logger.info(f"Edited collage {collage_name} from EDITOR")
    update_cache_for_collages(c, [collage_name], force=True)


def collage_path(c: Config, name: str) -> Path:
    return c.music_source_dir / "!collages" / f"{name}.toml"

# TESTS

import tomllib
from pathlib import Path
from typing import Any

from rose.cache import connect, update_cache
from rose.collages import (
    add_release_to_collage,
    create_collage,
    delete_collage,
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
        cursor = conn.execute("SELECT release_id FROM collages_releases WHERE collage_name = 'Rose Gold'")
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
        cursor = conn.execute("SELECT release_id FROM collages_releases WHERE collage_name = 'All Eyes'")
        assert {r["release_id"] for r in cursor} == {"ilovecarly"}

    # Add another release.
    add_release_to_collage(config, "All Eyes", "ilovenewjeans")
    with (source_dir / "!collages" / "All Eyes.toml").open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["releases"]} == {"ilovecarly", "ilovenewjeans"}
    with connect(config) as conn:
        cursor = conn.execute("SELECT release_id FROM collages_releases WHERE collage_name = 'All Eyes'")
        assert {r["release_id"] for r in cursor} == {"ilovecarly", "ilovenewjeans"}

    # Delete one release.
    remove_release_from_collage(config, "All Eyes", "ilovenewjeans")
    with filepath.open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["releases"]} == {"ilovecarly"}
    with connect(config) as conn:
        cursor = conn.execute("SELECT release_id FROM collages_releases WHERE collage_name = 'All Eyes'")
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
        cursor = conn.execute("SELECT release_id FROM collages_releases WHERE collage_name = 'Black Pink'")
        assert {r["release_id"] for r in cursor} == {"ghost", "ilovecarly", "ilovenewjeans"}

    # Delete that release.
    remove_release_from_collage(config, "Black Pink", "ilovenewjeans")
    with filepath.open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["releases"]} == {"ghost", "ilovecarly"}
        assert next(r for r in diskdata["releases"] if r["uuid"] == "ghost")["missing"]
    with connect(config) as conn:
        cursor = conn.execute("SELECT release_id FROM collages_releases WHERE collage_name = 'Black Pink'")
        assert {r["release_id"] for r in cursor} == {"ghost", "ilovecarly"}

    # And delete the collage.
    delete_collage(config, "Black Pink")
    assert not filepath.is_file()
    with connect(config) as conn:
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM collages WHERE name = 'Black Pink')")
        assert not cursor.fetchone()[0]
