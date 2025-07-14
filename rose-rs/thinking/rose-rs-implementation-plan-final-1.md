# Rose-rs Implementation Plan - Final 1

## Executive Summary

This document provides a comprehensive, checkpoint-based plan for reimplementing Rose in Rust. Each checkpoint includes:
- Complete list of tests to port from Python
- Exact functions/modules to implement
- Dependencies and validation criteria
- Test data requirements

The plan follows strict Test-Driven Development with proper dependency ordering to avoid refactoring.

## Project Structure

```
rose-rs/
├── Cargo.toml (workspace)
├── rose-core/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── common.rs
│       ├── config.rs
│       ├── genre_hierarchy.rs
│       ├── audiotags/
│       ├── cache/
│       ├── rules/
│       └── templates/
├── rose-cli/
│   ├── Cargo.toml
│   └── src/
├── rose-vfs/
│   ├── Cargo.toml
│   └── src/
├── rose-watch/
│   ├── Cargo.toml
│   └── src/
└── tests/
    ├── common/
    ├── testdata/ (copied from rose-py)
    └── integration/
```

## Core Dependencies

```toml
[workspace.dependencies]
# Core
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"
thiserror = "1.0"
anyhow = "1.0"

# Database
rusqlite = { version = "0.31", features = ["bundled", "backup", "chrono"] }
r2d2 = "0.8"
r2d2_sqlite = "0.24"

# Audio
lofty = "0.18"  # Evaluate during implementation
id3 = "1.13"    # Fallback for specific ID3 features

# CLI
clap = { version = "4.5", features = ["derive", "cargo", "env"] }
indicatif = "0.17"

# Templates
tera = "1.19"

# Async/Parallel
tokio = { version = "1.37", features = ["full"] }
rayon = "1.10"

# Utils
walkdir = "2.5"
regex = "1.10"
lazy_static = "1.4"
chrono = "0.4"
uuid = { version = "1.8", features = ["v4", "serde"] }
tempfile = "3.10"
which = "6.0"

# Testing
proptest = "1.4"
criterion = "0.5"
pretty_assertions = "1.4"
serial_test = "3.1"
```

## Phase 1: Foundation Layer (Week 1)

### Checkpoint 1.1: Common Types and Utilities

#### Tests to Port
From `rose-py/rose/common.py` inline tests and usage:
```rust
// tests/common_test.rs
#[test]
fn test_artist_new() { /* Artist::new("Name") */ }

#[test]
fn test_artist_with_alias() { /* Artist::new("Name").with_alias(true) */ }

#[test]
fn test_artist_mapping_new() { /* ArtistMapping::new() */ }

#[test]
fn test_artist_mapping_builder() { /* Build with all roles */ }

#[test]
fn test_valid_uuid() { /* valid_uuid("valid-uuid-here") */ }

#[test]
fn test_invalid_uuid() { /* valid_uuid("not-a-uuid") */ }

#[test]
fn test_sanitize_filename_basic() { /* "a/b" -> "a_b" */ }

#[test]
fn test_sanitize_filename_dots() { /* ".." -> "_" */ }

#[test]
fn test_sanitize_filename_unicode() { /* Test unicode handling */ }

#[test]
fn test_error_hierarchy() { /* Test all error types */ }
```

#### Implementation Checklist
`rose-core/src/common.rs`:
```rust
// Data structures
pub struct Artist {
    pub name: String,
    pub alias: bool,
}

pub struct ArtistMapping {
    pub main: Vec<Artist>,
    pub guest: Vec<Artist>,
    pub remixer: Vec<Artist>,
    pub producer: Vec<Artist>,
    pub composer: Vec<Artist>,
    pub conductor: Vec<Artist>,
    pub djmixer: Vec<Artist>,
}

// From cache_test.py test data - these are the actual constants
pub const SUPPORTED_AUDIO_EXTENSIONS: &[&str] = &["mp3", "m4a", "ogg", "opus", "flac"];
pub const SUPPORTED_IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png"];

// Utility functions
pub fn valid_uuid(s: &str) -> bool { }
pub fn sanitize_filename(s: &str) -> String { }
pub fn musicfile(p: &Path) -> bool { }
pub fn imagefile(p: &Path) -> bool { }

// Error types matching Python hierarchy
#[derive(Error, Debug)]
pub enum RoseError {
    #[error("Config not found at {path}")]
    ConfigNotFound { path: PathBuf },
    
    #[error("Config decode error: {message}")]
    ConfigDecode { message: String },
    
    #[error("Invalid path template: {message}")]
    InvalidPathTemplate { message: String },
    
    #[error("Unsupported audio format: {format}")]
    UnsupportedAudioFormat { format: String },
    
    #[error("Tag not allowed: {tag}")]
    TagNotAllowed { tag: String },
    
    #[error("Unknown artist role: {role}")]
    UnknownArtistRole { role: String },
    
    #[error("File not found: {path}")]
    FileNotFound { path: PathBuf },
    
    #[error("IO error: {message}")]
    Io { message: String, source: io::Error },
}

// Type aliases from Python
pub type ArtistSet = Vec<Artist>;  // Python uses list[Artist]
```

