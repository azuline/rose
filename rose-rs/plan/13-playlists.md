# Milestone 13: Playlists

## Scope
M3U8 playlist generation for releases and collages.

## Components
- M3U8 format generation
- Relative path handling
- Playlist metadata
- Automatic regeneration

## Required Behaviors
- Extended M3U8 format (#EXTM3U)
- Track info includes artist - title
- Relative paths from playlist location
- Updates when cache changes
- Handles missing tracks gracefully
- Sorted by disc/track number

## Functions to Implement
From `playlists.py`:
- `playlists.rs:create_playlist`
- `playlists.rs:delete_playlist`
- `playlists.rs:rename_playlist`
- `playlists.rs:remove_track_from_playlist`
- `playlists.rs:add_track_to_playlist`
- `playlists.rs:edit_playlist_in_editor`
- `playlists.rs:set_playlist_cover_art`
- `playlists.rs:delete_playlist_cover_art`
- `playlists.rs:playlist_path`

## Tests to Implement
From `playlists_test.py`:
- `playlists_test.rs:test_remove_track_from_playlist`
- `playlists_test.rs:test_playlist_lifecycle`
- `playlists_test.rs:test_playlist_add_duplicate`
- `playlists_test.rs:test_rename_playlist`
- `playlists_test.rs:test_edit_playlists_ordering`
- `playlists_test.rs:test_edit_playlists_remove_track`
- `playlists_test.rs:test_edit_playlists_duplicate_track_name`
- `playlists_test.rs:test_playlist_handle_missing_track`
- `playlists_test.rs:test_set_playlist_cover_art`
- `playlists_test.rs:test_remove_playlist_cover_art`

## Critical: This module is missing from consultant's plan

## Python Tests: 10
## Minimum Rust Tests: 10