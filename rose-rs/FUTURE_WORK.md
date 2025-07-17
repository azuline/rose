# Future Work for Cache.rs Implementation

## Overview

This document outlines the exact differences in behavior between the Python reference implementation and the current Rust implementation, focusing on the 10 failing tests. Each section describes the missing functionality and provides a specific implementation plan.

## Missing Features Analysis

### 1. Description Metadata Auto-Update System

**Missing in Rust**: The Python implementation automatically updates `description_meta` fields in collage/playlist TOML files whenever releases/tracks are updated in the cache.

**Python Behavior**:
- When updating collages/playlists, it queries the database for current release/track metadata
- Formats description_meta as: `[date] Artist - Title` (with optional `{MISSING}` suffix)
- Writes back to TOML file if any description_meta changed
- This happens during `update_cache_for_collages` and `update_cache_for_playlists`

**Implementation Required**:
```rust
// In update_cache_for_collages:
1. After checking missing status, query releases_view for all release IDs
2. Build description_meta using format: "[{date}] {artists} - {title}"
3. Compare with existing description_meta in TOML
4. If different, update and set data_changed = true
5. Add "{MISSING}" suffix for missing releases

// Similar logic for update_cache_for_playlists with tracks_view
```

### 2. Cascading Updates After Release/Track Changes

**Missing in Rust**: The Python implementation schedules collage/playlist updates when their member releases/tracks change.

**Python Behavior**:
- In `execute_cache_updates`, after updating releases/tracks, it queries for affected collages/playlists
- Calls `update_cache_for_collages` and `update_cache_for_playlists` with `force=True` for affected items
- This ensures description_meta stays synchronized

**Implementation Required**:
```rust
// In execute_cache_updates:
1. After updating releases, query: 
   SELECT DISTINCT collage_name FROM collages_releases WHERE release_id IN (updated_ids)
2. After updating tracks, query:
   SELECT DISTINCT playlist_name FROM playlists_tracks WHERE track_id IN (updated_ids)
3. Call update_cache_for_collages/playlists with force=true for these names
```

### 3. Full-Text Search (FTS) Table Updates

**Missing in Rust**: The FTS tables aren't being properly updated when releases/tracks change.

**Python Behavior**:
- Deletes FTS entries for updated tracks/releases
- Re-inserts with processed strings using `process_string_for_fts`
- Uses custom delimiter `☆` for multi-value fields

**Implementation Required**:
```rust
// In execute_cache_updates:
1. Before inserting releases/tracks:
   DELETE FROM rules_engine_fts WHERE rowid IN (affected track rowids)
2. After inserting:
   INSERT INTO rules_engine_fts with all fields processed through process_string_for_fts
3. Ensure process_string_for_fts handles the ☆ delimiter correctly
```

### 4. Nested Directory Flattening During Rename

**Missing in Rust**: When renaming track files, Python moves them to the release root and cleans up empty directories.

**Python Behavior**:
- Track template evaluation produces just a filename (no path)
- Files are moved from nested directories to release root
- Empty parent directories are recursively removed
- Uses `relpath` to track nested structure

**Implementation Required**:
```rust
// In rename_source_files section:
1. Calculate relative path from release root
2. Move file to release_root.join(wanted_filename)
3. After rename, walk up parent directories:
   - Check if directory is empty
   - If empty, remove it
   - Continue until non-empty parent or release root
```

### 5. Track Deletion Detection

**Missing in Rust**: The system doesn't properly detect and remove tracks that have been deleted from disk.

**Python Behavior**:
- Maintains `unknown_cached_tracks` set during directory scan
- Adds all cached track paths initially
- Removes paths as files are found
- Remaining paths are deleted tracks
- Deletes these tracks: `DELETE FROM tracks WHERE release_id = ? AND source_path IN (?)`

**Implementation Required**:
```rust
// In _update_cache_for_releases_executor:
1. Before scanning files, query all cached tracks for the release
2. Create HashSet of cached track paths
3. As each file is processed, remove from set
4. After scanning, remaining paths are deleted tracks
5. Add to upd_unknown_cached_tracks_args for deletion
```

### 6. Playlist Track Association Preservation

**Missing in Rust**: When releases are renamed/moved, playlist track associations may be lost.

**Python Behavior**:
- Track IDs are stable across renames
- Playlist associations use track IDs, not paths
- The issue might be in how track updates are handled

**Implementation Required**:
```rust
// Verify that:
1. Track IDs remain stable during directory renames
2. Track path updates don't trigger ID changes
3. Playlist-track associations aren't deleted during updates
```

### 7. Multiprocessing Missing Detection

**Missing in Rust**: Force multiprocessing mode has issues with missing detection.

**Python Behavior**:
- Uses `collages_to_force_update_receiver` and `playlists_to_force_update_receiver`
- Accumulates updates across processes
- Applies updates after all processes complete

**Implementation Required**:
```rust
// In multiprocessing mode:
1. Collect collage/playlist names that need updates
2. Pass them through Arc<Mutex<Vec<String>>> or similar
3. After all batches complete, run force updates on collected names
```

## Task List

### Phase 1: Core Missing Features (High Priority)
1. **Implement description_meta updates** (fixes 4 tests)
   - [ ] Add description_meta query and formatting in update_cache_for_collages
   - [ ] Add description_meta query and formatting in update_cache_for_playlists
   - [ ] Add "{MISSING}" suffix handling
   - [ ] Ensure TOML files are written back when changed

2. **Implement cascading updates** (supports description_meta)
   - [ ] Query affected collages after release updates
   - [ ] Query affected playlists after track updates
   - [ ] Call force updates on affected collections

3. **Fix FTS updates** (fixes 1 test)
   - [ ] Delete old FTS entries before updates
   - [ ] Insert new FTS entries after updates
   - [ ] Verify process_string_for_fts implementation

### Phase 2: File Management (Medium Priority)
4. **Implement track deletion detection** (fixes 1 test)
   - [ ] Track cached vs actual files
   - [ ] Delete missing tracks from database
   - [ ] Add to upd_unknown_cached_tracks_args

5. **Implement nested directory flattening** (fixes 1 test)
   - [ ] Move files to release root during rename
   - [ ] Recursively clean empty directories
   - [ ] Update track paths correctly

### Phase 3: Edge Cases (Lower Priority)
6. **Fix playlist-release associations** (fixes 1 test)
   - [ ] Verify track ID stability
   - [ ] Check association preservation logic
   - [ ] Add tests for edge cases

7. **Fix multiprocessing synchronization** (fixes 2 tests)
   - [ ] Implement proper collection name accumulation
   - [ ] Ensure force updates run after all batches
   - [ ] Add proper synchronization primitives

## Testing Strategy

1. **Unit Tests**: Add tests for each new function (description_meta formatting, FTS updates)
2. **Integration Tests**: Ensure the 10 failing tests pass
3. **Performance Tests**: Verify no significant performance regression
4. **Edge Case Tests**: Add tests for race conditions and error cases

## Estimated Effort

- Phase 1: 2-3 days (most critical, fixes 5 tests)
- Phase 2: 1-2 days (fixes 2 tests)
- Phase 3: 1-2 days (fixes 3 tests)
- Total: 4-7 days to achieve 100% test parity

## Success Criteria

1. All 73 cache.rs tests pass
2. No performance regression compared to Python implementation
3. Code maintains Rust idioms and safety guarantees
4. Comprehensive documentation for complex logic