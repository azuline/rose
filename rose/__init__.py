from rose.audiotags import (
    SUPPORTED_AUDIO_EXTENSIONS,
    AudioTags,
    UnsupportedFiletypeError,
)
from rose.cache import (
    STORED_DATA_FILE_REGEX,
    CachedCollage,
    CachedPlaylist,
    CachedRelease,
    CachedTrack,
    DescriptorEntry,
    GenreEntry,
    LabelEntry,
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
    initialize_logging,
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
    # Plumbing
    "initialize_logging",
    "VERSION",  # TODO: get_version()
    # Errors
    "RoseError",
    "RoseExpectedError",
    "UnsupportedFiletypeError",
    # Utilities
    "sanitize_dirname",
    "sanitize_filename",
    "calculate_release_logtext",  # TODO: Rename.
    "calculate_track_logtext",  # TODO: Rename.
    "STORED_DATA_FILE_REGEX",  # TODO: Revise: is_release_directory() / is_track_file()
    "SUPPORTED_AUDIO_EXTENSIONS",  # TODO: is_supported_audio_file()
    # Configuration
    "Config",
    # Cache
    "maybe_invalidate_cache_database",
    "update_cache",
    "update_cache_for_releases",
    # Tagging
    "AudioTags",
    # Rule Engine
    "MetadataAction",
    "MetadataMatcher",
    "MetadataRule",
    "execute_metadata_rule",
    "execute_stored_metadata_rules",
    "run_actions_on_release",
    "run_actions_on_track",
    # Path Templates
    "PathContext",
    "PathTemplate",
    "eval_release_template",  # TODO: Rename.
    "eval_track_template",  # TODO: Rename.
    "preview_path_templates",
    # Watchdog
    "start_watchdog",  # TODO: Move into its own separate package.
    # Releases
    "CachedRelease",
    "create_single_release",
    "delete_release",
    "delete_release_cover_art",
    "dump_all_releases",
    "dump_release",
    "edit_release",
    "get_release",
    "set_release_cover_art",
    "toggle_release_new",
    # Tracks
    "CachedTrack",
    "dump_all_tracks",
    "dump_track",
    "get_track",
    "get_tracks_associated_with_release",  # TODO: Rename: `get_tracks_of_release` / `dump_release(with_tracks=tracks)`
    # Artists
    "artist_exists",
    "list_artists",
    # Genres
    "GenreEntry",
    "list_genres",
    "genre_exists",
    # Descriptors
    "DescriptorEntry",
    "list_descriptors",
    "descriptor_exists",
    # Labels
    "LabelEntry",
    "list_labels",
    "label_exists",
    # Collages
    "CachedCollage",
    "add_release_to_collage",
    "collage_exists",
    "create_collage",
    "delete_collage",
    "dump_all_collages",
    "dump_collage",
    "edit_collage_in_editor",  # TODO: Move editor part to CLI, make this file-submissions.
    "get_collage",
    "list_collages",
    "remove_release_from_collage",
    "rename_collage",
    # Playlists
    "CachedPlaylist",
    "add_track_to_playlist",
    "list_playlists",
    "playlist_exists",
    "create_playlist",
    "delete_playlist",
    "delete_playlist_cover_art",
    "get_playlist",
    "dump_all_playlists",
    "dump_playlist",
    "edit_playlist_in_editor",  # TODO: Move editor part to CLI, make this file-submissions.
    "get_path_of_track_in_playlist",  # TODO: Redesign.
    "get_playlist_cover_path",  # TODO: Remove.
    "remove_track_from_playlist",
    "rename_playlist",
    "set_playlist_cover_art",
]

initialize_logging(__name__)
