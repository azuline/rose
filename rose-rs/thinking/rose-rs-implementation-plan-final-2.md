# Rose-rs Implementation Plan - Final 2 (rose-py only)

## Executive Summary

This document provides a comprehensive plan for reimplementing only the `rose-py` library in Rust. The implementation:
- Focuses solely on the core library (no CLI, VFS, or watcher)
- Uses a flat `src/` directory structure
- Follows Test-Driven Development
- Maintains full API compatibility with rose-py

## Project Structure

```
rose-rs/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── common.rs
│   ├── config.rs
│   ├── genre_hierarchy.rs
│   ├── audiotags.rs
│   ├── audiotags_id3.rs
│   ├── audiotags_mp4.rs
│   ├── audiotags_vorbis.rs
│   ├── audiotags_flac.rs
│   ├── cache.rs
│   ├── cache_schema.sql
│   ├── rule_parser.rs
│   ├── rules.rs
│   ├── templates.rs
│   ├── releases.rs
│   ├── tracks.rs
│   ├── collages.rs
│   └── playlists.rs
├── tests/
│   ├── common/
│   │   └── mod.rs
│   ├── testdata/ (copy from rose-py)
│   └── *.rs (test files)
└── benches/
    └── cache_bench.rs
```

## Dependencies

```toml
[package]
name = "rose-rs"
version = "0.5.0"
edition = "2021"

[dependencies]
# Core
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"
thiserror = "1.0"
anyhow = "1.0"

# Database
rusqlite = { version = "0.31", features = ["bundled", "backup", "chrono", "functions"] }

# Audio
lofty = "0.18"
id3 = "1.13"  # For specific ID3 features

# Templates
tera = "1.19"

# Utils
walkdir = "2.5"
regex = "1.10"
lazy_static = "1.4"
chrono = "0.4"
uuid = { version = "1.8", features = ["v4", "serde"] }
home = "0.5"  # For ~ expansion
rayon = "1.10"  # For parallel processing

# File locking
fs2 = "0.4"

[dev-dependencies]
tempfile = "3.10"
proptest = "1.4"
criterion = "0.5"
pretty_assertions = "1.4"
```

## Phase 1: Foundation Layer (Week 1)

### Checkpoint 1.1: Common Types and Utilities (Days 1-2)

#### Tests to Port
```rust
// tests/common_test.rs
#[test]
fn test_artist_new() { 
    let artist = Artist::new("BLACKPINK");
    assert_eq!(artist.name, "BLACKPINK");
    assert!(!artist.alias);
}

#[test]
fn test_artist_mapping_new() {
    let mapping = ArtistMapping::new();
    assert!(mapping.main.is_empty());
}

#[test]
fn test_valid_uuid() {
    assert!(valid_uuid("123e4567-e89b-12d3-a456-426614174000"));
    assert!(!valid_uuid("not-a-uuid"));
}

#[test]
fn test_sanitize_filename() {
    assert_eq!(sanitize_filename("a/b"), "a_b");
    assert_eq!(sanitize_filename(".."), "_");
    assert_eq!(sanitize_filename("a:b"), "a_b");
}

#[test]
fn test_musicfile() {
    assert!(musicfile(Path::new("test.mp3")));
    assert!(musicfile(Path::new("test.flac")));
    assert!(!musicfile(Path::new("test.txt")));
}

#[test]
fn test_imagefile() {
    assert!(imagefile(Path::new("cover.jpg")));
    assert!(!imagefile(Path::new("cover.txt")));
}
```

#### Implementation - `src/common.rs`
```rust
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artist {
    pub name: String,
    pub alias: bool,
}

impl Artist {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), alias: false }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArtistMapping {
    pub main: Vec<Artist>,
    pub guest: Vec<Artist>,
    pub remixer: Vec<Artist>,
    pub producer: Vec<Artist>,
    pub composer: Vec<Artist>,
    pub conductor: Vec<Artist>,
    pub djmixer: Vec<Artist>,
}

// Constants from Python
pub const SUPPORTED_AUDIO_EXTENSIONS: &[&str] = &["mp3", "m4a", "ogg", "opus", "flac"];
pub const SUPPORTED_IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png"];

// Error hierarchy matching Python
#[derive(Error, Debug)]
pub enum RoseError {
    #[error("Rose error: {0}")]
    Base(String),
    
    #[error(transparent)]
    Expected(#[from] RoseExpectedError),
    
    #[error(transparent)]
    Unexpected(#[from] RoseUnexpectedError),
}

#[derive(Error, Debug)]
pub enum RoseExpectedError {
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
}

#[derive(Error, Debug)]
pub enum RoseUnexpectedError {
    #[error("File not found: {path}")]
    FileNotFound { path: PathBuf },
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// Utility functions
pub fn valid_uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok()
}

pub fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            '.' if s == "." || s == ".." => '_',
            c => c,
        })
        .collect()
}

pub fn musicfile(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_AUDIO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn imagefile(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_IMAGE_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

// Type alias
pub type Result<T> = std::result::Result<T, RoseError>;
```

### Checkpoint 1.2: Genre Hierarchy (Days 3-4)