### Checkpoint 1.2: Genre Hierarchy

#### Tests to Port
From usage in `cache_test.py` and genre behavior:
```rust
// tests/genre_hierarchy_test.rs
#[test]
fn test_genres_list_length() { 
    // From genre_hierarchy.py: ~24,000 genres
    assert!(GENRES.len() > 20000);
}

#[test]
fn test_genre_exists() {
    assert!(GENRES.contains(&"K-Pop"));
    assert!(GENRES.contains(&"Dance-Pop"));
}

#[test]
fn test_genre_parents() {
    // From cache_test.py test_genre_parent_genres_not_assigned
    let parents = GENRE_PARENTS.get("K-Pop").unwrap();
    assert!(parents.contains(&"Pop"));
}

#[test]
fn test_no_unknown_genres() {
    // Unknown genres should not exist in hierarchy
    assert!(!GENRES.contains(&"Unknown"));
}
```

#### Implementation Checklist
`rose-core/src/genre_hierarchy.rs`:
```rust
// Generate from rose-py/rose/genre_hierarchy.py
lazy_static! {
    pub static ref GENRES: Vec<&'static str> = vec![
        // Port the 24,000+ genre list
    ];
    
    pub static ref GENRE_PARENTS: HashMap<&'static str, Vec<&'static str>> = {
        // Port the parent relationships
    };
}

// Helper to check if genre exists
pub fn genre_exists(genre: &str) -> bool { }

// Get all parent genres (recursive)
pub fn get_all_parents(genre: &str) -> Vec<&str> { }
```

## Phase 2: Configuration System (Week 2, Days 1-3)

### Checkpoint 2.1: Configuration Loading and Validation

#### Tests to Port
From `config_test.py`:
```rust
// tests/config_test.rs
#[test]
fn test_config_full() {
    // Test with full configuration from testdata
    let config_content = r#"
        music_source_dir = "~/.music-source"
        cache_dir = "~/.cache/rose"
        # ... full config
    "#;
}

#[test]
fn test_config_minimal() {
    // Only music_source_dir is required
    let config_content = r#"
        music_source_dir = "~/.music-source"
    "#;
}

#[test]
fn test_config_not_found() {
    // ConfigNotFoundError when no config exists
}

#[test]
fn test_config_path_templates_error() {
    // Invalid template syntax should error
}

#[test]
fn test_config_validate_artist_aliases_resolve_to_self() {
    // "X" -> "X" alias should error
}

#[test]
fn test_config_validate_duplicate_artist_aliases() {
    // Duplicate aliases should error
}

// Additional tests from Python implementation
#[test]
fn test_parse_config_filesystem_changes() {
    // Test path_templates parsing
}

#[test]
fn test_config_expanded_paths() {
    // Test ~ expansion in paths
}
```

#### Implementation Checklist
`rose-core/src/config.rs`:
```rust
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub music_source_dir: PathBuf,
    
    #[serde(default = "default_cache_dir")]
    pub cache_dir: PathBuf,
    
    #[serde(default = "default_max_proc")]
    pub max_proc: usize,
    
    #[serde(rename = "artist_aliases", default)]
    pub artist_aliases_raw: Vec<ArtistAlias>,
    
    #[serde(rename = "rules", default)]
    pub stored_rules: Vec<StoredRule>,
    
    #[serde(default)]
    pub path_templates: PathTemplates,
    
    #[serde(default)]
    pub cover_art_regexes: Vec<String>,
    
    #[serde(default = "default_multi_disc_flag")]
    pub multi_disc_toggle_flag: String,
    
    // Computed fields (not in TOML)
    #[serde(skip)]
    pub artist_aliases: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PathTemplates {
    #[serde(default = "default_release_template")]
    pub release: String,
    
    #[serde(default = "default_track_template")]
    pub track: String,
    
    #[serde(default = "default_all_pattern")]
    pub all_patterns: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ArtistAlias {
    pub artist: String,
    pub alias: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StoredRule {
    pub name: String,
    pub rule: String,
}

// Functions to implement
pub fn parse(config_path_override: Option<&Path>) -> Result<Config> {
    // 1. Find config file (XDG_CONFIG_HOME, ~/.config/rose/config.toml)
    // 2. Read and parse TOML
    // 3. Expand ~ in paths
    // 4. Validate artist aliases
    // 5. Compile regexes
    // 6. Build computed fields
}

fn find_config_file() -> Result<PathBuf> { }
fn validate_artist_aliases(aliases: &[ArtistAlias]) -> Result<()> { }
fn expand_tilde(path: &Path) -> PathBuf { }

// Default functions for serde
fn default_cache_dir() -> PathBuf { }
fn default_max_proc() -> usize { 4 }
fn default_multi_disc_flag() -> String { "DEFAULT_MULTI_DISC".to_string() }
fn default_release_template() -> String { 
    "[{release_year}] {album}{multi_disc_flag}".to_string() 
}
```

## Phase 3: Templates Engine (Week 2, Days 4-7)

### Checkpoint 3.1: Path Template System

