# Milestone 8: Cache Core

## Scope
SQLite database operations and schema management.

## Components
- Schema creation and migrations
- Release/track CRUD operations
- Many-to-many relationship handling
- Full-text search setup
- Update tracking and scanning

## Required Behaviors
- Track totals calculated, not stored
- Null duration treated as 0
- Multiprocessing kicks in at 50+ releases
- Lock files with active retry (not just timeout)
- Skip in-progress directories (`.in-progress.*`)
- FTS5 for full-text search
- Transaction per release during updates
- Preserve existing UUIDs

## Functions to Implement
From `cache.py`:
- `cache.rs:connect`
- `cache.rs:maybe_invalidate_cache_database`
- `cache.rs:lock`
- `cache.rs:update_cache`
- `cache.rs:update_cache_for_releases`
- `cache.rs:update_cache_for_collages`
- `cache.rs:update_cache_for_playlists`
- `cache.rs:filter_releases`
- `cache.rs:filter_tracks`
- `cache.rs:list_releases`
- `cache.rs:get_release`
- `cache.rs:get_release_logtext`
- `cache.rs:list_tracks`
- `cache.rs:get_track`
- `cache.rs:get_tracks_of_release`
- `cache.rs:track_within_release`
- `cache.rs:track_within_playlist`
- `cache.rs:release_within_collage`
- `cache.rs:list_playlists`
- `cache.rs:get_playlist`
- `cache.rs:list_collages`
- `cache.rs:get_collage`
- `cache.rs:list_artists`
- `cache.rs:artist_exists`
- `cache.rs:list_genres`
- `cache.rs:genre_exists`
- `cache.rs:list_descriptors`
- `cache.rs:descriptor_exists`
- `cache.rs:list_labels`
- `cache.rs:label_exists`

## Tests to Implement (selected key tests from 65 total):
- `cache_test.rs:test_schema`
- `cache_test.rs:test_migration`
- `cache_test.rs:test_locks`
- `cache_test.rs:test_update_cache_all`
- `cache_test.rs:test_update_cache_multiprocessing`
- `cache_test.rs:test_update_cache_releases`
- `cache_test.rs:test_update_cache_releases_preserves_track_ids_across_rebuilds`
- `cache_test.rs:test_update_cache_releases_writes_ids_to_tags`
- `cache_test.rs:test_update_cache_releases_ignores_partially_written_directory`
- `cache_test.rs:test_update_cache_releases_updates_full_text_search`
- `cache_test.rs:test_update_cache_collages`
- `cache_test.rs:test_update_cache_playlists`
- `cache_test.rs:test_list_releases`
- `cache_test.rs:test_get_release_and_associated_tracks`
- `cache_test.rs:test_get_release_applies_artist_aliases`
- `cache_test.rs:test_artist_exists`
- `cache_test.rs:test_genre_exists`
- `cache_test.rs:test_unpack`

## Python Tests: 65
## Minimum Rust Tests: 65