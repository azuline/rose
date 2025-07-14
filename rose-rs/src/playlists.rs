// The playlists module provides functions for interacting with playlists.

use crate::cache::{connect, lock, playlist_lock_name, unlock};
use crate::cache_update::{update_cache_evict_nonexistent_playlists, update_cache_for_playlists};
use crate::config::Config;
use crate::error::{Result, RoseError, RoseExpectedError};
use crate::tracks::{get_track, TrackDoesNotExistError};
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
pub struct PlaylistDoesNotExistError(pub String);

impl std::fmt::Display for PlaylistDoesNotExistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Playlist does not exist: {}", self.0)
    }
}

impl std::error::Error for PlaylistDoesNotExistError {}

#[derive(Debug)]
pub struct PlaylistAlreadyExistsError(pub String);

impl std::fmt::Display for PlaylistAlreadyExistsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Playlist already exists: {}", self.0)
    }
}

impl std::error::Error for PlaylistAlreadyExistsError {}

#[derive(Debug)]
pub struct InvalidCoverArtFileError(pub String);

impl std::fmt::Display for InvalidCoverArtFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid cover art file: {}", self.0)
    }
}

impl std::error::Error for InvalidCoverArtFileError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlaylistTrack {
    uuid: String,
    description_meta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlaylistData {
    #[serde(default)]
    tracks: Vec<PlaylistTrack>,
}

// Python: def create_playlist(c: Config, name: str) -> None:
pub fn create_playlist(config: &Config, name: &str) -> Result<()> {
    // Create the playlists directory if it doesn't exist
    let playlists_dir = config.music_source_dir.join("!playlists");
    fs::create_dir_all(&playlists_dir)?;
    
    let path = playlist_path(config, name);
    let conn = connect(config)?;
    lock(config, &playlist_lock_name(name), 60.0)?;
    
    if path.exists() {
        unlock(&conn, &playlist_lock_name(name))?;
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            format!("Playlist {} already exists", name)
        )));
    }
    
    // Create empty playlist file
    fs::File::create(&path)?;
    
    unlock(&conn, &playlist_lock_name(name))?;
    
    info!("Created playlist {} in source directory", name);
    update_cache_for_playlists(config, Some(vec![name.to_string()]), true)?;
    
    Ok(())
}

// Python: def delete_playlist(c: Config, name: str) -> None:
pub fn delete_playlist(config: &Config, name: &str) -> Result<()> {
    let path = playlist_path(config, name);
    let conn = connect(config)?;
    lock(config, &playlist_lock_name(name), 60.0)?;
    
    if !path.exists() {
        unlock(&conn, &playlist_lock_name(name))?;
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            format!("Playlist {} does not exist", name)
        )));
    }
    
    // Move to trash instead of deleting permanently
    let trash_dir = config.cache_dir.join("trash");
    fs::create_dir_all(&trash_dir)?;
    let trash_path = trash_dir.join(format!("{}.toml", name));
    fs::rename(&path, &trash_path)?;
    
    unlock(&conn, &playlist_lock_name(name))?;
    
    info!("Deleted playlist {} from source directory", name);
    update_cache_evict_nonexistent_playlists(config)?;
    
    Ok(())
}

