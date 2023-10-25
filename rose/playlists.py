import json
import logging
from pathlib import Path
from typing import Any

import click
import tomli_w
import tomllib
from send2trash import send2trash

from rose.cache import (
    get_playlist,
    get_track_filename,
    list_playlists,
    lock,
    playlist_lock_name,
    update_cache_evict_nonexistent_playlists,
    update_cache_for_playlists,
)
from rose.common import RoseError
from rose.config import Config

logger = logging.getLogger(__name__)


class DescriptionMismatchError(RoseError):
    pass


class PlaylistDoesNotExistError(RoseError):
    pass


class PlaylistAlreadyExistsError(RoseError):
    pass


def create_playlist(c: Config, name: str) -> None:
    (c.music_source_dir / "!playlists").mkdir(parents=True, exist_ok=True)
    path = playlist_path(c, name)
    with lock(c, playlist_lock_name(name)):
        if path.exists():
            raise PlaylistAlreadyExistsError(f"Playlist {name} already exists")
        path.touch()
    update_cache_for_playlists(c, [name], force=True)


def delete_playlist(c: Config, name: str) -> None:
    path = playlist_path(c, name)
    with lock(c, playlist_lock_name(name)):
        if not path.exists():
            raise PlaylistDoesNotExistError(f"Playlist {name} does not exist")
        send2trash(path)
    update_cache_evict_nonexistent_playlists(c)


def rename_playlist(c: Config, old_name: str, new_name: str) -> None:
    logger.info(f"Renaming playlist {old_name} to {new_name}")
    old_path = playlist_path(c, old_name)
    new_path = playlist_path(c, new_name)
    with lock(c, playlist_lock_name(old_name)), lock(c, playlist_lock_name(new_name)):
        if not old_path.exists():
            raise PlaylistDoesNotExistError(f"Playlist {old_name} does not exist")
        if new_path.exists():
            raise PlaylistAlreadyExistsError(f"Playlist {new_name} already exists")
        old_path.rename(new_path)
    update_cache_for_playlists(c, [new_name], force=True)
    update_cache_evict_nonexistent_playlists(c)


def remove_track_from_playlist(
    c: Config,
    playlist_name: str,
    track_id: str,
) -> None:
    track_filename = get_track_filename(c, track_id)
    path = playlist_path(c, playlist_name)
    if not path.exists():
        raise PlaylistDoesNotExistError(f"Playlist {playlist_name} does not exist")
    with lock(c, playlist_lock_name(playlist_name)):
        with path.open("rb") as fp:
            data = tomllib.load(fp)
        data["tracks"] = data.get("tracks", [])
        data["tracks"] = [r for r in data.get("tracks", []) if r["uuid"] != track_id]
        with path.open("wb") as fp:
            tomli_w.dump(data, fp)
    logger.info(f"Removed track {track_filename} from playlist {playlist_name}")
    update_cache_for_playlists(c, [playlist_name], force=True)


def add_track_to_playlist(
    c: Config,
    playlist_name: str,
    track_id: str,
) -> None:
    track_filename = get_track_filename(c, track_id)
    path = playlist_path(c, playlist_name)
    if not path.exists():
        raise PlaylistDoesNotExistError(f"Playlist {playlist_name} does not exist")
    with lock(c, playlist_lock_name(playlist_name)):
        with path.open("rb") as fp:
            data = tomllib.load(fp)
        data["tracks"] = data.get("tracks", [])
        # Check to see if track is already in the playlist. If so, no op. We don't support
        # duplicate playlist entries.
        for r in data["tracks"]:
            if r["uuid"] == track_id:
                logger.debug(
                    f"No-Opping: Track {track_filename} already in playlist {playlist_name}"
                )
                return
        data["tracks"].append({"uuid": track_id, "description_meta": track_filename})
        with path.open("wb") as fp:
            tomli_w.dump(data, fp)
    logger.info(f"Added track {track_filename} to playlist {playlist_name}")
    update_cache_for_playlists(c, [playlist_name], force=True)


def dump_playlists(c: Config) -> str:
    out: dict[str, list[dict[str, Any]]] = {}
    playlist_names = list(list_playlists(c))
    for name in playlist_names:
        out[name] = []
        cachedata = get_playlist(c, name)
        assert cachedata is not None
        _, tracks = cachedata
        for idx, track in enumerate(tracks):
            out[name].append(
                {
                    "position": idx + 1,
                    "track_id": track.id,
                    "track_filename": track.virtual_filename,
                }
            )
    return json.dumps(out)


def edit_playlist_in_editor(c: Config, playlist_name: str) -> None:
    path = playlist_path(c, playlist_name)
    if not path.exists():
        raise PlaylistDoesNotExistError(f"Playlist {playlist_name} does not exist")
    with lock(c, playlist_lock_name(playlist_name), timeout=60.0):
        with path.open("rb") as fp:
            data = tomllib.load(fp)
        raw_tracks = data.get("tracks", [])
        edited_track_descriptions = click.edit(
            "\n".join([r["description_meta"] for r in raw_tracks])
        )
        if edited_track_descriptions is None:
            logger.info("Aborting: metadata file not submitted.")
            return
        uuid_mapping = {r["description_meta"]: r["uuid"] for r in raw_tracks}

        edited_tracks: list[dict[str, Any]] = []
        for desc in edited_track_descriptions.strip().split("\n"):
            try:
                uuid = uuid_mapping[desc]
            except KeyError as e:
                raise DescriptionMismatchError(
                    f"Track {desc} does not match a known track in the playlist. "
                    "Was the line edited?"
                ) from e
            edited_tracks.append({"uuid": uuid, "description_meta": desc})
        data["tracks"] = edited_tracks

        with path.open("wb") as fp:
            tomli_w.dump(data, fp)
    logger.info(f"Edited playlist {playlist_name} from EDITOR")
    update_cache_for_playlists(c, [playlist_name], force=True)


def playlist_path(c: Config, name: str) -> Path:
    return c.music_source_dir / "!playlists" / f"{name}.toml"
