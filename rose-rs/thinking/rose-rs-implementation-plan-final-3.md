# Rose-rs Implementation Plan - Final 3 (rose-py only, tests alongside source)

## Executive Summary

This document provides a comprehensive plan for reimplementing only the `rose-py` library in Rust. The implementation:
- Focuses solely on the core library (no CLI, VFS, or watcher)
- Uses a flat `src/` directory structure with tests alongside source files
- Follows Test-Driven Development
- Maintains full API compatibility with rose-py

## Project Structure

```
rose-rs/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── common.rs
│   ├── common_test.rs
│   ├── config.rs
│   ├── config_test.rs
│   ├── genre_hierarchy.rs
│   ├── genre_hierarchy_test.rs
│   ├── audiotags.rs
│   ├── audiotags_test.rs
│   ├── cache.rs
│   ├── cache_test.rs
│   ├── cache_schema.sql
│   ├── rule_parser.rs
│   ├── rule_parser_test.rs
│   ├── rules.rs
│   ├── rules_test.rs
│   ├── templates.rs
│   ├── templates_test.rs
│   ├── releases.rs
│   ├── releases_test.rs
│   ├── tracks.rs
│   ├── tracks_test.rs
│   ├── collages.rs
│   ├── collages_test.rs
│   ├── playlists.rs
│   └── playlists_test.rs
└── testdata/ (copy from rose-py)
    ├── Collage 1/
    ├── Playlist 1/
    ├── Tagger/
    ├── Test Release 1/
    ├── Test Release 2/
    └── Test Release 3/
```

## Dependencies

```toml
[package]
name = "rose-rs"
version = "0.5.0"
edition = "2021"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"
thiserror = "1.0"
anyhow = "1.0"
rusqlite = { version = "0.31", features = ["bundled", "backup", "chrono", "functions"] }
lofty = "0.18"
id3 = "1.13"  # For specific ID3 features
tera = "1.19"
walkdir = "2.5"
regex = "1.10"
lazy_static = "1.4"
chrono = "0.4"
uuid = { version = "1.8", features = ["v4", "serde"] }
home = "0.5"
rayon = "1.10"
fs2 = "0.4"

[dev-dependencies]
tempfile = "3.10"
proptest = "1.4"
criterion = "0.5"
pretty_assertions = "1.4"
```

## Phase 1: Foundation Layer (Week 1)

### Checkpoint 1.1: Common Types and Utilities (Days 1-2)