#### Tests to Port
From `templates_test.py`:
```rust
// tests/templates_test.rs
#[test]
fn test_execute_release_template() {
    // Test default: "[{release_year}] {album}{multi_disc_flag}"
    let release = CachedRelease {
        title: "Test Album",
        release_year: Some(2023),
        // ...
    };
    assert_eq!(result, "[2023] Test Album");
}

#[test]
fn test_execute_track_template() {
    // Test default: "{track_number}. {title}"
    let track = CachedTrack {
        track_number: "01",
        title: "Test Track",
        // ...
    };
    assert_eq!(result, "01. Test Track");
}

// Additional template tests
#[test]
fn test_template_missing_variable() { }

#[test]
fn test_template_multi_disc_flag() { }

#[test]
fn test_template_sanitization() { }

#[test]
fn test_template_custom_filters() { }
```

#### Implementation Checklist
`rose-core/src/templates/mod.rs`:
```rust
pub struct TemplateEngine {
    tera: Tera,
}

impl TemplateEngine {
    pub fn new() -> Result<Self> { }
    
    // Register custom filters
    fn register_filters(&mut self) { }
}

// From templates.py
pub fn parse_release_template(template: &str) -> Result<Template> { }
pub fn parse_track_template(template: &str) -> Result<Template> { }

pub fn execute_release_template(
    config: &Config,
    release: &CachedRelease,
) -> Result<String> { }

pub fn execute_track_template(
    config: &Config,
    track: &CachedTrack,
) -> Result<String> { }

// Path resolution functions
pub fn resolve_release_path(
    config: &Config,
    release: &CachedRelease,
) -> PathBuf { }

pub fn resolve_track_path(
    config: &Config,
    track: &CachedTrack,
) -> PathBuf { }

// Build template context
fn build_release_context(release: &CachedRelease) -> Context { }
fn build_track_context(track: &CachedTrack) -> Context { }

// Custom Tera filters
fn filter_sanitize(value: &Value, _: &HashMap<String, Value>) -> Result<Value> { }
```

## Phase 4: Rule Parser (Week 3, Days 1-4)

### Checkpoint 4.1: Rule DSL Parser

#### Tests to Port
All 44 tests from `rule_parser_test.py`:
```rust
// tests/rule_parser_test.rs

mod tokenizer {
    #[test]
    fn test_tokenize_single_value() {
        // "artist:foo" -> vec!["artist", ":", "foo"]
    }
    
    #[test]
    fn test_tokenize_multi_value() {
        // "artist:foo,bar" -> vec!["artist", ":", "foo", ",", "bar"]
    }
    
    #[test]
    fn test_tokenize_quoted_value() {
        // 'artist:"foo:bar"' -> vec!["artist", ":", "foo:bar"]
    }
    
    #[test]
    fn test_tokenize_regex() {
        // "artist:/foo.*/" -> vec!["artist", ":", "/foo.*/"]
    }
    
    #[test]
    fn test_tokenize_bad_pattern() {
        // Should error on malformed patterns
    }
    
    #[test]
    fn test_tokenize_bad_values() {
        // Should error on unclosed quotes, etc.
    }
    
    #[test]
    fn test_tokenize_escaped_quotes() {
        // 'artist:"foo\"bar"' -> "foo\"bar"
    }
    
    #[test]
    fn test_tokenize_escaped_delimiter() {
        // "artist:foo\,bar" -> "foo,bar"
    }
    
    #[test]
    fn test_tokenize_escaped_slash() {
        // "artist:/foo\/bar/" -> regex "foo/bar"
    }
    
    // ... continue with all 44 tests
}

mod parser {
    #[test]
    fn test_parse_tag() {
        // Parse simple tag matchers
    }
    
    #[test]
    fn test_parse_tag_regex() {
        // Parse regex matchers
    }
    
    #[test]
    fn test_parse_bool_ops() {
        // Parse and/or/not operators
    }
    
    #[test]
    fn test_parse_action_replace() {
        // Parse replacement actions
    }
    
    // ... all parser tests
}

mod actions {
    #[test]
    fn test_execute_sed_artist() {
        // Test sed on artist fields
    }
    
    #[test]
    fn test_execute_sed_global() {
        // Test global flag
    }
    
    // ... all action tests
}
```

#### Implementation Checklist
`rose-core/src/rules/parser.rs`:
```rust
// Token types
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Field(String),
    Colon,
    Value(String),
    Regex(String),
    Comma,
    And,
    Or,
    Not,
    LeftParen,
    RightParen,
    // Action tokens
    Replace,
    Add,
    Delete,
    Split,
    Sed,
}

// Tokenizer
pub fn tokenize(input: &str) -> Result<Vec<Token>> { }

// Parser structures
#[derive(Debug, Clone)]
pub enum Matcher {
    Tag { tag: String, pattern: Pattern },
    Release(Box<Matcher>),
    Track(Box<Matcher>),
    And(Box<Matcher>, Box<Matcher>),
    Or(Box<Matcher>, Box<Matcher>),
    Not(Box<Matcher>),
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Exact(String),
    Regex(regex::Regex),
    List(Vec<String>),
}

#[derive(Debug, Clone)]
pub enum Action {
    Replace { tag: String, value: String },
    Add { tag: String, value: String },
    Delete { tag: String },
    DeleteTag { tag: String },
    Split { tag: String, delimiter: String },
    Sed { tag: String, pattern: regex::Regex, replacement: String, flags: SedFlags },
}

#[derive(Debug, Clone)]
pub struct SedFlags {
    pub global: bool,
    pub case_insensitive: bool,
    pub multiline: bool,
}

// Parser functions
pub fn parse_matcher(tokens: &[Token]) -> Result<Matcher> { }
pub fn parse_actions(tokens: &[Token]) -> Result<Vec<Action>> { }
pub fn parse_rule(input: &str) -> Result<(Matcher, Vec<Action>)> { }

// Helper parsers
fn parse_pattern(tokens: &[Token]) -> Result<Pattern> { }
fn parse_boolean_expr(tokens: &[Token]) -> Result<Matcher> { }
```