// Python: def rename_playlist(c: Config, old_name: str, new_name: str) -> None:
pub fn rename_playlist(config: &Config, old_name: &str, new_name: &str) -> Result<()> {
    let old_path = playlist_path(config, old_name);
    let new_path = playlist_path(config, new_name);
    
    let conn = connect(config)?;
    lock(config, &playlist_lock_name(old_name), 60.0)?;
    lock(config, &playlist_lock_name(new_name), 60.0)?;
    
    if !old_path.exists() {
        unlock(&conn, &playlist_lock_name(new_name))?;
        unlock(&conn, &playlist_lock_name(old_name))?;
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            format!("Playlist {} does not exist", old_name)
        )));
    }
    
    if new_path.exists() {
        unlock(&conn, &playlist_lock_name(new_name))?;
        unlock(&conn, &playlist_lock_name(old_name))?;
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            format!("Playlist {} already exists", new_name)
        )));
    }
    
    fs::rename(&old_path, &new_path)?;
    
    // Also rename all files with the same stem (e.g. cover arts)
    let playlists_dir = config.music_source_dir.join("!playlists");
    for entry in fs::read_dir(&playlists_dir)? {
        let entry = entry?;
        let file_path = entry.path();
        if let Some(stem) = file_path.file_stem() {
            if stem == old_name {
                let extension = file_path.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                if extension != "toml" {
                    let new_adjacent_file = playlists_dir.join(format!("{}.{}", new_name, extension));
                    if !new_adjacent_file.exists() {
                        fs::rename(&file_path, &new_adjacent_file)?;
                        debug!("Renaming playlist-adjacent file {:?} to {:?}", file_path, new_adjacent_file);
                    }
                }
            }
        }
    }
    
    unlock(&conn, &playlist_lock_name(new_name))?;
    unlock(&conn, &playlist_lock_name(old_name))?;
    
    info!("Renamed playlist {} to {}", old_name, new_name);
    update_cache_for_playlists(config, Some(vec![new_name.to_string()]), true)?;
    update_cache_evict_nonexistent_playlists(config)?;
    
    Ok(())
}

// Python: def remove_track_from_playlist(
pub fn remove_track_from_playlist(
    config: &Config,
    playlist_name: &str,
    track_id: &str,
) -> Result<()> {
    let track_logtext = get_track_logtext(config, track_id)?
        .ok_or_else(|| RoseError::Expected(RoseExpectedError::Generic(
            format!("Track {} does not exist", track_id)
        )))?;
    
    let path = playlist_path(config, playlist_name);
    if !path.exists() {
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            format!("Playlist {} does not exist", playlist_name)
        )));
    }
    
    let conn = connect(config)?;
    lock(config, &playlist_lock_name(playlist_name), 60.0)?;
    
    // Read the playlist data
    let data_str = fs::read_to_string(&path)?;
    let mut data: PlaylistData = toml::from_str(&data_str)
        .map_err(|e| RoseError::Generic(format!("Failed to parse playlist TOML: {}", e)))?;
    
    let old_len = data.tracks.len();
    data.tracks.retain(|t| t.uuid != track_id);
    
    if old_len == data.tracks.len() {
        unlock(&conn, &playlist_lock_name(playlist_name))?;
        info!("No-Op: Track {} not in playlist {}", track_logtext, playlist_name);
        return Ok(());
    }
    
    // Write back
    let toml_string = toml::to_string_pretty(&data)
        .map_err(|e| RoseError::Generic(format!("Failed to serialize playlist TOML: {}", e)))?;
    fs::write(&path, toml_string)?;
    
    unlock(&conn, &playlist_lock_name(playlist_name))?;
    
    info!("Removed track {} from playlist {}", track_logtext, playlist_name);
    update_cache_for_playlists(config, Some(vec![playlist_name.to_string()]), true)?;
    
    Ok(())
}

// Python: def add_track_to_playlist(
pub fn add_track_to_playlist(
    config: &Config,
    playlist_name: &str,
    track_id: &str,
) -> Result<()> {
    let track = get_track(config, track_id)?
        .ok_or_else(|| RoseError::Expected(RoseExpectedError::Generic(
            format!("Track {} does not exist", track_id)
        )))?;
    
    let path = playlist_path(config, playlist_name);
    if !path.exists() {
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            format!("Playlist {} does not exist", playlist_name)
        )));
    }
    
    let conn = connect(config)?;
    lock(config, &playlist_lock_name(playlist_name), 60.0)?;
    
    // Read the playlist data
    let data_str = fs::read_to_string(&path).unwrap_or_else(|_| "".to_string());
    let mut data: PlaylistData = if data_str.is_empty() {
        PlaylistData { tracks: Vec::new() }
    } else {
        toml::from_str(&data_str)
            .map_err(|e| RoseError::Generic(format!("Failed to parse playlist TOML: {}", e)))?
    };
    
    // Check if track is already in the playlist
    for t in &data.tracks {
        if t.uuid == track_id {
            unlock(&conn, &playlist_lock_name(playlist_name))?;
            info!("No-Op: Track {} already in playlist {}", track.tracktitle, playlist_name);
            return Ok(());
        }
    }
    
    // Add the track
    let desc = format!("{} - {}", 
        track.trackartists.main.iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        track.tracktitle
    );
    
    data.tracks.push(PlaylistTrack {
        uuid: track_id.to_string(),
        description_meta: desc.clone(),
    });
    
    // Write back
    let toml_string = toml::to_string_pretty(&data)
        .map_err(|e| RoseError::Generic(format!("Failed to serialize playlist TOML: {}", e)))?;
    fs::write(&path, toml_string)?;
    
    unlock(&conn, &playlist_lock_name(playlist_name))?;
    
    info!("Added track {} to playlist {}", desc, playlist_name);
    update_cache_for_playlists(config, Some(vec![playlist_name.to_string()]), true)?;
    
    Ok(())
}

