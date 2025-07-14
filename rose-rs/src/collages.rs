// The collages module provides functions for interacting with collages.

use crate::cache::{collage_lock_name, connect, lock, unlock};
use crate::cache_update::{update_cache_evict_nonexistent_collages, update_cache_for_collages};
use crate::config::Config;
use crate::error::{Result, RoseError, RoseExpectedError};
use crate::releases::ReleaseDoesNotExistError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{debug, info};

#[derive(Debug)]
pub struct DescriptionMismatchError(pub String);

impl std::fmt::Display for DescriptionMismatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Description mismatch: {}", self.0)
    }
}

impl std::error::Error for DescriptionMismatchError {}

#[derive(Debug)]
pub struct CollageDoesNotExistError(pub String);

impl std::fmt::Display for CollageDoesNotExistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Collage does not exist: {}", self.0)
    }
}

impl std::error::Error for CollageDoesNotExistError {}

#[derive(Debug)]
pub struct CollageAlreadyExistsError(pub String);

impl std::fmt::Display for CollageAlreadyExistsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Collage already exists: {}", self.0)
    }
}

impl std::error::Error for CollageAlreadyExistsError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CollageRelease {
    uuid: String,
    description_meta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CollageData {
    #[serde(default)]
    releases: Vec<CollageRelease>,
}

// Python: def create_collage(c: Config, name: str) -> None:
pub fn create_collage(config: &Config, name: &str) -> Result<()> {
    // Create the collages directory if it doesn't exist
    let collages_dir = config.music_source_dir.join("!collages");
    fs::create_dir_all(&collages_dir)?;
    
    let path = collage_path(config, name);
    let conn = connect(config)?;
    lock(config, &collage_lock_name(name), 60.0)?;
    
    if path.exists() {
        unlock(&conn, &collage_lock_name(name))?;
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            format!("Collage {} already exists", name)
        )));
    }
    
    // Create empty collage file
    fs::File::create(&path)?;
    
    unlock(&conn, &collage_lock_name(name))?;
    
    info!("Created collage {} in source directory", name);
    update_cache_for_collages(config, Some(vec![name.to_string()]), true)?;
    
    Ok(())
}

// Python: def delete_collage(c: Config, name: str) -> None:
pub fn delete_collage(config: &Config, name: &str) -> Result<()> {
    let path = collage_path(config, name);
    let conn = connect(config)?;
    lock(config, &collage_lock_name(name), 60.0)?;
    
    if !path.exists() {
        unlock(&conn, &collage_lock_name(name))?;
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            format!("Collage {} does not exist", name)
        )));
    }
    
    // Move to trash instead of deleting permanently
    let trash_dir = config.cache_dir.join("trash");
    fs::create_dir_all(&trash_dir)?;
    let trash_path = trash_dir.join(format!("{}.toml", name));
    fs::rename(&path, &trash_path)?;
    
    unlock(&conn, &collage_lock_name(name))?;
    
    info!("Deleted collage {} from source directory", name);
    update_cache_evict_nonexistent_collages(config)?;
    
    Ok(())
}

// Python: def rename_collage(c: Config, old_name: str, new_name: str) -> None:
pub fn rename_collage(config: &Config, old_name: &str, new_name: &str) -> Result<()> {
    let old_path = collage_path(config, old_name);
    let new_path = collage_path(config, new_name);
    
    let conn = connect(config)?;
    lock(config, &collage_lock_name(old_name), 60.0)?;
    lock(config, &collage_lock_name(new_name), 60.0)?;
    
    if !old_path.exists() {
        unlock(&conn, &collage_lock_name(new_name))?;
        unlock(&conn, &collage_lock_name(old_name))?;
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            format!("Collage {} does not exist", old_name)
        )));
    }
    
    if new_path.exists() {
        unlock(&conn, &collage_lock_name(new_name))?;
        unlock(&conn, &collage_lock_name(old_name))?;
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            format!("Collage {} already exists", new_name)
        )));
    }
    
    fs::rename(&old_path, &new_path)?;
    
    // Also rename all files with the same stem (e.g. cover arts)
    let collages_dir = config.music_source_dir.join("!collages");
    for entry in fs::read_dir(&collages_dir)? {
        let entry = entry?;
        let file_path = entry.path();
        if let Some(stem) = file_path.file_stem() {
            if stem == old_name {
                let extension = file_path.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                if extension != "toml" {
                    let new_adjacent_file = collages_dir.join(format!("{}.{}", new_name, extension));
                    if !new_adjacent_file.exists() {
                        fs::rename(&file_path, &new_adjacent_file)?;
                        debug!("Renaming collage-adjacent file {:?} to {:?}", file_path, new_adjacent_file);
                    }
                }
            }
        }
    }
    
    unlock(&conn, &collage_lock_name(new_name))?;
    unlock(&conn, &collage_lock_name(old_name))?;
    
    info!("Renamed collage {} to {}", old_name, new_name);
    update_cache_for_collages(config, Some(vec![new_name.to_string()]), true)?;
    update_cache_evict_nonexistent_collages(config)?;
    
    Ok(())
}

