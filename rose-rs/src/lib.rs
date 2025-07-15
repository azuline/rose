// Allow dead code warnings during development
#![allow(dead_code)]

// Internal modules - not exposed to library users
mod audiotags;
mod cache;
mod common;
mod config;
mod errors;
mod genre_hierarchy;
mod rule_parser;
mod templates;

#[cfg(test)]
mod testing;

// Re-export public API
// From errors module
pub use errors::{Result, RoseError, RoseExpectedError};

// From common module
pub use common::{
    // Functions
    initialize_logging,
    sanitize_dirname,
    sanitize_filename,
    // Core types
    Artist,
    ArtistMapping,
    RoseDate,
    // Constants
    VERSION,
};

// From rule_parser module
pub use rule_parser::{
    Action,
    ActionBehavior,
    AddAction,
    DeleteAction,
    ExpandableTag,
    // Errors
    InvalidRuleError,
    Matcher,
    Pattern,
    // Action types
    ReplaceAction,
    // Core types
    Rule,
    RuleSyntaxError,
    SedAction,
    SplitAction,
    // Tag types
    Tag,
};

// From config module
pub use config::{get_config_path, Config, VirtualFSConfig};

// From audiotags module
pub use audiotags::{
    format_artist_string, parse_artist_string, AudioTags, UnsupportedFiletypeError, UnsupportedTagValueTypeError, SUPPORTED_AUDIO_EXTENSIONS,
    SUPPORTED_RELEASE_TYPES,
};

// from rose.audiotags import (
//     SUPPORTED_AUDIO_EXTENSIONS,
//     AudioTags,
//     RoseDate,
//     UnsupportedFiletypeError,
//     UnsupportedTagValueTypeError,
// )
// from rose.cache import (
//     Collage,
//     DescriptorEntry,
//     GenreEntry,
//     LabelEntry,
//     Playlist,
//     Release,
//     Track,
//     artist_exists,
//     collage_lock_name,
//     descriptor_exists,
//     genre_exists,
//     get_collage,
//     get_collage_releases,
//     get_playlist,
//     get_playlist_tracks,
//     get_release,
//     get_track,
//     get_tracks_of_release,
//     get_tracks_of_releases,
//     label_exists,
//     list_artists,
//     list_collages,
//     list_descriptors,
//     list_genres,
//     list_labels,
//     list_playlists,
//     list_releases,
//     list_tracks,
//     lock,
//     make_release_logtext,
//     make_track_logtext,
//     maybe_invalidate_cache_database,
//     playlist_lock_name,
//     release_lock_name,
//     release_within_collage,
//     track_within_playlist,
//     track_within_release,
//     update_cache,
//     update_cache_evict_nonexistent_collages,
//     update_cache_evict_nonexistent_playlists,
//     update_cache_evict_nonexistent_releases,
//     update_cache_for_collages,
//     update_cache_for_playlists,
//     update_cache_for_releases,
// )
// from rose.collages import (
//     CollageAlreadyExistsError,
//     CollageDoesNotExistError,
//     DescriptionMismatchError,
//     add_release_to_collage,
//     create_collage,
//     delete_collage,
//     edit_collage_in_editor,
//     remove_release_from_collage,
//     rename_collage,
// )
// from rose.common import (
//     VERSION,
//     Artist,
//     ArtistDoesNotExistError,
//     ArtistMapping,
//     DescriptorDoesNotExistError,
//     GenreDoesNotExistError,
//     LabelDoesNotExistError,
//     RoseError,
//     RoseExpectedError,
//     initialize_logging,
//     sanitize_dirname,
//     sanitize_filename,
// )
// from rose.config import (
//     Config,
//     ConfigDecodeError,
//     ConfigNotFoundError,
//     InvalidConfigValueError,
//     MissingConfigKeyError,
// )
// from rose.playlists import (
//     PlaylistAlreadyExistsError,
//     PlaylistDoesNotExistError,
//     add_track_to_playlist,
//     create_playlist,
//     delete_playlist,
//     delete_playlist_cover_art,
//     edit_playlist_in_editor,
//     remove_track_from_playlist,
//     rename_playlist,
//     set_playlist_cover_art,
// )
// from rose.releases import (
//     InvalidCoverArtFileError,
//     ReleaseDoesNotExistError,
//     ReleaseEditFailedError,
//     UnknownArtistRoleError,
//     create_single_release,
//     delete_release,
//     delete_release_cover_art,
//     edit_release,
//     find_releases_matching_rule,
//     run_actions_on_release,
//     set_release_cover_art,
//     toggle_release_new,
// )
// from rose.rule_parser import (
//     Action,
//     AddAction,
//     DeleteAction,
//     InvalidRuleError,
//     Matcher,
//     Pattern,
//     ReplaceAction,
//     Rule,
//     SedAction,
//     SplitAction,
// )
// from rose.rules import (
//     InvalidReplacementValueError,
//     TrackTagNotAllowedError,
//     execute_metadata_rule,
//     execute_stored_metadata_rules,
// )
// from rose.templates import (
//     InvalidPathTemplateError,
//     PathContext,
//     PathTemplate,
//     evaluate_release_template,
//     evaluate_track_template,
//     get_sample_music,
// )
// from rose.tracks import TrackDoesNotExistError, find_tracks_matching_rule, run_actions_on_track

