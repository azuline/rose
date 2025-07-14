use crate::error::{RoseError, RoseExpectedError};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Configuration file not found ({0})")]
    NotFound(PathBuf),

    #[error("Failed to decode configuration file: invalid TOML: {0}")]
    DecodeError(String),

    #[error("Missing key {0} in configuration file")]
    MissingKey(String),

    #[error("Invalid value for {key}: {message}")]
    InvalidValue { key: String, message: String },
}

impl From<ConfigError> for RoseError {
    fn from(err: ConfigError) -> Self {
        RoseError::Expected(RoseExpectedError::Generic(err.to_string()))
    }
}

// PathTemplate wraps a template string
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathTemplate(pub String);

impl FromStr for PathTemplate {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(PathTemplate(s.to_string()))
    }
}

impl<'de> Deserialize<'de> for PathTemplate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(PathTemplate(s))
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PathTemplateTriad {
    pub release: PathTemplate,
    pub track: PathTemplate,
    pub all_tracks: PathTemplate,
}

impl Default for PathTemplateTriad {
    fn default() -> Self {
        Self {
            release: PathTemplate("{{ albumartist }}/{{ title }}".to_string()),
            track: PathTemplate("{{ trackartist }} - {{ tracktitle }}".to_string()),
            all_tracks: PathTemplate("All Tracks - {{ albumartist }} - {{ title }}".to_string()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PathTemplates {
    #[serde(default)]
    pub source: PathTemplateTriad,
    #[serde(default)]
    pub releases: PathTemplateTriad,
    #[serde(default)]
    pub releases_new: PathTemplateTriad,
    #[serde(default)]
    pub releases_added_on: PathTemplateTriad,
    #[serde(default)]
    pub releases_released_on: PathTemplateTriad,
    #[serde(default)]
    pub artists: PathTemplateTriad,
    #[serde(default)]
    pub genres: PathTemplateTriad,
    #[serde(default)]
    pub descriptors: PathTemplateTriad,
    #[serde(default)]
    pub labels: PathTemplateTriad,
    #[serde(default)]
    pub loose_tracks: PathTemplateTriad,
    #[serde(default)]
    pub collages: PathTemplateTriad,
    #[serde(default = "default_playlists_template")]
    pub playlists: PathTemplate,
}

fn default_playlists_template() -> PathTemplate {
    PathTemplate("{{ title }}".to_string())
}

impl Default for PathTemplates {
    fn default() -> Self {
        Self {
            source: PathTemplateTriad::default(),
            releases: PathTemplateTriad::default(),
            releases_new: PathTemplateTriad::default(),
            releases_added_on: PathTemplateTriad::default(),
            releases_released_on: PathTemplateTriad::default(),
            artists: PathTemplateTriad::default(),
            genres: PathTemplateTriad::default(),
            descriptors: PathTemplateTriad::default(),
            labels: PathTemplateTriad::default(),
            loose_tracks: PathTemplateTriad::default(),
            collages: PathTemplateTriad::default(),
            playlists: default_playlists_template(),
        }
    }
}

// Placeholder for Rule - will be implemented in milestone 6
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Rule {
    pub matcher: String,
    pub actions: Vec<String>,
    #[serde(default)]
    pub ignore: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArtistAlias {
    pub artist: String,
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VirtualFSConfig {
    pub mount_dir: PathBuf,

    #[serde(default)]
    pub artists_whitelist: Option<Vec<String>>,
    #[serde(default)]
    pub genres_whitelist: Option<Vec<String>>,
    #[serde(default)]
    pub descriptors_whitelist: Option<Vec<String>>,
    #[serde(default)]
    pub labels_whitelist: Option<Vec<String>>,

    #[serde(default)]
    pub artists_blacklist: Option<Vec<String>>,
    #[serde(default)]
    pub genres_blacklist: Option<Vec<String>>,
    #[serde(default)]
    pub descriptors_blacklist: Option<Vec<String>>,
    #[serde(default)]
    pub labels_blacklist: Option<Vec<String>>,

    #[serde(default)]
    pub hide_genres_with_only_new_releases: bool,
    #[serde(default)]
    pub hide_descriptors_with_only_new_releases: bool,
    #[serde(default)]
    pub hide_labels_with_only_new_releases: bool,
}

impl Default for VirtualFSConfig {
    fn default() -> Self {
        Self {
            mount_dir: PathBuf::from("/mnt/virtual"),
            artists_whitelist: None,
            genres_whitelist: None,
            descriptors_whitelist: None,
            labels_whitelist: None,
            artists_blacklist: None,
            genres_blacklist: None,
            descriptors_blacklist: None,
            labels_blacklist: None,
            hide_genres_with_only_new_releases: false,
            hide_descriptors_with_only_new_releases: false,
            hide_labels_with_only_new_releases: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfigToml {
    pub music_source_dir: PathBuf,
    #[serde(default)]
    pub cache_dir: Option<PathBuf>,
    #[serde(default)]
    pub max_proc: Option<i32>,
    #[serde(default)]
    pub ignore_release_directories: Vec<String>,
    #[serde(default)]
    pub rename_source_files: bool,
    #[serde(default = "default_max_filename_bytes")]
    pub max_filename_bytes: usize,
    #[serde(default = "default_cover_art_stems")]
    pub cover_art_stems: Vec<String>,
    #[serde(default = "default_valid_art_exts")]
    pub valid_art_exts: Vec<String>,
    #[serde(default)]
    pub write_parent_genres: bool,
    #[serde(default)]
    pub artist_aliases: Vec<ArtistAlias>,
    #[serde(default)]
    pub stored_metadata_rules: Vec<Rule>,
    #[serde(default)]
    pub path_templates: PathTemplates,
    pub vfs: VirtualFSConfig,
}

fn default_max_filename_bytes() -> usize {
    180
}

fn default_cover_art_stems() -> Vec<String> {
    vec![
        "folder".to_string(),
        "cover".to_string(),
        "art".to_string(),
        "front".to_string(),
    ]
}

fn default_valid_art_exts() -> Vec<String> {
    vec!["jpg".to_string(), "jpeg".to_string(), "png".to_string()]
}

#[derive(Debug, Clone)]
pub struct Config {
    pub music_source_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub max_proc: usize,
    pub ignore_release_directories: Vec<String>,
    pub rename_source_files: bool,
    pub max_filename_bytes: usize,
    pub cover_art_stems: Vec<String>,
    pub valid_art_exts: Vec<String>,
    pub write_parent_genres: bool,
    pub artist_aliases_map: HashMap<String, Vec<String>>,
    pub artist_aliases_parents_map: HashMap<String, Vec<String>>,
    pub path_templates: PathTemplates,
    pub stored_metadata_rules: Vec<Rule>,
    pub vfs: VirtualFSConfig,
}

impl Config {
    /// Get the default config path for the platform
    pub fn default_config_path() -> PathBuf {
        let config_dir = dirs::config_dir()
            .expect("Could not determine config directory")
            .join("rose");

        // Create config dir if it doesn't exist
        std::fs::create_dir_all(&config_dir).ok();

        config_dir.join("config.toml")
    }

    /// Get the default cache directory for the platform
    pub fn default_cache_dir() -> PathBuf {
        let cache_dir = dirs::cache_dir()
            .expect("Could not determine cache directory")
            .join("rose");

        // Create cache dir if it doesn't exist
        std::fs::create_dir_all(&cache_dir).ok();

        cache_dir
    }

    /// Parse config from a file path
    pub fn parse(config_path: Option<&Path>) -> Result<Self, ConfigError> {
        let config_path = config_path
            .map(PathBuf::from)
            .unwrap_or_else(Self::default_config_path);

        let config_text = std::fs::read_to_string(&config_path)
            .map_err(|_| ConfigError::NotFound(config_path.clone()))?;

        let mut config_toml: ConfigToml =
            toml::from_str(&config_text).map_err(|e| ConfigError::DecodeError(e.to_string()))?;

        // Expand home directory in paths
        config_toml.music_source_dir = expand_home(&config_toml.music_source_dir);
        if let Some(cache_dir) = &mut config_toml.cache_dir {
            *cache_dir = expand_home(cache_dir);
        }
        config_toml.vfs.mount_dir = expand_home(&config_toml.vfs.mount_dir);

        // Set defaults
        let cache_dir = config_toml
            .cache_dir
            .unwrap_or_else(Self::default_cache_dir);

        let max_proc = match config_toml.max_proc {
            Some(p) if p <= 0 => {
                return Err(ConfigError::InvalidValue {
                    key: "max_proc".to_string(),
                    message: "must be a positive integer".to_string(),
                })
            }
            Some(p) => p as usize,
            None => std::cmp::max(1, num_cpus() / 2),
        };

        let cover_art_stems: Vec<String> = config_toml
            .cover_art_stems
            .into_iter()
            .map(|s| s.to_lowercase())
            .collect();
        let valid_art_exts: Vec<String> = config_toml
            .valid_art_exts
            .into_iter()
            .map(|s| s.to_lowercase())
            .collect();

        // Build artist alias maps
        let mut artist_aliases_map = HashMap::new();
        let mut artist_aliases_parents_map: HashMap<String, Vec<String>> = HashMap::new();

        for alias_entry in &config_toml.artist_aliases {
            artist_aliases_map.insert(alias_entry.artist.clone(), alias_entry.aliases.clone());

            for alias in &alias_entry.aliases {
                artist_aliases_parents_map
                    .entry(alias.clone())
                    .or_default()
                    .push(alias_entry.artist.clone());
            }
        }

        // Create cache directory if it doesn't exist
        std::fs::create_dir_all(&cache_dir).ok();

        Ok(Config {
            music_source_dir: config_toml.music_source_dir,
            cache_dir,
            max_proc,
            ignore_release_directories: config_toml.ignore_release_directories,
            rename_source_files: config_toml.rename_source_files,
            max_filename_bytes: config_toml.max_filename_bytes,
            cover_art_stems,
            valid_art_exts,
            write_parent_genres: config_toml.write_parent_genres,
            artist_aliases_map,
            artist_aliases_parents_map,
            path_templates: config_toml.path_templates,
            stored_metadata_rules: config_toml.stored_metadata_rules,
            vfs: config_toml.vfs,
        })
    }

    pub fn valid_cover_arts(&self) -> Vec<String> {
        self.cover_art_stems
            .iter()
            .flat_map(|stem| {
                self.valid_art_exts
                    .iter()
                    .map(move |ext| format!("{stem}.{ext}"))
            })
            .collect()
    }

    /// Get the cache database path
    pub fn cache_database_path(&self) -> PathBuf {
        self.cache_dir.join("cache.sqlite3")
    }

    /// Get the watchdog PID path
    pub fn watchdog_pid_path(&self) -> PathBuf {
        self.cache_dir.join("watchdog.pid")
    }
}

/// Expand ~ to home directory
fn expand_home(path: &Path) -> PathBuf {
    path.to_str()
        .filter(|s| s.starts_with('~'))
        .and_then(|s| dirs::home_dir().map(|h| h.join(&s[2..])))
        .unwrap_or_else(|| path.to_path_buf())
}

fn num_cpus() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

impl Default for Config {
    fn default() -> Self {
        Self {
            music_source_dir: PathBuf::from("~/Music"),
            cache_dir: Self::default_cache_dir(),
            max_proc: std::cmp::max(1, num_cpus() / 2),
            ignore_release_directories: vec![],
            rename_source_files: false,
            max_filename_bytes: default_max_filename_bytes(),
            cover_art_stems: default_cover_art_stems(),
            valid_art_exts: default_valid_art_exts(),
            write_parent_genres: false,
            artist_aliases_map: HashMap::new(),
            artist_aliases_parents_map: HashMap::new(),
            path_templates: PathTemplates::default(),
            stored_metadata_rules: vec![],
            vfs: VirtualFSConfig {
                mount_dir: PathBuf::from("~/Music/VirtualFS"),
                artists_whitelist: None,
                genres_whitelist: None,
                descriptors_whitelist: None,
                labels_whitelist: None,
                artists_blacklist: None,
                genres_blacklist: None,
                descriptors_blacklist: None,
                labels_blacklist: None,
                hide_genres_with_only_new_releases: false,
                hide_descriptors_with_only_new_releases: false,
                hide_labels_with_only_new_releases: false,
            },
        }
    }
}
