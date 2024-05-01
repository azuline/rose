import logging
import logging.handlers
import os
import sys
from pathlib import Path

import appdirs

from rose.audiotags import (
    SUPPORTED_AUDIO_EXTENSIONS,
    AudioTags,
    UnsupportedFiletypeError,
)
from rose.cache import (
    STORED_DATA_FILE_REGEX,
    CachedRelease,
    CachedTrack,
    artist_exists,
    calculate_release_logtext,
    calculate_track_logtext,
    collage_exists,
    descriptor_exists,
    genre_exists,
    get_collage,
    get_path_of_track_in_playlist,
    get_playlist,
    get_playlist_cover_path,
    get_release,
    get_track,
    get_tracks_associated_with_release,
    label_exists,
    list_artists,
    list_collages,
    list_descriptors,
    list_genres,
    list_labels,
    list_playlists,
    maybe_invalidate_cache_database,
    playlist_exists,
    update_cache,
    update_cache_for_releases,
)
from rose.collages import (
    add_release_to_collage,
    create_collage,
    delete_collage,
    dump_all_collages,
    dump_collage,
    edit_collage_in_editor,
    remove_release_from_collage,
    rename_collage,
)
from rose.common import (
    VERSION,
    RoseError,
    RoseExpectedError,
    sanitize_dirname,
    sanitize_filename,
)
from rose.config import Config
from rose.playlists import (
    add_track_to_playlist,
    create_playlist,
    delete_playlist,
    delete_playlist_cover_art,
    dump_all_playlists,
    dump_playlist,
    edit_playlist_in_editor,
    remove_track_from_playlist,
    rename_playlist,
    set_playlist_cover_art,
)
from rose.releases import (
    create_single_release,
    delete_release,
    delete_release_cover_art,
    dump_all_releases,
    dump_release,
    edit_release,
    run_actions_on_release,
    set_release_cover_art,
    toggle_release_new,
)
from rose.rule_parser import MetadataAction, MetadataMatcher, MetadataRule
from rose.rules import execute_metadata_rule, execute_stored_metadata_rules
from rose.templates import (
    PathContext,
    PathTemplate,
    eval_release_template,
    eval_track_template,
    preview_path_templates,
)
from rose.tracks import dump_all_tracks, dump_track, run_actions_on_track
from rose.watcher import start_watchdog

__all__ = [
    "AudioTags",
    "CachedRelease",
    "CachedTrack",
    "Config",
    "MetadataAction",
    "MetadataMatcher",
    "MetadataRule",
    "PathTemplate",
    "RoseError",
    "RoseExpectedError",
    "STORED_DATA_FILE_REGEX",  # TODO: Revise: is_release_directory / is_track_file
    "SUPPORTED_AUDIO_EXTENSIONS",
    "UnsupportedFiletypeError",
    "VERSION",
    "add_release_to_collage",
    "add_track_to_playlist",
    "artist_exists",
    "calculate_release_logtext",  # TODO: Rename.
    "calculate_track_logtext",  # TODO: Rename.
    "collage_exists",
    "create_collage",
    "create_playlist",
    "create_single_release",
    "delete_collage",
    "delete_playlist",
    "delete_playlist_cover_art",
    "delete_release",
    "delete_release_cover_art",
    "descriptor_exists",
    "dump_all_collages",
    "dump_all_playlists",
    "PathContext",
    "dump_all_releases",
    "dump_all_tracks",
    "dump_collage",
    "dump_playlist",
    "dump_release",
    "dump_track",
    "edit_collage_in_editor",  # TODO: Move editor part to CLI, make this file-submissions.
    "edit_playlist_in_editor",  # TODO: Move editor part to CLI, make this file-submissions.
    "edit_release",
    "eval_release_template",  # TODO: Rename.
    "eval_track_template",  # TODO: Rename.
    "execute_metadata_rule",
    "execute_stored_metadata_rules",
    "genre_exists",
    "get_collage",
    "get_path_of_track_in_playlist",  # TODO: Redesign.
    "get_playlist",
    "get_playlist_cover_path",  # TODO: Remove.
    "get_release",
    "get_track",
    "get_tracks_associated_with_release",  # TODO: Rename: `get_tracks_of_release` / `dump_release(with_tracks=tracks)`
    "label_exists",
    "list_artists",
    "list_collages",
    "list_descriptors",
    "list_genres",
    "list_labels",
    "list_playlists",
    "maybe_invalidate_cache_database",
    "playlist_exists",
    "preview_path_templates",
    "remove_release_from_collage",
    "remove_track_from_playlist",
    "rename_collage",
    "rename_playlist",
    "run_actions_on_release",
    "run_actions_on_track",
    "sanitize_dirname",
    "sanitize_filename",
    "set_playlist_cover_art",
    "set_release_cover_art",
    "start_watchdog",
    "toggle_release_new",
    "update_cache",
    "update_cache_for_releases",
]

__logging_initialized = False


def initialize_logging() -> None:
    global __logging_initialized
    if __logging_initialized:
        return
    __logging_initialized = True

    logger = logging.getLogger()
    logger.setLevel(logging.INFO)

    # appdirs by default has Unix log to $XDG_CACHE_HOME, but I'd rather write logs to $XDG_STATE_HOME.
    log_home = Path(appdirs.user_state_dir("rose"))
    if appdirs.system == "darwin":
        log_home = Path(appdirs.user_log_dir("rose"))

    log_home.mkdir(parents=True, exist_ok=True)
    log_file = log_home / "rose.log"

    # Useful for debugging problems with the virtual FS, since pytest doesn't capture that debug logging
    # output.
    log_despite_testing = os.environ.get("LOG_TEST", False)

    # Add a logging handler for stdout unless we are testing. Pytest
    # captures logging output on its own, so by default, we do not attach our own.
    if "pytest" not in sys.modules or log_despite_testing:  # pragma: no cover
        simple_formatter = logging.Formatter(
            "[%(asctime)s] %(levelname)s: %(message)s",
            datefmt="%H:%M:%S",
        )
        verbose_formatter = logging.Formatter(
            "[ts=%(asctime)s.%(msecs)03d] [pid=%(process)d] [src=%(name)s:%(lineno)s] %(levelname)s: %(message)s",
            datefmt="%Y-%m-%d %H:%M:%S",
        )

        stream_handler = logging.StreamHandler(sys.stderr)
        stream_handler.setFormatter(
            simple_formatter if not log_despite_testing else verbose_formatter
        )
        logger.addHandler(stream_handler)

        file_handler = logging.handlers.RotatingFileHandler(
            log_file,
            maxBytes=20 * 1024 * 1024,
            backupCount=10,
        )
        file_handler.setFormatter(verbose_formatter)
        logger.addHandler(file_handler)


initialize_logging()