// __all__ = [
//     # Plumbing
//     "initialize_logging",
//     "VERSION",
//     "RoseError",
//     "RoseExpectedError",
//     "DescriptionMismatchError",
//     "InvalidCoverArtFileError",
//     "ReleaseDoesNotExistError",
//     "ReleaseEditFailedError",
//     # Utilities
//     "sanitize_dirname",
//     "sanitize_filename",
//     "make_release_logtext",
//     "make_track_logtext",
//     "SUPPORTED_AUDIO_EXTENSIONS",
//     # Configuration
//     "Config",
//     "ConfigNotFoundError",
//     "ConfigDecodeError",
//     "MissingConfigKeyError",
//     "InvalidConfigValueError",
//     # Cache
//     "maybe_invalidate_cache_database",
//     "update_cache",
//     "update_cache_evict_nonexistent_collages",
//     "update_cache_evict_nonexistent_playlists",
//     "update_cache_evict_nonexistent_releases",
//     "update_cache_for_collages",
//     "update_cache_for_playlists",
//     "update_cache_for_releases",
//     # Locks
//     "lock",
//     "release_lock_name",
//     "collage_lock_name",
//     "playlist_lock_name",
//     # Tagging
//     "AudioTags",
//     "RoseDate",
//     "UnsupportedFiletypeError",
//     "UnsupportedTagValueTypeError",
//     # Rule Engine
//     "Action",
//     "Matcher",
//     "Rule",
//     "Pattern",
//     "ReplaceAction",
//     "SedAction",
//     "SplitAction",
//     "AddAction",
//     "DeleteAction",
//     "execute_metadata_rule",
//     "execute_stored_metadata_rules",
//     "run_actions_on_release",
//     "run_actions_on_track",
//     "InvalidRuleError",
//     "InvalidReplacementValueError",
//     "TrackTagNotAllowedError",
//     # Path Templates
//     "PathContext",
//     "PathTemplate",
//     "evaluate_release_template",
//     "evaluate_track_template",
//     "get_sample_music",
//     "InvalidPathTemplateError",
//     # Releases
//     "Release",
//     "create_single_release",
//     "delete_release",
//     "delete_release_cover_art",
//     "edit_release",
//     "list_releases",
//     "find_releases_matching_rule",
//     "get_release",
//     "set_release_cover_art",
//     "toggle_release_new",
//     # Tracks
//     "Track",
//     "get_track",
//     "find_tracks_matching_rule",
//     "list_tracks",
//     "get_tracks_of_release",
//     "get_tracks_of_releases",
//     "track_within_release",
//     "TrackDoesNotExistError",
//     # Artists
//     "Artist",
//     "ArtistMapping",
//     "artist_exists",
//     "list_artists",
//     "UnknownArtistRoleError",
//     "ArtistDoesNotExistError",
//     # Genres
//     "GenreEntry",
//     "list_genres",
//     "genre_exists",
//     "GenreDoesNotExistError",
//     # Descriptors
//     "DescriptorEntry",
//     "list_descriptors",
//     "descriptor_exists",
//     "DescriptorDoesNotExistError",
//     # Labels
//     "LabelEntry",
//     "list_labels",
//     "label_exists",
//     "LabelDoesNotExistError",
//     # Collages
//     "Collage",
//     "add_release_to_collage",
//     "create_collage",
//     "delete_collage",
//     "edit_collage_in_editor",  # TODO: Move editor part to CLI, make this file-submissions.
//     "get_collage",
//     "get_collage_releases",
//     "list_collages",
//     "remove_release_from_collage",
//     "release_within_collage",
//     "rename_collage",
//     "CollageDoesNotExistError",
//     "CollageAlreadyExistsError",
//     # Playlists
//     "Playlist",
//     "add_track_to_playlist",
//     "list_playlists",
//     "create_playlist",
//     "delete_playlist",
//     "delete_playlist_cover_art",
//     "get_playlist",
//     "get_playlist_tracks",
//     "edit_playlist_in_editor",  # TODO: Move editor part to CLI, make this file-submissions.
//     "track_within_playlist",
//     "remove_track_from_playlist",
//     "rename_playlist",
//     "set_playlist_cover_art",
//     "PlaylistDoesNotExistError",
//     "PlaylistAlreadyExistsError",
// ]

// initialize_logging(__name__, output="file")
