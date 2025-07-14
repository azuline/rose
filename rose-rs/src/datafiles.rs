use crate::error::{Result, RoseError, RoseExpectedError};
use chrono::Local;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

lazy_static::lazy_static! {
    // Pattern for .rose.{uuid}.toml files
    static ref DATAFILE_REGEX: Regex = Regex::new(r"^\.rose\.([a-fA-F0-9\-]+)\.toml$").unwrap();
}

/// The stored data file structure containing metadata about a release
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredDataFile {
    /// Whether this release is marked as "new"
    #[serde(default = "default_new")]
    pub new: bool,

    /// When this release was added to the library
    #[serde(default = "default_added_at")]
    pub added_at: String,
}

fn default_new() -> bool {
    true
}

fn default_added_at() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

impl Default for StoredDataFile {
    fn default() -> Self {
        Self {
            new: default_new(),
            added_at: default_added_at(),
        }
    }
}

impl StoredDataFile {
    /// Create a new datafile with current timestamp
    pub fn new() -> Self {
        Self::default()
    }
}

/// Find the first .rose.{uuid}.toml file in a directory
pub fn find_release_datafile(dir: &Path) -> Result<Option<(PathBuf, Uuid)>> {
    if !dir.is_dir() {
        return Ok(None);
    }

    for entry in fs::read_dir(dir).map_err(|_| {
        RoseError::Expected(RoseExpectedError::FileNotFound {
            path: dir.to_path_buf(),
        })
    })? {
        let entry = entry.map_err(|e| {
            RoseError::Expected(RoseExpectedError::Generic(format!(
                "Failed to read directory entry: {e}"
            )))
        })?;

        let filename = entry.file_name();
        let filename_str = filename.to_string_lossy();

        if let Some(captures) = DATAFILE_REGEX.captures(&filename_str) {
            if let Some(uuid_str) = captures.get(1) {
                match Uuid::parse_str(uuid_str.as_str()) {
                    Ok(uuid) => return Ok(Some((entry.path(), uuid))),
                    Err(_) => {
                        // Invalid UUID in filename, skip this file
                        continue;
                    }
                }
            }
        }
    }

    Ok(None)
}

/// Read a datafile from disk
pub fn read_datafile(path: &Path) -> Result<StoredDataFile> {
    let contents = fs::read_to_string(path).map_err(|_| {
        RoseError::Expected(RoseExpectedError::FileNotFound {
            path: path.to_path_buf(),
        })
    })?;

    match toml::from_str::<StoredDataFile>(&contents) {
        Ok(mut datafile) => {
            // Validate and provide defaults for missing fields
            if datafile.added_at.is_empty() {
                datafile.added_at = default_added_at();
            }
            Ok(datafile)
        }
        Err(e) => {
            // Handle corrupt/invalid TOML by returning a default datafile
            tracing::warn!(
                "Failed to parse datafile at {:?}: {}. Using defaults.",
                path,
                e
            );
            Ok(StoredDataFile::default())
        }
    }
}

/// Write a datafile to disk
pub fn write_datafile(path: &Path, datafile: &StoredDataFile) -> Result<()> {
    let toml_string = toml::to_string_pretty(datafile).map_err(|e| {
        RoseError::Expected(RoseExpectedError::Generic(format!(
            "Failed to serialize datafile: {e}"
        )))
    })?;

    fs::write(path, toml_string).map_err(|e| {
        RoseError::Expected(RoseExpectedError::Generic(format!(
            "Failed to write datafile to {path:?}: {e}"
        )))
    })?;

    Ok(())
}

/// Create a new datafile with a new UUID
pub fn create_datafile(dir: &Path) -> Result<(PathBuf, Uuid, StoredDataFile)> {
    let uuid = Uuid::now_v7();
    let filename = format!(".rose.{uuid}.toml");
    let path = dir.join(&filename);
    let datafile = StoredDataFile::new();

    write_datafile(&path, &datafile)?;

    Ok((path, uuid, datafile))
}

/// Upgrade a datafile, ensuring all required fields are present
pub fn upgrade_datafile(path: &Path) -> Result<StoredDataFile> {
    let datafile = read_datafile(path).unwrap_or_default();

    // Write back the upgraded datafile
    write_datafile(path, &datafile)?;

    Ok(datafile)
}

/// Validate that a datafile has all required fields
pub fn validate_datafile(datafile: &StoredDataFile) -> bool {
    // Currently, we just need to ensure fields are non-empty
    !datafile.added_at.is_empty()
}

/// Extract UUID from a datafile path
pub fn extract_uuid_from_path(path: &Path) -> Option<Uuid> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| DATAFILE_REGEX.captures(name))
        .and_then(|captures| captures.get(1))
        .and_then(|uuid_str| Uuid::parse_str(uuid_str.as_str()).ok())
}

/// Create a datafile path from a directory and UUID
pub fn datafile_path(dir: &Path, uuid: &Uuid) -> PathBuf {
    dir.join(format!(".rose.{uuid}.toml"))
}

/// Check if a file is a datafile based on its name
pub fn is_datafile(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| DATAFILE_REGEX.is_match(name))
        .unwrap_or(false)
}

/// Toggle the "new" flag in a datafile
pub fn toggle_new_flag(path: &Path) -> Result<()> {
    let mut datafile = read_datafile(path)?;
    datafile.new = !datafile.new;
    write_datafile(path, &datafile)?;
    Ok(())
}

/// Update the added_at timestamp in a datafile
pub fn update_added_at(path: &Path, timestamp: &str) -> Result<()> {
    let mut datafile = read_datafile(path)?;
    datafile.added_at = timestamp.to_string();
    write_datafile(path, &datafile)?;
    Ok(())
}

/// Read datafile from a directory, creating one if it doesn't exist
pub fn read_or_create_datafile(dir: &Path) -> Result<(PathBuf, Uuid, StoredDataFile)> {
    match find_release_datafile(dir)? {
        Some((path, uuid)) => {
            let datafile = read_datafile(&path)?;
            Ok((path, uuid, datafile))
        }
        None => create_datafile(dir),
    }
}
