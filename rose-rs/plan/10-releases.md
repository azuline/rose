# Milestone 10: Releases

## Scope
High-level operations on music releases.

## Components
- Release creation and editing
- TOML-based interactive editing
- Cover art management
- Virtual singles creation
- Datafile integration

## Required Behaviors
- Edit via TOML with $EDITOR
- Validate edited data before applying
- Cover art: first image in directory
- Cover art names case-insensitive
- Singles: Virtual releases from tracks
- Delete uses trash, not permanent
- Handles new/unknown artists
- Creates collages/playlists as needed

## Functions to Implement
From `releases.py`:
- `releases.rs:find_releases_matching_rule`
- `releases.rs:delete_release`
- `releases.rs:toggle_release_new`
- `releases.rs:create_release`
- `releases.rs:edit_release`
- `releases.rs:set_release_cover_art`
- `releases.rs:delete_release_cover_art`
- `releases.rs:run_actions_on_release`
- `releases.rs:create_single_release`

## Tests to Implement
From `releases_test.py`:
- `releases_test.rs:test_delete_release`
- `releases_test.rs:test_toggle_release_new`
- `releases_test.rs:test_set_release_cover_art`
- `releases_test.rs:test_remove_release_cover_art`
- `releases_test.rs:test_edit_release`
- `releases_test.rs:test_edit_release_failure_and_resume`
- `releases_test.rs:test_extract_single_release`
- `releases_test.rs:test_extract_single_release_with_trailing_space`
- `releases_test.rs:test_run_action_on_release`
- `releases_test.rs:test_find_matching_releases`

## Python Tests: 10
## Minimum Rust Tests: 10