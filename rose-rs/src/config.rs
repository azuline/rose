/// The config module provides the config spec and parsing logic.
///
/// We take special care to optimize the configuration experience: Rose provides detailed errors when an
/// invalid configuration is detected, and emits warnings when unrecognized keys are found.
use directories::ProjectDirs;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use thiserror::Error;
use toml::Value;
use tracing::warn;

use crate::errors::{Result, RoseError, RoseExpectedError};
use crate::rule_parser::{Rule, RuleSyntaxError};
use crate::templates::{PathTemplate, PathTemplateConfig, DEFAULT_TEMPLATE_PAIR};

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error(transparent)]
    Expected(#[from] RoseExpectedError),
    #[error(transparent)]
    RuleSyntax(#[from] RuleSyntaxError),
}

impl From<ConfigError> for RoseError {
    fn from(err: ConfigError) -> Self {
        match err {
            ConfigError::Expected(e) => RoseError::Expected(e),
            ConfigError::RuleSyntax(e) => RoseError::Generic(e.to_string()),
        }
    }
}

fn get_config_dir() -> Result<PathBuf> {
    let proj_dirs = ProjectDirs::from("", "", "rose").ok_or_else(|| RoseError::Generic("Failed to get project directories".to_string()))?;
    let config_dir = proj_dirs.config_dir().to_path_buf();
    std::fs::create_dir_all(&config_dir)?;
    Ok(config_dir)
}

fn get_cache_dir() -> Result<PathBuf> {
    let proj_dirs = ProjectDirs::from("", "", "rose").ok_or_else(|| RoseError::Generic("Failed to get project directories".to_string()))?;
    let cache_dir = proj_dirs.cache_dir().to_path_buf();
    std::fs::create_dir_all(&cache_dir)?;
    Ok(cache_dir)
}

pub fn get_config_path() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("config.toml"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualFSConfig {
    pub mount_dir: PathBuf,

    pub artists_whitelist: Option<Vec<String>>,
    pub genres_whitelist: Option<Vec<String>>,
    pub descriptors_whitelist: Option<Vec<String>>,
    pub labels_whitelist: Option<Vec<String>>,
    pub artists_blacklist: Option<Vec<String>>,
    pub genres_blacklist: Option<Vec<String>>,
    pub descriptors_blacklist: Option<Vec<String>>,
    pub labels_blacklist: Option<Vec<String>>,

    pub hide_genres_with_only_new_releases: bool,
    pub hide_descriptors_with_only_new_releases: bool,
    pub hide_labels_with_only_new_releases: bool,
}

impl VirtualFSConfig {
    /// Modifies `data` by removing any keys read.
    fn parse(cfgpath: &Path, data: &mut toml::value::Table) -> std::result::Result<VirtualFSConfig, ConfigError> {
        let mount_dir = match data.remove("mount_dir") {
            Some(Value::String(s)) => shellexpand::tilde(&s).into_owned().into(),
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for vfs.mount_dir in configuration file ({}): must be a path",
                    cfgpath.display()
                ))
                .into())
            }
            None => return Err(RoseExpectedError::Generic(format!("Missing key vfs.mount_dir in configuration file ({})", cfgpath.display())).into()),
        };

        let artists_whitelist = match data.remove("artists_whitelist") {
            Some(Value::Array(arr)) => {
                let mut result = Vec::new();
                for v in arr {
                    match v {
                        Value::String(s) => result.push(s),
                        _ => {
                            return Err(RoseExpectedError::Generic(format!(
                                "Invalid value for vfs.artists_whitelist in configuration file ({}): Each artist must be of type str: got {:?}",
                                cfgpath.display(),
                                v.type_str()
                            ))
                            .into())
                        }
                    }
                }
                Some(result)
            }
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for vfs.artists_whitelist in configuration file ({}): Must be a list[str]",
                    cfgpath.display()
                ))
                .into())
            }
            None => None,
        };

        let genres_whitelist = match data.remove("genres_whitelist") {
            Some(Value::Array(arr)) => {
                let mut result = Vec::new();
                for v in arr {
                    match v {
                        Value::String(s) => result.push(s),
                        _ => {
                            return Err(RoseExpectedError::Generic(format!(
                                "Invalid value for vfs.genres_whitelist in configuration file ({}): Each genre must be of type str: got {:?}",
                                cfgpath.display(),
                                v.type_str()
                            ))
                            .into())
                        }
                    }
                }
                Some(result)
            }
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for vfs.genres_whitelist in configuration file ({}): Must be a list[str]",
                    cfgpath.display()
                ))
                .into())
            }
            None => None,
        };

        let descriptors_whitelist = match data.remove("descriptors_whitelist") {
            Some(Value::Array(arr)) => {
                let mut result = Vec::new();
                for v in arr {
                    match v {
                        Value::String(s) => result.push(s),
                        _ => {
                            return Err(RoseExpectedError::Generic(format!(
                                "Invalid value for vfs.descriptors_whitelist in configuration file ({}): Each descriptor must be of type str: got {:?}",
                                cfgpath.display(),
                                v.type_str()
                            ))
                            .into())
                        }
                    }
                }
                Some(result)
            }
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for vfs.descriptors_whitelist in configuration file ({}): Must be a list[str]",
                    cfgpath.display()
                ))
                .into())
            }
            None => None,
        };

        let labels_whitelist = match data.remove("labels_whitelist") {
            Some(Value::Array(arr)) => {
                let mut result = Vec::new();
                for v in arr {
                    match v {
                        Value::String(s) => result.push(s),
                        _ => {
                            return Err(RoseExpectedError::Generic(format!(
                                "Invalid value for vfs.labels_whitelist in configuration file ({}): Each label must be of type str: got {:?}",
                                cfgpath.display(),
                                v.type_str()
                            ))
                            .into())
                        }
                    }
                }
                Some(result)
            }
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for vfs.labels_whitelist in configuration file ({}): Must be a list[str]",
                    cfgpath.display()
                ))
                .into())
            }
            None => None,
        };

        let artists_blacklist = match data.remove("artists_blacklist") {
            Some(Value::Array(arr)) => {
                let mut result = Vec::new();
                for v in arr {
                    match v {
                        Value::String(s) => result.push(s),
                        _ => {
                            return Err(RoseExpectedError::Generic(format!(
                                "Invalid value for vfs.artists_blacklist in configuration file ({}): Each artist must be of type str: got {:?}",
                                cfgpath.display(),
                                v.type_str()
                            ))
                            .into())
                        }
                    }
                }
                Some(result)
            }
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for vfs.artists_blacklist in configuration file ({}): Must be a list[str]",
                    cfgpath.display()
                ))
                .into())
            }
            None => None,
        };

        let genres_blacklist = match data.remove("genres_blacklist") {
            Some(Value::Array(arr)) => {
                let mut result = Vec::new();
                for v in arr {
                    match v {
                        Value::String(s) => result.push(s),
                        _ => {
                            return Err(RoseExpectedError::Generic(format!(
                                "Invalid value for vfs.genres_blacklist in configuration file ({}): Each genre must be of type str: got {:?}",
                                cfgpath.display(),
                                v.type_str()
                            ))
                            .into())
                        }
                    }
                }
                Some(result)
            }
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for vfs.genres_blacklist in configuration file ({}): Must be a list[str]",
                    cfgpath.display()
                ))
                .into())
            }
            None => None,
        };

        let descriptors_blacklist = match data.remove("descriptors_blacklist") {
            Some(Value::Array(arr)) => {
                let mut result = Vec::new();
                for v in arr {
                    match v {
                        Value::String(s) => result.push(s),
                        _ => {
                            return Err(RoseExpectedError::Generic(format!(
                                "Invalid value for vfs.descriptors_blacklist in configuration file ({}): Each descriptor must be of type str: got {:?}",
                                cfgpath.display(),
                                v.type_str()
                            ))
                            .into())
                        }
                    }
                }
                Some(result)
            }
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for vfs.descriptors_blacklist in configuration file ({}): Must be a list[str]",
                    cfgpath.display()
                ))
                .into())
            }
            None => None,
        };

        let labels_blacklist = match data.remove("labels_blacklist") {
            Some(Value::Array(arr)) => {
                let mut result = Vec::new();
                for v in arr {
                    match v {
                        Value::String(s) => result.push(s),
                        _ => {
                            return Err(RoseExpectedError::Generic(format!(
                                "Invalid value for vfs.labels_blacklist in configuration file ({}): Each label must be of type str: got {:?}",
                                cfgpath.display(),
                                v.type_str()
                            ))
                            .into())
                        }
                    }
                }
                Some(result)
            }
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for vfs.labels_blacklist in configuration file ({}): Must be a list[str]",
                    cfgpath.display()
                ))
                .into())
            }
            None => None,
        };

        if artists_whitelist.is_some() && artists_blacklist.is_some() {
            return Err(RoseExpectedError::Generic(format!(
                "Cannot specify both vfs.artists_whitelist and vfs.artists_blacklist in configuration file ({}): must specify only one or the other",
                cfgpath.display()
            ))
            .into());
        }
        if genres_whitelist.is_some() && genres_blacklist.is_some() {
            return Err(RoseExpectedError::Generic(format!(
                "Cannot specify both vfs.genres_whitelist and vfs.genres_blacklist in configuration file ({}): must specify only one or the other",
                cfgpath.display()
            ))
            .into());
        }
        if labels_whitelist.is_some() && labels_blacklist.is_some() {
            return Err(RoseExpectedError::Generic(format!(
                "Cannot specify both vfs.labels_whitelist and vfs.labels_blacklist in configuration file ({}): must specify only one or the other",
                cfgpath.display()
            ))
            .into());
        }

        let hide_genres_with_only_new_releases = match data.remove("hide_genres_with_only_new_releases") {
            Some(Value::Boolean(b)) => b,
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for vfs.hide_genres_with_only_new_releases in configuration file ({}): Must be a bool",
                    cfgpath.display()
                ))
                .into())
            }
            None => false,
        };

        let hide_descriptors_with_only_new_releases = match data.remove("hide_descriptors_with_only_new_releases") {
            Some(Value::Boolean(b)) => b,
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for vfs.hide_descriptors_with_only_new_releases in configuration file ({}): Must be a bool",
                    cfgpath.display()
                ))
                .into())
            }
            None => false,
        };

        let hide_labels_with_only_new_releases = match data.remove("hide_labels_with_only_new_releases") {
            Some(Value::Boolean(b)) => b,
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for vfs.hide_labels_with_only_new_releases in configuration file ({}): Must be a bool",
                    cfgpath.display()
                ))
                .into())
            }
            None => false,
        };

        Ok(VirtualFSConfig {
            mount_dir,
            artists_whitelist,
            genres_whitelist,
            descriptors_whitelist,
            labels_whitelist,
            artists_blacklist,
            genres_blacklist,
            descriptors_blacklist,
            labels_blacklist,
            hide_genres_with_only_new_releases,
            hide_descriptors_with_only_new_releases,
            hide_labels_with_only_new_releases,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub music_source_dir: PathBuf,
    pub cache_dir: PathBuf,
    /// Maximum parallel processes for cache updates. Defaults to nproc/2.
    pub max_proc: usize,
    pub ignore_release_directories: Vec<String>,

    pub rename_source_files: bool,
    pub max_filename_bytes: usize,
    pub cover_art_stems: Vec<String>,
    pub valid_art_exts: Vec<String>,
    pub write_parent_genres: bool,

    /// A map from parent artist -> subartists.
    pub artist_aliases_map: HashMap<String, Vec<String>>,
    /// A map from subartist -> parent artists.
    pub artist_aliases_parents_map: HashMap<String, Vec<String>>,

    pub path_templates: PathTemplateConfig,
    pub stored_metadata_rules: Vec<Rule>,

    pub vfs: VirtualFSConfig,
}

impl Config {
    pub fn parse(config_path_override: Option<&Path>) -> Result<Config> {
        // As we parse, delete consumed values from the data dictionary. If any are left over at the
        // end of the config, warn that unknown config keys were found.
        let cfgpath = match config_path_override {
            Some(p) => p.to_path_buf(),
            None => get_config_path()?,
        };

        let cfgtext = std::fs::read_to_string(&cfgpath).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                RoseExpectedError::Generic(format!("Configuration file not found ({})", cfgpath.display()))
            } else {
                RoseExpectedError::Generic(format!("Failed to read configuration file: {e}"))
            }
        })?;

        let mut data: toml::value::Table =
            toml::from_str(&cfgtext).map_err(|e| RoseExpectedError::Generic(format!("Failed to decode configuration file: invalid TOML: {e}")))?;

        let music_source_dir = match data.remove("music_source_dir") {
            Some(Value::String(s)) => shellexpand::tilde(&s).into_owned().into(),
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for music_source_dir in configuration file ({}): must be a path",
                    cfgpath.display()
                ))
                .into())
            }
            None => return Err(RoseExpectedError::Generic(format!("Missing key music_source_dir in configuration file ({})", cfgpath.display())).into()),
        };

        let cache_dir = match data.remove("cache_dir") {
            Some(Value::String(s)) => {
                let expanded: PathBuf = shellexpand::tilde(&s).into_owned().into();
                std::fs::create_dir_all(&expanded)?;
                expanded
            }
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for cache_dir in configuration file ({}): must be a path",
                    cfgpath.display()
                ))
                .into())
            }
            None => {
                let dir = get_cache_dir()?;
                std::fs::create_dir_all(&dir)?;
                dir
            }
        };

        let max_proc = match data.remove("max_proc") {
            Some(Value::Integer(i)) => {
                if i <= 0 {
                    return Err(RoseExpectedError::Generic(format!(
                        "Invalid value for max_proc in configuration file ({}): must be a positive integer",
                        cfgpath.display()
                    ))
                    .into());
                }
                i as usize
            }
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for max_proc in configuration file ({}): must be a positive integer",
                    cfgpath.display()
                ))
                .into())
            }
            None => std::cmp::max(1, num_cpus::get() / 2),
        };

        let mut artist_aliases_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut artist_aliases_parents_map: HashMap<String, Vec<String>> = HashMap::new();
        if let Some(Value::Array(aliases)) = data.remove("artist_aliases") {
            for entry in aliases {
                let table = entry.as_table().ok_or_else(|| {
                    RoseExpectedError::Generic(format!(
                        "Invalid value for artist_aliases in configuration file ({}): must be a list of {{ artist = str, aliases = list[str] }} records",
                        cfgpath.display()
                    ))
                })?;

                let artist = table
                    .get("artist")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        RoseExpectedError::Generic(format!(
                            "Invalid value for artist_aliases in configuration file ({}): must be a list of {{ artist = str, aliases = list[str] }} records",
                            cfgpath.display()
                        ))
                    })?
                    .to_string();

                let aliases_arr = table.get("aliases").and_then(|v| v.as_array()).ok_or_else(|| {
                    RoseExpectedError::Generic(format!(
                        "Invalid value for artist_aliases in configuration file ({}): must be a list of {{ artist = str, aliases = list[str] }} records",
                        cfgpath.display()
                    ))
                })?;

                let mut aliases_vec = Vec::new();
                for alias in aliases_arr {
                    let alias_str = alias.as_str().ok_or_else(|| {
                        RoseExpectedError::Generic(format!(
                            "Invalid value for artist_aliases in configuration file ({}): must be a list of {{ artist = str, aliases = list[str] }} records",
                            cfgpath.display()
                        ))
                    })?;
                    aliases_vec.push(alias_str.to_string());
                    artist_aliases_parents_map.entry(alias_str.to_string()).or_default().push(artist.clone());
                }
                artist_aliases_map.insert(artist, aliases_vec);
            }
        }

        let cover_art_stems = match data.remove("cover_art_stems") {
            Some(Value::Array(arr)) => {
                let mut result = Vec::new();
                for v in arr {
                    match v {
                        Value::String(s) => result.push(s.to_lowercase()),
                        _ => {
                            return Err(RoseExpectedError::Generic(format!(
                                "Invalid value for cover_art_stems in configuration file ({}): Each cover art stem must be of type str: got {:?}",
                                cfgpath.display(),
                                v.type_str()
                            ))
                            .into())
                        }
                    }
                }
                result
            }
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for cover_art_stems in configuration file ({}): Must be a list[str]",
                    cfgpath.display()
                ))
                .into())
            }
            None => vec!["folder".to_string(), "cover".to_string(), "art".to_string(), "front".to_string()],
        };

        let valid_art_exts = match data.remove("valid_art_exts") {
            Some(Value::Array(arr)) => {
                let mut result = Vec::new();
                for v in arr {
                    match v {
                        Value::String(s) => result.push(s.to_lowercase()),
                        _ => {
                            return Err(RoseExpectedError::Generic(format!(
                                "Invalid value for valid_art_exts in configuration file ({}): Each art extension must be of type str: got {:?}",
                                cfgpath.display(),
                                v.type_str()
                            ))
                            .into())
                        }
                    }
                }
                result
            }
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for valid_art_exts in configuration file ({}): Must be a list[str]",
                    cfgpath.display()
                ))
                .into())
            }
            None => vec!["jpg".to_string(), "jpeg".to_string(), "png".to_string()],
        };

        let write_parent_genres = match data.remove("write_parent_genres") {
            Some(Value::Boolean(b)) => b,
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for write_parent_genres in configuration file ({}): Must be a bool",
                    cfgpath.display()
                ))
                .into())
            }
            None => false,
        };

        let max_filename_bytes = match data.remove("max_filename_bytes") {
            Some(Value::Integer(i)) => i as usize,
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for max_filename_bytes in configuration file ({}): Must be an int",
                    cfgpath.display()
                ))
                .into())
            }
            None => 180,
        };

        let rename_source_files = match data.remove("rename_source_files") {
            Some(Value::Boolean(b)) => b,
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for rename_source_files in configuration file ({}): Must be a bool",
                    cfgpath.display()
                ))
                .into())
            }
            None => false,
        };

        let ignore_release_directories = match data.remove("ignore_release_directories") {
            Some(Value::Array(arr)) => {
                let mut result = Vec::new();
                for v in arr {
                    match v {
                        Value::String(s) => result.push(s),
                        _ => {
                            return Err(RoseExpectedError::Generic(format!(
                                "Invalid value for ignore_release_directories in configuration file ({}): Each release directory must be of type str: got {:?}",
                                cfgpath.display(),
                                v.type_str()
                            ))
                            .into())
                        }
                    }
                }
                result
            }
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!(
                    "Invalid value for ignore_release_directories in configuration file ({}): Must be a list[str]",
                    cfgpath.display()
                ))
                .into())
            }
            None => vec![],
        };

        let mut stored_metadata_rules = Vec::new();
        if let Some(Value::Array(rules)) = data.remove("stored_metadata_rules") {
            for rule_val in rules {
                let rule_table = rule_val.as_table().ok_or_else(|| {
                    RoseExpectedError::Generic(format!(
                        "Invalid value in stored_metadata_rules in configuration file ({}): list values must be a dict",
                        cfgpath.display()
                    ))
                })?;

                let matcher = rule_table.get("matcher").and_then(|v| v.as_str()).ok_or_else(|| {
                    RoseExpectedError::Generic(format!(
                        "Missing key `matcher` in stored_metadata_rules in configuration file ({}): rule {:?}",
                        cfgpath.display(),
                        rule_table
                    ))
                })?;

                let actions = rule_table.get("actions").and_then(|v| v.as_array()).ok_or_else(|| {
                    RoseExpectedError::Generic(format!(
                        "Missing key `actions` in stored_metadata_rules in configuration file ({}): rule {:?}",
                        cfgpath.display(),
                        rule_table
                    ))
                })?;

                let mut actions_vec = Vec::new();
                for action in actions {
                    let action_str = action.as_str().ok_or_else(|| {
                        RoseExpectedError::Generic(format!(
                            "Invalid value for `actions` in stored_metadata_rules in configuration file ({}): rule {:?}: must be a list of strings",
                            cfgpath.display(),
                            rule_table
                        ))
                    })?;
                    actions_vec.push(action_str.to_string());
                }

                let mut ignore_vec = Vec::new();
                if let Some(ignore) = rule_table.get("ignore").and_then(|v| v.as_array()) {
                    for i in ignore {
                        let i_str = i.as_str().ok_or_else(|| {
                            RoseExpectedError::Generic(format!(
                                "Invalid value for `ignore` in stored_metadata_rules in configuration file ({}): rule {:?}: must be a list of strings",
                                cfgpath.display(),
                                rule_table
                            ))
                        })?;
                        ignore_vec.push(i_str.to_string());
                    }
                }

                let rule = Rule::parse(matcher, actions_vec, if ignore_vec.is_empty() { None } else { Some(ignore_vec) }).map_err(|e| {
                    RoseExpectedError::Generic(format!(
                        "Failed to parse stored_metadata_rules in configuration file ({}): rule {:?}: {}",
                        cfgpath.display(),
                        rule_table,
                        e
                    ))
                })?;
                stored_metadata_rules.push(rule);
            }
        }

        // Get the potential default template before evaluating the rest.
        let mut default_templates = DEFAULT_TEMPLATE_PAIR.clone();
        let path_template_config = if let Some(Value::Table(mut path_templates)) = data.remove("path_templates") {
            if let Some(Value::Table(mut default)) = path_templates.remove("default") {
                if let Some(Value::String(s)) = default.remove("release") {
                    default_templates.release = PathTemplate::new(s);
                }
                if let Some(Value::String(s)) = default.remove("track") {
                    default_templates.track = PathTemplate::new(s);
                }
                if let Some(Value::String(s)) = default.remove("all_tracks") {
                    default_templates.all_tracks = PathTemplate::new(s);
                }
            }

            let mut path_template_config = PathTemplateConfig::with_defaults(default_templates.clone());

            // Parse all the other template sections
            for key in &[
                "source",
                "releases",
                "releases_new",
                "releases_added_on",
                "releases_released_on",
                "artists",
                "genres",
                "descriptors",
                "labels",
                "loose_tracks",
                "collages",
            ] {
                if let Some(Value::Table(mut section)) = path_templates.remove(*key) {
                    let field = match *key {
                        "source" => &mut path_template_config.source,
                        "releases" => &mut path_template_config.releases,
                        "releases_new" => &mut path_template_config.releases_new,
                        "releases_added_on" => &mut path_template_config.releases_added_on,
                        "releases_released_on" => &mut path_template_config.releases_released_on,
                        "artists" => &mut path_template_config.artists,
                        "genres" => &mut path_template_config.genres,
                        "descriptors" => &mut path_template_config.descriptors,
                        "labels" => &mut path_template_config.labels,
                        "loose_tracks" => &mut path_template_config.loose_tracks,
                        "collages" => &mut path_template_config.collages,
                        _ => unreachable!(),
                    };

                    if let Some(Value::String(s)) = section.remove("release") {
                        field.release = PathTemplate::new(s);
                    }
                    if let Some(Value::String(s)) = section.remove("track") {
                        field.track = PathTemplate::new(s);
                    }
                    if let Some(Value::String(s)) = section.remove("all_tracks") {
                        field.all_tracks = PathTemplate::new(s);
                    }
                }
            }

            if let Some(Value::String(s)) = path_templates.remove("playlists") {
                path_template_config.playlists = PathTemplate::new(s);
            }

            // Re-add remaining path_templates if any
            if !path_templates.is_empty() {
                data.insert("path_templates".to_string(), Value::Table(path_templates));
            }

            path_template_config
        } else {
            PathTemplateConfig::with_defaults(default_templates.clone())
        };

        let mut vfs_data = match data.remove("vfs") {
            Some(Value::Table(t)) => t,
            Some(_) => {
                return Err(RoseExpectedError::Generic(format!("Invalid value for vfs in configuration file ({}): must be a table", cfgpath.display())).into())
            }
            None => toml::value::Table::new(),
        };

        let vfs = VirtualFSConfig::parse(&cfgpath, &mut vfs_data).map_err(|e| -> RoseError { e.into() })?;

        // Re-add remaining vfs data if any
        if !vfs_data.is_empty() {
            data.insert("vfs".to_string(), Value::Table(vfs_data));
        }

        // Check for unrecognized keys
        if !data.is_empty() {
            let mut unrecognized_accessors = Vec::new();
            // Do a DFS over the data keys to assemble the map of unknown keys. State is a tuple of
            // ("accessor", node).
            let mut dfs_state: VecDeque<(String, &Value)> = VecDeque::new();
            for (k, v) in &data {
                dfs_state.push_back((k.clone(), v));
            }

            while let Some((accessor, node)) = dfs_state.pop_back() {
                match node {
                    Value::Table(t) => {
                        for (k, v) in t {
                            let child_accessor = if accessor.is_empty() { k.clone() } else { format!("{accessor}.{k}") };
                            dfs_state.push_back((child_accessor, v));
                        }
                    }
                    _ => unrecognized_accessors.push(accessor),
                }
            }

            if !unrecognized_accessors.is_empty() {
                warn!("Unrecognized options found in configuration file: {}", unrecognized_accessors.join(", "));
            }
        }

        Ok(Config {
            music_source_dir,
            cache_dir,
            max_proc,
            artist_aliases_map,
            artist_aliases_parents_map,
            cover_art_stems,
            valid_art_exts,
            write_parent_genres,
            max_filename_bytes,
            path_templates: path_template_config,
            rename_source_files,
            ignore_release_directories,
            stored_metadata_rules,
            vfs,
        })
    }

    pub fn valid_cover_arts(&self) -> Vec<String> {
        let mut result = Vec::new();
        for stem in &self.cover_art_stems {
            for ext in &self.valid_art_exts {
                result.push(format!("{stem}.{ext}"));
            }
        }
        result
    }

    pub fn cache_database_path(&self) -> PathBuf {
        self.cache_dir.join("cache.sqlite3")
    }

    pub fn watchdog_pid_path(&self) -> PathBuf {
        self.cache_dir.join("watchdog.pid")
    }

    pub fn validate_path_templates_expensive(&self) -> Result<()> {
        // Validate all the path templates. This is expensive, so we don't do it when reading the
        // configuration, only on demand.
        self.path_templates
            .parse()
            .map_err(|e| RoseExpectedError::Generic(format!("Invalid path template in for template {}: {}", e.key, e)).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule_parser::ActionBehavior;
    use tempfile::TempDir;

    #[test]
    fn test_config_minimal() {
        let tmpdir = TempDir::new().unwrap();
        let path = tmpdir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
                music_source_dir = "~/.music-src"
                vfs.mount_dir = "~/music"
            "#,
        )
        .unwrap();

        let c = Config::parse(Some(&path)).unwrap();
        assert_eq!(c.music_source_dir, PathBuf::from(shellexpand::tilde("~/.music-src").into_owned()));
        assert_eq!(c.vfs.mount_dir, PathBuf::from(shellexpand::tilde("~/music").into_owned()));
    }

    #[test]
    fn test_config_full() {
        let tmpdir = TempDir::new().unwrap();
        let path = tmpdir.path().join("config.toml");
        let cache_dir = tmpdir.path().join("cache");
        std::fs::write(
            &path,
            format!(
                r#"
                music_source_dir = "~/.music-src"
                cache_dir = "{}"
                max_proc = 8
                artist_aliases = [
                  {{ artist = "Abakus", aliases = ["Cinnamon Chasers"] }},
                  {{ artist = "tripleS", aliases = ["EVOLution", "LOVElution", "+(KR)ystal Eyes", "Acid Angel From Asia", "Acid Eyes"] }},
                ]

                cover_art_stems = [ "aa", "bb" ]
                valid_art_exts = [ "tiff" ]
                write_parent_genres = true
                max_filename_bytes = 255
                ignore_release_directories = [ "dummy boy" ]
                rename_source_files = true

                [[stored_metadata_rules]]
                matcher = "tracktitle:lala"
                actions = ["replace:hihi"]

                [[stored_metadata_rules]]
                matcher = "trackartist[main]:haha"
                actions = ["replace:bibi", "split: "]
                ignore = ["releasetitle:blabla"]

                [path_templates]
                default.release = "{{{{ title }}}}"
                default.track = "{{{{ title }}}}"
                default.all_tracks = "{{{{ title }}}}"
                source.release = "{{{{ title }}}}"
                source.track = "{{{{ title }}}}"
                source.all_tracks = "{{{{ title }}}}"
                releases.release = "{{{{ title }}}}"
                releases.track = "{{{{ title }}}}"
                releases.all_tracks = "{{{{ title }}}}"
                releases_new.release = "{{{{ title }}}}"
                releases_new.track = "{{{{ title }}}}"
                releases_new.all_tracks = "{{{{ title }}}}"
                releases_added_on.release = "{{{{ title }}}}"
                releases_added_on.track = "{{{{ title }}}}"
                releases_added_on.all_tracks = "{{{{ title }}}}"
                releases_released_on.release = "{{{{ title }}}}"
                releases_released_on.track = "{{{{ title }}}}"
                releases_released_on.all_tracks = "{{{{ title }}}}"
                artists.release = "{{{{ title }}}}"
                artists.track = "{{{{ title }}}}"
                artists.all_tracks = "{{{{ title }}}}"
                labels.release = "{{{{ title }}}}"
                labels.track = "{{{{ title }}}}"
                labels.all_tracks = "{{{{ title }}}}"
                loose_tracks.release = "{{{{ title }}}}"
                loose_tracks.track = "{{{{ title }}}}"
                loose_tracks.all_tracks = "{{{{ title }}}}"
                collages.release = "{{{{ title }}}}"
                collages.track = "{{{{ title }}}}"
                collages.all_tracks = "{{{{ title }}}}"
                # Genres and descriptors omitted to test the defaults.
                playlists = "{{{{ title }}}}"

                [vfs]
                mount_dir = "~/music"
                artists_blacklist = [ "www" ]
                genres_blacklist = [ "xxx" ]
                descriptors_blacklist = [ "yyy" ]
                labels_blacklist = [ "zzz" ]
                hide_genres_with_only_new_releases = true
                hide_descriptors_with_only_new_releases = true
                hide_labels_with_only_new_releases = true
                "#,
                cache_dir.display()
            ),
        )
        .unwrap();

        let c = Config::parse(Some(&path)).unwrap();

        // Check basic fields
        assert_eq!(c.music_source_dir, PathBuf::from(shellexpand::tilde("~/.music-src").into_owned()));
        assert_eq!(c.cache_dir, cache_dir);
        assert_eq!(c.max_proc, 8);
        assert_eq!(c.cover_art_stems, vec!["aa", "bb"]);
        assert_eq!(c.valid_art_exts, vec!["tiff"]);
        assert!(c.write_parent_genres);
        assert_eq!(c.max_filename_bytes, 255);
        assert!(c.rename_source_files);
        assert_eq!(c.ignore_release_directories, vec!["dummy boy"]);

        // Check artist aliases
        assert_eq!(c.artist_aliases_map.get("Abakus"), Some(&vec!["Cinnamon Chasers".to_string()]));
        assert_eq!(
            c.artist_aliases_map.get("tripleS"),
            Some(&vec![
                "EVOLution".to_string(),
                "LOVElution".to_string(),
                "+(KR)ystal Eyes".to_string(),
                "Acid Angel From Asia".to_string(),
                "Acid Eyes".to_string(),
            ])
        );
        assert_eq!(c.artist_aliases_parents_map.get("Cinnamon Chasers"), Some(&vec!["Abakus".to_string()]));

        // Check stored metadata rules
        assert_eq!(c.stored_metadata_rules.len(), 2);
        assert_eq!(c.stored_metadata_rules[0].matcher.tags, vec![crate::rule_parser::Tag::TrackTitle]);
        assert_eq!(c.stored_metadata_rules[0].matcher.pattern.needle, "lala");
        assert_eq!(c.stored_metadata_rules[0].actions.len(), 1);
        match &c.stored_metadata_rules[0].actions[0].behavior {
            ActionBehavior::Replace(r) => assert_eq!(r.replacement, "hihi"),
            _ => panic!("Expected replace action"),
        }

        // Check VFS config
        assert_eq!(c.vfs.mount_dir, PathBuf::from(shellexpand::tilde("~/music").into_owned()));
        assert_eq!(c.vfs.artists_blacklist, Some(vec!["www".to_string()]));
        assert_eq!(c.vfs.genres_blacklist, Some(vec!["xxx".to_string()]));
        assert_eq!(c.vfs.descriptors_blacklist, Some(vec!["yyy".to_string()]));
        assert_eq!(c.vfs.labels_blacklist, Some(vec!["zzz".to_string()]));
        assert!(c.vfs.hide_genres_with_only_new_releases);
        assert!(c.vfs.hide_descriptors_with_only_new_releases);
        assert!(c.vfs.hide_labels_with_only_new_releases);
    }

    #[test]
    fn test_config_whitelist() {
        let tmpdir = TempDir::new().unwrap();
        let path = tmpdir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
                music_source_dir = "~/.music-src"
                vfs.mount_dir = "~/music"
                vfs.artists_whitelist = [ "www" ]
                vfs.genres_whitelist = [ "xxx" ]
                vfs.descriptors_whitelist = [ "yyy" ]
                vfs.labels_whitelist = [ "zzz" ]
            "#,
        )
        .unwrap();

        let c = Config::parse(Some(&path)).unwrap();
        assert_eq!(c.vfs.artists_whitelist, Some(vec!["www".to_string()]));
        assert_eq!(c.vfs.genres_whitelist, Some(vec!["xxx".to_string()]));
        assert_eq!(c.vfs.descriptors_whitelist, Some(vec!["yyy".to_string()]));
        assert_eq!(c.vfs.labels_whitelist, Some(vec!["zzz".to_string()]));
        assert_eq!(c.vfs.artists_blacklist, None);
        assert_eq!(c.vfs.genres_blacklist, None);
        assert_eq!(c.vfs.descriptors_blacklist, None);
        assert_eq!(c.vfs.labels_blacklist, None);
    }

    #[test]
    fn test_config_not_found() {
        let tmpdir = TempDir::new().unwrap();
        let path = tmpdir.path().join("config.toml");
        let err = Config::parse(Some(&path)).unwrap_err();
        match err {
            RoseError::Expected(RoseExpectedError::Generic(msg)) => {
                assert!(msg.contains("Configuration file not found"));
            }
            _ => panic!("Expected configuration not found error"),
        }
    }

    #[test]
    fn test_config_missing_key_validation() {
        let tmpdir = TempDir::new().unwrap();
        let path = tmpdir.path().join("config.toml");
        std::fs::write(&path, r#"music_source_dir = "/""#).unwrap();

        let err = Config::parse(Some(&path)).unwrap_err();
        match err {
            RoseError::Expected(RoseExpectedError::Generic(msg)) => {
                assert!(msg.contains("Missing key vfs.mount_dir"));
            }
            _ => panic!("Expected missing key error"),
        }
    }

    #[test]
    fn test_config_value_validation() {
        let tmpdir = TempDir::new().unwrap();
        let path = tmpdir.path().join("config.toml");

        // Test music_source_dir validation
        std::fs::write(&path, "music_source_dir = 123").unwrap();
        let err = Config::parse(Some(&path)).unwrap_err();
        match err {
            RoseError::Expected(RoseExpectedError::Generic(msg)) => {
                assert!(msg.contains("Invalid value for music_source_dir"));
                assert!(msg.contains("must be a path"));
            }
            _ => panic!("Expected invalid value error"),
        }

        // Test max_proc validation
        std::fs::write(
            &path,
            r#"
            music_source_dir = "~/.music-src"
            vfs.mount_dir = "~/music"
            max_proc = "lalala"
            "#,
        )
        .unwrap();
        let err = Config::parse(Some(&path)).unwrap_err();
        match err {
            RoseError::Expected(RoseExpectedError::Generic(msg)) => {
                assert!(msg.contains("Invalid value for max_proc"));
                assert!(msg.contains("must be a positive integer"));
            }
            _ => panic!("Expected invalid value error"),
        }

        // Test negative max_proc
        std::fs::write(
            &path,
            r#"
            music_source_dir = "~/.music-src"
            vfs.mount_dir = "~/music"
            max_proc = -1
            "#,
        )
        .unwrap();
        let err = Config::parse(Some(&path)).unwrap_err();
        match err {
            RoseError::Expected(RoseExpectedError::Generic(msg)) => {
                assert!(msg.contains("Invalid value for max_proc"));
                assert!(msg.contains("must be a positive integer"));
            }
            _ => panic!("Expected invalid value error"),
        }
    }

    #[test]
    fn test_vfs_config_value_validation() {
        let tmpdir = TempDir::new().unwrap();
        let path = tmpdir.path().join("config.toml");

        // Test mount_dir validation
        std::fs::write(
            &path,
            r#"
            music_source_dir = "~/.music-src"
            [vfs]
            mount_dir = 123
            "#,
        )
        .unwrap();
        let err = Config::parse(Some(&path)).unwrap_err();
        match err {
            RoseError::Expected(RoseExpectedError::Generic(msg)) => {
                assert!(msg.contains("Invalid value for vfs.mount_dir"));
                assert!(msg.contains("must be a path"));
            }
            _ => panic!("Expected invalid value error"),
        }

        // Test whitelist/blacklist mutual exclusion
        std::fs::write(
            &path,
            r#"
            music_source_dir = "~/.music-src"
            vfs.mount_dir = "~/music"
            vfs.artists_whitelist = ["a"]
            vfs.artists_blacklist = ["b"]
            "#,
        )
        .unwrap();
        let err = Config::parse(Some(&path)).unwrap_err();
        match err {
            RoseError::Expected(RoseExpectedError::Generic(msg)) => {
                assert!(msg.contains("Cannot specify both vfs.artists_whitelist and vfs.artists_blacklist"));
            }
            _ => panic!("Expected mutual exclusion error"),
        }
    }
}
