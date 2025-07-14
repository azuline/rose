# Rose Rust Implementation Plan - Missing Behaviors Addendum

This document supplements the existing milestone plans with critical behaviors, edge cases, and implementation details discovered through analysis of the Python codebase and test suite.

## Cross-Cutting Concerns

### 1. Datafile Management
**Missing from plans:**
- `.rose.{uuid}.toml` files contain only `new` and `added_at` fields
- Datafiles are automatically created when missing
- Datafile upgrades: The cache update process updates datafiles to add missing default values
- Lock acquisition is only needed when writing datafiles, not reading (performance optimization)

**Implementation notes:**
```rust
struct StoredDataFile {
    new: bool,
    added_at: String, // ISO8601 timestamp
}
```

### 2. Platform-Specific Behavior
**Missing from plans:**
- Log directory differs between platforms:
  - Linux/Unix: `$XDG_STATE_HOME/rose/` 
  - macOS: `$XDG_LOG_HOME/rose/`
- Use `dirs` crate for proper XDG directory resolution

### 3. Performance Thresholds
**Missing from plans:**
- Cache updates use different strategies based on release count:
  - < 50 releases: Process in same thread/process
  - >= 50 releases: Use multiprocessing with batch_size = count / max_proc + 1
- This avoids multiprocessing overhead for small operations

## Module-Specific Additions

### Common Module Additions

**Filesystem sanitization edge cases:**
```rust
fn sanitize_dirname(name: &str, max_bytes: usize, enforce_maxlen: bool) -> String {
    // Replace illegal chars with _
    // If enforce_maxlen, truncate UTF-8 safely to max_bytes
    // Apply Unicode NFD normalization
}

fn sanitize_filename(name: &str, max_bytes: usize, enforce_maxlen: bool) -> String {
    // Same as dirname but preserve extension
    // If extension > 6 bytes, treat as part of filename
    // Example: "file.verylongextension" -> entire string subject to truncation
}
```

**SHA256 hashing for dataclasses:**
```rust
fn sha256_dataclass<T: Serialize>(value: &T) -> String {
    // Recursively hash with sorted keys for consistency
    // Used for cache invalidation detection
}
```

### Configuration Module Additions

**Config hash for cache invalidation:**
```rust
// These fields affect cache population and require cache rebuild on change:
struct CacheHashFields {
    music_source_dir: PathBuf,
    cache_dir: PathBuf, 
    cover_art_stems: Vec<String>,
    valid_art_exts: Vec<String>,
    ignore_release_directories: Vec<String>,
}
```

**Validation behavior:**
- Unrecognized config keys generate warnings via logger, not errors
- DFS traversal to find all unrecognized keys in nested structures

### Audio Tags Module Additions

**Format-specific quirks not in plans:**

1. **ID3 (MP3) paired frames:**
   - TIPL/IPLS frames store role/artist pairs
   - Extract producer and DJ-mix roles from these frames

2. **MP4 track/disc number handling:**
   - Must be stored as `Vec<(u32, u32)>` (current, total)
   - Preserve existing totals when updating

3. **Tag splitting patterns:**
   - Split on: `\\`, ` / `, `; `, ` vs. `
   - Parent genre encoding: `genre1;genre2\\PARENTS:\\parent1;parent2`

4. **Artist string parsing patterns:**
   ```
   "A feat. B" -> main: [A], guest: [B]
   "A pres. B" -> djmixer: [A], main: [B]
   "A performed by B" -> composer: [A], main: [B]
   ```

### Cache Module Additions

**Critical behaviors:**

1. **In-progress directory detection:**
   ```rust
   fn is_in_progress_directory(dir: &Path, force: bool) -> bool {
       // If .rose.{uuid}.toml missing but audio files have release_id tag
       // AND force is false -> skip directory
       // Prevents processing directories during copy operations
   }
   ```

2. **Rename collision handling:**
   ```rust
   fn find_available_name(base: &str, exists_fn: impl Fn(&str) -> bool) -> String {
       // Try base name
       // If exists, try "base [2]", "base [3]", etc.
       // Account for max_filename_bytes when adding collision number
   }
   ```

3. **Empty directory cleanup:**
   ```rust
   fn cleanup_empty_dirs(path: &Path, root: &Path) {
       // After moving files, remove empty parent directories
       // Stop at root (release directory)
   }
   ```

