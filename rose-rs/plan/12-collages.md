# Milestone 12: Collages

## Scope
Virtual collections of releases.

## Components
- Collage CRUD operations
- Release membership management
- Directory structure management

## Required Behaviors
- Stored as `.toml` files in collages directory
- Name conflicts prevented
- Releases tracked by UUID
- Cover art from first release
- Creates directory structure on add

## Functions to Implement
From `collages.py`:
- `collages.rs:create_collage`
- `collages.rs:delete_collage`
- `collages.rs:rename_collage`
- `collages.rs:add_release_to_collage`
- `collages.rs:remove_release_from_collage`
- `collages.rs:edit_collage_in_editor`

## Tests to Implement
From `collages_test.py`:
- `collages_test.rs:test_remove_release_from_collage`
- `collages_test.rs:test_collage_lifecycle`
- `collages_test.rs:test_collage_add_duplicate`
- `collages_test.rs:test_rename_collage`
- `collages_test.rs:test_edit_collages_ordering`
- `collages_test.rs:test_edit_collages_remove_release`
- `collages_test.rs:test_collage_handle_missing_release`

## Critical: This module is missing from consultant's plan

## Python Tests: 7
## Minimum Rust Tests: 7