## Phase 5: Audio Tags (Week 3, Days 5-7 + Week 4, Days 1-2)

### Checkpoint 5.1: Audio Metadata Abstraction

#### Tests to Port
From `audiotags_test.py`:
```rust
// tests/audiotags_test.rs
use tempfile::TempDir;

#[test]
fn test_mp3() {
    // Copy test file, read tags, modify, write, verify
    let td = TempDir::new().unwrap();
    let src = "testdata/Tagger/track1.mp3";
    // Test all tag operations
}

#[test]
fn test_m4a() {
    // Same for M4A format
}

#[test]
fn test_ogg() {
    // OGG Vorbis format
}

#[test]
fn test_opus() {
    // OPUS format
}

#[test]
fn test_flac() {
    // FLAC format
}

#[test]
fn test_unsupported_text_file() {
    // Should error on .txt files
}

#[test]
fn test_id3_delete_explicit_v1() {
    // ID3v1 tag handling
}

#[test]
fn test_preserve_unknown_tags() {
    // Unknown tags should be preserved
}

// Additional tests from usage
#[test]
fn test_roseid_tag() {
    // Custom rose ID tags
}

#[test]
fn test_multi_value_artists() {
    // Multiple artist handling
}

#[test]
fn test_cover_art_extraction() {
    // Extract embedded images
}
```

#### Implementation Checklist
`rose-core/src/audiotags/mod.rs`:
```rust
// Trait matching Python ABC
pub trait AudioTags: Send + Sync {
    fn can_write(&self) -> bool;
    
    // Required tag accessors
    fn title(&self) -> Option<&str>;
    fn album(&self) -> Option<&str>;
    fn artist(&self) -> Option<ArtistMapping>;
    fn date(&self) -> Option<i32>;
    fn track_number(&self) -> Option<&str>;
    fn disc_number(&self) -> Option<&str>;
    fn duration_seconds(&self) -> Option<i32>;
    
    // Tag setters
    fn set_title(&mut self, value: Option<&str>) -> Result<()>;
    fn set_album(&mut self, value: Option<&str>) -> Result<()>;
    fn set_artist(&mut self, value: ArtistMapping) -> Result<()>;
    // ... other setters
    
    // Rose-specific tags
    fn roseid(&self) -> Option<&str>;
    fn set_roseid(&mut self, id: &str) -> Result<()>;
    
    // Serialization
    fn dump(&self) -> HashMap<String, Value>;
    fn flush(&mut self, path: &Path) -> Result<()>;
}

// Factory function
pub fn read_tags(path: &Path) -> Result<Box<dyn AudioTags>> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("mp3") => Ok(Box::new(ID3Tags::from_file(path)?)),
        Some("m4a") | Some("mp4") => Ok(Box::new(MP4Tags::from_file(path)?)),
        Some("ogg") => Ok(Box::new(VorbisTags::from_file(path)?)),
        Some("opus") => Ok(Box::new(OpusTags::from_file(path)?)),
        Some("flac") => Ok(Box::new(FLACTags::from_file(path)?)),
        _ => Err(RoseError::UnsupportedAudioFormat { 
            format: path.display().to_string() 
        }),
    }
}

// Format-specific implementations
mod id3;   // ID3Tags
mod mp4;   // MP4Tags
mod vorbis; // VorbisTags
mod opus;  // OpusTags
mod flac;  // FLACTags

// Helper functions
fn parse_artists(raw: &str) -> Vec<Artist> { }
fn format_artists(artists: &[Artist]) -> String { }
```

## Phase 6: Cache Foundation (Week 4, Days 3-7)

### Checkpoint 6.1: Database Schema and Basic Operations

#### Tests to Port (First 30 from cache_test.py)
```rust
// tests/cache_basic_test.rs

#[test]
fn test_create() {
    // Create new cache database
    let config = test_config();
    create_cache(&config).unwrap();
    assert!(cache_path(&config).exists());
}

#[test]
fn test_update() {
    // Basic cache update from filesystem
    let config = test_config();
    let release_dir = add_test_release("Test Release");
    update_cache(&config, false).unwrap();
    
    // Verify release was cached
    let releases = list_releases(&config, None);
    assert_eq!(releases.count(), 1);
}

#[test]
fn test_update_releases_and_delete_orphans() {
    // Add release, cache it, delete it, update again
    // Orphan should be removed
}

#[test]
fn test_force_update() {
    // Force update should re-read even unchanged files
}

#[test]
fn test_evict_nonexistent_releases() {
    // Releases with missing source should be evicted
}

// ... continue with basic CRUD tests
```

