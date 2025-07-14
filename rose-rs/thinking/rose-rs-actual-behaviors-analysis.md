# Rose Python Implementation Analysis

## Core Behaviors and Features

### 1. Common Module (`common.py`)
**Key Features:**
- Version management from `.version` file
- Error hierarchy with `RoseError` base and `RoseExpectedError` for user-facing errors
- Artist and ArtistMapping dataclasses with aliasing support
- Filesystem sanitization functions with configurable max filename bytes
- SHA256 hashing for dataclasses (used for cache invalidation)
- Logging initialization with file/stderr output and log rotation

**Edge Cases:**
- Filesystem sanitization handles illegal characters, Unicode normalization (NFD), and length limits
- File extension preservation in `sanitize_filename` (ignores extensions > 6 chars as "bullshit")
- Artist deduplication in `ArtistMapping.all` property
- Platform-specific log directory handling (XDG on Linux, different on macOS)

### 2. Configuration System (`config.py`)
**Key Features:**
- TOML-based configuration with detailed validation
- Nested configuration structures (VirtualFSConfig, PathTemplateConfig)
- Artist aliases with bidirectional mapping
- Path templates with Jinja2 support
- Stored metadata rules for batch operations
- Whitelist/blacklist support for artists, genres, descriptors, labels

**Edge Cases:**
- Cannot specify both whitelist and blacklist for same entity type
- Unrecognized config keys generate warnings (not errors)
- Config changes invalidate cache via hash comparison
- Default values for optional fields (cache_dir, max_proc, etc.)
- Path expansion for home directories

### 3. Audio Tags (`audiotags.py`)
**Key Features:**
- Multi-format support: MP3 (ID3), M4A (MP4), FLAC, Vorbis, Opus
- Rose-specific tags (ROSEID, ROSERELEASEID) for stable identifiers
- Complex artist role handling (main, guest, remixer, producer, composer, conductor, djmixer)
- Tag splitting with multiple delimiters (`\\`, `/`, `;`, `vs.`)
- Release type normalization and validation
- Parent genre encoding in tags (optional)

**Edge Cases:**
- Year/date parsing handles multiple formats
- Track/disc number parsing from "X/Y" format in ID3
- Case-insensitive release type matching
- Special artist string parsing ("feat.", "pres.", "performed by")
- Preservation of existing track/disc totals in MP4
- ID3 paired frame handling for producer/DJ roles

### 4. Cache System (`cache.py`)
**Key Features:**
- SQLite-based with automatic schema migration
- Multiprocessing support with configurable process count
- Locking mechanism with timeouts
- Stable UUID-based identifiers for releases and tracks
- Incremental updates based on mtime
- Virtual filesystem support data

**Edge Cases:**
- In-progress directory detection (missing `.rose.{uuid}.toml` but files have IDs)
- Collision handling for directory renames
- Empty directory cleanup after file renames
- Track ID preservation across cache rebuilds
- Performance optimizations (batch operations, minimal file access)
- Force flag to bypass mtime checks

### 5. Release Management (`releases.py`)
**Key Features:**
- CRUD operations on releases
- Cover art management with validation
- Metadata editing via TOML with resume capability
- Rule-based bulk operations
- Toggle new/not-new status

**Edge Cases:**
- Failed edits save resume file for retry
- Cover art validation by extension
- Automatic collage/playlist updates on deletion
- Optimized rule matching for common filters
- Track total recalculation during edits

### 6. Track Management (`tracks.py`)
**Key Features:**
- Rule-based track operations
- Optimized lookups for common filters
- Integration with audio tag writing

**Edge Cases:**
- Minimal compared to releases - tracks are mostly managed through their parent releases

### 7. Collages and Playlists
**Key Features:**
- TOML-based storage in `!collages` and `!playlists` directories
- UUID-based references to releases/tracks
- Cover art support
- Order preservation and editing
- Description metadata for human readability

**Edge Cases:**
- Duplicate entry prevention
- Lock handling for concurrent access
- Adjacent file renaming (cover arts)
- UUID disambiguation for duplicate track descriptions

### 8. Rules Engine (`rules.py`, `rule_parser.py`)
**Key Features:**
- DSL for matching and modifying tags
- Pattern matching with start/end anchors and case insensitivity
- Multiple action types: replace, sed, split, add, delete
- Fast path optimizations for exact matches
- Batch operations with dry-run support

**Edge Cases:**
- Escape handling in patterns (`\^`, `\$`)
- Multi-value vs single-value tag constraints
- Action validation (e.g., split only on multi-value tags)
- Slash replacement for filesystem-unsafe artists

### 9. Templates (`templates.py`)
**Key Features:**
- Jinja2-based path templating
- Context-aware templates (genre, artist, label, etc.)
- Custom filters (artistsfmt, arrayfmt, sortorder, etc.)
- Position support for ordered collections

**Edge Cases:**
- Lazy initialization for performance
- Spacing collapse in rendered output
- Null date handling
- Artist formatting with role-based prefixes/suffixes

### 10. Virtual Filesystem (not analyzed here but referenced)
**Key Features:**
- Multiple view types (by artist, genre, label, etc.)
- Whitelist/blacklist filtering
- "Hide with only new releases" option

## Test Coverage Insights

### Well-Tested Areas:
1. **Cache operations**: UUID generation, mtime-based updates, multiprocessing
2. **Audio tag reading/writing**: All formats, special tags, artist parsing
3. **Configuration parsing**: Validation, defaults, error messages
4. **Release editing**: Success and failure paths, resume functionality
5. **Rules engine**: Pattern matching, action execution
6. **Template rendering**: Various contexts and edge cases

### Edge Cases Explicitly Tested:
1. Empty directory handling during cache updates
2. In-progress directory creation detection
3. File rename collision handling
4. Lock timeout and retry behavior
5. Schema migration on cache invalidation
6. Tag normalization and validation
7. Resume file generation on edit failure

### Error Handling Patterns:
1. Hierarchical exceptions with user-friendly messages
2. File not found handling during concurrent operations
3. Lock timeouts with retry logic
4. Validation at parse time vs. execution time
5. Graceful handling of malformed data

## Implementation Considerations for Rust

### Data Integrity:
- UUID-based stable identifiers prevent data loss during renames
- Locking prevents concurrent modifications
- Cache invalidation on schema/config changes
- Automatic datafile creation for untracked releases

### Performance Optimizations:
- Mtime-based change detection
- Batch SQL operations
- Multiprocessing for large updates
- Lazy template compilation
- Fast path for common rule patterns

### User Experience:
- Detailed error messages with context
- Progress logging for long operations
- Dry-run support for dangerous operations
- Resume capability for failed edits
- Warning (not failure) on unrecognized config

### Filesystem Safety:
- Character sanitization for all platforms
- Length limits with extension preservation
- Unicode normalization
- Collision detection and numbering

## Missing from Current Plans

Based on this analysis, some aspects that might be missing from the Rust implementation plan:

1. **Datafile versioning** - The Python code updates datafiles in place
2. **Multiprocessing thresholds** - Different strategies for <50 vs >=50 releases
3. **Platform-specific paths** - Different log locations for macOS vs Linux
4. **Lock retry logic** - Not just timeout but active retry
5. **Resume file handling** - For failed release edits
6. **Adjacent file handling** - Cover art renaming with collages/playlists
7. **In-progress directory detection** - Important for concurrent tools
8. **Tag format-specific quirks** - ID3 paired frames, MP4 tuple format
9. **Genre parent resolution** - Both transitive and encoding in tags
10. **Performance-critical paths** - Where to focus optimization efforts