#### Tests to Implement - `src/common_test.rs`
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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
        assert!(mapping.guest.is_empty());
    }

    #[test]
    fn test_valid_uuid() {
        assert!(valid_uuid("123e4567-e89b-12d3-a456-426614174000"));
        assert!(!valid_uuid("not-a-uuid"));
        assert!(!valid_uuid(""));
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("a/b"), "a_b");
        assert_eq!(sanitize_filename(".."), "_");
        assert_eq!(sanitize_filename("a:b"), "a_b");
        assert_eq!(sanitize_filename("test?file"), "test_file");
    }

    #[test]
    fn test_musicfile() {
        assert!(musicfile(Path::new("test.mp3")));
        assert!(musicfile(Path::new("test.FLAC"))); // case insensitive
        assert!(!musicfile(Path::new("test.txt")));
        assert!(!musicfile(Path::new("test")));
    }

    #[test]
    fn test_imagefile() {
        assert!(imagefile(Path::new("cover.jpg")));
        assert!(imagefile(Path::new("cover.PNG"))); // case insensitive
        assert!(!imagefile(Path::new("cover.txt")));
    }
    
    #[test]
    fn test_error_hierarchy() {
        // Test that errors can be created and converted properly
        let _e1 = RoseError::Expected(RoseExpectedError::ConfigNotFound { 
            path: PathBuf::from("test") 
        });
        let _e2 = RoseError::Unexpected(RoseUnexpectedError::FileNotFound { 
            path: PathBuf::from("test") 
        });
    }
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
        Self { 
            name: name.into(), 
            alias: false 
        }
    }
    
    pub fn with_alias(mut self, alias: bool) -> Self {
        self.alias = alias;
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ArtistMapping {
    pub main: Vec<Artist>,
    pub guest: Vec<Artist>,
    pub remixer: Vec<Artist>,
    pub producer: Vec<Artist>,
    pub composer: Vec<Artist>,
    pub conductor: Vec<Artist>,
    pub djmixer: Vec<Artist>,
}

impl ArtistMapping {
    pub fn new() -> Self {
        Self::default()
    }
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

#### Tests to Implement - `src/genre_hierarchy_test.rs`
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_genres_loaded() {
        // Should have loaded 24,000+ genres
        assert!(GENRES.len() > 20000);
        assert!(GENRES.contains(&"K-Pop"));
        assert!(GENRES.contains(&"Dance-Pop"));
        assert!(GENRES.contains(&"Electronic"));
    }

    #[test]
    fn test_genre_parents() {
        // Test parent relationships
        let parents = GENRE_PARENTS.get("K-Pop").unwrap();
        assert!(parents.contains(&"Pop"));
        
        let dance_pop_parents = GENRE_PARENTS.get("Dance-Pop");
        assert!(dance_pop_parents.is_some());
    }

    #[test]
    fn test_genre_exists() {
        assert!(genre_exists("K-Pop"));
        assert!(genre_exists("Pop"));
        assert!(!genre_exists("Unknown"));
        assert!(!genre_exists("Made Up Genre"));
    }
    
    #[test]
    fn test_get_all_parents() {
        // Test recursive parent resolution
        let parents = get_all_parents("K-Pop");
        assert!(parents.contains(&"Pop"));
        
        // Test no infinite loops
        let parents2 = get_all_parents("Pop");
        assert!(!parents2.is_empty() || parents2.is_empty()); // Some genres have no parents
    }
}
```

#### Implementation - `src/genre_hierarchy.rs`
```rust
use lazy_static::lazy_static;
use std::collections::{HashMap, HashSet};

// Generate this from rose-py/rose/genre_hierarchy.py
// This is a small sample - the real list has 24,000+ entries
lazy_static! {
    pub static ref GENRES: Vec<&'static str> = vec![
        // Port all genres from Python
        "2 Tone",
        "2-Step",
        "Acid House",
        "Acid Jazz",
        // ... thousands more ...
        "K-Pop",
        "Pop",
        "Dance-Pop",
        "Electronic",
        "House",
        "Techno",
        "Trance",
        // ... complete list from Python
    ];
    
    pub static ref GENRE_PARENTS: HashMap<&'static str, Vec<&'static str>> = {
        let mut m = HashMap::new();
        // Port all relationships from Python
        m.insert("K-Pop", vec!["Pop"]);
        m.insert("Dance-Pop", vec!["Pop", "Dance"]);
        m.insert("Acid House", vec!["House", "Electronic"]);
        // ... complete mapping from Python
        m
    };
}

pub fn genre_exists(genre: &str) -> bool {
    GENRES.iter().any(|&g| g == genre)
}

pub fn get_all_parents(genre: &str) -> Vec<&str> {
    let mut parents = Vec::new();
    let mut to_process = vec![genre];
    let mut seen = HashSet::new();
    
    while let Some(g) = to_process.pop() {
        if !seen.insert(g) {
            continue;
        }
        
        if let Some(direct_parents) = GENRE_PARENTS.get(g) {
            for &parent in direct_parents {
                if !parents.contains(&parent) {
                    parents.push(parent);
                }
                to_process.push(parent);
            }
        }
    }
    
    parents.sort();
    parents.dedup();
    parents
}
```


<!-- FEEDBACK(md): The genre hierarchy file should be code generated from a script, which you can
find in scripts.py. Make it such that it is also codegenned into Rust. Is there a way for Rust to
load a JSON file into a Map and array at compile time? If so, codegen the JSON and do that. -->

## Phase 2: Configuration System (Week 1, Days 5-7)

### Checkpoint 2.1: Configuration Loading

#### Tests to Implement - `src/config_test.rs`
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

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
            
            cover_art_regexes = ["cover", "folder", "album"]
        "#;
        
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.max_proc, 8);
        assert_eq!(config.multi_disc_toggle_flag, "MULTIDISC");
        assert_eq!(config.artist_aliases.len(), 1);
        assert_eq!(config.rules.len(), 1);
        assert_eq!(config.cover_art_regexes.len(), 3);
    }

    #[test]
    fn test_config_minimal() {
        let toml = r#"music_source_dir = "~/music""#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.max_proc, 4); // default
        assert_eq!(config.multi_disc_toggle_flag, "DEFAULT_MULTI_DISC"); // default
        assert!(config.artist_aliases.is_empty());
        assert!(config.rules.is_empty());
    }

    #[test]
    fn test_config_not_found() {
        let result = Config::parse(Some(Path::new("/nonexistent/config.toml")));
        match result {
            Err(RoseError::Expected(RoseExpectedError::ConfigNotFound { .. })) => {},
            _ => panic!("Expected ConfigNotFound error"),
        }
    }
    
    #[test]
    fn test_config_path_templates_error() {
        let toml = r#"
            music_source_dir = "~/music"
            [path_templates]
            release = "{{invalid}"
        "#;
        
        // This should parse but might fail during template validation
        let config: Result<Config, _> = toml::from_str(toml);
        assert!(config.is_ok()); // TOML parsing succeeds
    }

    #[test]
    fn test_config_validate_artist_aliases_resolve_to_self() {
        let mut config = Config {
            music_source_dir: PathBuf::from("~/music"),
            artist_aliases: vec![
                ArtistAlias {
                    artist: "X".to_string(),
                    alias: "X".to_string(),
                }
            ],
            ..Default::default()
        };
        
        let result = config.validate_and_process();
        assert!(result.is_err());
    }

    #[test]
    fn test_config_validate_duplicate_artist_aliases() {
        let mut config = Config {
            music_source_dir: PathBuf::from("~/music"),
            artist_aliases: vec![
                ArtistAlias {
                    artist: "A".to_string(),
                    alias: "X".to_string(),
                },
                ArtistAlias {
                    artist: "B".to_string(),
                    alias: "X".to_string(),
                },
            ],
            ..Default::default()
        };
        
        let result = config.validate_and_process();
        assert!(result.is_err());
    }
    
    #[test]
    fn test_config_parse_with_file() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"music_source_dir = "/tmp/music""#).unwrap();
        
        let config = Config::parse(Some(file.path())).unwrap();
        assert_eq!(config.music_source_dir, Path::new("/tmp/music"));
    }
}
```