#### Implementation Checklist
`rose-core/src/cache/mod.rs`:
```rust
pub mod schema;
pub mod connection;
pub mod models;
pub mod queries;

use schema::SCHEMA_SQL;

// Core cache functions from cache.py
pub fn connect(config: &Config) -> Result<Connection> { }
pub fn create(config: &Config) -> Result<()> { }
pub fn migrate(config: &Config, from_version: u32) -> Result<()> { }

pub fn update_cache(config: &Config, force: bool) -> Result<()> { }
pub fn update_cache_for_releases(
    config: &Config,
    release_dirs: &[PathBuf],
    force: bool,
) -> Result<UpdateCacheResult> { }

// Basic queries
pub fn list_releases(
    config: &Config,
    matcher: Option<&Matcher>,
) -> Result<impl Iterator<Item = CachedRelease>> { }

pub fn get_release(config: &Config, id: &str) -> Result<Option<CachedRelease>> { }
```

`rose-core/src/cache/schema.rs`:
```rust
// Port schema from cache.sql
pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS releases (
    id TEXT PRIMARY KEY,
    source_path TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    release_type TEXT,
    release_year INTEGER,
    new BOOLEAN NOT NULL,
    -- ... all columns from Python
);

-- ... all other tables
"#;

pub const CURRENT_VERSION: u32 = 1;
```

`rose-core/src/cache/models.rs`:
```rust
// Cached data models matching Python
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
    // ... all fields
}

#[derive(Debug, Clone)]
pub struct CachedTrack {
    pub id: String,
    pub source_path: PathBuf,
    pub title: String,
    pub release_id: String,
    pub track_number: String,
    pub disc_number: String,
    pub artists: ArtistMapping,
    // ... all fields
}
```

## Phase 7: Cache Advanced Features (Week 5)

### Checkpoint 7.1: Complex Queries and Full-Text Search

#### Tests to Port (Remaining 57 from cache_test.py)
```rust
// tests/cache_advanced_test.rs

mod metadata_handling {
    #[test]
    fn test_release_type_albumartist_writeback() {
        // Test albumartist field propagation
    }
    
    #[test]
    fn test_release_type_compilation_propagates() {
        // Compilation flag should propagate
    }
    
    #[test]
    fn test_year_uses_original_date_and_falls_back_to_date() {
        // Date field precedence
    }
    
    // ... all metadata tests
}

mod artist_handling {
    #[test]
    fn test_read_artist_fields_album() { }
    
    #[test]
    fn test_handle_albumartists_field() { }
    
    #[test]
    fn test_artist_aliases() { }
    
    // ... all artist tests
}

mod fts_search {
    #[test]
    fn test_fts_search_query() {
        // Test full-text search
    }
    
    #[test]
    fn test_fts_parse_query() { }
    
    #[test]
    fn test_matcher_query() { }
}

mod performance {
    #[test]
    fn test_multiprocessing() {
        // Test parallel updates
    }
    
    #[test]
    fn test_locking() {
        // Test concurrent access
    }
}
```

#### Implementation Checklist
`rose-core/src/cache/fts.rs`:
```rust
// Full-text search implementation
pub fn create_fts_tables(conn: &Connection) -> Result<()> {
    // Create FTS5 virtual tables
}

pub fn index_release_text(conn: &Connection, release: &CachedRelease) -> Result<()> { }
pub fn index_track_text(conn: &Connection, track: &CachedTrack) -> Result<()> { }

pub fn search_releases_fts(
    conn: &Connection,
    query: &str,
) -> Result<Vec<String>> { }
```

`rose-core/src/cache/update.rs`:
```rust
// Complex update logic
pub struct CacheUpdater<'a> {
    config: &'a Config,
    conn: Connection,
    force: bool,
}

impl<'a> CacheUpdater<'a> {
    pub fn update_releases(&mut self, dirs: &[PathBuf]) -> Result<UpdateStats> { }
    
    fn scan_directory(&self, dir: &Path) -> Result<ReleaseInfo> { }
    fn read_metadata(&self, track_path: &Path) -> Result<TrackMetadata> { }
    fn compute_release_metadata(&self, tracks: &[TrackMetadata]) -> ReleaseMetadata { }
    fn apply_updates(&mut self, updates: Vec<Update>) -> Result<()> { }
}

// Parallel update support
pub fn update_parallel(
    config: &Config,
    release_dirs: &[PathBuf],
) -> Result<UpdateStats> {
    use rayon::prelude::*;
    // Implement parallel scanning
}
```

## Phase 8: Entity Management (Week 6)

### Checkpoint 8.1: Releases Module