// Python: def edit_playlist_in_editor(c: Config, playlist_name: str) -> None:
pub fn edit_playlist_in_editor(config: &Config, playlist_name: &str) -> Result<()> {
    let path = playlist_path(config, playlist_name);
    if !path.exists() {
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            format!("Playlist {} does not exist", playlist_name)
        )));
    }
    
    let conn = connect(config)?;
    lock(config, &playlist_lock_name(playlist_name), 300.0)?; // 5 minute timeout for editing
    
    // Read the playlist data
    let data_str = fs::read_to_string(&path)?;
    let data: PlaylistData = toml::from_str(&data_str)
        .map_err(|e| RoseError::Generic(format!("Failed to parse playlist TOML: {}", e)))?;
    
    // Count occurrences of each description
    let mut line_occurrences: HashMap<String, usize> = HashMap::new();
    for track in &data.tracks {
        *line_occurrences.entry(track.description_meta.clone()).or_insert(0) += 1;
    }
    
    // Create text for editing with UUID disambiguation for duplicates
    let mut lines_to_edit = Vec::new();
    let mut uuid_mapping: HashMap<String, String> = HashMap::new();
    
    for track in &data.tracks {
        let line = if line_occurrences[&track.description_meta] > 1 {
            format!("{} [{}]", track.description_meta, track.uuid)
        } else {
            track.description_meta.clone()
        };
        lines_to_edit.push(line.clone());
        uuid_mapping.insert(line, track.uuid.clone());
    }
    
    let edit_content = lines_to_edit.join("\n");
    
    // Write to temporary file
    let temp_file = config.cache_dir.join(format!("rose-edit-playlist-{}.txt", playlist_name));
    fs::write(&temp_file, &edit_content)?;
    
    // Open in editor
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
    let status = Command::new(&editor)
        .arg(&temp_file)
        .status()?;
    
    if !status.success() {
        unlock(&conn, &playlist_lock_name(playlist_name))?;
        fs::remove_file(&temp_file).ok();
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            "Editor exited with non-zero status".to_string()
        )));
    }
    
    // Read back and parse
    let edited_content = fs::read_to_string(&temp_file)?;
    fs::remove_file(&temp_file).ok();
    
    if edited_content.trim() == edit_content.trim() {
        unlock(&conn, &playlist_lock_name(playlist_name))?;
        info!("Aborting: no changes detected in playlist edit");
        return Ok(());
    }
    
    // Parse edited tracks
    let mut edited_tracks = Vec::new();
    for desc in edited_content.trim().split('\n') {
        let desc = desc.trim();
        if desc.is_empty() {
            continue;
        }
        
        let uuid = uuid_mapping.get(desc)
            .ok_or_else(|| RoseError::Expected(RoseExpectedError::Generic(
                format!("Track {} does not match a known track in the playlist. Was the line edited?", desc)
            )))?;
        
        // Remove the UUID suffix if present
        let clean_desc = if desc.contains(" [") && desc.ends_with(']') {
            desc.split(" [").next().unwrap_or(desc).to_string()
        } else {
            desc.to_string()
        };
        
        edited_tracks.push(PlaylistTrack {
            uuid: uuid.clone(),
            description_meta: clean_desc,
        });
    }
    
    // Update data
    let new_data = PlaylistData {
        tracks: edited_tracks,
    };
    
    // Write back
    let toml_string = toml::to_string_pretty(&new_data)
        .map_err(|e| RoseError::Generic(format!("Failed to serialize playlist TOML: {}", e)))?;
    fs::write(&path, toml_string)?;
    
    unlock(&conn, &playlist_lock_name(playlist_name))?;
    
    info!("Edited playlist {} from EDITOR", playlist_name);
    update_cache_for_playlists(config, Some(vec![playlist_name.to_string()]), true)?;
    
    Ok(())
}