#### Tests to Port
```rust
// tests/genre_hierarchy_test.rs
#[test]
fn test_genres_loaded() {
    assert!(GENRES.len() > 20000);
    assert!(GENRES.contains(&"K-Pop"));
    assert!(GENRES.contains(&"Dance-Pop"));
}

#[test]
fn test_genre_parents() {
    let parents = GENRE_PARENTS.get("K-Pop").unwrap();
    assert!(parents.contains(&"Pop"));
}

#[test]
fn test_genre_exists() {
    assert!(genre_exists("K-Pop"));
    assert!(!genre_exists("Unknown"));
}
```

#### Implementation - `src/genre_hierarchy.rs`
```rust
use lazy_static::lazy_static;
use std::collections::HashMap;

// Generate this from rose-py/rose/genre_hierarchy.py
lazy_static! {
    pub static ref GENRES: Vec<&'static str> = vec![
        // Port all 24,000+ genres from Python
        "K-Pop", "Pop", "Dance-Pop", "Electronic", // ... etc
    ];
    
    pub static ref GENRE_PARENTS: HashMap<&'static str, Vec<&'static str>> = {
        let mut m = HashMap::new();
        // Port relationships from Python
        m.insert("K-Pop", vec!["Pop"]);
        m.insert("Dance-Pop", vec!["Pop", "Dance"]);
        // ... etc
        m
    };
}

pub fn genre_exists(genre: &str) -> bool {
    GENRES.iter().any(|&g| g == genre)
}

pub fn get_all_parents(genre: &str) -> Vec<&str> {
    let mut parents = Vec::new();
    let mut to_process = vec![genre];
    let mut seen = std::collections::HashSet::new();
    
    while let Some(g) = to_process.pop() {
        if !seen.insert(g) {
            continue;
        }
        
        if let Some(direct_parents) = GENRE_PARENTS.get(g) {
            parents.extend(direct_parents.iter().copied());
            to_process.extend(direct_parents.iter().copied());
        }
    }
    
    parents.sort();
    parents.dedup();
    parents
}
```

## Phase 2: Configuration System (Week 1, Days 5-7)

### Checkpoint 2.1: Configuration Loading

#### Tests to Port
All tests from `config_test.py`:
```rust
// tests/config_test.rs
#[test]
fn test_config_full() {
    let toml = r#"
        music_source_dir = "~/.music-source"
        cache_dir = "~/.cache/rose"
        max_proc = 8
        multi_disc_toggle_flag = "MULTIDISC"
        
        [[artist_aliases]]
        artist = "Blackpink"
        alias = "BLACKPINK"
        
        [[rules]]
        name = "fix-kpop"
        rule = "genre:K-Pop genre:='K-Pop'"
        
        [path_templates]
        release = "[{release_year}] {album}"
        track = "{track_number}. {title}"
    "#;
    
    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(config.max_proc, 8);
}

#[test]
fn test_config_minimal() {
    let toml = r#"music_source_dir = "~/music""#;
    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(config.max_proc, 4); // default
}

#[test]
fn test_config_not_found() {
    let result = Config::parse(Some(Path::new("/nonexistent")));
    assert!(matches!(result, Err(RoseError::Expected(RoseExpectedError::ConfigNotFound { .. }))));
}

#[test]
fn test_config_validate_artist_aliases_resolve_to_self() {
    let toml = r#"
        music_source_dir = "~/music"
        [[artist_aliases]]
        artist = "X"
        alias = "X"
    "#;
    let result: Result<Config, _> = toml::from_str(toml);
    // Should fail validation
}

#[test]
fn test_config_validate_duplicate_artist_aliases() {
    let toml = r#"
        music_source_dir = "~/music"
        [[artist_aliases]]
        artist = "A"
        alias = "X"
        
        [[artist_aliases]]
        artist = "B"
        alias = "X"
    "#;
    // Should fail - X points to both A and B
}
```

