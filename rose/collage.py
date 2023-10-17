import logging
from pathlib import Path

import tomli_w
import tomllib

from rose.cache import (
    get_release_id_from_virtual_dirname,
    update_cache_evict_nonexistent_collages,
    update_cache_for_collages,
)
from rose.config import Config

logger = logging.getLogger(__name__)


def delete_release_from_collage(
    c: Config,
    collage_name: str,
    release_virtual_dirname: str,
) -> None:
    release_id = get_release_id_from_virtual_dirname(c, release_virtual_dirname)
    fpath = collage_path(c, collage_name)
    with fpath.open("rb") as fp:
        data = tomllib.load(fp)
    data["releases"] = data.get("releases", [])
    data["releases"] = [r for r in data.get("releases", []) if r["uuid"] != release_id]
    with fpath.open("wb") as fp:
        tomli_w.dump(data, fp)
    update_cache_for_collages(c, [collage_name], force=True)


def add_release_to_collage(
    c: Config,
    collage_name: str,
    release_virtual_dirname: str,
) -> None:
    release_id = get_release_id_from_virtual_dirname(c, release_virtual_dirname)
    fpath = collage_path(c, collage_name)
    with fpath.open("rb") as fp:
        data = tomllib.load(fp)
    data["releases"] = data.get("releases", [])
    # Check to see if release is already in the collage. If so, no op. We don't support duplicate
    # collage entries.
    for r in data["releases"]:
        if r["uuid"] == release_id:
            logger.debug(
                f"No-Opping: Release {release_virtual_dirname} already in collage {collage_name}."
            )
            return
    data["releases"].append({"uuid": release_id, "description_meta": release_virtual_dirname})
    with fpath.open("wb") as fp:
        tomli_w.dump(data, fp)
    update_cache_for_collages(c, [collage_name], force=True)


def create_collage(c: Config, collage_name: str) -> None:
    collage_path(c, collage_name).touch()
    update_cache_for_collages(c, [collage_name], force=True)


def delete_collage(c: Config, collage_name: str) -> None:
    collage_path(c, collage_name).unlink()
    update_cache_evict_nonexistent_collages(c)


def collage_path(c: Config, name: str) -> Path:
    return c.music_source_dir / "!collages" / f"{name}.toml"
