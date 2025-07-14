# Milestone 9: Releases Management

## Overview
This milestone implements high-level operations for managing releases (albums, singles, EPs, etc.). It provides functions for creating, editing, deleting releases and managing their metadata.

## Dependencies
- cache module (for database operations)
- audiotags module (for tag writing)
- templates module (for path generation)
- rule_parser module (for find operations)
- External: EDITOR environment variable for interactive editing

## Implementation Guide (`src/releases.rs`)

### 1. Release Creation

```rust
pub fn create_release(config: &Config, source_dir: &Path) -> Result<String> {
    todo!()
}
```

This is primarily used internally when scanning directories.

Steps:
1. Scan directory for music files
2. Read metadata from tracks
3. Compute release metadata
4. Generate release UUID
5. Create .rose.{uuid}.toml file
6. Update cache
7. Return release ID

### 2. Single Release Creation

```rust
pub fn create_single_release(
    config: &Config,
    artist: &str,
    title: &str,
    track_paths: &[PathBuf],
) -> Result<String> {
    todo!()
}
```

Create a "virtual" single from individual tracks:

Steps:
1. Validate all tracks exist and are music files
2. Create a virtual release directory name
3. Copy or link tracks to virtual directory
4. Set release type to "single"
5. Override artist/title metadata
6. Create release in cache
7. Return release ID

### 3. Release Deletion

```rust
pub fn delete_release(config: &Config, release_id: &str) -> Result<()> {
    todo!()
}
```

Delete release and optionally its files:

Steps:
1. Get release from cache
2. Remove from any collages
3. Delete tracks from playlists
4. Delete .rose.*.toml file
5. Optionally move directory to trash
6. Remove from cache

```rust
pub fn delete_release_ignore_fs(config: &Config, release_id: &str) -> Result<()> {
    todo!()
}
```

Same as above but only removes from cache, leaves files untouched.

### 4. Release Editing

```rust
pub fn edit_release(config: &Config, release_id: &str) -> Result<()> {
    todo!()
}
```

Interactive metadata editing:

Steps:
1. Get release from cache with all tracks
2. Serialize to TOML format:
   ```toml
   title = "Album Title"
   release_type = "album"
   release_year = 2023
   new = false
   
   [[artists]]
   name = "Artist Name"
   role = "main"
   
   [[genres]]
   name = "K-Pop"
   
   [[labels]]
   name = "Label Name"
   
   [[tracks]]
   id = "track-uuid"
   title = "Track Title"
   disc_number = "1"
   track_number = "01"
   
   [[tracks.artists]]
   name = "Track Artist"
   role = "main"
   ```
3. Open in $EDITOR
4. Parse edited content
5. Validate changes
6. Apply to cache and files
7. Handle errors gracefully

### 5. Cover Art Management

```rust
pub fn set_release_cover_art(
    config: &Config,
    release_id: &str,
    cover_path: Option<&Path>,
) -> Result<()> {
    todo!()
}
```

Set or remove cover art:

Steps for setting:
1. Validate image file (jpg, png)
2. Copy to release directory as "cover.jpg" or similar
3. Update cache with cover path
4. Optionally embed in audio files

Steps for removing (cover_path = None):
1. Delete cover file
2. Clear cache cover path
3. Optionally remove from audio files

### 6. New Flag Management

```rust
pub fn toggle_release_new(config: &Config, release_id: &str) -> Result<()> {
    todo!()
}
```

Toggle the "new" status:

Steps:
1. Get current status from cache
2. Toggle boolean
3. Update cache
4. Update .rose.*.toml file

### 7. Single Track Extraction

```rust
pub fn extract_single_release(
    config: &Config,
    track_id: &str,
) -> Result<String> {
    todo!()
}
```

Create a single release from one track:

Steps:
1. Get track info from cache
2. Create virtual single with track
3. Use track artist as release artist
4. Set release type to "single"
5. Return new release ID

### 8. Rule-Based Operations

```rust
pub fn run_action_on_release(
    config: &Config,
    release_id: &str,
    action: &crate::rule_parser::Action,
) -> Result<()> {
    todo!()
}
```