#### Implementation - `src/config.rs`
```rust
use crate::common::{Result, RoseExpectedError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub music_source_dir: PathBuf,
    
    #[serde(default = "default_cache_dir")]
    pub cache_dir: PathBuf,
    
    #[serde(default = "default_max_proc")]
    pub max_proc: usize,
    
    #[serde(default)]
    pub artist_aliases: Vec<ArtistAlias>,
    
    #[serde(default)]
    pub rules: Vec<StoredRule>,
    
    #[serde(default)]
    pub path_templates: PathTemplates,
    
    #[serde(default)]
    pub cover_art_regexes: Vec<String>,
    
    #[serde(default = "default_multi_disc_flag")]
    pub multi_disc_toggle_flag: String,
    
    // Computed fields
    #[serde(skip)]
    pub artist_aliases_map: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArtistAlias {
    pub artist: String,
    pub alias: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StoredRule {
    pub name: String,
    pub rule: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PathTemplates {
    #[serde(default = "default_release_template")]
    pub release: String,
    
    #[serde(default = "default_track_template")]
    pub track: String,
    
    #[serde(default = "default_all_pattern")]
    pub all_patterns: String,
}

impl Default for PathTemplates {
    fn default() -> Self {
        Self {
            release: default_release_template(),
            track: default_track_template(),
            all_patterns: default_all_pattern(),
        }
    }
}

impl Config {
    pub fn parse(config_path_override: Option<&Path>) -> Result<Self> {
        let path = if let Some(p) = config_path_override {
            p.to_path_buf()
        } else {
            find_config_path()?
        };
        
        let content = std::fs::read_to_string(&path)
            .map_err(|_| RoseExpectedError::ConfigNotFound { path: path.clone() })?;
        
        let mut config: Config = toml::from_str(&content)
            .map_err(|e| RoseExpectedError::ConfigDecode { message: e.to_string() })?;
        
        // Expand ~ in paths
        config.music_source_dir = expand_home(&config.music_source_dir);
        config.cache_dir = expand_home(&config.cache_dir);
        
        // Validate and build alias map
        config.artist_aliases_map = validate_artist_aliases(&config.artist_aliases)?;
        
        Ok(config)
    }
}

fn find_config_path() -> Result<PathBuf> {
    // Check XDG_CONFIG_HOME first, then ~/.config/rose/config.toml
    let config_dir = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg).join("rose")
    } else if let Some(home) = home::home_dir() {
        home.join(".config").join("rose")
    } else {
        return Err(RoseExpectedError::ConfigNotFound { 
            path: PathBuf::from("~/.config/rose/config.toml") 
        }.into());
    };
    
    let path = config_dir.join("config.toml");
    if path.exists() {
        Ok(path)
    } else {
        Err(RoseExpectedError::ConfigNotFound { path }.into())
    }
}

fn expand_home(path: &Path) -> PathBuf {
    if let Ok(p) = path.strip_prefix("~") {
        if let Some(home) = home::home_dir() {
            return home.join(p);
        }
    }
    path.to_path_buf()
}

fn validate_artist_aliases(aliases: &[ArtistAlias]) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    
    for alias in aliases {
        if alias.artist == alias.alias {
            return Err(RoseExpectedError::ConfigDecode {
                message: format!("Artist alias cannot resolve to itself: {}", alias.artist)
            }.into());
        }
        
        if let Some(existing) = map.get(&alias.alias) {
            if existing != &alias.artist {
                return Err(RoseExpectedError::ConfigDecode {
                    message: format!("Duplicate alias '{}' for artists '{}' and '{}'", 
                                   alias.alias, existing, alias.artist)
                }.into());
            }
        }
        
        map.insert(alias.alias.clone(), alias.artist.clone());
    }
    
    Ok(map)
}

// Default functions
fn default_cache_dir() -> PathBuf {
    expand_home(Path::new("~/.cache/rose"))
}

fn default_max_proc() -> usize { 4 }

fn default_multi_disc_flag() -> String {
    "DEFAULT_MULTI_DISC".to_string()
}

fn default_release_template() -> String {
    "[{release_year}] {album}{multi_disc_flag}".to_string()
}

fn default_track_template() -> String {
    "{track_number}. {title}".to_string()
}

fn default_all_pattern() -> String {
    "{source_dir}/{release}/{track}".to_string()
}
```

## Phase 3: Templates Engine (Week 2, Days 1-2)

### Checkpoint 3.1: Path Templating

#### Tests to Port
From `templates_test.py`:
```rust
// tests/templates_test.rs
use crate::common::test_utils::*;

#[test]
fn test_execute_release_template() {
    let config = test_config();
    let release = test_release("Test Album", 2023);
    
    let result = execute_release_template(&config, &release).unwrap();
    assert_eq!(result, "[2023] Test Album");
}

#[test]
fn test_execute_track_template() {
    let config = test_config();
    let track = test_track("01", "Test Track");
    
    let result = execute_track_template(&config, &track).unwrap();
    assert_eq!(result, "01. Test Track");
}

#[test]
fn test_template_sanitization() {
    let config = test_config();
    let release = test_release("Test/Album", 2023);
    
    let result = execute_release_template(&config, &release).unwrap();
    assert_eq!(result, "[2023] Test_Album"); // Sanitized
}
```

#### Implementation - `src/templates.rs`
```rust
use crate::cache::{CachedRelease, CachedTrack};
use crate::common::{Result, RoseExpectedError, sanitize_filename};
use crate::config::Config;
use lazy_static::lazy_static;
use regex::Regex;
use tera::{Context, Tera};

lazy_static! {
    static ref TEMPLATE_ENGINE: Tera = {
        let mut tera = Tera::default();
        // Register custom filters
        tera.register_filter("sanitize", filter_sanitize);
        tera
    };
}

pub fn execute_release_template(config: &Config, release: &CachedRelease) -> Result<String> {
    let mut context = Context::new();
    
    // Add all release fields to context
    context.insert("album", &release.title);
    context.insert("release_year", &release.release_year.unwrap_or(0));
    context.insert("release_type", &release.release_type);
    context.insert("multi_disc_flag", &config.multi_disc_toggle_flag);
    // Add more fields...
    
    let rendered = TEMPLATE_ENGINE
        .render_str(&config.path_templates.release, &context)
        .map_err(|e| RoseExpectedError::InvalidPathTemplate { 
            message: e.to_string() 
        })?;
    
    Ok(sanitize_filename(&rendered))
}

pub fn execute_track_template(config: &Config, track: &CachedTrack) -> Result<String> {
    let mut context = Context::new();
    
    context.insert("track_number", &track.track_number);
    context.insert("title", &track.title);
    context.insert("disc_number", &track.disc_number);
    // Add more fields...
    
    let rendered = TEMPLATE_ENGINE
        .render_str(&config.path_templates.track, &context)
        .map_err(|e| RoseExpectedError::InvalidPathTemplate { 
            message: e.to_string() 
        })?;
    
    Ok(sanitize_filename(&rendered))
}

fn filter_sanitize(value: &tera::Value, _: &HashMap<String, tera::Value>) -> tera::Result<tera::Value> {
    if let Some(s) = value.as_str() {
        Ok(tera::Value::String(sanitize_filename(s)))
    } else {
        Ok(value.clone())
    }
}
```