// Python: def remove_release_from_collage(c: Config, collage_name: str, release_id: str) -> None:
pub fn remove_release_from_collage(
    config: &Config,
    collage_name: &str,
    release_id: &str,
) -> Result<()> {
    let release_logtext = get_release_logtext(config, release_id)?
        .ok_or_else(|| RoseError::Expected(RoseExpectedError::Generic(
            format!("Release {} does not exist", release_id)
        )))?;
    
    let path = collage_path(config, collage_name);
    if !path.exists() {
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            format!("Collage {} does not exist", collage_name)
        )));
    }
    
    let conn = connect(config)?;
    lock(config, &collage_lock_name(collage_name), 60.0)?;
    
    // Read the collage data
    let data_str = fs::read_to_string(&path)?;
    let mut data: CollageData = toml::from_str(&data_str)
        .map_err(|e| RoseError::Generic(format!("Failed to parse collage TOML: {}", e)))?;
    
    let old_len = data.releases.len();
    data.releases.retain(|r| r.uuid != release_id);
    
    if old_len == data.releases.len() {
        unlock(&conn, &collage_lock_name(collage_name))?;
        info!("No-Op: Release {} not in collage {}", release_logtext, collage_name);
        return Ok(());
    }
    
    // Write back
    let toml_string = toml::to_string_pretty(&data)
        .map_err(|e| RoseError::Generic(format!("Failed to serialize collage TOML: {}", e)))?;
    fs::write(&path, toml_string)?;
    
    unlock(&conn, &collage_lock_name(collage_name))?;
    
    info!("Removed release {} from collage {}", release_logtext, collage_name);
    update_cache_for_collages(config, Some(vec![collage_name.to_string()]), true)?;
    
    Ok(())
}

// Python: def add_release_to_collage(
pub fn add_release_to_collage(
    config: &Config,
    collage_name: &str,
    release_id: &str,
) -> Result<()> {
    let release_logtext = get_release_logtext(config, release_id)?
        .ok_or_else(|| RoseError::Expected(RoseExpectedError::Generic(
            format!("Release {} does not exist", release_id)
        )))?;
    
    let path = collage_path(config, collage_name);
    if !path.exists() {
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            format!("Collage {} does not exist", collage_name)
        )));
    }
    
    let conn = connect(config)?;
    lock(config, &collage_lock_name(collage_name), 60.0)?;
    
    // Read the collage data
    let data_str = fs::read_to_string(&path).unwrap_or_else(|_| "".to_string());
    let mut data: CollageData = if data_str.is_empty() {
        CollageData { releases: Vec::new() }
    } else {
        toml::from_str(&data_str)
            .map_err(|e| RoseError::Generic(format!("Failed to parse collage TOML: {}", e)))?
    };
    
    // Check if release is already in the collage
    for r in &data.releases {
        if r.uuid == release_id {
            unlock(&conn, &collage_lock_name(collage_name))?;
            info!("No-Op: Release {} already in collage {}", release_logtext, collage_name);
            return Ok(());
        }
    }
    
    // Add the release
    data.releases.push(CollageRelease {
        uuid: release_id.to_string(),
        description_meta: release_logtext.clone(),
    });
    
    // Write back
    let toml_string = toml::to_string_pretty(&data)
        .map_err(|e| RoseError::Generic(format!("Failed to serialize collage TOML: {}", e)))?;
    fs::write(&path, toml_string)?;
    
    unlock(&conn, &collage_lock_name(collage_name))?;
    
    info!("Added release {} to collage {}", release_logtext, collage_name);
    update_cache_for_collages(config, Some(vec![collage_name.to_string()]), true)?;
    
    Ok(())
}

