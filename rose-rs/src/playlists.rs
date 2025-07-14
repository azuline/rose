"""
The playlists module provides functions for interacting with playlists.
"""

import logging
import shutil
import tomllib
from collections import Counter
from pathlib import Path
from typing import Any

import click
import tomli_w
from send2trash import send2trash

from rose.cache import (
    get_track,
    get_track_logtext,
    lock,
    make_track_logtext,
    playlist_lock_name,
    update_cache_evict_nonexistent_playlists,
    update_cache_for_playlists,
)
from rose.collages import DescriptionMismatchError
from rose.common import RoseExpectedError
from rose.config import Config
from rose.releases import InvalidCoverArtFileError
from rose.templates import artistsfmt
from rose.tracks import TrackDoesNotExistError

logger = logging.getLogger(__name__)


class PlaylistDoesNotExistError(RoseExpectedError):
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
            new_adjacent_file = old_adjacent_file.with_name(new_path.stem + old_adjacent_file.suffix)
            if new_adjacent_file.exists():
                continue
            old_adjacent_file.rename(new_adjacent_file)
            logger.debug("Renaming playlist-adjacent file {old_adjacent_file} to {new_adjacent_file}")
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
    track = get_track(c, track_id)
    if not track:
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
                logger.info(f"No-Op: Track {track} already in playlist {playlist_name}")
                return

        desc = f"{artistsfmt(track.trackartists)} - {track.tracktitle}"
        data["tracks"].append({"uuid": track_id, "description_meta": desc})
        with path.open("wb") as fp:
            tomli_w.dump(data, fp)
    track_logtext = make_track_logtext(
        title=track.tracktitle,
        artists=track.trackartists,
        releasedate=track.release.releasedate,
        suffix=track.source_path.suffix,
    )
    logger.info(f"Added track {track_logtext} to playlist {playlist_name}")
    update_cache_for_playlists(c, [playlist_name], force=True)


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
                    f"Track {desc} does not match a known track in the playlist. Was the line edited?"
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

# TESTS

import shutil
import tomllib
from pathlib import Path
from typing import Any

from conftest import TEST_PLAYLIST_1, TEST_RELEASE_1
from rose.cache import connect, update_cache
from rose.config import Config
from rose.playlists import (
    add_track_to_playlist,
    create_playlist,
    delete_playlist,
    delete_playlist_cover_art,
    edit_playlist_in_editor,
    remove_track_from_playlist,
    rename_playlist,
    set_playlist_cover_art,
)


def test_remove_track_from_playlist(config: Config, source_dir: Path) -> None:
    remove_track_from_playlist(config, "Lala Lisa", "iloveloona")

    # Assert file is updated.
    with (source_dir / "!playlists" / "Lala Lisa.toml").open("rb") as fp:
        diskdata = tomllib.load(fp)
    assert len(diskdata["tracks"]) == 1
    assert diskdata["tracks"][0]["uuid"] == "ilovetwice"

    # Assert cache is updated.
    with connect(config) as conn:
        cursor = conn.execute("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'Lala Lisa'")
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
        cursor = conn.execute("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'")
        assert {r["track_id"] for r in cursor} == {"iloveloona"}

    # Add another track.
    add_track_to_playlist(config, "You & Me", "ilovetwice")
    with (source_dir / "!playlists" / "You & Me.toml").open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["tracks"]} == {"iloveloona", "ilovetwice"}
    with connect(config) as conn:
        cursor = conn.execute("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'")
        assert {r["track_id"] for r in cursor} == {"iloveloona", "ilovetwice"}

    # Delete one track.
    remove_track_from_playlist(config, "You & Me", "ilovetwice")
    with filepath.open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["tracks"]} == {"iloveloona"}
    with connect(config) as conn:
        cursor = conn.execute("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'")
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
    # And check that auxiliary files were renamed. Create an aux cover art here.
    (source_dir / "!playlists" / "Lala Lisa.jpg").touch(exist_ok=True)

    rename_playlist(config, "Lala Lisa", "Turtle Rabbit")
    assert not (source_dir / "!playlists" / "Lala Lisa.toml").exists()
    assert not (source_dir / "!playlists" / "Lala Lisa.jpg").exists()
    assert (source_dir / "!playlists" / "Turtle Rabbit.toml").exists()
    assert (source_dir / "!playlists" / "Turtle Rabbit.jpg").exists()

    with connect(config) as conn:
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM playlists WHERE name = 'Turtle Rabbit')")
        assert cursor.fetchone()[0]
        cursor = conn.execute("SELECT cover_path FROM playlists WHERE name = 'Turtle Rabbit'")
        assert Path(cursor.fetchone()[0]) == source_dir / "!playlists" / "Turtle Rabbit.jpg"
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM playlists WHERE name = 'Lala Lisa')")
        assert not cursor.fetchone()[0]