## Phase 4: Rule Parser (Week 2, Days 3-5)

### Checkpoint 4.1: Rule DSL Parser

#### Tests to Port (All 44 from rule_parser_test.py)
```rust
// tests/rule_parser_test.rs

mod tokenizer {
    use rose_rs::rule_parser::*;
    
    #[test]
    fn test_tokenize_single_value() {
        let tokens = tokenize("artist:foo").unwrap();
        assert_eq!(tokens, vec![
            Token::Field("artist".into()),
            Token::Colon,
            Token::Value("foo".into())
        ]);
    }
    
    #[test]
    fn test_tokenize_multi_value() {
        let tokens = tokenize("artist:foo,bar").unwrap();
        assert_eq!(tokens, vec![
            Token::Field("artist".into()),
            Token::Colon,
            Token::Value("foo".into()),
            Token::Comma,
            Token::Value("bar".into())
        ]);
    }
    
    #[test]
    fn test_tokenize_quoted_value() {
        let tokens = tokenize(r#"artist:"foo:bar""#).unwrap();
        assert_eq!(tokens, vec![
            Token::Field("artist".into()),
            Token::Colon,
            Token::Value("foo:bar".into())
        ]);
    }
    
    #[test]
    fn test_tokenize_regex() {
        let tokens = tokenize("artist:/foo.*/").unwrap();
        assert_eq!(tokens, vec![
            Token::Field("artist".into()),
            Token::Colon,
            Token::Regex("foo.*".into())
        ]);
    }
    
    // ... implement all 44 tests
}

mod parser {
    #[test]
    fn test_parse_tag() {
        let (matcher, _) = parse_rule("artist:BLACKPINK").unwrap();
        // Verify matcher structure
    }
    
    #[test]
    fn test_parse_action_replace() {
        let (_, actions) = parse_rule("artist:BLACKPINK artist:='Blackpink'").unwrap();
        assert_eq!(actions.len(), 1);
    }
}
```

#### Implementation - `src/rule_parser.rs`
```rust
use crate::common::{Result, RoseExpectedError};
use regex::Regex;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Field(String),
    Colon,
    Equals,
    Plus,
    Slash,
    Value(String),
    Regex(String),
    Comma,
    And,
    Or,
    Not,
    LeftParen,
    RightParen,
}

pub fn tokenize(input: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    
    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' => { chars.next(); }
            ':' => {
                chars.next();
                tokens.push(Token::Colon);
            }
            '=' => {
                chars.next();
                tokens.push(Token::Equals);
            }
            '+' => {
                chars.next();
                tokens.push(Token::Plus);
            }
            ',' => {
                chars.next();
                tokens.push(Token::Comma);
            }
            '(' => {
                chars.next();
                tokens.push(Token::LeftParen);
            }
            ')' => {
                chars.next();
                tokens.push(Token::RightParen);
            }
            '/' => {
                chars.next();
                // Parse regex
                let mut regex = String::new();
                let mut escaped = false;
                
                while let Some(ch) = chars.next() {
                    if escaped {
                        regex.push(ch);
                        escaped = false;
                    } else if ch == '\\' {
                        escaped = true;
                        regex.push(ch);
                    } else if ch == '/' {
                        break;
                    } else {
                        regex.push(ch);
                    }
                }
                tokens.push(Token::Regex(regex));
            }
            '"' => {
                chars.next();
                // Parse quoted string
                let mut value = String::new();
                let mut escaped = false;
                
                while let Some(ch) = chars.next() {
                    if escaped {
                        value.push(ch);
                        escaped = false;
                    } else if ch == '\\' {
                        escaped = true;
                    } else if ch == '"' {
                        break;
                    } else {
                        value.push(ch);
                    }
                }
                tokens.push(Token::Value(value));
            }
            _ => {
                // Parse field or value
                let mut word = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                        word.push(ch);
                        chars.next();
                    } else {
                        break;
                    }
                }
                
                // Check for keywords
                match word.as_str() {
                    "and" => tokens.push(Token::And),
                    "or" => tokens.push(Token::Or),
                    "not" => tokens.push(Token::Not),
                    _ => {
                        // Determine if field or value based on next token
                        if chars.peek() == Some(&':') {
                            tokens.push(Token::Field(word));
                        } else {
                            tokens.push(Token::Value(word));
                        }
                    }
                }
            }
        }
    }
    
    Ok(tokens)
}

#[derive(Debug, Clone)]
pub enum Matcher {
    Tag { field: String, pattern: Pattern },
    Release(Box<Matcher>),
    Track(Box<Matcher>),
    And(Box<Matcher>, Box<Matcher>),
    Or(Box<Matcher>, Box<Matcher>),
    Not(Box<Matcher>),
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Exact(String),
    Regex(Regex),
    List(Vec<String>),
}

#[derive(Debug, Clone)]
pub enum Action {
    Replace { field: String, value: String },
    Add { field: String, value: String },
    Delete { field: String },
    DeleteTag { field: String },
    Split { field: String, delimiter: String },
    Sed { field: String, find: Regex, replace: String, flags: SedFlags },
}

#[derive(Debug, Clone, Default)]
pub struct SedFlags {
    pub global: bool,
    pub case_insensitive: bool,
}

pub fn parse_rule(input: &str) -> Result<(Matcher, Vec<Action>)> {
    let tokens = tokenize(input)?;
    // Parse matcher and actions from tokens
    // This is complex - implement the full parser here
    todo!("Implement full parser")
}
```

