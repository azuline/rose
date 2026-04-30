//! Configuration module: TOML parsing, validation, defaults, artist alias
//! resolution, path template integration, stored metadata rules, VFS config,
//! and unknown-key detection.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use toml::Value as TomlValue;

use crate::common::RoseError;
use crate::rule_parser::Rule;
use crate::templates::{PathTemplate, PathTemplateConfig, PathTemplateTriad};

// ---------------------------------------------------------------------------
// XDG paths
// ---------------------------------------------------------------------------

/// Return the XDG config path for Rose: `~/.config/rose/config.toml`.
/// Creates the parent directory if it does not exist.
pub fn xdg_config_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("rose");
    let _ = std::fs::create_dir_all(&config_dir);
    config_dir.join("config.toml")
}

/// Return the XDG cache directory for Rose: `~/.cache/rose/`.
/// Creates the directory if it does not exist.
pub fn xdg_cache_path() -> PathBuf {
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("~/.cache"))
        .join("rose");
    let _ = std::fs::create_dir_all(&cache_dir);
    cache_dir
}

// ---------------------------------------------------------------------------
// VirtualFSConfig
// ---------------------------------------------------------------------------

/// Virtual filesystem configuration.
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
    fn parse(cfgpath: &Path, data: &mut toml::Table) -> Result<VirtualFSConfig, RoseError> {
        // mount_dir (required)
        let mount_dir = match data.remove("mount_dir") {
            Some(TomlValue::String(s)) => expand_tilde(&s),
            Some(_) => {
                return Err(RoseError::InvalidConfigValue(format!(
                    "Invalid value for vfs.mount_dir in configuration file ({}): must be a path",
                    cfgpath.display()
                )));
            }
            None => {
                return Err(RoseError::MissingConfigKey(format!(
                    "Missing key vfs.mount_dir in configuration file ({})",
                    cfgpath.display()
                )));
            }
        };

        let artists_whitelist =
            parse_optional_string_list(data, "artists_whitelist", "vfs", "artist", cfgpath)?;
        let genres_whitelist =
            parse_optional_string_list(data, "genres_whitelist", "vfs", "genre", cfgpath)?;
        let descriptors_whitelist = parse_optional_string_list(
            data,
            "descriptors_whitelist",
            "vfs",
            "descriptor",
            cfgpath,
        )?;
        let labels_whitelist =
            parse_optional_string_list(data, "labels_whitelist", "vfs", "label", cfgpath)?;
        let artists_blacklist =
            parse_optional_string_list(data, "artists_blacklist", "vfs", "artist", cfgpath)?;
        let genres_blacklist =
            parse_optional_string_list(data, "genres_blacklist", "vfs", "genre", cfgpath)?;
        let descriptors_blacklist = parse_optional_string_list(
            data,
            "descriptors_blacklist",
            "vfs",
            "descriptor",
            cfgpath,
        )?;
        let labels_blacklist =
            parse_optional_string_list(data, "labels_blacklist", "vfs", "label", cfgpath)?;

        // Mutual exclusion checks
        if artists_whitelist.is_some() && artists_blacklist.is_some() {
            return Err(RoseError::InvalidConfigValue(format!(
                "Cannot specify both vfs.artists_whitelist and vfs.artists_blacklist in configuration file ({}): must specify only one or the other",
                cfgpath.display()
            )));
        }
        if genres_whitelist.is_some() && genres_blacklist.is_some() {
            return Err(RoseError::InvalidConfigValue(format!(
                "Cannot specify both vfs.genres_whitelist and vfs.genres_blacklist in configuration file ({}): must specify only one or the other",
                cfgpath.display()
            )));
        }
        if labels_whitelist.is_some() && labels_blacklist.is_some() {
            return Err(RoseError::InvalidConfigValue(format!(
                "Cannot specify both vfs.labels_whitelist and vfs.labels_blacklist in configuration file ({}): must specify only one or the other",
                cfgpath.display()
            )));
        }

        let hide_genres_with_only_new_releases =
            parse_optional_bool(data, "hide_genres_with_only_new_releases", "vfs", cfgpath)?
                .unwrap_or(false);

        let hide_descriptors_with_only_new_releases = parse_optional_bool(
            data,
            "hide_descriptors_with_only_new_releases",
            "vfs",
            cfgpath,
        )?
        .unwrap_or(false);

        let hide_labels_with_only_new_releases =
            parse_optional_bool(data, "hide_labels_with_only_new_releases", "vfs", cfgpath)?
                .unwrap_or(false);

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

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Top-level Rose configuration.
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
    /// Map from parent artist name to its aliases.
    pub artist_aliases_map: HashMap<String, Vec<String>>,
    /// Map from alias name to its parent artist names.
    pub artist_aliases_parents_map: HashMap<String, Vec<String>>,
    pub path_templates: PathTemplateConfig,
    pub stored_metadata_rules: Vec<Rule>,
    pub vfs: VirtualFSConfig,
}