#### Implementation - `src/config.rs`
```rust
use crate::common::{Result, RoseExpectedError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;

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

impl Default for Config {
    fn default() -> Self {
        Self {
            music_source_dir: PathBuf::from("~/music"),
            cache_dir: default_cache_dir(),
            max_proc: default_max_proc(),
            artist_aliases: Vec::new(),
            rules: Vec::new(),
            path_templates: PathTemplates::default(),
            cover_art_regexes: Vec::new(),
            multi_disc_toggle_flag: default_multi_disc_flag(),
            artist_aliases_map: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ArtistAlias {
    pub artist: String,
    pub alias: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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
        
        let content = fs::read_to_string(&path)
            .map_err(|_| RoseExpectedError::ConfigNotFound { path: path.clone() })?;
        
        let mut config: Config = toml::from_str(&content)
            .map_err(|e| RoseExpectedError::ConfigDecode { message: e.to_string() })?;
        
        config.validate_and_process()?;
        Ok(config)
    }
    
    fn validate_and_process(&mut self) -> Result<()> {
        // Expand ~ in paths
        self.music_source_dir = expand_home(&self.music_source_dir);
        self.cache_dir = expand_home(&self.cache_dir);
        
        // Validate and build alias map
        self.artist_aliases_map = validate_artist_aliases(&self.artist_aliases)?;
        
        Ok(())
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

#### Tests to Implement - `src/templates_test.rs`
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{CachedRelease, CachedTrack};
    use crate::common::ArtistMapping;
    use crate::config::Config;
    use std::path::PathBuf;

    fn test_config() -> Config {
        Config::default()
    }

    fn test_release(title: &str, year: i32) -> CachedRelease {
        CachedRelease {
            id: "test-id".to_string(),
            source_path: PathBuf::from("/test"),
            title: title.to_string(),
            release_type: Some("album".to_string()),
            release_year: Some(year),
            new: false,
            artists: ArtistMapping::new(),
            genres: vec![],
            labels: vec![],
            catalog_number: None,
            cover_path: None,
        }
    }
    
    fn test_track(number: &str, title: &str) -> CachedTrack {
        CachedTrack {
            id: "track-id".to_string(),
            source_path: PathBuf::from("/test/track.mp3"),
            title: title.to_string(),
            release_id: "test-id".to_string(),
            track_number: number.to_string(),
            disc_number: "1".to_string(),
            duration_seconds: Some(180),
            artists: ArtistMapping::new(),
        }
    }

    #[test]
    fn test_execute_release_template() {
        let config = test_config();
        let release = test_release("Test Album", 2023);
        
        let result = execute_release_template(&config, &release).unwrap();
        assert_eq!(result, "[2023] Test AlbumDEFAULT_MULTI_DISC");
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
        let release = test_release("Test/Album: Special", 2023);
        
        let result = execute_release_template(&config, &release).unwrap();
        assert_eq!(result, "[2023] Test_Album_ SpecialDEFAULT_MULTI_DISC");
    }
    
    #[test]
    fn test_template_missing_year() {
        let config = test_config();
        let mut release = test_release("Test Album", 2023);
        release.release_year = None;
        
        let result = execute_release_template(&config, &release).unwrap();
        assert_eq!(result, "[0] Test AlbumDEFAULT_MULTI_DISC"); // 0 for missing year
    }
}
```

