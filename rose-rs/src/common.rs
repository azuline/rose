/// The common module is our ugly grab bag of common toys. Though a fully generalized common module
/// is _typically_ a bad idea, we have few enough things in it that it's OK for now.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::hash::{Hash, Hasher};
use sha2::{Sha256, Digest};
use regex::Regex;
use unicode_normalization::UnicodeNormalization;
use directories::ProjectDirs;
use serde::{Serialize, Deserialize};
use thiserror::Error;

// Version loaded from .version file at compile time
pub const VERSION: &str = include_str!(".version");

#[derive(Error, Debug, Clone)]
#[error("{message}")]
pub struct RoseError {
    pub message: String,
}

#[derive(Error, Debug, Clone)]
#[error("{message}")]
pub struct RoseExpectedError {
    pub message: String,
}

#[derive(Error, Debug, Clone)]
#[error("Genre does not exist: {genre}")]
pub struct GenreDoesNotExistError {
    pub genre: String,
}

#[derive(Error, Debug, Clone)]
#[error("Label does not exist: {label}")]
pub struct LabelDoesNotExistError {
    pub label: String,
}

#[derive(Error, Debug, Clone)]
#[error("Descriptor does not exist: {descriptor}")]
pub struct DescriptorDoesNotExistError {
    pub descriptor: String,
}

#[derive(Error, Debug, Clone)]
#[error("Artist does not exist: {artist}")]
pub struct ArtistDoesNotExistError {
    pub artist: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artist {
    pub name: String,
    #[serde(default)]
    pub alias: bool,
}

impl Hash for Artist {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.alias.hash(state);
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArtistMapping {
    #[serde(default)]
    pub main: Vec<Artist>,
    #[serde(default)]
    pub guest: Vec<Artist>,
    #[serde(default)]
    pub remixer: Vec<Artist>,
    #[serde(default)]
    pub producer: Vec<Artist>,
    #[serde(default)]
    pub composer: Vec<Artist>,
    #[serde(default)]
    pub conductor: Vec<Artist>,
    #[serde(default)]
    pub djmixer: Vec<Artist>,
}

impl ArtistMapping {
    pub fn all(&self) -> Vec<Artist> {
        let mut all = Vec::new();
        all.extend(self.main.clone());
        all.extend(self.guest.clone());
        all.extend(self.remixer.clone());
        all.extend(self.producer.clone());
        all.extend(self.composer.clone());
        all.extend(self.conductor.clone());
        all.extend(self.djmixer.clone());
        uniq(all)
    }

    pub fn dump(&self) -> HashMap<String, Vec<Artist>> {
        let mut map = HashMap::new();
        map.insert("main".to_string(), self.main.clone());
        map.insert("guest".to_string(), self.guest.clone());
        map.insert("remixer".to_string(), self.remixer.clone());
        map.insert("producer".to_string(), self.producer.clone());
        map.insert("composer".to_string(), self.composer.clone());
        map.insert("conductor".to_string(), self.conductor.clone());
        map.insert("djmixer".to_string(), self.djmixer.clone());
        map
    }

