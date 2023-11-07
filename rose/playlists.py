"""
The playlists module provides functions for interacting with playlists.
"""


import json
import logging
import shutil
from collections import Counter
from pathlib import Path
from typing import Any

import click
import tomli_w
import tomllib
from send2trash import send2trash

from rose.cache import (
    get_playlist,
    get_track_logtext,
    list_playlists,
    lock,
    playlist_lock_name,
    update_cache_evict_nonexistent_playlists,
    update_cache_for_playlists,
)
from rose.common import RoseExpectedError
from rose.config import Config

logger = logging.getLogger(__name__)


class InvalidCoverArtFileError(RoseExpectedError):
    pass


class DescriptionMismatchError(RoseExpectedError):
    pass


class PlaylistDoesNotExistError(RoseExpectedError):
    pass


class TrackDoesNotExistError(RoseExpectedError):
    pass


class PlaylistAlreadyExistsError(RoseExpectedError):
    pass


def create_playlist(c: Config, name: str) -> None:
    (c.music_source_dir / "!playlists").mkdir(parents=True, exist_ok=True)
    path = playlist_path(c, name)
    with lock(c, playlist_lock_name(name)):
        if path.exists():
            raise PlaylistAlreadyExistsError(f"Playlist {name} already exists")
        path.touch()
    logger.info(f"Created playlist {name} in source directory")
    update_cache_for_playlists(c, [name], force=True)


def delete_playlist(c: Config, name: str) -> None:
    path = playlist_path(c, name)
    with lock(c, playlist_lock_name(name)):
        if not path.exists():
            raise PlaylistDoesNotExistError(f"Playlist {name} does not exist")
        send2trash(path)
    logger.info(f"Deleted playlist {name} from source directory")
    update_cache_evict_nonexistent_playlists(c)


def rename_playlist(c: Config, old_name: str, new_name: str) -> None:
    logger.info(f"Renamed playlist {old_name} to {new_name}")
    old_path = playlist_path(c, old_name)
    new_path = playlist_path(c, new_name)
    with lock(c, playlist_lock_name(old_name)), lock(c, playlist_lock_name(new_name)):
        if not old_path.exists():
            raise PlaylistDoesNotExistError(f"Playlist {old_name} does not exist")
        if new_path.exists():
            raise PlaylistAlreadyExistsError(f"Playlist {new_name} already exists")
        old_path.rename(new_path)
        # And also rename all files with the same stem (e.g. cover arts).
        for old_adjacent_file in (c.music_source_dir / "!playlists").iterdir():
            if old_adjacent_file.stem != old_path.stem:
                continue
            new_adjacent_file = old_adjacent_file.with_name(
                new_path.stem + old_adjacent_file.suffix
            )
            if new_adjacent_file.exists():
                continue
            old_adjacent_file.rename(new_adjacent_file)
            logger.debug(
                "Renaming playlist-adjacent file {old_adjacent_file} to {new_adjacent_file}"
            )
    update_cache_for_playlists(c, [new_name], force=True)
    update_cache_evict_nonexistent_playlists(c)


def remove_track_from_playlist(
    c: Config,
    playlist_name: str,
    track_id: str,
) -> None:
    track_logtext = get_track_logtext(c, track_id)
    if not track_logtext:
        raise TrackDoesNotExistError(f"Track {track_id} does not exist")
    path = playlist_path(c, playlist_name)
    if not path.exists():
        raise PlaylistDoesNotExistError(f"Playlist {playlist_name} does not exist")
    with lock(c, playlist_lock_name(playlist_name)):
        with path.open("rb") as fp:
            data = tomllib.load(fp)
        old_tracks = data.get("tracks", [])
        new_tracks = [r for r in old_tracks if r["uuid"] != track_id]
        if old_tracks == new_tracks:
            logger.info(f"No-Op: Track {track_logtext} not in playlist {playlist_name}")
            return
        data["tracks"] = new_tracks
        with path.open("wb") as fp:
            tomli_w.dump(data, fp)
    logger.info(f"Removed track {track_logtext} from playlist {playlist_name}")
    update_cache_for_playlists(c, [playlist_name], force=True)