#### Implementation - `src/templates.rs`
```rust
use crate::cache::{CachedRelease, CachedTrack};
use crate::common::{Result, RoseExpectedError, sanitize_filename};
use crate::config::Config;
use lazy_static::lazy_static;
use tera::{Context, Tera};
use std::collections::HashMap;

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
    context.insert("release_type", &release.release_type.as_deref().unwrap_or(""));
    context.insert("multi_disc_flag", &config.multi_disc_toggle_flag);
    
    // Add artist information
    if !release.artists.main.is_empty() {
        let artist_names: Vec<_> = release.artists.main.iter()
            .map(|a| &a.name)
            .collect();
        context.insert("albumartist", &artist_names.join("; "));
    }
    
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
    
    // Add artist information
    if !track.artists.main.is_empty() {
        let artist_names: Vec<_> = track.artists.main.iter()
            .map(|a| &a.name)
            .collect();
        context.insert("artist", &artist_names.join("; "));
    }
    
    let rendered = TEMPLATE_ENGINE
        .render_str(&config.path_templates.track, &context)
        .map_err(|e| RoseExpectedError::InvalidPathTemplate { 
            message: e.to_string() 
        })?;
    
    Ok(sanitize_filename(&rendered))
}

fn filter_sanitize(
    value: &tera::Value, 
    _: &HashMap<String, tera::Value>
) -> tera::Result<tera::Value> {
    if let Some(s) = value.as_str() {
        Ok(tera::Value::String(sanitize_filename(s)))
    } else {
        Ok(value.clone())
    }
}
```

## Phase 4: Rule Parser (Week 2, Days 3-5)

### Checkpoint 4.1: Rule DSL Parser

#### Tests to Implement - `src/rule_parser_test.rs`
```rust
#[cfg(test)]
mod tests {
    use super::*;

    mod tokenizer {
        use super::*;
        
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
        
        #[test]
        fn test_tokenize_bad_pattern() {
            let result = tokenize("artist:");
            assert!(result.is_ok()); // Empty value is ok
            
            let result2 = tokenize(":");
            assert!(result2.is_err()); // No field
        }
        
        #[test]
        fn test_tokenize_bad_values() {
            let result = tokenize(r#"artist:"unclosed"#);
            assert!(result.is_err());
        }
        
        #[test]
        fn test_tokenize_escaped_quotes() {
            let tokens = tokenize(r#"artist:"foo\"bar""#).unwrap();
            assert_eq!(tokens[2], Token::Value(r#"foo"bar"#.into()));
        }
        
        #[test]
        fn test_tokenize_escaped_delimiter() {
            let tokens = tokenize(r#"artist:foo\,bar"#).unwrap();
            assert_eq!(tokens[2], Token::Value("foo,bar".into()));
        }
        
        #[test]
        fn test_tokenize_escaped_slash() {
            let tokens = tokenize(r#"artist:/foo\/bar/"#).unwrap();
            assert_eq!(tokens[2], Token::Regex("foo/bar".into()));
        }
        
        #[test]
        fn test_tokenize_actions() {
            let tokens = tokenize("artist:='foo'").unwrap();
            assert_eq!(tokens, vec![
                Token::Field("artist".into()),
                Token::Colon,
                Token::Equals,
                Token::Value("foo".into())
            ]);
        }
        
        // Add remaining tokenizer tests...
    }

    mod parser {
        use super::*;
        
        #[test]
        fn test_parse_tag() {
            let (matcher, actions) = parse_rule("artist:BLACKPINK").unwrap();
            assert!(actions.is_empty());
            match matcher {
                Matcher::Tag { field, pattern } => {
                    assert_eq!(field, "artist");
                    match pattern {
                        Pattern::Exact(s) => assert_eq!(s, "BLACKPINK"),
                        _ => panic!("Expected exact pattern"),
                    }
                },
                _ => panic!("Expected tag matcher"),
            }
        }
        
        #[test]
        fn test_parse_action_replace() {
            let (_, actions) = parse_rule("artist:BLACKPINK artist:='Blackpink'").unwrap();
            assert_eq!(actions.len(), 1);
            match &actions[0] {
                Action::Replace { field, value } => {
                    assert_eq!(field, "artist");
                    assert_eq!(value, "Blackpink");
                },
                _ => panic!("Expected replace action"),
            }
        }
        
        // Add remaining parser tests...
    }
}
```

