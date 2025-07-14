/// The common module is our ugly grab bag of common toys. Though a fully generalized common module
/// is _typically_ a bad idea, we have few enough things in it that it's OK for now.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;
use directories::ProjectDirs;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tracing::debug;
use tracing_subscriber::{fmt, EnvFilter};
use unicode_normalization::UnicodeNormalization;

pub const VERSION: &str = include_str!(".version");
#[derive(Error, Debug)]
pub enum RoseError {
    #[error("Rose error: {0}")]
    Generic(String),
    #[error(transparent)]
    Expected(#[from] RoseExpectedError),
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),
}

/// These errors are printed without traceback.
#[derive(Error, Debug, Clone)]
pub enum RoseExpectedError {
    #[error("{0}")]
    Generic(String),
    #[error("Genre does not exist: {name}")]
    GenreDoesNotExist { name: String },
    #[error("Label does not exist: {name}")]
    LabelDoesNotExist { name: String },
    #[error("Descriptor does not exist: {name}")]
    DescriptorDoesNotExist { name: String },
    #[error("Artist does not exist: {name}")]
    ArtistDoesNotExist { name: String },
    #[error("Invalid UUID: {uuid}")]
    InvalidUuid { uuid: String },
    #[error("File not found: {path}")]
    FileNotFound { path: PathBuf },
    #[error("Invalid file format: {format}")]
    InvalidFileFormat { format: String },
    #[error("Release does not exist: {id}")]
    ReleaseDoesNotExist { id: String },
    #[error("Track does not exist: {id}")]
    TrackDoesNotExist { id: String },
    #[error("Collage does not exist: {name}")]
    CollageDoesNotExist { name: String },
    #[error("Playlist does not exist: {name}")]
    PlaylistDoesNotExist { name: String },
}

pub type Result<T> = std::result::Result<T, RoseError>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Artist {
    pub name: String,
    #[serde(default)]
    pub alias: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
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
    pub fn all(&self) -> Vec<Artist> {
        uniq([ &self.main, &self.guest, &self.remixer, &self.producer, &self.composer, &self.conductor, &self.djmixer ]
        .into_iter()
        .flatten()
        .cloned()
        .collect())
    }

    pub fn dump(&self) -> HashMap<String, Vec<Artist>> {
        [
            ("main", &self.main),
            ("guest", &self.guest),
            ("remixer", &self.remixer),
            ("producer", &self.producer),
            ("composer", &self.composer),
            ("conductor", &self.conductor),
            ("djmixer", &self.djmixer),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
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

pub fn uniq<T: Eq + std::hash::Hash + Clone>(xs: Vec<T>) -> Vec<T> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for x in xs {
        if seen.insert(x.clone()) {
            result.push(x);
        }
    }
    result
}

static ILLEGAL_FS_CHARS_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"[:\?<>\\\*\|"/]"#).unwrap()
});

pub struct Config {
    pub max_filename_bytes: usize,
}

/// Replace illegal characters and truncate. We have 255 bytes in ext4, and we truncate to 240 in
/// order to leave room for any collision numbers.
///
/// enforce_maxlen is for host filesystems, which are sometimes subject to length constraints (e.g.
/// ext4).
pub fn sanitize_dirname(c: &Config, name: &str, enforce_maxlen: bool) -> String {
    let mut name = ILLEGAL_FS_CHARS_REGEX.replace_all(name, "_").into_owned();
    
    if enforce_maxlen && name.len() > c.max_filename_bytes {
        name = String::from_utf8_lossy(&name.as_bytes()[..c.max_filename_bytes])
            .trim()
            .to_string();
    }
    
    name.nfd().collect()
}

/// Same as sanitize dirname, except we preserve file extension.
pub fn sanitize_filename(c: &Config, name: &str, enforce_maxlen: bool) -> String {
    let mut name = ILLEGAL_FS_CHARS_REGEX.replace_all(name, "_").into_owned();
    
    if enforce_maxlen {
        // os.path.splitext returns ("stem", ".ext"), so check extension length including dot
        let (stem, ext) = match name.rfind('.') {
            Some(pos) => {
                let ext = &name[pos..];
                debug!("Found extension '{}' with length {}", ext, ext.as_bytes().len());
                if ext.as_bytes().len() > 6 {
                    (name.as_str(), "")
                } else {
                    (&name[..pos], ext)
                }
            },
            None => (name.as_str(), ""),
        };
        
        debug!("After extension check: stem='{}', ext='{}'", stem, ext);
        
        let stem_bytes = stem.as_bytes();
        let stem = if stem_bytes.len() > c.max_filename_bytes {
            String::from_utf8_lossy(&stem_bytes[..c.max_filename_bytes])
                .trim()
                .to_string()
        } else {
            stem.to_string()
        };
        
        name = format!("{}{}", stem, ext);
        debug!("Final name after enforce_maxlen: '{}'", name);
    }
    
    name.nfd().collect()
}

pub fn sha256_dataclass<T: Serialize>(dc: &T) -> String {
    let json = serde_json::to_string(dc).unwrap_or_default();
    format!("{:x}", Sha256::digest(json.as_bytes()))
}

static LOGGING_INITIALIZED: Lazy<Mutex<HashSet<Option<String>>>> = Lazy::new(|| Mutex::new(HashSet::new()));

