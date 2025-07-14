# Milestone 7: Cache Foundation

## Overview
This milestone implements the SQLite-based cache system that is the heart of Rose's performance. The cache stores all metadata in a normalized relational database with full-text search capabilities.

## Dependencies
- rusqlite (SQLite bindings with bundled SQLite)
- r2d2 (connection pooling - optional but recommended)
- chrono (for timestamp handling)
- rayon (for parallel processing)
- fs2 (for file locking)

## Database Schema

The schema (from `cache.sql`) includes these main tables:
- releases - Album/release metadata
- tracks - Individual track metadata  
- releases_artists - Many-to-many artist relationships
- tracks_artists - Many-to-many artist relationships
- releases_genres - Many-to-many genre relationships
- releases_labels - Many-to-many label relationships
- playlists - Playlist metadata
- playlists_tracks - Playlist track membership
- collages - Collage metadata
- collages_releases - Collage release membership
- Full-text search tables for fast searching

## Implementation Guide (`src/cache.rs`)

### 1. Core Data Models

```rust
#[derive(Debug, Clone)]
pub struct CachedRelease {
    pub id: String,
    pub source_path: PathBuf,
    pub title: String,
    pub release_type: Option<String>,
    pub release_year: Option<i32>,
    pub new: bool,
    pub artists: ArtistMapping,
    pub genres: Vec<String>,
    pub labels: Vec<String>,
    pub catalog_number: Option<String>,
    pub cover_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct CachedTrack {
    pub id: String,
    pub source_path: PathBuf,
    pub title: String,
    pub release_id: String,
    pub track_number: String,
    pub disc_number: String,
    pub duration_seconds: Option<i32>,
    pub artists: ArtistMapping,
}

#[derive(Debug, Clone)]
pub struct CachedPlaylist {
    pub name: String,
    pub tracks: Vec<String>, // Track IDs
}

#[derive(Debug, Clone)]
pub struct CachedCollage {
    pub name: String,
    pub releases: Vec<String>, // Release IDs
}

#[derive(Debug)]
pub struct UpdateCacheResult {
    pub releases_added: usize,
    pub releases_updated: usize,
    pub releases_deleted: usize,
    pub tracks_added: usize,
    pub tracks_updated: usize,
    pub tracks_deleted: usize,
}
```

### 2. Connection Management

```rust
pub fn connect(config: &Config) -> Result<Connection> {
    todo!()
}
```

Steps:
1. Create path to cache.sqlite3 in config.cache_dir
2. Open connection with `Connection::open()`
3. Set pragmas for performance:
   ```sql
   PRAGMA foreign_keys = ON;
   PRAGMA journal_mode = WAL;
   PRAGMA synchronous = NORMAL;
   PRAGMA cache_size = -64000;
   PRAGMA temp_store = MEMORY;
   ```
4. Return connection

### 3. Cache Creation

```rust
pub fn create_cache(config: &Config) -> Result<()> {
    todo!()
}
```

Steps:
1. Create cache directory if doesn't exist
2. Connect to database
3. Execute schema SQL (included from cache_schema.sql)
4. Create full-text search tables

### 4. Cache Update Operations

```rust
pub fn update_cache(config: &Config, force: bool) -> Result<UpdateCacheResult> {
    todo!()
}
```

High-level algorithm:
1. List all directories in music_source_dir
2. Filter to only directories (releases)
3. Call update_cache_for_releases with all dirs

```rust
pub fn update_cache_for_releases(
    config: &Config,
    release_dirs: &[PathBuf],
    force: bool,
) -> Result<UpdateCacheResult> {
    todo!()
}
```

Detailed algorithm:
1. Start transaction
2. For each release directory:
   - Check if needs update (mtime comparison) unless force=true
   - Scan for music files
   - Read metadata from files
   - Compute release metadata from tracks
   - Insert/update in database
3. Find and delete orphaned releases
4. Update full-text search indexes
5. Commit transaction
6. Return statistics

### 5. Query Operations

Implement these query functions:

```rust
pub fn list_releases(config: &Config, matcher: Option<&Matcher>) -> Result<impl Iterator<Item = CachedRelease>> {
    todo!()
}

pub fn get_release(config: &Config, release_id: &str) -> Result<Option<CachedRelease>> {
    todo!()
}

pub fn list_tracks(config: &Config, matcher: Option<&Matcher>) -> Result<impl Iterator<Item = CachedTrack>> {
    todo!()
}

pub fn get_track(config: &Config, track_id: &str) -> Result<Option<CachedTrack>> {
    todo!()
}

// ... and many more
```

### 6. Update Algorithm Details

#### Scanning Phase
1. Read directory for music files
2. Check each file's mtime against cached mtime
3. Build list of files needing update

#### Metadata Reading Phase
1. Read audio tags for each track
2. Extract all metadata fields
3. Generate or read track UUID
4. Handle missing/invalid metadata gracefully

#### Release Computation Phase
1. Aggregate track metadata:
   - Album artist = most common track artist
   - Release year = most common year
   - Genres = union of track genres
   - Labels = union of track labels
2. Determine release type (album, single, EP, etc.)
3. Find cover art file using regexes

#### Database Update Phase
1. Use prepared statements for efficiency
2. Batch inserts where possible
3. Update many-to-many relationships
4. Clean up orphaned relationships

### 7. Full-Text Search

Create FTS5 virtual tables:
```sql
CREATE VIRTUAL TABLE releases_fts USING fts5(
    id UNINDEXED,
    title,
    artists,
    genres,
    labels
);
```

For substring matching, index individual characters as "words".

## Test Implementation Guide (`src/cache_test.rs`)

This module has the most tests (78 total). Group them logically:

### Basic Operations (10 tests)
- `test_schema` - Verify schema creation
- `test_migration` - Test schema migrations
- `test_locks` - Test file locking
- `test_update_cache_all` - Basic update
- `test_update_cache_multiprocessing` - Parallel updates
- etc.

### Release Management (20 tests)
- Various update scenarios
- ID preservation
- Source path changes
- Empty directories
- Orphan cleanup

### Metadata Handling (15 tests)
- Artist aliases
- Genre hierarchies
- Multi-value tags
- Date handling
- Release type detection

### Query Operations (20 tests)
- List operations
- Get operations
- Search operations
- Relationship queries

### Collections (13 tests)
- Playlist operations
- Collage operations
- Update propagation

## Important Implementation Details

### 1. UUID Management
- Generate UUIDs for new tracks/releases
- Preserve existing UUIDs from tags
- Write UUIDs back to audio files

### 2. Transaction Management
- Use transactions for all updates
- Rollback on any error
- Progress callback support

### 3. Performance Optimizations
- Prepared statement caching
- Batch operations
- Parallel file scanning (if max_proc > 1)
- Minimal file I/O

### 4. Incremental Updates
- Only re-read changed files
- Use mtime for change detection
- Force flag overrides

### 5. Artist Alias Resolution
- Apply artist aliases during queries
- Store original names in database
- Apply aliases in getter methods

### 6. Full-Text Search
- Character-based indexing for substring search
- Handle special characters
- Case-insensitive searching

## Validation Checklist

- [ ] All 78 tests pass
- [ ] Python cache files can be read
- [ ] Incremental updates work correctly
- [ ] Parallel updates don't corrupt data
- [ ] Memory usage is reasonable
- [ ] Query performance is good