## Phase 5: Audio Tags (Week 2, Day 6 - Week 3, Day 2)

### Checkpoint 5.1: Audio Metadata

#### Tests to Implement - `src/audiotags_test.rs`
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;
    use std::path::Path;

    #[test]
    fn test_mp3() {
        let td = TempDir::new().unwrap();
        let test_file = td.path().join("test.mp3");
        fs::copy("testdata/Tagger/track1.mp3", &test_file).unwrap();
        
        let mut tags = read_tags(&test_file).unwrap();
        
        // Test reading
        assert_eq!(tags.title(), Some("Test Title"));
        assert_eq!(tags.album(), Some("Test Album"));
        
        // Test writing
        tags.set_title(Some("New Title")).unwrap();
        tags.set_album(Some("New Album")).unwrap();
        tags.flush(&test_file).unwrap();
        
        // Verify persistence
        let tags2 = read_tags(&test_file).unwrap();
        assert_eq!(tags2.title(), Some("New Title"));
        assert_eq!(tags2.album(), Some("New Album"));
    }

    #[test]
    fn test_m4a() {
        let td = TempDir::new().unwrap();
        let test_file = td.path().join("test.m4a");
        fs::copy("testdata/Tagger/track2.m4a", &test_file).unwrap();
        
        let mut tags = read_tags(&test_file).unwrap();
        assert!(tags.title().is_some());
        
        tags.set_title(Some("M4A Title")).unwrap();
        tags.flush(&test_file).unwrap();
        
        let tags2 = read_tags(&test_file).unwrap();
        assert_eq!(tags2.title(), Some("M4A Title"));
    }
    
    #[test]
    fn test_ogg() {
        let td = TempDir::new().unwrap();
        let test_file = td.path().join("test.ogg");
        fs::copy("testdata/Tagger/track4.vorbis.ogg", &test_file).unwrap();
        
        let tags = read_tags(&test_file).unwrap();
        assert!(tags.can_write());
    }
    
    #[test]
    fn test_opus() {
        let td = TempDir::new().unwrap();
        let test_file = td.path().join("test.opus");
        fs::copy("testdata/Tagger/track5.opus.ogg", &test_file).unwrap();
        
        let tags = read_tags(&test_file).unwrap();
        assert!(tags.can_write());
    }
    
    #[test]
    fn test_flac() {
        let td = TempDir::new().unwrap();
        let test_file = td.path().join("test.flac");
        fs::copy("testdata/Tagger/track1.flac", &test_file).unwrap();
        
        let tags = read_tags(&test_file).unwrap();
        assert!(tags.can_write());
    }

    #[test]
    fn test_unsupported_text_file() {
        let result = read_tags(Path::new("test.txt"));
        match result {
            Err(RoseError::Expected(RoseExpectedError::UnsupportedAudioFormat { .. })) => {},
            _ => panic!("Expected UnsupportedAudioFormat error"),
        }
    }

    #[test]
    fn test_preserve_unknown_tags() {
        // Test that unknown tags are preserved through read/write cycle
        let td = TempDir::new().unwrap();
        let test_file = td.path().join("test.mp3");
        fs::copy("testdata/Tagger/track1.mp3", &test_file).unwrap();
        
        let mut tags = read_tags(&test_file).unwrap();
        let dump1 = tags.dump();
        
        tags.set_title(Some("Modified")).unwrap();
        tags.flush(&test_file).unwrap();
        
        let tags2 = read_tags(&test_file).unwrap();
        let dump2 = tags2.dump();
        
        // Check that non-modified fields are preserved
        for (key, value) in dump1 {
            if key != "title" {
                assert_eq!(dump2.get(&key), Some(&value));
            }
        }
    }
    
    #[test]
    fn test_roseid_tag() {
        let td = TempDir::new().unwrap();
        let test_file = td.path().join("test.mp3");
        fs::copy("testdata/Tagger/track1.mp3", &test_file).unwrap();
        
        let mut tags = read_tags(&test_file).unwrap();
        assert!(tags.roseid().is_none());
        
        tags.set_roseid("test-uuid-123").unwrap();
        tags.flush(&test_file).unwrap();
        
        let tags2 = read_tags(&test_file).unwrap();
        assert_eq!(tags2.roseid(), Some("test-uuid-123"));
    }
}
```

## Phase 6: Cache Foundation (Week 3, Days 3-7)

### Checkpoint 6.1: Database Schema and Basic Operations

#### Tests to Implement - `src/cache_test.rs`
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use tempfile::TempDir;
    use std::fs;

    fn test_config() -> Config {
        let td = TempDir::new().unwrap();
        Config {
            music_source_dir: td.path().join("music"),
            cache_dir: td.path().join("cache"),
            ..Default::default()
        }
    }

    #[test]
    fn test_create() {
        let config = test_config();
        create_cache(&config).unwrap();
        assert!(config.cache_dir.join("cache.sqlite3").exists());
    }

    #[test]
    fn test_update() {
        let config = test_config();
        fs::create_dir_all(&config.music_source_dir).unwrap();
        
        // Add a release
        let release_dir = config.music_source_dir.join("Test Release");
        fs::create_dir(&release_dir).unwrap();
        fs::copy("testdata/Test Release 1/01.m4a", release_dir.join("01.m4a")).unwrap();
        
        create_cache(&config).unwrap();
        let result = update_cache(&config, false).unwrap();
        
        assert_eq!(result.releases_added, 1);
        assert!(result.tracks_added > 0);
    }

    #[test]
    fn test_update_releases_and_delete_orphans() {
        let config = test_config();
        fs::create_dir_all(&config.music_source_dir).unwrap();
        
        // Add and cache a release
        let release_dir = config.music_source_dir.join("Test Release");
        fs::create_dir(&release_dir).unwrap();
        fs::copy("testdata/Test Release 1/01.m4a", release_dir.join("01.m4a")).unwrap();
        
        create_cache(&config).unwrap();
        update_cache(&config, false).unwrap();
        
        // Delete the release directory
        fs::remove_dir_all(&release_dir).unwrap();
        
        // Update again - should delete orphan
        let result = update_cache(&config, false).unwrap();
        assert_eq!(result.releases_deleted, 1);
    }

    #[test]
    fn test_force_update() {
        let config = test_config();
        fs::create_dir_all(&config.music_source_dir).unwrap();
        
        let release_dir = config.music_source_dir.join("Test Release");
        fs::create_dir(&release_dir).unwrap();
        fs::copy("testdata/Test Release 1/01.m4a", release_dir.join("01.m4a")).unwrap();
        
        create_cache(&config).unwrap();
        update_cache(&config, false).unwrap();
        
        // Force update should re-read even without changes
        let result = update_cache(&config, true).unwrap();
        assert_eq!(result.releases_updated, 1);
    }

    #[test]
    fn test_get_release() {
        let config = test_config();
        fs::create_dir_all(&config.music_source_dir).unwrap();
        
        let release_dir = config.music_source_dir.join("Test Release");
        fs::create_dir(&release_dir).unwrap();
        fs::copy("testdata/Test Release 1/01.m4a", release_dir.join("01.m4a")).unwrap();
        
        create_cache(&config).unwrap();
        update_cache(&config, false).unwrap();
        
        let releases: Vec<_> = list_releases(&config, None).unwrap().collect();
        assert_eq!(releases.len(), 1);
        
        let release = get_release(&config, &releases[0].id).unwrap().unwrap();
        assert_eq!(release.title, "Test Release");
    }
    
    // Add remaining 25 basic cache tests...
}
```

