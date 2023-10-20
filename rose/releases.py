import json
import logging
from dataclasses import asdict
from pathlib import Path
from typing import Any

import tomli_w
import tomllib
from send2trash import send2trash

from rose.cache import (
    STORED_DATA_FILE_REGEX,
    get_release_id_from_virtual_dirname,
    get_release_source_path_from_id,
    get_release_virtual_dirname_from_id,
    list_releases,
    update_cache_evict_nonexistent_releases,
    update_cache_for_collages,
    update_cache_for_releases,
)
from rose.common import RoseError, valid_uuid
from rose.config import Config

logger = logging.getLogger()


class ReleaseDoesNotExistError(RoseError):
    pass


class CustomJSONEncoder(json.JSONEncoder):
    def default(self, obj: Any) -> Any:
        if isinstance(obj, Path):
            return str(obj)
        return super().default(obj)


def dump_releases(c: Config) -> str:
    releases = [asdict(r) for r in list_releases(c)]
    return json.dumps(releases, cls=CustomJSONEncoder)


def delete_release(c: Config, release_id_or_virtual_dirname: str) -> None:
    release_id, release_dirname = resolve_release_ids(c, release_id_or_virtual_dirname)
    source_path = get_release_source_path_from_id(c, release_id)
    if source_path is None:
        logger.debug(f"Failed to lookup source path for release {release_id} ({release_dirname})")
        return None
    send2trash(source_path)
    logger.info(f"Trashed release {release_dirname}")
    update_cache_evict_nonexistent_releases(c)
    # Update all collages so that the release is removed from whichever collages it was in.
    update_cache_for_collages(c, None, force=True)


def toggle_release_new(c: Config, release_id_or_virtual_dirname: str) -> None:
    release_id, release_dirname = resolve_release_ids(c, release_id_or_virtual_dirname)
    source_path = get_release_source_path_from_id(c, release_id)
    if source_path is None:
        logger.debug(f"Failed to lookup source path for release {release_id} ({release_dirname})")
        return None

    for f in source_path.iterdir():
        if not STORED_DATA_FILE_REGEX.match(f.name):
            continue

        with f.open("rb") as fp:
            data = tomllib.load(fp)
        data["new"] = not data["new"]
        with f.open("wb") as fp:
            tomli_w.dump(data, fp)
        update_cache_for_releases(c, [source_path], force=True)
        return

    logger.critical(f"Failed to find .rose.toml in {source_path}")


def resolve_release_ids(c: Config, release_id_or_virtual_dirname: str) -> tuple[str, str]:
    if valid_uuid(release_id_or_virtual_dirname):
        uuid = release_id_or_virtual_dirname
        virtual_dirname = get_release_virtual_dirname_from_id(c, uuid)
    else:
        virtual_dirname = release_id_or_virtual_dirname
        uuid = get_release_id_from_virtual_dirname(c, virtual_dirname)  # type: ignore
    if uuid is None or virtual_dirname is None:
        raise ReleaseDoesNotExistError(f"Release {uuid} ({virtual_dirname}) does not exist")
    return uuid, virtual_dirname