## Phase 5: Audio Tags (Week 2, Day 6 - Week 3, Day 2)

### Checkpoint 5.1: Audio Metadata

#### Tests to Port
From `audiotags_test.py`:
```rust
// tests/audiotags_test.rs
use tempfile::TempDir;
use std::fs;

#[test]
fn test_mp3() {
    let td = TempDir::new().unwrap();
    let test_file = td.path().join("test.mp3");
    fs::copy("tests/testdata/Tagger/track1.mp3", &test_file).unwrap();
    
    let mut tags = read_tags(&test_file).unwrap();
    assert_eq!(tags.title(), Some("Test Title"));
    
    tags.set_title(Some("New Title")).unwrap();
    tags.flush(&test_file).unwrap();
    
    let tags2 = read_tags(&test_file).unwrap();
    assert_eq!(tags2.title(), Some("New Title"));
}

// Similar tests for m4a, ogg, opus, flac...

#[test]
fn test_preserve_unknown_tags() {
    // Ensure unknown tags are preserved through read/write cycle
}

#[test]
fn test_multi_value_artists() {
    // Test multiple artists in different roles
}
```

#### Implementation - `src/audiotags.rs`
```rust
use crate::common::{Artist, ArtistMapping, Result, RoseExpectedError};
use std::path::Path;
use std::collections::HashMap;
use serde_json::Value;

pub trait AudioTags: Send + Sync {
    fn can_write(&self) -> bool { true }
    
    // Getters
    fn title(&self) -> Option<&str>;
    fn album(&self) -> Option<&str>;
    fn artist(&self) -> Option<ArtistMapping>;
    fn date(&self) -> Option<i32>;
    fn track_number(&self) -> Option<&str>;
    fn disc_number(&self) -> Option<&str>;
    fn duration_seconds(&self) -> Option<i32>;
    fn roseid(&self) -> Option<&str>;
    
    // Setters
    fn set_title(&mut self, value: Option<&str>) -> Result<()>;
    fn set_album(&mut self, value: Option<&str>) -> Result<()>;
    fn set_artist(&mut self, value: ArtistMapping) -> Result<()>;
    fn set_roseid(&mut self, id: &str) -> Result<()>;
    
    // Serialization
    fn dump(&self) -> HashMap<String, Value>;
    fn flush(&mut self, path: &Path) -> Result<()>;
}

pub fn read_tags(path: &Path) -> Result<Box<dyn AudioTags>> {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .ok_or_else(|| RoseExpectedError::UnsupportedAudioFormat {
            format: path.display().to_string()
        })?;
    
    match ext.to_lowercase().as_str() {
        "mp3" => Ok(Box::new(crate::audiotags_id3::ID3Tags::from_file(path)?)),
        "m4a" | "mp4" => Ok(Box::new(crate::audiotags_mp4::MP4Tags::from_file(path)?)),
        "ogg" => Ok(Box::new(crate::audiotags_vorbis::VorbisTags::from_file(path)?)),
        "opus" => Ok(Box::new(crate::audiotags_vorbis::OpusTags::from_file(path)?)),
        "flac" => Ok(Box::new(crate::audiotags_flac::FLACTags::from_file(path)?)),
        _ => Err(RoseExpectedError::UnsupportedAudioFormat {
            format: ext.to_string()
        }.into()),
    }
}

// Helper functions used by implementations
pub fn parse_artists(s: &str) -> Vec<Artist> {
    s.split(';')
        .map(|a| a.trim())
        .filter(|a| !a.is_empty())
        .map(|a| Artist::new(a))
        .collect()
}

pub fn format_artists(artists: &[Artist]) -> String {
    artists.iter()
        .map(|a| &a.name)
        .collect::<Vec<_>>()
        .join("; ")
}
```