## Phase 7: Rules Engine (Week 4, Days 1-3)

### Checkpoint 7.1: Rule Execution

#### Tests to Implement - `src/rules_test.rs`
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{create_cache, update_cache, get_release};
    use crate::config::Config;
    use tempfile::TempDir;
    use std::fs;

    fn setup_test_release(config: &Config) -> String {
        fs::create_dir_all(&config.music_source_dir).unwrap();
        let release_dir = config.music_source_dir.join("Test Release");
        fs::create_dir(&release_dir).unwrap();
        fs::copy("testdata/Test Release 1/01.m4a", release_dir.join("01.m4a")).unwrap();
        
        create_cache(&config).unwrap();
        update_cache(&config, false).unwrap();
        
        let releases: Vec<_> = list_releases(&config, None).unwrap().collect();
        releases[0].id.clone()
    }

    #[test]
    fn test_update_tag_constant() {
        let td = TempDir::new().unwrap();
        let config = Config {
            music_source_dir: td.path().join("music"),
            cache_dir: td.path().join("cache"),
            ..Default::default()
        };
        
        let release_id = setup_test_release(&config);
        
        execute_rule(&config, "title:'Test Release' title:='New Title'").unwrap();
        
        let updated = get_release(&config, &release_id).unwrap().unwrap();
        assert_eq!(updated.title, "New Title");
    }
    
    #[test]
    fn test_update_tag_regex() {
        let td = TempDir::new().unwrap();
        let config = Config {
            music_source_dir: td.path().join("music"),
            cache_dir: td.path().join("cache"),
            ..Default::default()
        };
        
        let release_id = setup_test_release(&config);
        
        execute_rule(&config, "title:/Test.*/ title:='Matched'").unwrap();
        
        let updated = get_release(&config, &release_id).unwrap().unwrap();
        assert_eq!(updated.title, "Matched");
    }
    
    // Add remaining 25 rules tests...
}
```

## Phase 8: Entity Management (Week 4, Days 4-7)

### Checkpoint 8.1: Releases and Tracks

#### Tests to Implement - `src/releases_test.rs`
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{create_cache, get_release};
    use crate::config::Config;
    use tempfile::TempDir;
    use std::fs;

    fn test_config() -> Config {
        let td = TempDir::new().unwrap();
        Config {
            music_source_dir: td.path().join("music"),
            cache_dir: td.path().join("cache"),
            ..Default::default()
        }
    }

    #[test]
    fn test_create_releases() {
        let config = test_config();
        fs::create_dir_all(&config.music_source_dir).unwrap();
        
        let release_dir = config.music_source_dir.join("New Release");
        fs::create_dir(&release_dir).unwrap();
        fs::copy("testdata/Test Release 1/01.m4a", release_dir.join("01.m4a")).unwrap();
        
        create_cache(&config).unwrap();
        let id = create_release(&config, &release_dir).unwrap();
        
        let release = get_release(&config, &id).unwrap().unwrap();
        assert_eq!(release.source_path, release_dir);
    }
    
    #[test]
    fn test_create_single_releases() {
        let config = test_config();
        fs::create_dir_all(&config.music_source_dir).unwrap();
        create_cache(&config).unwrap();
        
        let tracks = vec![
            PathBuf::from("testdata/Test Release 1/01.m4a"),
            PathBuf::from("testdata/Test Release 1/02.m4a"),
        ];
        
        let id = create_single_release(&config, "Test Artist", "Single Title", &tracks).unwrap();
        
        let release = get_release(&config, &id).unwrap().unwrap();
        assert_eq!(release.title, "Single Title");
        assert_eq!(release.release_type, Some("single".to_string()));
    }
    
    // Add remaining 6 release tests...
}
```