4. **Lock retry behavior:**
   ```rust
   async fn acquire_lock_with_retry(name: &str, timeout: Duration) -> Result<Lock> {
       loop {
           match try_acquire_lock(name, timeout).await {
               Ok(lock) => return Ok(lock),
               Err(_) => {
                   let sleep_time = get_lock_expiry(name) - now();
                   sleep(sleep_time).await;
               }
           }
       }
   }
   ```

### Releases Module Additions

**Edit failure and resume:**
```rust
struct ResumeFile {
    path: PathBuf, // cache_dir/failed-release-edit.{uuid}.toml
    content: String, // The edited TOML that failed
}

fn edit_release_with_resume(release_id: &str, resume_file: Option<&Path>) -> Result<()> {
    // If resume_file provided, use its content
    // On failure, save edited TOML to resume file
    // Include resume command in error message
}
```

**Cover art validation:**
- Extensions must be from config.valid_art_exts
- Delete all existing cover art files before setting new one
- Cover art files are those matching config.cover_art_stems + valid_art_exts

### Rules Engine Additions

**Fast path optimizations:**
```rust
fn can_use_fast_path(matcher: &Matcher) -> Option<FastPathQuery> {
    // If pattern is exact match (^...$) and single tag type:
    // - artist -> all_artist_filter
    // - releaseartist -> release_artist_filter  
    // - genre -> genre_filter
    // - label -> label_filter
    // - descriptor -> descriptor_filter
    // - releasetype -> release_type_filter
}
```

**Action validation edge cases:**
- Split action only valid on multi-value tags
- Add action only valid on multi-value tags
- Replace on single-value tags cannot contain delimiters
- Delete sets value to None/empty, not removes from database

### Collages/Playlists Additions

**File organization:**
```
!collages/
  ├── CollectionName.toml      # The collage data
  └── CollectionName.jpg        # Optional cover art
  
!playlists/
  ├── PlaylistName.toml         # The playlist data  
  └── PlaylistName.png          # Optional cover art
```

**Adjacent file handling:**
```rust
fn rename_adjacent_files(old_stem: &str, new_stem: &str, dir: &Path) {
    // Find all files with old_stem but different extensions
    // Rename them to new_stem keeping extension
    // Skip if new name already exists
}
```

**Editor integration:**
- Playlist duplicate handling: Append `[{uuid}]` to description if needed
- Line-based editing where each line maps to UUID via stored mapping

## Testing Considerations

### Edge Cases to Test

1. **Concurrent operations:**
   - Multiple cache updates on same release
   - Directory deletion during cache update
   - File modification during read

2. **Filesystem limits:**
   - Filenames at exactly max_bytes
   - Unicode characters that expand when normalized
   - Invalid UTF-8 sequences in paths

3. **Data consistency:**
   - UUID preservation across cache rebuilds
   - Datafile upgrades with missing fields
   - Lock expiry during long operations

4. **Error recovery:**
   - Resume file generation and parsing
   - Partial cache update rollback
   - Corrupt datafile handling

## Migration and Compatibility

### Datafile Compatibility
- Must read old datafiles with missing fields
- Add default values during read
- Write back complete datafiles

### Cache Database Compatibility  
- Schema version checking via exact hash match
- Config version checking for cache-affecting fields
- Rose version checking
- Any mismatch triggers full rebuild

### Path Template Compatibility
- Must produce identical output to Python/Jinja2
- Special handling for None/empty values
- Whitespace collapsing rules

## Performance Considerations

### Hot Paths to Optimize

1. **Cache update mtime checks:**
   - Skip files with unchanged mtime
   - Batch SQL operations
   - Reuse prepared statements

2. **Rule matching:**
   - Fast path for exact matches
   - Index on commonly searched fields
   - Avoid regex compilation in loops

3. **Template rendering:**
   - Cache compiled templates
   - Lazy initialization
   - Avoid string allocations

### Memory Management

1. **Large collections:**
   - Stream results instead of loading all
   - Use iterators over vectors where possible
   - Clear intermediate collections

2. **Multiprocessing boundaries:**
   - Serialize only necessary data
   - Use shared memory for large readonly data
   - Clean up after subprocess completion