#### Implementation - `src/audiotags_id3.rs`
```rust
use crate::audiotags::{AudioTags, parse_artists, format_artists};
use crate::common::{ArtistMapping, Result};
use id3::{Tag, TagLike};
use std::path::Path;

pub struct ID3Tags {
    tag: Tag,
    path: PathBuf,
}

impl ID3Tags {
    pub fn from_file(path: &Path) -> Result<Self> {
        let tag = Tag::read_from_path(path)?;
        Ok(Self { tag, path: path.to_path_buf() })
    }
}

impl AudioTags for ID3Tags {
    fn title(&self) -> Option<&str> {
        self.tag.title()
    }
    
    fn album(&self) -> Option<&str> {
        self.tag.album()
    }
    
    fn artist(&self) -> Option<ArtistMapping> {
        let mut mapping = ArtistMapping::default();
        
        if let Some(artist) = self.tag.artist() {
            mapping.main = parse_artists(artist);
        }
        
        // Read additional artist frames
        // TPE2 = Album Artist
        // TPE3 = Conductor
        // TPE4 = Remixer
        // TMCL = Musician Credits List
        
        Some(mapping)
    }
    
    fn set_title(&mut self, value: Option<&str>) -> Result<()> {
        if let Some(v) = value {
            self.tag.set_title(v);
        } else {
            self.tag.remove_title();
        }
        Ok(())
    }
    
    fn flush(&mut self, path: &Path) -> Result<()> {
        self.tag.write_to_path(path, id3::Version::Id3v24)?;
        Ok(())
    }
    
    // ... implement other methods
}
```

## Phase 6: Cache Foundation (Week 3, Days 3-7)

### Checkpoint 6.1: Database Schema and Basic Operations

#### Tests to Port (First 30 from cache_test.py)
```rust
// tests/cache_test.rs

#[test]
fn test_create() {
    let config = test_config();
    create_cache(&config).unwrap();
    assert!(config.cache_dir.join("cache.sqlite3").exists());
}

#[test]
fn test_update() {
    let config = test_config();
    let td = test_library();
    
    // Add a release
    let release_dir = td.path().join("Test Release");
    fs::create_dir(&release_dir).unwrap();
    fs::write(release_dir.join("01.mp3"), b"fake").unwrap();
    
    update_cache(&config, false).unwrap();
    
    let releases: Vec<_> = list_releases(&config, None).unwrap().collect();
    assert_eq!(releases.len(), 1);
}

#[test]
fn test_update_releases_and_delete_orphans() {
    // Test orphan cleanup
}

// ... more basic tests
```

#### Implementation - `src/cache.rs`
```rust
use crate::common::{Result, ArtistMapping};
use crate::config::Config;
use rusqlite::{Connection, params};
use std::path::{Path, PathBuf};

// Models
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

#[derive(Debug)]
pub struct UpdateCacheResult {
    pub releases_added: usize,
    pub releases_updated: usize,
    pub releases_deleted: usize,
    pub tracks_added: usize,
    pub tracks_updated: usize,
    pub tracks_deleted: usize,
}

// Include schema
const SCHEMA_SQL: &str = include_str!("cache_schema.sql");

pub fn connect(config: &Config) -> Result<Connection> {
    let path = config.cache_dir.join("cache.sqlite3");
    let conn = Connection::open(&path)?;
    
    // Enable foreign keys and performance settings
    conn.execute_batch("
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA cache_size = -64000;
        PRAGMA temp_store = MEMORY;
    ")?;
    
    Ok(conn)
}

pub fn create_cache(config: &Config) -> Result<()> {
    std::fs::create_dir_all(&config.cache_dir)?;
    let conn = connect(config)?;
    conn.execute_batch(SCHEMA_SQL)?;
    Ok(())
}

pub fn update_cache(config: &Config, force: bool) -> Result<UpdateCacheResult> {
    let release_dirs: Vec<_> = std::fs::read_dir(&config.music_source_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path())
        .collect();
    
    update_cache_for_releases(config, &release_dirs, force)
}

pub fn update_cache_for_releases(
    config: &Config,
    release_dirs: &[PathBuf],
    force: bool,
) -> Result<UpdateCacheResult> {
    let mut conn = connect(config)?;
    let tx = conn.transaction()?;
    
    let mut result = UpdateCacheResult::default();
    
    for dir in release_dirs {
        // Check if update needed
        if !force && !needs_update(&tx, dir)? {
            continue;
        }
        
        // Scan directory for tracks
        let tracks = scan_directory(config, dir)?;
        if tracks.is_empty() {
            continue;
        }
        
        // Compute release metadata
        let release = compute_release_metadata(config, dir, &tracks)?;
        
        // Update database
        if release_exists(&tx, &release.id)? {
            update_release(&tx, &release)?;
            result.releases_updated += 1;
        } else {
            insert_release(&tx, &release)?;
            result.releases_added += 1;
        }
        
        // Update tracks
        for track in tracks {
            if track_exists(&tx, &track.id)? {
                update_track(&tx, &track)?;
                result.tracks_updated += 1;
            } else {
                insert_track(&tx, &track)?;
                result.tracks_added += 1;
            }
        }
    }
    
    // Delete orphans
    let orphans = find_orphan_releases(&tx, &config.music_source_dir)?;
    for orphan_id in orphans {
        delete_release(&tx, &orphan_id)?;
        result.releases_deleted += 1;
    }
    
    tx.commit()?;
    Ok(result)
}

pub fn list_releases(
    config: &Config,
    matcher: Option<&crate::rule_parser::Matcher>,
) -> Result<impl Iterator<Item = CachedRelease>> {
    let conn = connect(config)?;
    
    let query = if let Some(m) = matcher {
        build_release_query(m)?
    } else {
        "SELECT * FROM releases ORDER BY title".to_string()
    };
    
    // This is simplified - need proper iterator implementation
    let releases = Vec::new();
    Ok(releases.into_iter())
}

pub fn get_release(config: &Config, release_id: &str) -> Result<Option<CachedRelease>> {
    let conn = connect(config)?;
    // Query and construct CachedRelease
    todo!()
}

// Helper functions
fn needs_update(conn: &Connection, dir: &Path) -> Result<bool> {
    // Check mtimes
    todo!()
}

fn scan_directory(config: &Config, dir: &Path) -> Result<Vec<CachedTrack>> {
    // Scan for music files and read metadata
    todo!()
}

fn compute_release_metadata(
    config: &Config, 
    dir: &Path, 
    tracks: &[CachedTrack]
) -> Result<CachedRelease> {
    // Aggregate track metadata into release
    todo!()
}

// ... more helper functions
```