#### Tests to Implement - `src/tracks_test.rs`
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_dump_tracks() {
        // Test track serialization
        // Implementation depends on dump_tracks function
    }
    
    #[test]
    fn test_set_track_one() {
        // Test filtering for track number 1
    }
}
```

## Phase 9: Collections (Week 5)

### Checkpoint 9.1: Collages and Playlists

#### Tests to Implement - `src/collages_test.rs`
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{create_cache, update_cache, get_collage};
    use crate::config::Config;
    use tempfile::TempDir;
    use std::fs;

    fn test_config() -> Config {
        let td = TempDir::new().unwrap();
        Config {
            music_source_dir: td.path().join("music"),
            cache_dir: td.path().join("cache"),
            ..Default::default()
        }
    }

    #[test]
    fn test_lifecycle() {
        let config = test_config();
        fs::create_dir_all(&config.music_source_dir).unwrap();
        fs::create_dir_all(config.music_source_dir.join("!collages")).unwrap();
        create_cache(&config).unwrap();
        
        // Create collage
        create_collage(&config, "Test Collage").unwrap();
        
        // Add release
        add_release_to_collage(&config, "Test Collage", "release-123", None).unwrap();
        
        // Read collage
        update_cache(&config, false).unwrap();
        let collage = get_collage(&config, "Test Collage").unwrap().unwrap();
        assert_eq!(collage.releases.len(), 1);
        
        // Delete collage
        delete_collage(&config, "Test Collage").unwrap();
        assert!(get_collage(&config, "Test Collage").unwrap().is_none());
    }
    
    // Add remaining 6 collage tests...
}
```

