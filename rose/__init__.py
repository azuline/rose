from rose.audiotags import (
    SUPPORTED_AUDIO_EXTENSIONS,
    AudioTags,
    RoseDate,
    UnsupportedFiletypeError,
)
from rose.cache import (
    Collage,
    DescriptorEntry,
    GenreEntry,
    LabelEntry,
    Playlist,
    Release,
    Track,
    artist_exists,
    collage_lock_name,
    descriptor_exists,
    genre_exists,
    get_collage,
    get_collage_releases,
    get_playlist,
    get_playlist_tracks,
    get_release,
    get_track,
    get_tracks_of_release,
    label_exists,
    list_artists,
    list_collages,
    list_descriptors,
    list_genres,
    list_labels,
    list_playlists,
    lock,
    make_release_logtext,
    make_track_logtext,
    maybe_invalidate_cache_database,
    playlist_lock_name,
    release_lock_name,
    release_within_collage,
    track_within_playlist,
    track_within_release,
    update_cache,
    update_cache_evict_nonexistent_collages,
    update_cache_evict_nonexistent_playlists,
    update_cache_evict_nonexistent_releases,
    update_cache_for_collages,
    update_cache_for_playlists,
    update_cache_for_releases,
)
from rose.collages import (
    add_release_to_collage,
    create_collage,
    delete_collage,
    edit_collage_in_editor,
    remove_release_from_collage,
    rename_collage,
)
from rose.common import (
    VERSION,
    Artist,
    ArtistMapping,
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
    edit_playlist_in_editor,
    remove_track_from_playlist,
    rename_playlist,
    set_playlist_cover_art,
)
from rose.releases import (
    create_single_release,
    delete_release,
    delete_release_cover_art,
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
    evaluate_release_template,
    evaluate_track_template,
    get_sample_music,
)
from rose.tracks import run_actions_on_track

__all__ = [
    # Plumbing
    "initialize_logging",
    "VERSION",
    # Errors
    "RoseError",
    "RoseExpectedError",
    "UnsupportedFiletypeError",
    # Utilities
    "sanitize_dirname",
    "sanitize_filename",
    "make_release_logtext",
    "make_track_logtext",
    "SUPPORTED_AUDIO_EXTENSIONS",
    # Configuration
    "Config",
    # Cache
    "maybe_invalidate_cache_database",
    "update_cache",
    "update_cache_evict_nonexistent_collages",
    "update_cache_evict_nonexistent_playlists",
    "update_cache_evict_nonexistent_releases",
    "update_cache_for_collages",
    "update_cache_for_playlists",
    "update_cache_for_releases",
    # Locks
    "lock",
    "release_lock_name",
    "collage_lock_name",
    "playlist_lock_name",
    # Tagging
    "AudioTags",
    "RoseDate",
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
    "evaluate_release_template",
    "evaluate_track_template",
    "get_sample_music",
    # Releases
    "Release",
    "create_single_release",
    "delete_release",
    "delete_release_cover_art",
    "edit_release",
    "get_release",
    "set_release_cover_art",
    "toggle_release_new",
    # Tracks
    "Track",
    "get_track",
    "get_tracks_of_release",
    "track_within_release",
    # Artists
    "Artist",
    "ArtistMapping",
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
    "Collage",
    "add_release_to_collage",
    "create_collage",
    "delete_collage",
    "edit_collage_in_editor",  # TODO: Move editor part to CLI, make this file-submissions.
    "get_collage",
    "get_collage_releases",
    "list_collages",
    "remove_release_from_collage",
    "release_within_collage",
    "rename_collage",
    # Playlists
    "Playlist",
    "add_track_to_playlist",
    "list_playlists",
    "create_playlist",
    "delete_playlist",
    "delete_playlist_cover_art",
    "get_playlist",
    "get_playlist_tracks",
    "edit_playlist_in_editor",  # TODO: Move editor part to CLI, make this file-submissions.
    "track_within_playlist",
    "remove_track_from_playlist",
    "rename_playlist",
    "set_playlist_cover_art",
]

initialize_logging(__name__)