#### Implementation - `src/cache_schema.sql`
```sql
-- Direct port from rose-py/rose/cache.sql
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY
);

INSERT INTO schema_version (version) VALUES (1);

CREATE TABLE IF NOT EXISTS releases (
    id TEXT PRIMARY KEY,
    source_path TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    release_type TEXT,
    release_year INTEGER,
    new BOOLEAN NOT NULL DEFAULT 0,
    catalog_number TEXT,
    cover_path TEXT,
    added_at INTEGER NOT NULL,
    mtime INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tracks (
    id TEXT PRIMARY KEY,
    source_path TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    release_id TEXT NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
    track_number TEXT NOT NULL,
    disc_number TEXT NOT NULL,
    duration_seconds INTEGER,
    added_at INTEGER NOT NULL,
    mtime INTEGER NOT NULL
);

-- Artist tables
CREATE TABLE IF NOT EXISTS releases_artists (
    release_id TEXT NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
    artist TEXT NOT NULL,
    alias BOOLEAN NOT NULL,
    role TEXT NOT NULL,
    PRIMARY KEY (release_id, artist, role)
);

-- ... continue with all tables from Python schema
```

## Phase 7: Rules Engine (Week 4, Days 1-3)

### Checkpoint 7.1: Rule Execution

#### Tests to Port (27 from rules_test.py)
```rust
// tests/rules_test.rs

#[test]
fn test_update_tag_constant() {
    let config = test_config();
    let release = create_test_release("Test");
    
    execute_rule(&config, "title:Test title:='New Title'").unwrap();
    
    let updated = get_release(&config, &release.id).unwrap().unwrap();
    assert_eq!(updated.title, "New Title");
}

// ... all 27 tests
```

#### Implementation - `src/rules.rs`
```rust
use crate::cache::{connect, CachedRelease, CachedTrack};
use crate::common::Result;
use crate::config::Config;
use crate::rule_parser::{parse_rule, Matcher, Action};

pub fn execute_rule(config: &Config, rule_str: &str) -> Result<()> {
    let (matcher, actions) = parse_rule(rule_str)?;
    
    // Find matching items
    let release_ids = fast_search_for_matching_releases(config, &matcher)?;
    let track_ids = fast_search_for_matching_tracks(config, &matcher)?;
    
    // Apply actions
    for release_id in release_ids {
        for action in &actions {
            apply_action_to_release(config, &release_id, action)?;
        }
    }
    
    for track_id in track_ids {
        for action in &actions {
            apply_action_to_track(config, &track_id, action)?;
        }
    }
    
    Ok(())
}

pub fn fast_search_for_matching_releases(
    config: &Config,
    matcher: &Matcher,
) -> Result<Vec<String>> {
    // Use FTS to quickly find candidates
    todo!()
}

fn apply_action_to_release(
    config: &Config,
    release_id: &str,
    action: &Action,
) -> Result<()> {
    // Apply action to release
    todo!()
}
```

## Phase 8: Entity Management (Week 4, Days 4-7)

### Checkpoint 8.1: Releases and Tracks

#### Tests to Port
From `releases_test.py` and `tracks_test.py`:
```rust
// tests/releases_test.rs

#[test]
fn test_create_single_release() {
    let config = test_config();
    let tracks = vec![
        test_track_path("track1.mp3"),
        test_track_path("track2.mp3"),
    ];
    
    let id = create_single_release(&config, "Artist", "Title", &tracks).unwrap();
    
    let release = get_release(&config, &id).unwrap().unwrap();
    assert_eq!(release.title, "Title");
}

// ... 8 release tests

// tests/tracks_test.rs
#[test]
fn test_dump_tracks() {
    // Test track export
}
```

#### Implementation - `src/releases.rs`
```rust
use crate::cache::{CachedRelease, update_cache_for_releases};
use crate::common::Result;
use crate::config::Config;
use std::path::{Path, PathBuf};

pub fn create_single_release(
    config: &Config,
    artist: &str,
    title: &str,
    track_paths: &[PathBuf],
) -> Result<String> {
    // Create virtual single release
    todo!()
}

pub fn delete_release(config: &Config, release_id: &str) -> Result<()> {
    // Delete release and files
    todo!()
}

pub fn edit_release(config: &Config, release_id: &str) -> Result<()> {
    // Interactive edit
    todo!()
}

pub fn toggle_release_new(config: &Config, release_id: &str) -> Result<()> {
    // Toggle new flag
    todo!()
}
```