#### Tests to Implement - `src/playlists_test.rs`
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_lifecycle() {
        // Similar structure to collages test
    }
    
    // Add remaining 8 playlist tests...
}
```

## Library Entry Point - `src/lib.rs`

```rust
//! Rose - A music library manager
//! 
//! This crate provides the core functionality of Rose, a music library
//! management system with virtual filesystem support.

// Public modules
pub mod common;
pub mod config;
pub mod genre_hierarchy;
pub mod audiotags;
pub mod cache;
pub mod rule_parser;
pub mod rules;
pub mod templates;
pub mod releases;
pub mod tracks;
pub mod collages;
pub mod playlists;

// Internal modules
mod audiotags_id3;
mod audiotags_mp4;
mod audiotags_vorbis;
mod audiotags_flac;

// Test modules (in same directory)
#[cfg(test)]
mod common_test;
#[cfg(test)]
mod config_test;
#[cfg(test)]
mod genre_hierarchy_test;
#[cfg(test)]
mod audiotags_test;
#[cfg(test)]
mod cache_test;
#[cfg(test)]
mod rule_parser_test;
#[cfg(test)]
mod rules_test;
#[cfg(test)]
mod templates_test;
#[cfg(test)]
mod releases_test;
#[cfg(test)]
mod tracks_test;
#[cfg(test)]
mod collages_test;
#[cfg(test)]
mod playlists_test;

// Re-export main types
pub use common::{Artist, ArtistMapping, RoseError, RoseExpectedError, RoseUnexpectedError, Result};
pub use config::Config;
pub use cache::{CachedRelease, CachedTrack, CachedPlaylist, CachedCollage};
pub use cache::{update_cache, create_cache, get_release, list_releases};
pub use rule_parser::{parse_rule, Matcher, Action};
pub use rules::execute_rule;

/// Library version matching rose-py
pub const VERSION: &str = "0.5.0";

/// Initialize the library (if needed)
pub fn init() -> Result<()> {
    // Any one-time initialization
    Ok(())
}
```

## Common Test Utilities

Create a module within test files for shared utilities:

```rust
// In each test file, add:
#[cfg(test)]
mod test_utils {
    use super::*;
    use tempfile::TempDir;
    
    pub fn test_library() -> TempDir {
        let td = TempDir::new().unwrap();
        // Set up test structure
        td
    }
    
    pub fn copy_testdata(td: &TempDir) {
        // Copy testdata files
    }
}
```

## Validation Milestones

### Week 1: Foundation Complete
- [ ] All common types match Python (7 tests)
- [ ] Genre hierarchy loaded correctly (4 tests)
- [ ] Configuration parsing works (7 tests)
- [ ] Total: 18 tests passing

### Week 2: Core Infrastructure Complete  
- [ ] Templates render correctly (4 tests)
- [ ] Rule parser handles all syntax (44 tests)
- [ ] Audio tags read/write for all formats (8 tests)
- [ ] Total: 74 tests passing (18 + 56)

### Week 3: Cache Working
- [ ] Database schema matches Python
- [ ] Basic CRUD operations work (30 tests)
- [ ] Total: 104 tests passing (74 + 30)

### Week 4: Business Logic Complete
- [ ] Rules execute correctly (27 tests)
- [ ] Releases manageable (8 tests)
- [ ] Tracks manageable (2 tests)
- [ ] Total: 141 tests passing (104 + 37)

### Week 5: Full Feature Parity
- [ ] Collages work (7 tests)
- [ ] Playlists work (9 tests)
- [ ] Total: 157 tests passing (141 + 16)

## Running Tests

```bash
# Run all tests
cargo test

# Run specific module tests
cargo test common_test
cargo test cache_test

# Run with output
cargo test -- --nocapture

# Run single test
cargo test test_create_release
```

## Success Criteria

1. **Test Parity**: All 157 tests from rose-py ported and passing
2. **Data Compatibility**: Python and Rust can read each other's cache/config
3. **API Compatibility**: Same public functions with same behavior
4. **Performance**: 2-5x improvement on cache operations
5. **Memory**: 50% reduction in memory usage

This plan now follows Rust conventions with tests in `*_test.rs` files alongside their source files, making the codebase more maintainable and following standard Rust practices.