pub fn initialize_logging(logger_name: Option<&str>, output: &str) -> Result<()> {
    let mut initialized = LOGGING_INITIALIZED.lock().unwrap();
    if !initialized.insert(logger_name.map(str::to_string)) {
        return Ok(());
    }
    drop(initialized);

    let proj_dirs = ProjectDirs::from("", "", "rose")
        .ok_or_else(|| RoseError::Generic("Failed to get project directories".to_string()))?;
    
    let log_dir = if cfg!(target_os = "macos") {
        proj_dirs.cache_dir()
    } else {
        proj_dirs.state_dir().unwrap_or(proj_dirs.cache_dir())
    };

    std::fs::create_dir_all(log_dir).map_err(RoseError::Io)?;

    let log_despite_testing = std::env::var("LOG_TEST").is_ok();
    let is_testing = std::env::var("CARGO_TEST").is_ok();

    if is_testing && !log_despite_testing {
        return Ok(());
    }

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    match output {
        "stderr" => {
            let subscriber = fmt()
                .with_env_filter(env_filter)
                .with_target(!log_despite_testing)
                .with_thread_ids(log_despite_testing)
                .with_line_number(log_despite_testing)
                .with_file(log_despite_testing)
                .finish();
            
            tracing::subscriber::set_global_default(subscriber)
                .map_err(|e| RoseError::Generic(format!("Failed to set tracing subscriber: {}", e)))?;
        }
        "file" => {
            let file_appender = tracing_appender::rolling::daily(log_dir, "rose.log");
            let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
            
            let subscriber = fmt()
                .with_env_filter(env_filter)
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true)
                .with_line_number(true)
                .with_file(true)
                .finish();
            
            tracing::subscriber::set_global_default(subscriber)
                .map_err(|e| RoseError::Generic(format!("Failed to set tracing subscriber: {}", e)))?;
        }
        _ => {}
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(test)]
    use crate::testing;

    #[test]
    fn test_uniq() {
        let _ = crate::testing::init();
        assert_eq!(uniq(vec![1, 2, 2, 3, 1, 4, 3]), vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_artist_equality() {
        let _ = crate::testing::init();
        let artist1 = Artist { name: "Test".to_string(), alias: false };
        let artist2 = Artist { name: "Test".to_string(), alias: false };
        let artist3 = Artist { name: "Test".to_string(), alias: true };
        
        assert_eq!(artist1, artist2);
        assert_ne!(artist1, artist3);
    }

    #[test]
    fn test_artist_mapping_all() {
        let _ = crate::testing::init();
        let mut mapping = ArtistMapping::default();
        mapping.main = vec![
            Artist { name: "Artist1".to_string(), alias: false },
            Artist { name: "Artist2".to_string(), alias: false },
        ];
        mapping.guest = vec![
            Artist { name: "Artist3".to_string(), alias: false },
            Artist { name: "Artist1".to_string(), alias: false },
        ];
        
        let all = mapping.all();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_sanitize_dirname() {
        let _ = crate::testing::init();
        let config = Config { max_filename_bytes: 20 };
        
        assert_eq!(sanitize_dirname(&config, "test:file?", false), "test_file_");
        assert_eq!(sanitize_dirname(&config, "test<>file", false), "test__file");
        assert!(sanitize_dirname(&config, "a".repeat(30).as_str(), true).len() <= 20);
    }

    #[test]
    fn test_sanitize_filename() {
        let _ = crate::testing::init();
        
        let config = Config { max_filename_bytes: 20 };
        
        assert_eq!(sanitize_filename(&config, "test:file?.mp3", false), "test_file_.mp3");
        
        let result = sanitize_filename(&config, "very_long_filename.mp3", true);
        assert!(result.ends_with(".mp3"));
        assert!(result.len() <= 24);
        
        let result = sanitize_filename(&config, "file.verylongext", true);
        // The extension ".verylongext" is 12 bytes, which is > 6, so it's ignored
        // The whole filename becomes the stem, so the dot remains
        assert_eq!(result, "file.verylongext");
    }

    #[test]
    fn test_sha256_dataclass() {
        let _ = crate::testing::init();
        #[derive(Serialize)]
        struct Test { field: &'static str }
        
        let t1 = Test { field: "hello" };
        let t2 = Test { field: "hello" };
        let t3 = Test { field: "world" };
        
        assert_eq!(sha256_dataclass(&t1), sha256_dataclass(&t2));
        assert_ne!(sha256_dataclass(&t1), sha256_dataclass(&t3));
    }
    
    #[test]
    fn test_with_test_config() {
        // Example of using the test fixtures
        let (config, temp_dir) = testing::config();
        
        // Test that the config was created with expected values
        assert_eq!(config.max_filename_bytes, 180);
        
        // Test that directories were created
        let base_path = temp_dir.path();
        assert!(base_path.join("cache").exists());
        assert!(base_path.join("source").exists());
        assert!(base_path.join("mount").exists());
    }
    
    #[test]
    fn test_seeded_cache() {
        // Test the seeded cache function
        let (config, temp_dir) = testing::seeded_cache();
        
        // Test that the config was created
        assert_eq!(config.max_filename_bytes, 180);
        
        // Test that directories and files were created
        let base_path = temp_dir.path();
        let source_dir = base_path.join("source");
        
        // Check that release directories exist
        assert!(source_dir.join("r1").exists());
        assert!(source_dir.join("r2").exists());
        assert!(source_dir.join("r3").exists());
        assert!(source_dir.join("r4").exists());
        
        // Check that music files exist
        assert!(source_dir.join("r1").join("01.m4a").exists());
        assert!(source_dir.join("r1").join("02.m4a").exists());
        
        // Check that special directories exist
        assert!(source_dir.join("!collages").exists());
        assert!(source_dir.join("!playlists").exists());
        
        // Check that the database exists
        assert!(base_path.join("cache").join("cache.sqlite3").exists());
    }
}