// Python: def edit_collage_in_editor(c: Config, collage_name: str) -> None:
pub fn edit_collage_in_editor(config: &Config, collage_name: &str) -> Result<()> {
    let path = collage_path(config, collage_name);
    if !path.exists() {
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            format!("Collage {} does not exist", collage_name)
        )));
    }
    
    let conn = connect(config)?;
    lock(config, &collage_lock_name(collage_name), 300.0)?; // 5 minute timeout for editing
    
    // Read the collage data
    let data_str = fs::read_to_string(&path)?;
    let data: CollageData = toml::from_str(&data_str)
        .map_err(|e| RoseError::Generic(format!("Failed to parse collage TOML: {}", e)))?;
    
    // Create text for editing
    let descriptions: Vec<String> = data.releases.iter()
        .map(|r| r.description_meta.clone())
        .collect();
    let edit_content = descriptions.join("\n");
    
    // Write to temporary file
    let temp_file = config.cache_dir.join(format!("rose-edit-collage-{}.txt", collage_name));
    fs::write(&temp_file, &edit_content)?;
    
    // Open in editor
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
    let status = Command::new(&editor)
        .arg(&temp_file)
        .status()?;
    
    if !status.success() {
        unlock(&conn, &collage_lock_name(collage_name))?;
        fs::remove_file(&temp_file).ok();
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            "Editor exited with non-zero status".to_string()
        )));
    }
    
    // Read back and parse
    let edited_content = fs::read_to_string(&temp_file)?;
    fs::remove_file(&temp_file).ok();
    
    if edited_content.trim() == edit_content.trim() {
        unlock(&conn, &collage_lock_name(collage_name))?;
        info!("Aborting: no changes detected in collage edit");
        return Ok(());
    }
    
    // Create UUID mapping
    let uuid_mapping: HashMap<String, String> = data.releases.iter()
        .map(|r| (r.description_meta.clone(), r.uuid.clone()))
        .collect();
    
    // Parse edited releases
    let mut edited_releases = Vec::new();
    for desc in edited_content.trim().split('\n') {
        let desc = desc.trim();
        if desc.is_empty() {
            continue;
        }
        
        let uuid = uuid_mapping.get(desc)
            .ok_or_else(|| RoseError::Expected(RoseExpectedError::Generic(
                format!("Release {} does not match a known release in the collage. Was the line edited?", desc)
            )))?;
        
        edited_releases.push(CollageRelease {
            uuid: uuid.clone(),
            description_meta: desc.to_string(),
        });
    }
    
    // Update data
    let new_data = CollageData {
        releases: edited_releases,
    };
    
    // Write back
    let toml_string = toml::to_string_pretty(&new_data)
        .map_err(|e| RoseError::Generic(format!("Failed to serialize collage TOML: {}", e)))?;
    fs::write(&path, toml_string)?;
    
    unlock(&conn, &collage_lock_name(collage_name))?;
    
    info!("Edited collage {} from EDITOR", collage_name);
    update_cache_for_collages(config, Some(vec![collage_name.to_string()]), true)?;
    
    Ok(())
}

// Python: def collage_path(c: Config, name: str) -> Path:
fn collage_path(config: &Config, name: &str) -> PathBuf {
    config.music_source_dir.join("!collages").join(format!("{}.toml", name))
}

// Helper function to get release logtext
fn get_release_logtext(config: &Config, release_id: &str) -> Result<Option<String>> {
    use crate::cache::get_release;
    
    let release = get_release(config, release_id)?;
    Ok(release.map(|r| {
        format!("{} - {}", 
            r.releaseartists.main.iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            r.releasetitle
        )
    }))
}