Apply a single action to a release:

Steps:
1. Get release from cache
2. Apply action to metadata
3. Update cache
4. Write changes to files

```rust
pub fn find_matching_releases(
    config: &Config,
    matcher: &crate::rule_parser::Matcher,
) -> Result<Vec<String>> {
    todo!()
}
```

Find releases matching a pattern:

Steps:
1. Use fast search from rules module
2. Return list of release IDs

## Test Implementation Guide (`src/releases_test.rs`)

### Basic Operations (4 tests)

#### `test_delete_release`
- Create test release
- Delete it
- Verify removed from cache
- Verify files handled correctly

#### `test_toggle_release_new`
- Create release with new=false
- Toggle new flag
- Verify new=true
- Toggle again
- Verify new=false

#### `test_set_release_cover_art`
- Create release without cover
- Set cover art
- Verify cover file created
- Verify cache updated

#### `test_remove_release_cover_art`
- Create release with cover
- Remove cover (set to None)
- Verify file deleted
- Verify cache updated

### Editing (2 tests)

#### `test_edit_release`
- Create release
- Mock editor to modify TOML
- Verify changes applied
- Check both cache and files

#### `test_edit_release_failure_and_resume`
- Test error handling during edit
- Verify can resume/retry
- Ensure no partial updates

### Single Extraction (2 tests)

#### `test_extract_single_release`
- Create multi-track release
- Extract one track as single
- Verify single created correctly
- Check virtual directory handling

#### `test_extract_single_release_with_trailing_space`
- Test with track title having trailing space
- Ensure proper trimming
- Verify clean single creation

### Rule Operations (2 tests)

#### `test_run_action_on_release`
- Create release
- Apply action (e.g., change genre)
- Verify change in cache and files

#### `test_find_matching_releases`
- Create multiple releases
- Search with matcher
- Verify correct releases found

## TOML Format for Editing

The edit format should be human-friendly:

```toml
# Release metadata
title = "The Album"
release_type = "album"  # album, single, ep, compilation, etc.
release_year = 2023
new = true

# Artists (can have multiple)
[[artists]]
name = "BLACKPINK"
role = "main"  # main, guest, remixer, producer, composer, conductor, djmixer

# Genres (can have multiple)
[[genres]]
name = "K-Pop"

[[genres]]
name = "Dance-Pop"

# Labels (can have multiple)
[[labels]]
name = "YG Entertainment"

# Tracks
[[tracks]]
id = "550e8400-e29b-41d4-a716-446655440000"  # Don't change IDs
title = "How You Like That"
disc_number = "1"
track_number = "01"

[[tracks.artists]]
name = "BLACKPINK"
role = "main"

[[tracks]]
id = "550e8400-e29b-41d4-a716-446655440001"
title = "Pretty Savage"
disc_number = "1"
track_number = "02"

[[tracks.artists]]
name = "BLACKPINK"
role = "main"
```

## Important Implementation Details

### 1. File System Safety
- Use trash/recycle bin instead of permanent deletion
- Validate paths to prevent directory traversal
- Handle missing files gracefully

### 2. Transaction Consistency
- Database and file system changes should be atomic
- Rollback on any error
- No partial updates

### 3. Virtual Releases
- Singles extracted from tracks are "virtual"
- Store in special directory structure
- Handle cleanup of orphaned virtuals

### 4. Editor Integration
- Respect $EDITOR environment variable
- Fall back to common editors (vim, nano, notepad)
- Handle editor errors gracefully
- Validate edited content before applying

### 5. Cover Art Handling
- Support common formats: jpg, jpeg, png
- Standardize filename (e.g., always "cover.jpg")
- Thumbnail generation (future enhancement)

### 6. Change Propagation
- Changes to releases affect tracks
- Update collages when release deleted
- Update playlists when tracks affected

## Validation Checklist

- [ ] All 10 tests pass
- [ ] File operations are safe
- [ ] Editor integration works
- [ ] Virtual singles handled correctly
- [ ] Cover art management works
- [ ] Changes propagate correctly