    pub fn items(&self) -> Vec<(&str, &Vec<Artist>)> {
        vec![
            ("main", &self.main),
            ("guest", &self.guest),
            ("remixer", &self.remixer),
            ("producer", &self.producer),
            ("composer", &self.composer),
            ("conductor", &self.conductor),
            ("djmixer", &self.djmixer),
        ]
    }
}

pub fn flatten<T: Clone>(xxs: Vec<Vec<T>>) -> Vec<T> {
    let mut xs = Vec::new();
    for group in xxs {
        xs.extend(group);
    }
    xs
}

pub fn uniq<T: Clone + Eq + Hash>(xs: Vec<T>) -> Vec<T> {
    let mut rv = Vec::new();
    let mut seen = HashSet::new();
    for x in xs {
        if seen.insert(x.clone()) {
            rv.push(x);
        }
    }
    rv
}

static ILLEGAL_FS_CHARS_REGEX: OnceLock<Regex> = OnceLock::new();

fn get_illegal_fs_chars_regex() -> &'static Regex {
    ILLEGAL_FS_CHARS_REGEX.get_or_init(|| {
        Regex::new(r#"[:\?<>\\\*\|"/]+"#).unwrap()
    })
}

// Forward declaration for Config struct (will be in config.rs)
pub struct Config {
    pub max_filename_bytes: usize,
}

pub fn sanitize_dirname(c: &Config, name: &str, enforce_maxlen: bool) -> String {
    let regex = get_illegal_fs_chars_regex();
    let mut name = regex.replace_all(name, "_").to_string();
    
    if enforce_maxlen {
        let bytes = name.as_bytes();
        if bytes.len() > c.max_filename_bytes {
            name = String::from_utf8_lossy(&bytes[..c.max_filename_bytes])
                .trim()
                .to_string();
        }
    }
    
    name.nfd().collect::<String>()
}

pub fn sanitize_filename(c: &Config, name: &str, enforce_maxlen: bool) -> String {
    let regex = get_illegal_fs_chars_regex();
    let mut name = regex.replace_all(name, "_").to_string();
    
    if enforce_maxlen {
        // Preserve the extension
        let (stem, ext) = match name.rfind('.') {
            Some(pos) => {
                let (s, e) = name.split_at(pos);
                (s.to_string(), e.to_string())
            },
            None => (name.clone(), String::new()),
        };
        
        // But ignore if the extension is longer than 6 bytes
        let (stem, ext) = if ext.as_bytes().len() > 6 {
            (name.clone(), String::new())
        } else {
            (stem, ext)
        };
        
        let stem_bytes = stem.as_bytes();
        let truncated_stem = if stem_bytes.len() > c.max_filename_bytes {
            String::from_utf8_lossy(&stem_bytes[..c.max_filename_bytes])
                .trim()
                .to_string()
        } else {
            stem
        };
        
        name = format!("{}{}", truncated_stem, ext);
    }
    
    name.nfd().collect::<String>()
}

pub fn sha256_dataclass<T: Serialize>(dc: &T) -> String {
    let mut hasher = Sha256::new();
    _rec_sha256_dataclass(&mut hasher, dc);
    format!("{:x}", hasher.finalize())
}

fn _rec_sha256_dataclass<H: Digest, T: Serialize>(hasher: &mut H, value: &T) {
    // Serialize to JSON for consistent hashing
    let json = serde_json::to_string(value).unwrap_or_default();
    hasher.update(json.as_bytes());
}

// Logging initialization
use tracing_subscriber::{fmt, EnvFilter, prelude::*};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use std::sync::Mutex;

static LOGGING_INITIALIZED: Mutex<HashSet<Option<String>>> = Mutex::new(HashSet::new());

pub fn initialize_logging(logger_name: Option<&str>, output: &str) -> anyhow::Result<()> {
    let mut initialized = LOGGING_INITIALIZED.lock().unwrap();
    let key = logger_name.map(|s| s.to_string());
    if initialized.contains(&key) {
        return Ok(());
    }
    initialized.insert(key);
    drop(initialized);

    let proj_dirs = ProjectDirs::from("", "", "rose")
        .ok_or_else(|| anyhow::anyhow!("Failed to get project directories"))?;
    let log_dir = if cfg!(target_os = "macos") {
        proj_dirs.cache_dir()
    } else {
        proj_dirs.state_dir().unwrap_or(proj_dirs.cache_dir())
    };

    fs::create_dir_all(log_dir)?;
    let log_file_path = log_dir.join("rose.log");

    let log_despite_testing = std::env::var("LOG_TEST").is_ok();
    let is_testing = std::env::var("CARGO_TEST").is_ok();

    if !is_testing || log_despite_testing {
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info"));

        if output == "stderr" {
            let subscriber = fmt::Subscriber::builder()
                .with_env_filter(env_filter)
                .with_target(!log_despite_testing)
                .with_thread_ids(log_despite_testing)
                .with_line_number(log_despite_testing)
                .with_file(log_despite_testing)
                .finish();
            
            tracing::subscriber::set_global_default(subscriber)?;
        } else if output == "file" {
            let file_appender = RollingFileAppender::builder()
                .rotation(Rotation::NEVER)
                .max_log_files(10)
                .filename_prefix("rose")
                .filename_suffix("log")
                .build(log_dir)?;
            
            let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
            
            let subscriber = fmt::Subscriber::builder()
                .with_env_filter(env_filter)
                .with_writer(non_blocking)
                .with_target(true)
                .with_thread_ids(true)
                .with_line_number(true)
                .with_file(true)
                .finish();
            
            tracing::subscriber::set_global_default(subscriber)?;
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flatten() {
        let input = vec![vec![1, 2], vec![3, 4], vec![5]];
        let result = flatten(input);
        assert_eq!(result, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_uniq() {
        let input = vec![1, 2, 2, 3, 1, 4, 3];
        let result = uniq(input);
        assert_eq!(result, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_artist_hash() {
        let artist1 = Artist {
            name: "Test Artist".to_string(),
            alias: false,
        };
        let artist2 = Artist {
            name: "Test Artist".to_string(),
            alias: false,
        };
        let artist3 = Artist {
            name: "Test Artist".to_string(),
            alias: true,
        };

        use std::collections::hash_map::DefaultHasher;
        let mut hasher1 = DefaultHasher::new();
        let mut hasher2 = DefaultHasher::new();
        let mut hasher3 = DefaultHasher::new();
        
        artist1.hash(&mut hasher1);
        artist2.hash(&mut hasher2);
        artist3.hash(&mut hasher3);
        
        assert_eq!(hasher1.finish(), hasher2.finish());
        assert_ne!(hasher1.finish(), hasher3.finish());
    }

    #[test]
    fn test_artist_mapping_all() {
        let mut mapping = ArtistMapping::default();
        mapping.main = vec![
            Artist { name: "Artist1".to_string(), alias: false },
            Artist { name: "Artist2".to_string(), alias: false },
        ];
        mapping.guest = vec![
            Artist { name: "Artist3".to_string(), alias: false },
            Artist { name: "Artist1".to_string(), alias: false }, // Duplicate
        ];
        
        let all = mapping.all();
        assert_eq!(all.len(), 3); // Should be unique
        assert!(all.contains(&Artist { name: "Artist1".to_string(), alias: false }));
        assert!(all.contains(&Artist { name: "Artist2".to_string(), alias: false }));
        assert!(all.contains(&Artist { name: "Artist3".to_string(), alias: false }));
    }

    #[test]
    fn test_sanitize_dirname() {
        let config = Config {
            max_filename_bytes: 20,
        };
        
        // Test illegal characters replacement
        assert_eq!(sanitize_dirname(&config, "test:file?", false), "test_file_");
        assert_eq!(sanitize_dirname(&config, "test<>file", false), "test__file");
        
        // Test truncation
        assert_eq!(
            sanitize_dirname(&config, "this_is_a_very_long_filename_that_should_be_truncated", true).len(),
            20
        );
    }

    #[test]
    fn test_sanitize_filename() {
        let config = Config {
            max_filename_bytes: 20,
        };
        
        // Test with extension preservation
        assert_eq!(sanitize_filename(&config, "test:file?.mp3", false), "test_file_.mp3");
        
        // Test truncation with extension
        let long_name = "very_long_filename_that_needs_truncation.mp3";
        let result = sanitize_filename(&config, long_name, true);
        assert!(result.ends_with(".mp3"));
        assert!(result.len() <= 24); // 20 + ".mp3"
        
        // Test with very long extension (should be ignored)
        let long_ext = "file.verylongextension";
        let result = sanitize_filename(&config, long_ext, true);
        assert!(!result.contains("."));
    }

    #[test]
    fn test_sha256_dataclass() {
        #[derive(Serialize)]
        struct TestStruct {
            field1: String,
            field2: i32,
        }
        
        let test1 = TestStruct {
            field1: "hello".to_string(),
            field2: 42,
        };
        let test2 = TestStruct {
            field1: "hello".to_string(),
            field2: 42,
        };
        let test3 = TestStruct {
            field1: "world".to_string(),
            field2: 42,
        };
        
        assert_eq!(sha256_dataclass(&test1), sha256_dataclass(&test2));
        assert_ne!(sha256_dataclass(&test1), sha256_dataclass(&test3));
    }
}