def test_edit_playlists_ordering(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    filepath = source_dir / "!playlists" / "Lala Lisa.toml"
    monkeypatch.setattr("rose.playlists.click.edit", lambda x: "\n".join(reversed(x.split("\n"))))
    edit_playlist_in_editor(config, "Lala Lisa")

    with filepath.open("rb") as fp:
        data = tomllib.load(fp)
    assert data["tracks"][0]["uuid"] == "ilovetwice"
    assert data["tracks"][1]["uuid"] == "iloveloona"


def test_edit_playlists_remove_track(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    filepath = source_dir / "!playlists" / "Lala Lisa.toml"
    monkeypatch.setattr("rose.playlists.click.edit", lambda x: x.split("\n")[0])
    edit_playlist_in_editor(config, "Lala Lisa")

    with filepath.open("rb") as fp:
        data = tomllib.load(fp)
    assert len(data["tracks"]) == 1


def test_edit_playlists_duplicate_track_name(monkeypatch: Any, config: Config) -> None:
    """
    When there are duplicate virtual filenames, we append UUID. Check that it works by asserting on
    the seen text and checking that reversing the order works.
    """
    # Generate conflicting virtual tracknames by having two copies of a release in the library.
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / "a")
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / "b")
    update_cache(config)

    with connect(config) as conn:
        # Get the first track of each release.
        cursor = conn.execute("SELECT id FROM tracks WHERE source_path LIKE '%01.m4a'")
        track_ids = [r["id"] for r in cursor]
        assert len(track_ids) == 2

    create_playlist(config, "You & Me")
    for tid in track_ids:
        add_track_to_playlist(config, "You & Me", tid)

    seen = ""

    def editfn(x: str) -> str:
        nonlocal seen
        seen = x
        return "\n".join(reversed(x.split("\n")))

    monkeypatch.setattr("rose.playlists.click.edit", editfn)
    edit_playlist_in_editor(config, "You & Me")

    assert seen == "\n".join([f"[1990-02-05] BLACKPINK - Track 1 [{tid}]" for tid in track_ids])

    with (config.music_source_dir / "!playlists" / "You & Me.toml").open("rb") as fp:
        data = tomllib.load(fp)
    assert data["tracks"][0]["uuid"] == track_ids[1]
    assert data["tracks"][1]["uuid"] == track_ids[0]


def test_playlist_handle_missing_track(config: Config, source_dir: Path) -> None:
    """Test that the lifecycle of the playlist remains unimpeded despite a missing track."""
    filepath = source_dir / "!playlists" / "You & Me.toml"
    with filepath.open("w") as fp:
        fp.write(
            """\
[[tracks]]
uuid = "iloveloona"
description_meta = "lalala"
[[tracks]]
uuid = "ghost"
description_meta = "lalala {MISSING}"
missing = true
"""
        )
    update_cache(config)

    # Assert that adding another track works.
    add_track_to_playlist(config, "You & Me", "ilovetwice")
    with (source_dir / "!playlists" / "You & Me.toml").open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["tracks"]} == {"ghost", "iloveloona", "ilovetwice"}
        assert next(r for r in diskdata["tracks"] if r["uuid"] == "ghost")["missing"]
    with connect(config) as conn:
        cursor = conn.execute("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'")
        assert {r["track_id"] for r in cursor} == {"ghost", "iloveloona", "ilovetwice"}

    # Delete that track.
    remove_track_from_playlist(config, "You & Me", "ilovetwice")
    with filepath.open("rb") as fp:
        diskdata = tomllib.load(fp)
        assert {r["uuid"] for r in diskdata["tracks"]} == {"ghost", "iloveloona"}
        assert next(r for r in diskdata["tracks"] if r["uuid"] == "ghost")["missing"]
    with connect(config) as conn:
        cursor = conn.execute("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'")
        assert {r["track_id"] for r in cursor} == {"ghost", "iloveloona"}

    # And delete the playlist.
    delete_playlist(config, "You & Me")
    assert not filepath.is_file()
    with connect(config) as conn:
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM playlists WHERE name = 'You & Me')")
        assert not cursor.fetchone()[0]


def test_set_playlist_cover_art(isolated_dir: Path, config: Config) -> None:
    imagepath = isolated_dir / "folder.png"
    with imagepath.open("w") as fp:
        fp.write("lalala")

    playlists_dir = config.music_source_dir / "!playlists"
    shutil.copytree(TEST_PLAYLIST_1, playlists_dir)
    (playlists_dir / "Turtle Rabbit.toml").touch()
    (playlists_dir / "Turtle Rabbit.jpg").touch()
    (playlists_dir / "Lala Lisa.txt").touch()
    update_cache(config)

    set_playlist_cover_art(config, "Lala Lisa", imagepath)
    assert (playlists_dir / "Lala Lisa.png").is_file()
    assert not (playlists_dir / "Lala Lisa.jpg").exists()
    assert (playlists_dir / "Lala Lisa.txt").is_file()
    assert len(list(playlists_dir.iterdir())) == 5

    with connect(config) as conn:
        cursor = conn.execute("SELECT cover_path FROM playlists WHERE name = 'Lala Lisa'")
        assert Path(cursor.fetchone()["cover_path"]) == playlists_dir / "Lala Lisa.png"


def test_remove_playlist_cover_art(config: Config) -> None:
    playlists_dir = config.music_source_dir / "!playlists"
    playlists_dir.mkdir()
    (playlists_dir / "Turtle Rabbit.toml").touch()
    (playlists_dir / "Turtle Rabbit.jpg").touch()
    update_cache(config)

    delete_playlist_cover_art(config, "Turtle Rabbit")
    assert not (playlists_dir / "Turtle Rabbit.jpg").exists()
    with connect(config) as conn:
        cursor = conn.execute("SELECT cover_path FROM playlists")
        assert not cursor.fetchone()["cover_path"]