// Python: def set_playlist_cover_art(
pub fn set_playlist_cover_art(
    config: &Config,
    playlist_name: &str,
    cover_art_path: &Path,
) -> Result<()> {
    let extension = cover_art_path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    
    if !config.valid_art_exts.iter().any(|ext| ext.eq_ignore_ascii_case(extension)) {
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            format!("Invalid cover art file extension: {}", extension)
        )));
    }
    
    let path = playlist_path(config, playlist_name);
    if !path.exists() {
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            format!("Playlist {} does not exist", playlist_name)
        )));
    }
    
    // Remove existing cover arts
    let playlists_dir = config.music_source_dir.join("!playlists");
    for entry in fs::read_dir(&playlists_dir)? {
        let entry = entry?;
        let file_path = entry.path();
        if let Some(stem) = file_path.file_stem() {
            if stem == playlist_name {
                if let Some(ext) = file_path.extension() {
                    if config.valid_art_exts.iter().any(|valid_ext| valid_ext.eq_ignore_ascii_case(&ext.to_string_lossy())) {
                        debug!("Deleting existing cover art {:?} in playlists", file_path);
                        fs::remove_file(&file_path)?;
                    }
                }
            }
        }
    }
    
    // Copy new cover art
    let dest_path = playlists_dir.join(format!("{}.{}", playlist_name, extension));
    fs::copy(cover_art_path, &dest_path)?;
    
    info!("Set the cover of playlist {} to {:?}", playlist_name, cover_art_path.file_name().unwrap());
    update_cache_for_playlists(config, Some(vec![playlist_name.to_string()]), false)?;
    
    Ok(())
}

// Python: def delete_playlist_cover_art(c: Config, playlist_name: str) -> None:
pub fn delete_playlist_cover_art(config: &Config, playlist_name: &str) -> Result<()> {
    let path = playlist_path(config, playlist_name);
    if !path.exists() {
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            format!("Playlist {} does not exist", playlist_name)
        )));
    }
    
    let mut found = false;
    let playlists_dir = config.music_source_dir.join("!playlists");
    for entry in fs::read_dir(&playlists_dir)? {
        let entry = entry?;
        let file_path = entry.path();
        if let Some(stem) = file_path.file_stem() {
            if stem == playlist_name {
                if let Some(ext) = file_path.extension() {
                    if config.valid_art_exts.iter().any(|valid_ext| valid_ext.eq_ignore_ascii_case(&ext.to_string_lossy())) {
                        debug!("Deleting existing cover art {:?} in playlists", file_path);
                        fs::remove_file(&file_path)?;
                        found = true;
                    }
                }
            }
        }
    }
    
    if found {
        info!("Deleted cover arts of playlist {}", playlist_name);
    } else {
        info!("No-Op: No cover arts found for playlist {}", playlist_name);
    }
    
    update_cache_for_playlists(config, Some(vec![playlist_name.to_string()]), false)?;
    
    Ok(())
}

// Python: def playlist_path(c: Config, name: str) -> Path:
pub fn playlist_path(config: &Config, name: &str) -> PathBuf {
    config.music_source_dir.join("!playlists").join(format!("{}.toml", name))
}

// Helper function to get track logtext
fn get_track_logtext(config: &Config, track_id: &str) -> Result<Option<String>> {
    let track = get_track(config, track_id)?;
    Ok(track.map(|t| {
        format!("{} - {}", 
            t.trackartists.main.iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            t.tracktitle
        )
    }))
}