#### Implementation - `src/tracks.rs`
```rust
use crate::cache::CachedTrack;
use crate::common::Result;
use crate::config::Config;

pub fn delete_track(config: &Config, track_id: &str) -> Result<()> {
    // Delete track
    todo!()
}

pub fn edit_track(config: &Config, track_id: &str) -> Result<()> {
    // Edit track metadata
    todo!()
}
```

## Phase 9: Collections (Week 5)

### Checkpoint 9.1: Collages and Playlists

#### Tests to Port
From `collages_test.py` and `playlists_test.py`:
```rust
// tests/collages_test.rs
#[test]
fn test_lifecycle() {
    let config = test_config();
    
    create_collage(&config, "Test Collage").unwrap();
    add_release_to_collage(&config, "Test Collage", "release-id", None).unwrap();
    
    let collage = get_collage(&config, "Test Collage").unwrap().unwrap();
    assert_eq!(collage.releases.len(), 1);
    
    delete_collage(&config, "Test Collage").unwrap();
}

// ... 7 collage tests

// tests/playlists_test.rs
// ... 9 playlist tests
```

#### Implementation - `src/collages.rs`
```rust
use crate::cache::CachedCollage;
use crate::common::Result;
use crate::config::Config;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct CollageFile {
    releases: Vec<CollageRelease>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CollageRelease {
    id: String,
    position: i32,
}

pub fn create_collage(config: &Config, name: &str) -> Result<()> {
    let path = config.music_source_dir.join("!collages").join(format!("{}.toml", name));
    // Create collage file
    todo!()
}

pub fn add_release_to_collage(
    config: &Config,
    collage_name: &str,
    release_id: &str,
    position: Option<i32>,
) -> Result<()> {
    // Add release
    todo!()
}
```

#### Implementation - `src/playlists.rs`
```rust
use crate::cache::CachedPlaylist;
use crate::common::Result;
use crate::config::Config;

pub fn create_playlist(config: &Config, name: &str) -> Result<()> {
    // Create playlist
    todo!()
}

pub fn add_track_to_playlist(
    config: &Config,
    playlist_name: &str,
    track_id: &str,
    position: Option<i32>,
) -> Result<()> {
    // Add track
    todo!()
}
```

## Library Entry Point - `src/lib.rs`

```rust
//! Rose - A music library manager
//! 
//! This crate provides the core functionality of Rose, a music library
//! management system with virtual filesystem support.

pub mod common;
pub mod config;
pub mod genre_hierarchy;
pub mod audiotags;
mod audiotags_id3;
mod audiotags_mp4;
mod audiotags_vorbis;
mod audiotags_flac;
pub mod cache;
pub mod rule_parser;
pub mod rules;
pub mod templates;
pub mod releases;
pub mod tracks;
pub mod collages;
pub mod playlists;

// Re-export main types
pub use common::{Artist, ArtistMapping, RoseError, Result};
pub use config::Config;
pub use cache::{CachedRelease, CachedTrack, update_cache};

/// Library version matching rose-py
pub const VERSION: &str = "0.5.0";
```

## Test Utilities

```rust
// tests/common/mod.rs
use rose_rs::config::Config;
use tempfile::TempDir;
use std::path::PathBuf;

pub fn test_config() -> Config {
    Config {
        music_source_dir: PathBuf::from("/tmp/test-music"),
        cache_dir: PathBuf::from("/tmp/test-cache"),
        max_proc: 1,
        ..Default::default()
    }
}

pub fn test_library() -> TempDir {
    let td = TempDir::new().unwrap();
    // Copy testdata structure
    td
}
```

## Validation Milestones

### Week 1: Foundation Complete
- [ ] All common types match Python
- [ ] Genre hierarchy loaded correctly
- [ ] Configuration parsing works
- [ ] 13 tests passing

### Week 2: Core Infrastructure Complete  
- [ ] Templates render correctly
- [ ] Rule parser handles all syntax
- [ ] Audio tags read/write for all formats
- [ ] 59 tests passing (13 + 2 + 44)

### Week 3: Cache Working
- [ ] Database schema matches Python
- [ ] Basic CRUD operations work
- [ ] 89 tests passing (59 + 30)

### Week 4: Business Logic Complete
- [ ] Rules execute correctly
- [ ] Releases/tracks manageable
- [ ] 124 tests passing (89 + 27 + 8)

### Week 5: Full Feature Parity
- [ ] Collections work
- [ ] All 140 tests passing
- [ ] Python compatibility verified

## Success Criteria

1. **Test Parity**: All tests from rose-py ported and passing
2. **Data Compatibility**: Python and Rust can read each other's cache/config
3. **API Compatibility**: Same public functions with same behavior
4. **Performance**: 2-5x improvement on cache operations
5. **Memory**: 50% reduction in memory usage

This focused plan implements only the core rose-py library with a flat structure, making it simpler to develop and maintain while preserving all essential functionality.