#### Tests to Port
From `releases_test.py`:
```rust
// tests/releases_test.rs

#[test]
fn test_create_releases() {
    // Test creating releases from directory
}

#[test]
fn test_create_single_releases() {
    // Test single release creation
}

#[test]
fn test_delete_release() {
    // Test release deletion
}

#[test]
fn test_edit_release() {
    // Test metadata editing
}

#[test]
fn test_set_release_cover_art() {
    // Test cover art management
}

#[test]
fn test_run_rule_on_release() {
    // Test rule application
}

#[test]
fn test_toggle_new_flag() {
    // Test new flag toggling
}

#[test]
fn test_dump_releases() {
    // Test serialization
}
```

#### Implementation Checklist
`rose-core/src/releases.rs`:
```rust
// From releases.py
pub fn create_release(config: &Config, source_dir: &Path) -> Result<String> { }

pub fn create_single_release(
    config: &Config,
    artist: &str,
    title: &str,
    track_paths: &[PathBuf],
) -> Result<String> { }

pub fn delete_release(config: &Config, release_id: &str) -> Result<()> { }
pub fn delete_release_ignore_fs(config: &Config, release_id: &str) -> Result<()> { }

pub fn edit_release(config: &Config, release_id: &str) -> Result<()> {
    // 1. Get current metadata
    // 2. Write to TOML file
    // 3. Open in $EDITOR
    // 4. Parse changes
    // 5. Apply updates
}

pub fn set_release_cover_art(
    config: &Config,
    release_id: &str,
    cover_path: Option<&Path>,
) -> Result<()> { }

pub fn toggle_release_new(config: &Config, release_id: &str) -> Result<()> { }

pub fn dump_releases(
    config: &Config,
    matcher: &Matcher,
    output: &Path,
) -> Result<()> { }

// Helper functions
fn write_release_to_toml(release: &CachedRelease) -> String { }
fn parse_release_from_toml(content: &str) -> Result<ReleaseEdit> { }
fn apply_release_edit(config: &Config, id: &str, edit: ReleaseEdit) -> Result<()> { }
```

### Checkpoint 8.2: Tracks Module

#### Tests to Port
From `tracks_test.py`:
```rust
// tests/tracks_test.rs

#[test]
fn test_dump_tracks() {
    // Test track serialization
}

#[test]
fn test_set_track_one() {
    // Test track number filtering
}

// Additional track operations
#[test]
fn test_edit_track() { }

#[test]
fn test_delete_track() { }
```

#### Implementation Checklist
`rose-core/src/tracks.rs`:
```rust
// From tracks.py
pub fn delete_track(config: &Config, track_id: &str) -> Result<()> { }
pub fn delete_track_ignore_fs(config: &Config, track_id: &str) -> Result<()> { }

pub fn edit_track(config: &Config, track_id: &str) -> Result<()> { }

pub fn extract_track_art(
    config: &Config,
    track_id: &str,
    output_path: &Path,
) -> Result<()> { }

pub fn set_track_audio(
    config: &Config,
    track_id: &str,
    audio_path: &Path,
) -> Result<()> { }

pub fn dump_tracks(
    config: &Config,
    matcher: &Matcher,
    output: &Path,
) -> Result<()> { }
```

## Phase 9: Rules Engine (Week 7)

### Checkpoint 9.1: Rule Execution

#### Tests to Port
From `rules_test.py` (27 tests):
```rust
// tests/rules_test.rs

mod tag_operations {
    #[test]
    fn test_update_tag_constant() {
        // Replace tag with constant
    }
    
    #[test]
    fn test_update_tag_regex() {
        // Match with regex
    }
    
    #[test]
    fn test_update_tag_sed_replace() {
        // Sed-style replacement
    }
    
    #[test]
    fn test_update_tag_delete() { }
    
    #[test]
    fn test_update_tag_add() { }
    
    #[test]
    fn test_update_tag_split() { }
}

mod artist_operations {
    #[test]
    fn test_artist_tag_replace() { }
    
    #[test]
    fn test_artist_tag_sed() { }
    
    #[test]
    fn test_artist_tag_delete() { }
    
    #[test]
    fn test_artist_tag_split() { }
    
    #[test]
    fn test_artist_tag_multi_delete() { }
    
    #[test]
    fn test_artist_tag_role_delete() { }
}

mod special_fields {
    #[test]
    fn test_releasedate_update() { }
    
    #[test]
    fn test_tracknum_update() { }
    
    #[test]
    fn test_genre_update() { }
    
    #[test]
    fn test_genre_update_insert_parent() { }
    
    #[test]
    fn test_label_update() { }
}

mod execution {
    #[test]
    fn test_matcher_release() { }
    
    #[test]
    fn test_fast_search_release_matcher() { }
    
    #[test]
    fn test_execute_stored_rule() { }
}
```

#### Implementation Checklist
`rose-core/src/rules/engine.rs`:
```rust
use crate::rules::parser::{Matcher, Action};

// Main execution function
pub fn execute_rule(
    config: &Config,
    matcher: &Matcher,
    actions: &[Action],
) -> Result<ExecutionStats> {
    // 1. Find matching items
    // 2. Apply actions
    // 3. Update cache
    // 4. Write tags
}

// Fast search using FTS
pub fn fast_search_for_matching_releases(
    config: &Config,
    matcher: &Matcher,
) -> Result<Vec<String>> { }

pub fn fast_search_for_matching_tracks(
    config: &Config,
    matcher: &Matcher,
) -> Result<Vec<String>> { }

// Action execution
pub fn apply_action_to_release(
    config: &Config,
    release_id: &str,
    action: &Action,
) -> Result<()> { }

pub fn apply_action_to_track(
    config: &Config,
    track_id: &str,
    action: &Action,
) -> Result<()> { }

// Stored rules
pub fn execute_stored_rule(
    config: &Config,
    rule_name: &str,
) -> Result<ExecutionStats> { }
```

