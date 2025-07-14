# Milestone 10: Tracks Management

## Overview
This milestone implements track-level operations. While releases are the primary organization unit in Rose, individual track management is needed for playlists and specific metadata operations.

## Dependencies
- cache module (for database operations)
- audiotags module (for tag writing)
- rule_parser module (for find operations)
- releases module (for track-release relationships)

## Implementation Guide (`src/tracks.rs`)

### 1. Track Deletion

```rust
pub fn delete_track(config: &Config, track_id: &str) -> Result<()> {
    todo!()
}
```

Delete a track and optionally its file:

Steps:
1. Get track from cache
2. Remove from any playlists
3. Check if last track in release
   - If yes, consider deleting release
4. Delete audio file (move to trash)
5. Remove from cache
6. Update release metadata if needed

```rust
pub fn delete_track_ignore_fs(config: &Config, track_id: &str) -> Result<()> {
    todo!()
}
```

Same as above but only removes from cache, leaves file untouched.

### 2. Track Editing

```rust
pub fn edit_track(config: &Config, track_id: &str) -> Result<()> {
    todo!()
}
```

Interactive metadata editing for a single track:

Steps:
1. Get track from cache
2. Serialize to TOML format:
   ```toml
   title = "Track Title"
   disc_number = "1"
   track_number = "01"
   
   [[artists]]
   name = "Artist Name"
   role = "main"
   
   # Release context (read-only)
   [release]
   id = "release-uuid"
   title = "Album Title"
   ```
3. Open in $EDITOR
4. Parse edited content
5. Validate changes
6. Apply to cache and file
7. Update release metadata if needed

### 3. Rule-Based Operations

```rust
pub fn run_action_on_track(
    config: &Config,
    track_id: &str,
    action: &crate::rule_parser::Action,
) -> Result<()> {
    todo!()
}
```

Apply a single action to a track:

Steps:
1. Get track from cache
2. Apply action to metadata
3. Update cache
4. Write changes to audio file
5. Update release metadata if needed

```rust
pub fn find_matching_tracks(
    config: &Config,
    matcher: &crate::rule_parser::Matcher,
) -> Result<Vec<String>> {
    todo!()
}
```

Find tracks matching a pattern:

Steps:
1. Use fast search from rules module
2. Filter to track-level matches
3. Return list of track IDs

## Test Implementation Guide (`src/tracks_test.rs`)

### `test_run_action_on_track`

Test applying an action to a track:

1. Create test release with tracks
2. Apply action to one track (e.g., change title)
3. Verify change in cache
4. Verify change in audio file
5. Verify other tracks unchanged

### `test_find_matching_tracks`

Test track searching:

1. Create releases with various tracks
2. Search with different matchers:
   - By title
   - By artist
   - By track number
   - Combined criteria
3. Verify correct tracks found
4. Test boundary cases

## Track-Specific Considerations

### 1. Track Numbers

Track numbers can have various formats:
- Simple: "1", "2", "3"
- Padded: "01", "02", "03"
- With total: "1/12", "2/12"
- Complex: "A1", "B2" (vinyl)

Parsing rules:
- Extract numeric portion for sorting
- Preserve original format for display
- Handle non-numeric gracefully

### 2. Disc Numbers

Similar to track numbers:
- Usually simple numbers
- Sometimes "CD1", "CD2"
- Preserve format

### 3. Track Artists vs Album Artists

Tracks can have different artists than the release:
- Compilation albums
- Featured artists
- Remixers

Rules:
- Track artist overrides album artist
- If no track artist, inherit from release
- Maintain both in cache

### 4. Orphaned Tracks

Tracks without a valid release:
- Can happen during failed operations
- Should be cleaned up
- Or assigned to "Unknown Release"

## TOML Format for Track Editing

Keep it simple for single track:

```toml
# Track metadata
title = "Song Title"
disc_number = "1"
track_number = "01"

# Track artists
[[artists]]
name = "Main Artist"
role = "main"

[[artists]]
name = "Featured Artist"
role = "guest"

# Read-only context
[release]
id = "550e8400-e29b-41d4-a716-446655440000"
title = "Album Title"
artist = "Album Artist"
```

## Integration with Releases

### Metadata Propagation

When track metadata changes:
1. Check if it affects release metadata
2. Update release artists if needed
3. Update release genres if needed
4. Recompute release type if needed

### Consistency Rules

1. All tracks in a release should have:
   - Same album tag
   - Same date/year
   - Consistent disc numbering

2. Rose can fix inconsistencies:
   - Normalize album names
   - Propagate release year
   - Fix disc numbers

## Important Implementation Details

### 1. Performance

Track operations should be fast:
- Use prepared statements
- Batch updates when possible
- Only write changed files

### 2. File Handling

- Validate file exists before operations
- Handle missing files gracefully
- Use appropriate error types

### 3. Cache Consistency

- Track and release data must stay in sync
- Use transactions for multi-table updates
- Validate foreign keys

### 4. Playlist Updates

When track metadata changes:
- Playlist membership unaffected
- But displayed info should update
- Cache playlist description metadata

## Error Handling

Common error scenarios:
1. Track file missing
2. Track file corrupted
3. No write permissions
4. Release doesn't exist
5. Invalid metadata values

Each should have specific error messages.

## Validation Checklist

- [ ] Both tests pass
- [ ] Track edits propagate correctly
- [ ] File operations are safe
- [ ] Search is fast and accurate
- [ ] Consistency with releases maintained
- [ ] Error messages are helpful