impl Config {
    /// Parse configuration from a TOML file.
    ///
    /// If `config_path_override` is `None`, uses the default XDG config path.
    pub fn parse(config_path_override: Option<&Path>) -> Result<Config, RoseError> {
        let cfgpath = config_path_override
            .map(PathBuf::from)
            .unwrap_or_else(xdg_config_path);

        let cfgtext = std::fs::read_to_string(&cfgpath).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                RoseError::ConfigNotFound(format!(
                    "Configuration file not found ({})",
                    cfgpath.display()
                ))
            } else {
                RoseError::Internal(format!(
                    "Failed to read configuration file ({}): {}",
                    cfgpath.display(),
                    e
                ))
            }
        })?;

        let mut data: toml::Table = cfgtext.parse::<toml::Table>().map_err(|e| {
            RoseError::ConfigDecode(format!(
                "Failed to decode configuration file: invalid TOML: {}",
                e
            ))
        })?;

        // music_source_dir (required)
        let music_source_dir = match data.remove("music_source_dir") {
            Some(TomlValue::String(s)) => expand_tilde(&s),
            Some(_) => {
                return Err(RoseError::InvalidConfigValue(format!(
                    "Invalid value for music_source_dir in configuration file ({}): must be a path",
                    cfgpath.display()
                )));
            }
            None => {
                return Err(RoseError::MissingConfigKey(format!(
                    "Missing key music_source_dir in configuration file ({})",
                    cfgpath.display()
                )));
            }
        };

        // cache_dir (optional, default XDG)
        let cache_dir = match data.remove("cache_dir") {
            Some(TomlValue::String(s)) => expand_tilde(&s),
            Some(_) => {
                return Err(RoseError::InvalidConfigValue(format!(
                    "Invalid value for cache_dir in configuration file ({}): must be a path",
                    cfgpath.display()
                )));
            }
            None => xdg_cache_path(),
        };
        let _ = std::fs::create_dir_all(&cache_dir);

        // max_proc (optional, default nproc/2, min 1)
        let max_proc = match data.remove("max_proc") {
            Some(TomlValue::Integer(n)) => {
                if n <= 0 {
                    return Err(RoseError::InvalidConfigValue(format!(
                        "Invalid value for max_proc in configuration file ({}): must be a positive integer",
                        cfgpath.display()
                    )));
                }
                n as usize
            }
            Some(_) => {
                return Err(RoseError::InvalidConfigValue(format!(
                    "Invalid value for max_proc in configuration file ({}): must be a positive integer",
                    cfgpath.display()
                )));
            }
            None => {
                let cpus = std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(2);
                (cpus / 2).max(1)
            }
        };

        // artist_aliases
        let mut artist_aliases_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut artist_aliases_parents_map: HashMap<String, Vec<String>> = HashMap::new();
        if let Some(val) = data.remove("artist_aliases") {
            let entries = match val {
                TomlValue::Array(arr) => arr,
                _ => {
                    return Err(RoseError::InvalidConfigValue(format!(
                        "Invalid value for artist_aliases in configuration file ({}): must be a list of {{ artist = str, aliases = list[str] }} records",
                        cfgpath.display()
                    )));
                }
            };
            for entry in entries {
                let table = match entry {
                    TomlValue::Table(t) => t,
                    _ => {
                        return Err(RoseError::InvalidConfigValue(format!(
                            "Invalid value for artist_aliases in configuration file ({}): must be a list of {{ artist = str, aliases = list[str] }} records",
                            cfgpath.display()
                        )));
                    }
                };
                let artist = match table.get("artist") {
                    Some(TomlValue::String(s)) => s.clone(),
                    _ => {
                        return Err(RoseError::InvalidConfigValue(format!(
                            "Invalid value for artist_aliases in configuration file ({}): must be a list of {{ artist = str, aliases = list[str] }} records",
                            cfgpath.display()
                        )));
                    }
                };
                let aliases = match table.get("aliases") {
                    Some(TomlValue::Array(arr)) => {
                        let mut result = Vec::new();
                        for v in arr {
                            match v {
                                TomlValue::String(s) => result.push(s.clone()),
                                _ => {
                                    return Err(RoseError::InvalidConfigValue(format!(
                                        "Invalid value for artist_aliases in configuration file ({}): must be a list of {{ artist = str, aliases = list[str] }} records",
                                        cfgpath.display()
                                    )));
                                }
                            }
                        }
                        result
                    }
                    _ => {
                        return Err(RoseError::InvalidConfigValue(format!(
                            "Invalid value for artist_aliases in configuration file ({}): must be a list of {{ artist = str, aliases = list[str] }} records",
                            cfgpath.display()
                        )));
                    }
                };
                for alias in &aliases {
                    artist_aliases_parents_map
                        .entry(alias.clone())
                        .or_default()
                        .push(artist.clone());
                }
                artist_aliases_map.insert(artist, aliases);
            }
        }

        // cover_art_stems
        let cover_art_stems = parse_optional_string_list(
            &mut data,
            "cover_art_stems",
            "",
            "cover art stem",
            &cfgpath,
        )?
        .unwrap_or_else(|| {
            vec![
                "folder".to_string(),
                "cover".to_string(),
                "art".to_string(),
                "front".to_string(),
            ]
        });

        // valid_art_exts
        let valid_art_exts =
            parse_optional_string_list(&mut data, "valid_art_exts", "", "art extension", &cfgpath)?
                .unwrap_or_else(|| vec!["jpg".to_string(), "jpeg".to_string(), "png".to_string()]);

        // Lowercase cover_art_stems and valid_art_exts
        let cover_art_stems: Vec<String> = cover_art_stems
            .into_iter()
            .map(|s| s.to_lowercase())
            .collect();
        let valid_art_exts: Vec<String> = valid_art_exts
            .into_iter()
            .map(|s| s.to_lowercase())
            .collect();

        // write_parent_genres
        let write_parent_genres =
            parse_optional_bool(&mut data, "write_parent_genres", "", &cfgpath)?.unwrap_or(false);

        // max_filename_bytes
        let max_filename_bytes = match data.remove("max_filename_bytes") {
            Some(TomlValue::Integer(n)) => n as usize,
            Some(_) => {
                return Err(RoseError::InvalidConfigValue(format!(
                    "Invalid value for max_filename_bytes in configuration file ({}): Must be an int: got non-integer",
                    cfgpath.display()
                )));
            }
            None => 180,
        };

        // rename_source_files
        let rename_source_files =
            parse_optional_bool(&mut data, "rename_source_files", "", &cfgpath)?.unwrap_or(false);

        // ignore_release_directories
        let ignore_release_directories = parse_optional_string_list(
            &mut data,
            "ignore_release_directories",
            "",
            "release directory",
            &cfgpath,
        )?
        .unwrap_or_default();

        // stored_metadata_rules
        let mut stored_metadata_rules: Vec<Rule> = Vec::new();
        if let Some(val) = data.remove("stored_metadata_rules") {
            let entries = match val {
                TomlValue::Array(arr) => arr,
                _ => {
                    return Err(RoseError::InvalidConfigValue(format!(
                        "Invalid value in stored_metadata_rules in configuration file ({}): list values must be a dict",
                        cfgpath.display()
                    )));
                }
            };
            for entry in entries {
                let table = match entry {
                    TomlValue::Table(t) => t,
                    _ => {
                        return Err(RoseError::InvalidConfigValue(format!(
                            "Invalid value in stored_metadata_rules in configuration file ({}): list values must be a dict",
                            cfgpath.display()
                        )));
                    }
                };

                let matcher = match table.get("matcher") {
                    Some(TomlValue::String(s)) => s.clone(),
                    Some(_) => {
                        return Err(RoseError::InvalidConfigValue(format!(
                            "Invalid value for `matcher` in stored_metadata_rules in configuration file ({}): rule {:?}: must be a string",
                            cfgpath.display(), table
                        )));
                    }
                    None => {
                        return Err(RoseError::InvalidConfigValue(format!(
                            "Missing key `matcher` in stored_metadata_rules in configuration file ({}): rule {:?}",
                            cfgpath.display(), table
                        )));
                    }
                };

                let actions = match table.get("actions") {
                    Some(TomlValue::Array(arr)) => {
                        let mut result = Vec::new();
                        for v in arr {
                            match v {
                                TomlValue::String(s) => result.push(s.clone()),
                                _ => {
                                    return Err(RoseError::InvalidConfigValue(format!(
                                        "Invalid value for `actions` in stored_metadata_rules in configuration file ({}): rule {:?}: must be a list of strings",
                                        cfgpath.display(), table
                                    )));
                                }
                            }
                        }
                        result
                    }
                    Some(_) => {
                        return Err(RoseError::InvalidConfigValue(format!(
                            "Invalid value for `actions` in stored_metadata_rules in configuration file ({}): rule {:?}: must be a list of strings",
                            cfgpath.display(), table
                        )));
                    }
                    None => {
                        return Err(RoseError::InvalidConfigValue(format!(
                            "Missing key `actions` in stored_metadata_rules in configuration file ({}): rule {:?}",
                            cfgpath.display(), table
                        )));
                    }
                };

                let ignore: Vec<String> = match table.get("ignore") {
                    Some(TomlValue::Array(arr)) => {
                        let mut result = Vec::new();
                        for v in arr {
                            match v {
                                TomlValue::String(s) => result.push(s.clone()),
                                _ => {
                                    return Err(RoseError::InvalidConfigValue(format!(
                                        "Invalid value for `ignore` in stored_metadata_rules in configuration file ({}): rule {:?}: must be a list of strings",
                                        cfgpath.display(), table
                                    )));
                                }
                            }
                        }
                        result
                    }
                    Some(_) => {
                        return Err(RoseError::InvalidConfigValue(format!(
                            "Invalid value for `ignore` in stored_metadata_rules in configuration file ({}): rule {:?}: must be a list of strings",
                            cfgpath.display(), table
                        )));
                    }
                    None => Vec::new(),
                };

                let action_refs: Vec<&str> = actions.iter().map(|s| s.as_str()).collect();
                let ignore_refs: Vec<&str> = ignore.iter().map(|s| s.as_str()).collect();
                let ignore_opt: Option<&[&str]> = if ignore_refs.is_empty() {
                    None
                } else {
                    Some(&ignore_refs)
                };
                match Rule::parse(&matcher, &action_refs, ignore_opt) {
                    Ok(rule) => stored_metadata_rules.push(rule),
                    Err(e) => {
                        return Err(RoseError::InvalidConfigValue(format!(
                            "Failed to parse stored_metadata_rules in configuration file ({}): rule {:?}: {}",
                            cfgpath.display(), table, e
                        )));
                    }
                }
            }
        }

        // path_templates
        let path_templates = parse_path_templates(&mut data)?;

        // vfs
        let mut vfs_table = match data.remove("vfs") {
            Some(TomlValue::Table(t)) => t,
            Some(_) => {
                return Err(RoseError::InvalidConfigValue(format!(
                    "Invalid value for vfs in configuration file ({}): must be a table",
                    cfgpath.display()
                )));
            }
            None => toml::Table::new(),
        };
        let vfs = VirtualFSConfig::parse(&cfgpath, &mut vfs_table)?;
        // Put any remaining vfs keys back for unknown-key detection
        if !vfs_table.is_empty() {
            data.insert("vfs".to_string(), TomlValue::Table(vfs_table));
        }

        // Unknown-key detection via DFS
        if !data.is_empty() {
            let mut unrecognized: Vec<String> = Vec::new();
            let mut stack: Vec<(String, TomlValue)> = data.into_iter().collect();
            while let Some((accessor, node)) = stack.pop() {
                match node {
                    TomlValue::Table(table) => {
                        for (k, v) in table {
                            let child = if accessor.is_empty() {
                                k
                            } else {
                                format!("{}.{}", accessor, k)
                            };
                            stack.push((child, v));
                        }
                    }
                    _ => {
                        unrecognized.push(accessor);
                    }
                }
            }
            if !unrecognized.is_empty() {
                unrecognized.sort();
                tracing::warn!(
                    "Unrecognized options found in configuration file: {}",
                    unrecognized.join(", ")
                );
            }
        }

        Ok(Config {
            music_source_dir,
            cache_dir,
            max_proc,
            ignore_release_directories,
            rename_source_files,
            max_filename_bytes,
            cover_art_stems,
            valid_art_exts,
            write_parent_genres,
            artist_aliases_map,
            artist_aliases_parents_map,
            path_templates,
            stored_metadata_rules,
            vfs,
        })
    }

    /// Cross product of `cover_art_stems` x `valid_art_exts`.
    pub fn valid_cover_arts(&self) -> Vec<String> {
        let mut result = Vec::new();
        for stem in &self.cover_art_stems {
            for ext in &self.valid_art_exts {
                result.push(format!("{}.{}", stem, ext));
            }
        }
        result
    }

    /// Path to the SQLite cache database.
    pub fn cache_database_path(&self) -> PathBuf {
        self.cache_dir.join("cache.sqlite3")
    }

    /// Path to the watchdog PID file.
    pub fn watchdog_pid_path(&self) -> PathBuf {
        self.cache_dir.join("watchdog.pid")
    }

    /// Validate all path templates by attempting to compile them.
    /// This is expensive, so it is only done on demand.
    pub fn validate_path_templates_expensive(&self) -> Result<(), RoseError> {
        self.path_templates.parse().map_err(|e| {
            if let RoseError::InvalidPathTemplate { key, message } = &e {
                RoseError::InvalidConfigValue(format!(
                    "Invalid path template in for template {}: {}",
                    key, message
                ))
            } else {
                e
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Path template parsing
// ---------------------------------------------------------------------------

fn parse_path_templates(data: &mut toml::Table) -> Result<PathTemplateConfig, RoseError> {
    let mut tmpl_table = match data.remove("path_templates") {
        Some(TomlValue::Table(t)) => t,
        Some(_) => {
            // Not a table, just ignore and use defaults
            return Ok(PathTemplateConfig::with_defaults());
        }
        None => {
            return Ok(PathTemplateConfig::with_defaults());
        }
    };

    // Extract the potential default template overrides first
    let mut custom_default: Option<PathTemplateTriad> = None;
    if let Some(TomlValue::Table(default_table)) = tmpl_table.get_mut("default") {
        let mut triad = PathTemplateTriad {
            release: PathTemplate::new(crate::templates::DEFAULT_RELEASE_TEMPLATE_TEXT),
            track: PathTemplate::new(crate::templates::DEFAULT_TRACK_TEMPLATE_TEXT),
            all_tracks: PathTemplate::new(crate::templates::DEFAULT_ALL_TRACKS_TEMPLATE_TEXT),
        };
        if let Some(TomlValue::String(s)) = default_table.remove("release") {
            triad.release = PathTemplate::new(s);
        }
        if let Some(TomlValue::String(s)) = default_table.remove("track") {
            triad.track = PathTemplate::new(s);
        }
        if let Some(TomlValue::String(s)) = default_table.remove("all_tracks") {
            triad.all_tracks = PathTemplate::new(s);
        }
        custom_default = Some(triad);
    }
    // Clean up the default key if empty
    if let Some(TomlValue::Table(t)) = tmpl_table.get("default") {
        if t.is_empty() {
            tmpl_table.remove("default");
        }
    }

    let mut path_templates = PathTemplateConfig::with_defaults_from(custom_default);

    // Per-view overrides
    let view_keys = [
        "source",
        "releases",
        "releases_favorite",
        "releases_new",
        "releases_added_on",
        "releases_released_on",
        "artists",
        "genres",
        "descriptors",
        "labels",
        "loose_tracks",
        "collages",
    ];

    for key in &view_keys {
        if let Some(TomlValue::Table(view_table)) = tmpl_table.get_mut(*key) {
            let triad = match *key {
                "source" => &mut path_templates.source,
                "releases" => &mut path_templates.releases,
                "releases_favorite" => &mut path_templates.releases_favorite,
                "releases_new" => &mut path_templates.releases_new,
                "releases_added_on" => &mut path_templates.releases_added_on,
                "releases_released_on" => &mut path_templates.releases_released_on,
                "artists" => &mut path_templates.artists,
                "genres" => &mut path_templates.genres,
                "descriptors" => &mut path_templates.descriptors,
                "labels" => &mut path_templates.labels,
                "loose_tracks" => &mut path_templates.loose_tracks,
                "collages" => &mut path_templates.collages,
                _ => unreachable!(),
            };
            if let Some(TomlValue::String(s)) = view_table.remove("release") {
                triad.release = PathTemplate::new(s);
            }
            if let Some(TomlValue::String(s)) = view_table.remove("track") {
                triad.track = PathTemplate::new(s);
            }
            if let Some(TomlValue::String(s)) = view_table.remove("all_tracks") {
                triad.all_tracks = PathTemplate::new(s);
            }
        }
        // Clean up the view key if empty
        if let Some(TomlValue::Table(t)) = tmpl_table.get(*key) {
            if t.is_empty() {
                tmpl_table.remove(*key);
            }
        }
    }

    // playlists is a single template, not a triad
    if let Some(TomlValue::String(s)) = tmpl_table.remove("playlists") {
        path_templates.playlists = PathTemplate::new(s);
    }

    // Put remaining unknown keys back into data for unknown-key detection
    if !tmpl_table.is_empty() {
        data.insert("path_templates".to_string(), TomlValue::Table(tmpl_table));
    }

    Ok(path_templates)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Expand `~` at the start of a path to the user's home directory.
fn expand_tilde(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    if s == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    PathBuf::from(s)
}

/// Parse an optional `Vec<String>` from a TOML table, removing the key if found.
/// `section` is the parent key (e.g., "vfs") for error messages; empty string for top-level.
/// `item_name` is used for error messages (e.g., "artist", "genre").
fn parse_optional_string_list(
    data: &mut toml::Table,
    key: &str,
    section: &str,
    item_name: &str,
    cfgpath: &Path,
) -> Result<Option<Vec<String>>, RoseError> {
    let full_key = if section.is_empty() {
        key.to_string()
    } else {
        format!("{}.{}", section, key)
    };
    match data.remove(key) {
        Some(TomlValue::Array(arr)) => {
            let mut result = Vec::new();
            for v in arr {
                match v {
                    TomlValue::String(s) => result.push(s),
                    _ => {
                        return Err(RoseError::InvalidConfigValue(format!(
                            "Invalid value for {} in configuration file ({}): Each {} must be of type str",
                            full_key,
                            cfgpath.display(),
                            item_name
                        )));
                    }
                }
            }
            Ok(Some(result))
        }
        Some(_) => Err(RoseError::InvalidConfigValue(format!(
            "Invalid value for {} in configuration file ({}): Must be a list[str]",
            full_key,
            cfgpath.display()
        ))),
        None => Ok(None),
    }
}

/// Parse an optional `bool` from a TOML table, removing the key if found.
fn parse_optional_bool(
    data: &mut toml::Table,
    key: &str,
    section: &str,
    cfgpath: &Path,
) -> Result<Option<bool>, RoseError> {
    let full_key = if section.is_empty() {
        key.to_string()
    } else {
        format!("{}.{}", section, key)
    };
    match data.remove(key) {
        Some(TomlValue::Boolean(b)) => Ok(Some(b)),
        Some(_) => Err(RoseError::InvalidConfigValue(format!(
            "Invalid value for {} in configuration file ({}): Must be a bool",
            full_key,
            cfgpath.display()
        ))),
        None => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Public template text constants (re-exported for config parsing)
// ---------------------------------------------------------------------------
// These are accessible from templates.rs but we need them for
// config's default triad construction.

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    /// Helper to write a config file and return its path.
    fn write_config(dir: &TempDir, content: &str) -> PathBuf {
        let path = dir.path().join("config.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    // 1. Minimal config
    #[test]
    fn test_config_minimal() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "~/.music-src"
            vfs.mount_dir = "~/music"
            "#,
        );
        let c = Config::parse(Some(&path)).unwrap();
        assert_eq!(
            c.music_source_dir,
            dirs::home_dir().unwrap().join(".music-src")
        );
        assert_eq!(c.vfs.mount_dir, dirs::home_dir().unwrap().join("music"));
        // Check defaults
        assert_eq!(c.max_proc.max(1), c.max_proc); // at least 1
        assert_eq!(c.cover_art_stems, vec!["folder", "cover", "art", "front"]);
        assert_eq!(c.valid_art_exts, vec!["jpg", "jpeg", "png"]);
        assert!(!c.write_parent_genres);
        assert_eq!(c.max_filename_bytes, 180);
        assert!(!c.rename_source_files);
        assert!(c.ignore_release_directories.is_empty());
        assert!(c.artist_aliases_map.is_empty());
        assert!(c.artist_aliases_parents_map.is_empty());
        assert!(c.stored_metadata_rules.is_empty());
        assert!(!c.vfs.hide_genres_with_only_new_releases);
        assert!(!c.vfs.hide_descriptors_with_only_new_releases);
        assert!(!c.vfs.hide_labels_with_only_new_releases);
    }

    // 2. Full config
    #[test]
    fn test_config_full() {
        let dir = TempDir::new().unwrap();
        let cache_dir = dir.path().join("cache");
        let path = write_config(
            &dir,
            &format!(
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
        );
        let c = Config::parse(Some(&path)).unwrap();
        assert_eq!(
            c.music_source_dir,
            dirs::home_dir().unwrap().join(".music-src")
        );
        assert_eq!(c.cache_dir, cache_dir);
        assert_eq!(c.max_proc, 8);
        assert_eq!(c.cover_art_stems, vec!["aa", "bb"]);
        assert_eq!(c.valid_art_exts, vec!["tiff"]);
        assert!(c.write_parent_genres);
        assert_eq!(c.max_filename_bytes, 255);
        assert!(c.rename_source_files);
        assert_eq!(c.ignore_release_directories, vec!["dummy boy"]);

        // Artist aliases
        assert_eq!(
            c.artist_aliases_map.get("Abakus").unwrap(),
            &vec!["Cinnamon Chasers".to_string()]
        );
        assert_eq!(
            c.artist_aliases_map.get("tripleS").unwrap(),
            &vec![
                "EVOLution".to_string(),
                "LOVElution".to_string(),
                "+(KR)ystal Eyes".to_string(),
                "Acid Angel From Asia".to_string(),
                "Acid Eyes".to_string(),
            ]
        );
        assert_eq!(
            c.artist_aliases_parents_map
                .get("Cinnamon Chasers")
                .unwrap(),
            &vec!["Abakus".to_string()]
        );
        assert_eq!(
            c.artist_aliases_parents_map.get("EVOLution").unwrap(),
            &vec!["tripleS".to_string()]
        );

        // Stored metadata rules
        assert_eq!(c.stored_metadata_rules.len(), 2);

        // Path templates - all views should have "{{ title }}" for source, releases, etc.
        assert_eq!(c.path_templates.source.release.text, "{{ title }}");
        assert_eq!(c.path_templates.source.track.text, "{{ title }}");
        assert_eq!(c.path_templates.source.all_tracks.text, "{{ title }}");
        // Genres and descriptors were not overridden, so they should be the default templates
        // (based on the custom default "{{ title }}")
        assert_eq!(c.path_templates.genres.release.text, "{{ title }}");
        assert_eq!(c.path_templates.descriptors.release.text, "{{ title }}");
        assert_eq!(c.path_templates.playlists.text, "{{ title }}");

        // VFS
        assert_eq!(c.vfs.mount_dir, dirs::home_dir().unwrap().join("music"));
        assert_eq!(c.vfs.artists_blacklist, Some(vec!["www".to_string()]));
        assert_eq!(c.vfs.genres_blacklist, Some(vec!["xxx".to_string()]));
        assert_eq!(c.vfs.descriptors_blacklist, Some(vec!["yyy".to_string()]));
        assert_eq!(c.vfs.labels_blacklist, Some(vec!["zzz".to_string()]));
        assert!(c.vfs.artists_whitelist.is_none());
        assert!(c.vfs.genres_whitelist.is_none());
        assert!(c.vfs.descriptors_whitelist.is_none());
        assert!(c.vfs.labels_whitelist.is_none());
        assert!(c.vfs.hide_genres_with_only_new_releases);
        assert!(c.vfs.hide_descriptors_with_only_new_releases);
        assert!(c.vfs.hide_labels_with_only_new_releases);
    }

    // 3. Missing required key
    #[test]
    fn test_config_missing_music_source_dir() {
        let dir = TempDir::new().unwrap();
        let path = write_config(&dir, r#"vfs.mount_dir = "~/music""#);
        let err = Config::parse(Some(&path)).unwrap_err();
        assert!(matches!(err, RoseError::MissingConfigKey(_)));
        assert!(
            err.to_string().contains("music_source_dir"),
            "Error should mention music_source_dir: {}",
            err
        );
    }

    // 4. Invalid TOML
    #[test]
    fn test_config_invalid_toml() {
        let dir = TempDir::new().unwrap();
        let path = write_config(&dir, "{{{{not valid toml}}}}");
        let err = Config::parse(Some(&path)).unwrap_err();
        assert!(matches!(err, RoseError::ConfigDecode(_)));
    }

    // 5. File not found
    #[test]
    fn test_config_not_found() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.toml");
        let err = Config::parse(Some(&path)).unwrap_err();
        assert!(matches!(err, RoseError::ConfigNotFound(_)));
    }

    // 6. Artist aliases forward and reverse maps
    #[test]
    fn test_artist_aliases() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            vfs.mount_dir = "/"
            artist_aliases = [
              { artist = "A", aliases = ["A1", "A2"] },
              { artist = "B", aliases = ["A2", "B1"] },
            ]
            "#,
        );
        let c = Config::parse(Some(&path)).unwrap();
        assert_eq!(c.artist_aliases_map["A"], vec!["A1", "A2"]);
        assert_eq!(c.artist_aliases_map["B"], vec!["A2", "B1"]);
        assert_eq!(c.artist_aliases_parents_map["A1"], vec!["A"]);
        // A2 is an alias for both A and B
        assert_eq!(c.artist_aliases_parents_map["A2"], vec!["A", "B"]);
        assert_eq!(c.artist_aliases_parents_map["B1"], vec!["B"]);
    }

    // 7. Whitelist/blacklist mutual exclusion
    #[test]
    fn test_whitelist_blacklist_mutual_exclusion_artists() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            [vfs]
            mount_dir = "/"
            artists_whitelist = ["a"]
            artists_blacklist = ["b"]
            "#,
        );
        let err = Config::parse(Some(&path)).unwrap_err();
        assert!(matches!(err, RoseError::InvalidConfigValue(_)));
        assert!(err.to_string().contains("artists_whitelist"));
    }

    #[test]
    fn test_whitelist_blacklist_mutual_exclusion_genres() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            [vfs]
            mount_dir = "/"
            genres_whitelist = ["a"]
            genres_blacklist = ["b"]
            "#,
        );
        let err = Config::parse(Some(&path)).unwrap_err();
        assert!(matches!(err, RoseError::InvalidConfigValue(_)));
        assert!(err.to_string().contains("genres_whitelist"));
    }

    #[test]
    fn test_whitelist_blacklist_mutual_exclusion_labels() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            [vfs]
            mount_dir = "/"
            labels_whitelist = ["a"]
            labels_blacklist = ["b"]
            "#,
        );
        let err = Config::parse(Some(&path)).unwrap_err();
        assert!(matches!(err, RoseError::InvalidConfigValue(_)));
        assert!(err.to_string().contains("labels_whitelist"));
    }

    // 8. Stored metadata rules - valid and invalid
    #[test]
    fn test_stored_metadata_rules_valid() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            vfs.mount_dir = "/"
            [[stored_metadata_rules]]
            matcher = "tracktitle:lala"
            actions = ["replace:hihi"]
            "#,
        );
        let c = Config::parse(Some(&path)).unwrap();
        assert_eq!(c.stored_metadata_rules.len(), 1);
    }

    #[test]
    fn test_stored_metadata_rules_invalid_syntax() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            vfs.mount_dir = "/"
            [[stored_metadata_rules]]
            matcher = "tracktitle:hi"
            actions = ["delete:hi"]
            "#,
        );
        let err = Config::parse(Some(&path)).unwrap_err();
        assert!(matches!(err, RoseError::InvalidConfigValue(_)));
        assert!(
            err.to_string()
                .contains("Failed to parse stored_metadata_rules"),
            "Unexpected error: {}",
            err
        );
    }

    // 9. Path template overrides
    #[test]
    fn test_path_template_default_override() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            vfs.mount_dir = "/"
            [path_templates]
            default.release = "{{ releasetitle }}"
            "#,
        );
        let c = Config::parse(Some(&path)).unwrap();
        // The default override should propagate to views that weren't explicitly set
        assert_eq!(c.path_templates.source.release.text, "{{ releasetitle }}");
        assert_eq!(c.path_templates.releases.release.text, "{{ releasetitle }}");
        assert_eq!(c.path_templates.artists.release.text, "{{ releasetitle }}");
        // But releases_added_on should use the prefix + custom default
        assert!(
            c.path_templates
                .releases_added_on
                .release
                .text
                .contains("{{ releasetitle }}"),
            "releases_added_on should include custom default release template: {}",
            c.path_templates.releases_added_on.release.text
        );
    }

    #[test]
    fn test_path_template_per_view_override() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            vfs.mount_dir = "/"
            [path_templates]
            source.release = "custom-source-release"
            "#,
        );
        let c = Config::parse(Some(&path)).unwrap();
        assert_eq!(
            c.path_templates.source.release.text,
            "custom-source-release"
        );
        // Other views should still have defaults
        assert_ne!(
            c.path_templates.releases.release.text,
            "custom-source-release"
        );
    }

    // 10. Unknown keys warning
    #[test]
    fn test_unknown_keys_warning() {
        // We test that parsing succeeds even with unknown keys (just warns).
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            vfs.mount_dir = "/"
            unknown_key = "value"
            another.nested.unknown = 42
            "#,
        );
        // Should succeed without error
        let c = Config::parse(Some(&path)).unwrap();
        assert_eq!(c.music_source_dir, PathBuf::from("/"));
    }

    // 11. max_proc validation
    #[test]
    fn test_max_proc_zero_is_error() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            vfs.mount_dir = "/"
            max_proc = 0
            "#,
        );
        let err = Config::parse(Some(&path)).unwrap_err();
        assert!(matches!(err, RoseError::InvalidConfigValue(_)));
    }

    #[test]
    fn test_max_proc_valid() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            vfs.mount_dir = "/"
            max_proc = 4
            "#,
        );
        let c = Config::parse(Some(&path)).unwrap();
        assert_eq!(c.max_proc, 4);
    }

    // 12. valid_cover_arts
    #[test]
    fn test_valid_cover_arts_default() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            vfs.mount_dir = "/"
            "#,
        );
        let c = Config::parse(Some(&path)).unwrap();
        let arts = c.valid_cover_arts();
        assert_eq!(
            arts,
            vec![
                "folder.jpg",
                "folder.jpeg",
                "folder.png",
                "cover.jpg",
                "cover.jpeg",
                "cover.png",
                "art.jpg",
                "art.jpeg",
                "art.png",
                "front.jpg",
                "front.jpeg",
                "front.png",
            ]
        );
    }

    // Test cache_database_path and watchdog_pid_path
    #[test]
    fn test_derived_paths() {
        let dir = TempDir::new().unwrap();
        let cache_dir = dir.path().join("cache");
        let path = write_config(
            &dir,
            &format!(
                r#"
                music_source_dir = "/"
                cache_dir = "{}"
                vfs.mount_dir = "/"
                "#,
                cache_dir.display()
            ),
        );
        let c = Config::parse(Some(&path)).unwrap();
        assert_eq!(c.cache_database_path(), cache_dir.join("cache.sqlite3"));
        assert_eq!(c.watchdog_pid_path(), cache_dir.join("watchdog.pid"));
    }

    // Test missing vfs.mount_dir
    #[test]
    fn test_missing_vfs_mount_dir() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            "#,
        );
        let err = Config::parse(Some(&path)).unwrap_err();
        assert!(matches!(err, RoseError::MissingConfigKey(_)));
        assert!(err.to_string().contains("vfs.mount_dir"));
    }

    // Test whitelist without blacklist
    #[test]
    fn test_whitelist_only() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            vfs.mount_dir = "/"
            vfs.artists_whitelist = ["www"]
            vfs.genres_whitelist = ["xxx"]
            vfs.descriptors_whitelist = ["yyy"]
            vfs.labels_whitelist = ["zzz"]
            "#,
        );
        let c = Config::parse(Some(&path)).unwrap();
        assert_eq!(c.vfs.artists_whitelist, Some(vec!["www".to_string()]));
        assert_eq!(c.vfs.genres_whitelist, Some(vec!["xxx".to_string()]));
        assert_eq!(c.vfs.descriptors_whitelist, Some(vec!["yyy".to_string()]));
        assert_eq!(c.vfs.labels_whitelist, Some(vec!["zzz".to_string()]));
        assert!(c.vfs.artists_blacklist.is_none());
        assert!(c.vfs.genres_blacklist.is_none());
        assert!(c.vfs.descriptors_blacklist.is_none());
        assert!(c.vfs.labels_blacklist.is_none());
    }

    // Test invalid type for music_source_dir
    #[test]
    fn test_invalid_music_source_dir_type() {
        let dir = TempDir::new().unwrap();
        let path = write_config(&dir, "music_source_dir = 123\nvfs.mount_dir = \"/\"");
        let err = Config::parse(Some(&path)).unwrap_err();
        assert!(matches!(err, RoseError::InvalidConfigValue(_)));
        assert!(err.to_string().contains("music_source_dir"));
        assert!(err.to_string().contains("must be a path"));
    }

    // Test invalid type for cache_dir
    #[test]
    fn test_invalid_cache_dir_type() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            cache_dir = 123
            vfs.mount_dir = "/"
            "#,
        );
        let err = Config::parse(Some(&path)).unwrap_err();
        assert!(matches!(err, RoseError::InvalidConfigValue(_)));
        assert!(err.to_string().contains("cache_dir"));
        assert!(err.to_string().contains("must be a path"));
    }

    // Test invalid type for max_proc
    #[test]
    fn test_invalid_max_proc_type() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            max_proc = "lalala"
            vfs.mount_dir = "/"
            "#,
        );
        let err = Config::parse(Some(&path)).unwrap_err();
        assert!(matches!(err, RoseError::InvalidConfigValue(_)));
        assert!(err.to_string().contains("max_proc"));
    }

    // Test invalid vfs.mount_dir type
    #[test]
    fn test_invalid_vfs_mount_dir_type() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            [vfs]
            mount_dir = 123
            "#,
        );
        let err = Config::parse(Some(&path)).unwrap_err();
        assert!(matches!(err, RoseError::InvalidConfigValue(_)));
        assert!(err.to_string().contains("vfs.mount_dir"));
        assert!(err.to_string().contains("must be a path"));
    }

    // Test invalid vfs boolean
    #[test]
    fn test_invalid_vfs_bool() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            [vfs]
            mount_dir = "/"
            hide_genres_with_only_new_releases = "lalala"
            "#,
        );
        let err = Config::parse(Some(&path)).unwrap_err();
        assert!(matches!(err, RoseError::InvalidConfigValue(_)));
        assert!(err
            .to_string()
            .contains("hide_genres_with_only_new_releases"));
        assert!(err.to_string().contains("Must be a bool"));
    }

    // Test validate_path_templates_expensive succeeds with defaults
    #[test]
    fn test_validate_path_templates_expensive_defaults() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            vfs.mount_dir = "/"
            "#,
        );
        let c = Config::parse(Some(&path)).unwrap();
        c.validate_path_templates_expensive().unwrap();
    }

    // Test stored_metadata_rules with ignore
    #[test]
    fn test_stored_metadata_rules_with_ignore() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            vfs.mount_dir = "/"
            [[stored_metadata_rules]]
            matcher = "tracktitle:lala"
            actions = ["replace:hihi"]
            ignore = ["releasetitle:blabla"]
            "#,
        );
        let c = Config::parse(Some(&path)).unwrap();
        assert_eq!(c.stored_metadata_rules.len(), 1);
        assert_eq!(c.stored_metadata_rules[0].ignore.len(), 1);
    }

    // Test cover_art_stems and valid_art_exts are lowercased
    #[test]
    fn test_cover_art_stems_lowercased() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            vfs.mount_dir = "/"
            cover_art_stems = ["FOLDER", "Cover"]
            valid_art_exts = ["JPG", "Png"]
            "#,
        );
        let c = Config::parse(Some(&path)).unwrap();
        assert_eq!(c.cover_art_stems, vec!["folder", "cover"]);
        assert_eq!(c.valid_art_exts, vec!["jpg", "png"]);
    }

    // Test invalid stored_metadata_rules ignore syntax
    #[test]
    fn test_stored_metadata_rules_invalid_ignore() {
        let dir = TempDir::new().unwrap();
        let path = write_config(
            &dir,
            r#"
            music_source_dir = "/"
            vfs.mount_dir = "/"
            [[stored_metadata_rules]]
            matcher = "tracktitle:hi"
            actions = ["delete"]
            ignore = ["tracktitle:bye:"]
            "#,
        );
        let err = Config::parse(Some(&path)).unwrap_err();
        assert!(matches!(err, RoseError::InvalidConfigValue(_)));
        assert!(err
            .to_string()
            .contains("Failed to parse stored_metadata_rules"));
    }
}