## Phase 10: Collections (Week 8)

### Checkpoint 10.1: Collages

#### Tests to Port
From `collages_test.py`:
```rust
// tests/collages_test.rs

#[test]
fn test_lifecycle() {
    // Create -> add releases -> read -> delete
}

#[test]
fn test_edit() {
    // Edit collage metadata
}

#[test]
fn test_duplicate_name() {
    // Error on duplicate names
}

#[test]
fn test_add_release_resets_release_added_at() {
    // Timestamp updates
}

#[test]
fn test_remove_release_from_collage() { }

#[test]
fn test_add_releases_in_middle() {
    // Position management
}

#[test]
fn test_collages_are_updated_on_general_cache_update() {
    // Cache sync
}
```

#### Implementation Checklist
`rose-core/src/collages.rs`:
```rust
// From collages.py
pub fn create_collage(config: &Config, name: &str) -> Result<()> { }
pub fn delete_collage(config: &Config, name: &str) -> Result<()> { }

pub fn add_release_to_collage(
    config: &Config,
    collage_name: &str,
    release_id: &str,
    position: Option<i32>,
) -> Result<()> { }

pub fn delete_release_from_collage(
    config: &Config,
    collage_name: &str,
    release_id: &str,
) -> Result<()> { }

pub fn edit_collage(config: &Config, name: &str) -> Result<()> { }

// File operations
fn collage_path(config: &Config, name: &str) -> PathBuf { }
fn read_collage_file(path: &Path) -> Result<CollageFile> { }
fn write_collage_file(path: &Path, collage: &CollageFile) -> Result<()> { }
```

### Checkpoint 10.2: Playlists

#### Tests to Port
From `playlists_test.py`:
```rust
// tests/playlists_test.rs

#[test]
fn test_lifecycle() { }

#[test]
fn test_duplicate_name() { }

#[test]
fn test_add_track_resets_track_added_at() { }

#[test]
fn test_remove_track_from_playlist() { }

#[test]
fn test_edit() { }

#[test]
fn test_playlist_cover_art() { }

#[test]
fn test_playlist_cover_art_square() {
    // Dimension validation
}

#[test]
fn test_add_tracks_in_middle() { }

#[test]
fn test_playlists_are_updated_on_general_cache_update() { }
```

#### Implementation Checklist
`rose-core/src/playlists.rs`:
```rust
// From playlists.py
pub fn create_playlist(config: &Config, name: &str) -> Result<()> { }
pub fn delete_playlist(config: &Config, name: &str) -> Result<()> { }

pub fn add_track_to_playlist(
    config: &Config,
    playlist_name: &str,
    track_id: &str,
    position: Option<i32>,
) -> Result<()> { }

pub fn delete_track_from_playlist(
    config: &Config,
    playlist_name: &str,
    track_id: &str,
) -> Result<()> { }

pub fn set_playlist_cover_art(
    config: &Config,
    name: &str,
    cover_path: Option<&Path>,
) -> Result<()> { }

// M3U generation
pub fn generate_m3u_for_playlist(
    config: &Config,
    name: &str,
) -> Result<String> { }
```

## Phase 11: CLI Implementation (Week 9)

### Checkpoint 11.1: CLI Framework and Commands

#### Tests to Port
From `cli_test.py` and command usage:
```rust
// tests/cli_test.rs

#[test]
fn test_cache_update_command() {
    // rose cache update
}

#[test]
fn test_cache_update_with_directories() {
    // rose cache update -d dir1 -d dir2
}

#[test]
fn test_releases_list() {
    // rose releases list
}

#[test]
fn test_releases_edit() {
    // rose releases edit <id>
}

// Test all commands...
```

#### Implementation Checklist
`rose-cli/src/main.rs`:
```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "rose")]
#[command(about = "A music manager with a virtual filesystem")]
struct Cli {
    #[arg(short, long)]
    verbose: bool,
    
    #[arg(short, long)]
    config: Option<PathBuf>,
    
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage the read cache
    Cache {
        #[command(subcommand)]
        action: CacheCommands,
    },
    
    /// Manage releases
    Releases {
        #[command(subcommand)]
        action: ReleaseCommands,
    },
    
    // ... all other commands
}

#[derive(Subcommand)]
enum CacheCommands {
    /// Update the cache
    Update {
        #[arg(short, long)]
        force: bool,
        
        #[arg(short, long)]
        directories: Vec<PathBuf>,
    },
    
    /// Watch for changes
    Watch,
}

// ... implement all command enums

fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Set up logging
    env_logger::Builder::new()
        .filter_level(if cli.verbose { 
            log::LevelFilter::Debug 
        } else { 
            log::LevelFilter::Info 
        })
        .init();
    
    // Load config
    let config = Config::parse(cli.config.as_deref())?;
    
    // Execute command
    match cli.command {
        Commands::Cache { action } => handle_cache_command(&config, action),
        Commands::Releases { action } => handle_releases_command(&config, action),
        // ... handle all commands
    }
}
```