def add_track_to_playlist(
    c: Config,
    playlist_name: str,
    track_id: str,
) -> None:
    track_logtext = get_track_logtext(c, track_id)
    if not track_logtext:
        raise TrackDoesNotExistError(f"Track {track_id} does not exist")
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
                logger.info(f"No-Op: Track {track_logtext} already in playlist {playlist_name}")
                return
        data["tracks"].append({"uuid": track_id, "description_meta": track_logtext})
        with path.open("wb") as fp:
            tomli_w.dump(data, fp)
    logger.info(f"Added track {track_logtext} to playlist {playlist_name}")
    update_cache_for_playlists(c, [playlist_name], force=True)


def dump_playlist(c: Config, playlist_name: str) -> str:
    pdata = get_playlist(c, playlist_name)
    if pdata is None:
        raise PlaylistDoesNotExistError(f"Playlist {playlist_name} does not exist")
    tracks: list[dict[str, Any]] = []
    for idx, trk in enumerate(pdata[1]):
        tracks.append({"position": idx + 1, **trk.dump()})
    return json.dumps(
        {
            "name": playlist_name,
            "cover_image_path": str(pdata[0].cover_path) if pdata[0].cover_path else None,
            "tracks": tracks,
        }
    )


def dump_playlists(c: Config) -> str:
    out: list[dict[str, Any]] = []
    for name in list_playlists(c):
        pdata = get_playlist(c, name)
        assert pdata is not None
        tracks: list[dict[str, Any]] = []
        for idx, trk in enumerate(pdata[1]):
            tracks.append({"position": idx + 1, **trk.dump()})
        out.append(
            {
                "name": name,
                "cover_image_path": str(pdata[0].cover_path) if pdata[0].cover_path else None,
                "tracks": tracks,
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

        # Because tracks are not globally unique, we append the UUID if there are any conflicts.
        # discriminator.
        lines_to_edit: list[str] = []
        uuid_mapping: dict[str, str] = {}
        line_occurrences = Counter([r["description_meta"] for r in raw_tracks])
        for r in raw_tracks:
            if line_occurrences[r["description_meta"]] > 1:
                line = f'{r["description_meta"]} [{r["uuid"]}]'
            else:
                line = r["description_meta"]
            lines_to_edit.append(line)
            uuid_mapping[line] = r["uuid"]

        edited_track_descriptions = click.edit("\n".join(lines_to_edit))
        if edited_track_descriptions is None:
            logger.info("Aborting: metadata file not submitted.")
            return

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


def set_playlist_cover_art(c: Config, playlist_name: str, new_cover_art_path: Path) -> None:
    """
    This function removes all potential cover arts for the playlist, and then copies the file
    file located at the passed in path to be the playlist's art file.
    """
    suffix = new_cover_art_path.suffix.lower()
    if suffix[1:] not in c.valid_art_exts:
        raise InvalidCoverArtFileError(
            f"File {new_cover_art_path.name}'s extension is not supported for cover images: "
            "To change this, please read the configuration documentation"
        )

    path = playlist_path(c, playlist_name)
    if not path.exists():
        raise PlaylistDoesNotExistError(f"Playlist {playlist_name} does not exist")
    for f in (c.music_source_dir / "!playlists").iterdir():
        if f.stem == playlist_name and f.suffix[1:].lower() in c.valid_art_exts:
            logger.debug(f"Deleting existing cover art {f.name} in playlists")
            f.unlink()
    shutil.copyfile(new_cover_art_path, path.with_suffix(suffix))
    logger.info(f"Set the cover of playlist {playlist_name} to {new_cover_art_path.name}")
    update_cache_for_playlists(c, [playlist_name])


def delete_playlist_cover_art(c: Config, playlist_name: str) -> None:
    """This function removes all potential cover arts for the playlist."""
    path = playlist_path(c, playlist_name)
    if not path.exists():
        raise PlaylistDoesNotExistError(f"Playlist {playlist_name} does not exist")
    found = False
    for f in (c.music_source_dir / "!playlists").iterdir():
        if f.stem == playlist_name and f.suffix[1:].lower() in c.valid_art_exts:
            logger.debug(f"Deleting existing cover art {f.name} in playlists")
            f.unlink()
            found = True
    if found:
        logger.info(f"Deleted cover arts of playlist {playlist_name}")
    else:
        logger.info(f"No-Op: No cover arts found for playlist {playlist_name}")
    update_cache_for_playlists(c, [playlist_name])


def playlist_path(c: Config, name: str) -> Path:
    return c.music_source_dir / "!playlists" / f"{name}.toml"