## Phase 12: Virtual Filesystem (Week 10)

### Checkpoint 12.1: FUSE Implementation

#### Tests to Port
From `virtualfs_test.py`:
```rust
// tests/virtualfs_test.rs

#[test]
fn test_mount_unmount() {
    // Test basic mount/unmount
}

#[test]
fn test_browse_releases() {
    // List /1. Releases/
}

#[test]
fn test_ghost_files() {
    // Test ghost file behavior
}

// ... comprehensive VFS tests
```

#### Implementation Checklist
`rose-vfs/src/lib.rs`:
```rust
use fuser::{FileSystem, Request, ReplyEntry, ReplyData};

pub struct RoseVFS {
    config: Config,
    cache: CacheConnection,
    ghost_files: Arc<Mutex<HashMap<PathBuf, Instant>>>,
}

impl FileSystem for RoseVFS {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        // Implement path resolution
    }
    
    fn read(&mut self, _req: &Request, ino: u64, fh: u64, offset: i64, size: u32, reply: ReplyData) {
        // Implement file reading
    }
    
    // ... all FUSE operations
}

// Mount function
pub fn mount_virtualfs(config: &Config, mount_point: &Path) -> Result<()> {
    let vfs = RoseVFS::new(config)?;
    fuser::mount2(vfs, mount_point, &[])?;
    Ok(())
}
```

## Phase 13: File Watcher (Week 11, Days 1-3)

### Checkpoint 13.1: Filesystem Monitoring

#### Tests to Port
From `watcher_test.py`:
```rust
// tests/watcher_test.rs

#[test]
fn test_file_creation_triggers_update() { }

#[test]
fn test_file_modification_triggers_update() { }

#[test]
fn test_file_deletion_triggers_update() { }
```

#### Implementation Checklist
`rose-watch/src/lib.rs`:
```rust
use notify::{Watcher, RecursiveMode, Event};

pub struct RoseWatcher {
    config: Config,
    watcher: RecommendedWatcher,
}

impl RoseWatcher {
    pub fn new(config: Config) -> Result<Self> { }
    
    pub fn watch(&mut self) -> Result<()> {
        // Set up inotify watches
        // Process events
        // Trigger cache updates
    }
    
    fn handle_event(&mut self, event: Event) -> Result<()> { }
}
```

## Phase 14: Integration and Polish (Week 11, Days 4-7)

### Checkpoint 14.1: Cross-Feature Integration Tests

#### Integration Tests
```rust
// tests/integration/test_full_workflow.rs

#[test]
fn test_complete_workflow() {
    // 1. Create config
    // 2. Add music files
    // 3. Update cache
    // 4. Apply rules
    // 5. Create playlist
    // 6. Mount VFS
    // 7. Verify everything works
}

#[test]
fn test_python_compatibility() {
    // Verify Python can read Rust-created data
}

#[test]
fn test_concurrent_operations() {
    // Test parallel cache updates, etc.
}
```

### Checkpoint 14.2: Performance Optimization

#### Benchmarks
```rust
// benches/cache_bench.rs
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_cache_update(c: &mut Criterion) {
    c.bench_function("cache update 1000 releases", |b| {
        b.iter(|| {
            // Benchmark cache update
        });
    });
}

criterion_group!(benches, bench_cache_update);
criterion_main!(benches);
```

## Test Data Setup

### Required Test Files
Copy from rose-py:
- `testdata/` directory with all test music files
- Test TOML configurations
- Sample playlists and collages

### Test Utilities
```rust
// tests/common/mod.rs

pub fn test_config() -> Config {
    // Standard test configuration
}

pub fn setup_test_library() -> TempDir {
    // Create test music library
}

pub fn add_test_release(name: &str) -> PathBuf {
    // Add a test release
}
```

## Validation Criteria

### Each Checkpoint Must:
1. Have all tests written and failing (red)
2. Implement until all tests pass (green)
3. Refactor for clarity/performance
4. Pass all previous checkpoints' tests
5. Have documentation for public APIs

### Final Validation:
1. All Python tests have Rust equivalents
2. Python and Rust can read each other's data
3. Performance metrics meet targets
4. Binary size is reasonable
5. Memory usage is improved

## Common Pitfalls to Avoid

1. **Don't skip tests**: Write tests first, always
2. **Match Python behavior exactly**: Even quirks
3. **Preserve all metadata**: Including unknown tags
4. **Handle Unicode properly**: In paths and tags
5. **Respect file permissions**: Like Python does
6. **Use the same IDs**: For compatibility
7. **Keep SQL schema identical**: For data interchange

## Success Metrics

- 100% test parity with Python
- 2-5x performance improvement
- 50% less memory usage
- Zero data loss in migration
- Identical CLI behavior
- Same virtual filesystem structure

This plan provides everything needed to implement rose-rs with confidence that it will work correctly and be compatible with rose-py.