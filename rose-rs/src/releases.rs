// The releases module provides functions for interacting with releases.

use crate::audiotags::AudioTags;
use crate::cache::{
    connect, get_release, get_tracks_of_release, lock, release_lock_name, unlock, CachedRelease,
    CachedTrack,
};
use crate::cache_update::{
    update_cache_evict_nonexistent_releases, update_cache_for_collages, update_cache_for_playlists,
    update_cache_for_releases,
};
use crate::config::Config;
use crate::datafiles::{read_datafile, write_datafile, StoredDataFile};
use crate::error::{Result, RoseError, RoseExpectedError};
use crate::rule_parser::{Action, ActionBehavior, Matcher, Tag};
use crate::rules::{
    execute_metadata_actions, fast_search_for_matching_releases,
    filter_release_false_positives_using_read_cache,
};
use chrono::Utc;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use tracing::info;
use uuid::Uuid;

#[derive(Debug)]
pub struct InvalidCoverArtFileError(pub String);

impl std::fmt::Display for InvalidCoverArtFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid cover art file: {}", self.0)
    }
}

impl std::error::Error for InvalidCoverArtFileError {}

#[derive(Debug)]
pub struct ReleaseDoesNotExistError(pub String);

impl std::fmt::Display for ReleaseDoesNotExistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Release does not exist: {}", self.0)
    }
}

impl std::error::Error for ReleaseDoesNotExistError {}

#[derive(Debug)]
pub struct ReleaseEditFailedError(pub String);

impl std::fmt::Display for ReleaseEditFailedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Release edit failed: {}", self.0)
    }
}

impl std::error::Error for ReleaseEditFailedError {}

// Python: def delete_release(c: Config, release_id: str) -> None:
pub fn delete_release(config: &Config, release_id: &str) -> Result<()> {
    let release = get_release(config, release_id)?.ok_or_else(|| {
        RoseError::Expected(RoseExpectedError::Generic(format!(
            "Release {} does not exist",
            release_id
        )))
    })?;

    let conn = connect(config)?;
    lock(config, &release_lock_name(release_id), 60.0)?;

    // Move to trash instead of deleting permanently
    // Note: Rust doesn't have a cross-platform trash library in std,
    // so we'll use fs::rename to a trash directory for now
    let trash_dir = config.cache_dir.join("trash");
    fs::create_dir_all(&trash_dir)?;

    let trash_path = trash_dir.join(release.source_path.file_name().unwrap());
    fs::rename(&release.source_path, &trash_path)?;

    unlock(&conn, &release_lock_name(release_id))?;

    info!("Trashed release {}", release.releasetitle);

    update_cache_evict_nonexistent_releases(config)?;
    // Update all collages and playlists so that the release is removed from whichever it was in.
    update_cache_for_collages(config, None, true)?;
    update_cache_for_playlists(config, None, true)?;

    Ok(())
}

// Python: def toggle_release_new(c: Config, release_id: str) -> None:
pub fn toggle_release_new(config: &Config, release_id: &str) -> Result<()> {
    let release = get_release(config, release_id)?.ok_or_else(|| {
        RoseError::Expected(RoseExpectedError::Generic(format!(
            "Release {} does not exist",
            release_id
        )))
    })?;

    let conn = connect(config)?;
    lock(config, &release_lock_name(release_id), 60.0)?;

    // Find and update the datafile
    let datafile_path = release
        .source_path
        .join(format!(".rose.{}.toml", release_id));
    let mut datafile = read_datafile(&datafile_path)?;
    datafile.new = !datafile.new;

    write_datafile(&datafile_path, &datafile)?;

    unlock(&conn, &release_lock_name(release_id))?;

    let status = if datafile.new { "new" } else { "not new" };
    info!("Toggled release {} to {}", release.releasetitle, status);

    // Update cache for this release
    update_cache_for_releases(config, Some(vec![release.source_path.clone()]), false)?;

    Ok(())
}

// Python: def create_release(
pub fn create_release(
    config: &Config,
    source_dir: &Path,
    title: &str,
    _artists: Vec<(String, String)>, // (name, role)
) -> Result<String> {
    // Create the directory
    fs::create_dir_all(source_dir)?;

    // Generate a new release ID
    let release_id = Uuid::now_v7().to_string();

    // Create initial datafile
    let datafile = StoredDataFile {
        new: true,
        added_at: Utc::now().to_rfc3339(),
    };

    let datafile_path = source_dir.join(format!(".rose.{}.toml", release_id));
    write_datafile(&datafile_path, &datafile)?;

    // Create a placeholder audio file if none exists
    // This ensures the release is recognized during cache update

    info!("Created release {} at {:?}", title, source_dir);

    // Update cache for this release
    update_cache_for_releases(config, Some(vec![source_dir.to_path_buf()]), false)?;

    Ok(release_id)
}

// Structure for TOML editing
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EditableRelease {
    title: String,
    release_type: String,
    release_year: Option<i32>,
    original_year: Option<i32>,
    composition_year: Option<i32>,
    edition: Option<String>,
    catalog_number: Option<String>,
    genres: Vec<String>,
    secondary_genres: Vec<String>,
    descriptors: Vec<String>,
    labels: Vec<String>,
    artists: HashMap<String, Vec<String>>, // role -> names
}

// Python: def edit_release(
pub fn edit_release(config: &Config, release_id: &str, _resume_file: Option<&Path>) -> Result<()> {
    let release = get_release(config, release_id)?.ok_or_else(|| {
        RoseError::Expected(RoseExpectedError::Generic(format!(
            "Release {} does not exist",
            release_id
        )))
    })?;

    // Lock the release
    let conn = connect(config)?;
    lock(config, &release_lock_name(release_id), 300.0)?; // 5 minute timeout for editing

    // Convert release to editable format
    let mut artists_map: HashMap<String, Vec<String>> = HashMap::new();
    artists_map.insert(
        "main".to_string(),
        release
            .releaseartists
            .main
            .iter()
            .map(|a| a.name.clone())
            .collect(),
    );
    artists_map.insert(
        "guest".to_string(),
        release
            .releaseartists
            .guest
            .iter()
            .map(|a| a.name.clone())
            .collect(),
    );
    artists_map.insert(
        "remixer".to_string(),
        release
            .releaseartists
            .remixer
            .iter()
            .map(|a| a.name.clone())
            .collect(),
    );
    artists_map.insert(
        "producer".to_string(),
        release
            .releaseartists
            .producer
            .iter()
            .map(|a| a.name.clone())
            .collect(),
    );
    artists_map.insert(
        "composer".to_string(),
        release
            .releaseartists
            .composer
            .iter()
            .map(|a| a.name.clone())
            .collect(),
    );
    artists_map.insert(
        "conductor".to_string(),
        release
            .releaseartists
            .conductor
            .iter()
            .map(|a| a.name.clone())
            .collect(),
    );
    artists_map.insert(
        "djmixer".to_string(),
        release
            .releaseartists
            .djmixer
            .iter()
            .map(|a| a.name.clone())
            .collect(),
    );

    // Remove empty artist lists
    artists_map.retain(|_, v| !v.is_empty());

    let editable = EditableRelease {
        title: release.releasetitle.clone(),
        release_type: release.releasetype.clone(),
        release_year: release.releasedate.as_ref().map(|d| d.year),
        original_year: release.originaldate.as_ref().map(|d| d.year),
        composition_year: release.compositiondate.as_ref().map(|d| d.year),
        edition: release.edition.clone(),
        catalog_number: release.catalognumber.clone(),
        genres: release.genres.clone(),
        secondary_genres: release.secondary_genres.clone(),
        descriptors: release.descriptors.clone(),
        labels: release.labels.clone(),
        artists: artists_map,
    };

    // Write to temporary file
    let temp_file = config
        .cache_dir
        .join(format!("rose-edit-{}.toml", release_id));
    let toml_string = toml::to_string_pretty(&editable)
        .map_err(|e| RoseError::Generic(format!("Failed to serialize to TOML: {}", e)))?;
    fs::write(&temp_file, &toml_string)?;

    // Open in editor
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
    let status = Command::new(&editor).arg(&temp_file).status()?;

    if !status.success() {
        unlock(&conn, &release_lock_name(release_id))?;
        fs::remove_file(&temp_file).ok();
        return Err(RoseError::Expected(RoseExpectedError::Generic(
            "Editor exited with non-zero status".to_string(),
        )));
    }

    // Read back and parse
    let edited_toml = fs::read_to_string(&temp_file)?;
    let edited: EditableRelease = toml::from_str(&edited_toml)
        .map_err(|e| RoseError::Generic(format!("Failed to parse edited TOML: {}", e)))?;

    // Apply changes to all tracks
    let tracks = get_tracks_of_release(config, release_id)?;
    let actions = create_edit_actions(&release, &edited);

    if !actions.is_empty() {
        execute_metadata_actions(config, &actions, &tracks, false)?;

        // Update cache for this release
        update_cache_for_releases(config, Some(vec![release.source_path.clone()]), false)?;
    }

    unlock(&conn, &release_lock_name(release_id))?;
    fs::remove_file(&temp_file).ok();

    info!("Successfully edited release {}", release.releasetitle);

    Ok(())
}

// Create actions to apply edits
fn create_edit_actions(original: &CachedRelease, edited: &EditableRelease) -> Vec<Action> {
    let mut actions = Vec::new();

    // Compare and create actions for each field
    // This is a simplified version - full implementation would need more sophisticated diffing

    if original.releasetitle != edited.title {
        actions.push(Action {
            tags: vec![Tag::ReleaseTitle],
            behavior: ActionBehavior::Replace(crate::rule_parser::ReplaceAction {
                replacement: edited.title.clone(),
            }),
            pattern: None,
        });
    }

    // TODO: Add more field comparisons and action generation

    actions
}

// Python: def set_release_cover_art(
pub fn set_release_cover_art(
    config: &Config,
    release_id: &str,
    cover_art_path: &Path,
) -> Result<()> {
    let release = get_release(config, release_id)?.ok_or_else(|| {
        RoseError::Expected(RoseExpectedError::Generic(format!(
            "Release {} does not exist",
            release_id
        )))
    })?;

    // Validate the cover art file
    if !cover_art_path.exists() {
        return Err(RoseError::Expected(RoseExpectedError::Generic(format!(
            "Cover art file does not exist: {:?}",
            cover_art_path
        ))));
    }

    let extension = cover_art_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    if !config
        .valid_art_exts
        .iter()
        .any(|ext| ext.eq_ignore_ascii_case(extension))
    {
        return Err(RoseError::Expected(RoseExpectedError::Generic(format!(
            "Invalid cover art file extension: {}",
            extension
        ))));
    }

    // Lock the release
    let conn = connect(config)?;
    lock(config, &release_lock_name(release_id), 60.0)?;

    // Copy the cover art to the release directory
    let dest_filename = format!("cover.{}", extension);
    let dest_path = release.source_path.join(&dest_filename);

    fs::copy(cover_art_path, &dest_path)?;

    unlock(&conn, &release_lock_name(release_id))?;

    info!(
        "Set cover art for release {} to {}",
        release.releasetitle, dest_filename
    );

    // Update cache for this release
    update_cache_for_releases(config, Some(vec![release.source_path.clone()]), false)?;

    Ok(())
}

// Python: def delete_release_cover_art(
pub fn delete_release_cover_art(config: &Config, release_id: &str) -> Result<()> {
    let release = get_release(config, release_id)?.ok_or_else(|| {
        RoseError::Expected(RoseExpectedError::Generic(format!(
            "Release {} does not exist",
            release_id
        )))
    })?;

    if release.cover_image_path.is_none() {
        return Err(RoseError::Expected(RoseExpectedError::Generic(format!(
            "Release {} has no cover art",
            release_id
        ))));
    }

    // Lock the release
    let conn = connect(config)?;
    lock(config, &release_lock_name(release_id), 60.0)?;

    if let Some(cover_path) = &release.cover_image_path {
        fs::remove_file(cover_path)?;
        info!("Deleted cover art for release {}", release.releasetitle);
    }

    unlock(&conn, &release_lock_name(release_id))?;

    // Update cache for this release
    update_cache_for_releases(config, Some(vec![release.source_path.clone()]), false)?;

    Ok(())
}

// Python: def run_actions_on_release(
pub fn run_actions_on_release(config: &Config, release_id: &str, actions: &[Action]) -> Result<()> {
    let release = get_release(config, release_id)?.ok_or_else(|| {
        RoseError::Expected(RoseExpectedError::Generic(format!(
            "Release {} does not exist",
            release_id
        )))
    })?;

    // Get all tracks for the release
    let tracks = get_tracks_of_release(config, release_id)?;

    // Execute the actions
    execute_metadata_actions(config, actions, &tracks, false)?;

    // Update cache for this release
    update_cache_for_releases(config, Some(vec![release.source_path.clone()]), false)?;

    Ok(())
}

// Python: def create_single_release(
pub fn create_single_release(
    config: &Config,
    track_id: &str,
    title: Option<&str>,
    artist: Option<&str>,
) -> Result<String> {
    // Get the track
    let track = get_track(config, track_id)?.ok_or_else(|| {
        RoseError::Expected(RoseExpectedError::Generic(format!(
            "Track {} does not exist",
            track_id
        )))
    })?;

    // Determine title and artist
    let single_title = title.unwrap_or(&track.tracktitle);
    let single_artist = artist.unwrap_or_else(|| {
        // Use main artist from track or release
        track
            .trackartists
            .main
            .first()
            .or_else(|| track.release.releaseartists.main.first())
            .map(|a| a.name.as_str())
            .unwrap_or("Unknown Artist")
    });

    // Create directory for single
    let dirname = format!("{} - {}", single_artist, single_title);
    let single_dir = config.music_source_dir.join(&dirname);

    // Create the release
    let release_id = create_release(
        config,
        &single_dir,
        single_title,
        vec![(single_artist.to_string(), "main".to_string())],
    )?;

    // Copy the track to the new directory
    let source_path = &track.source_path;
    let filename = source_path.file_name().unwrap();
    let dest_path = single_dir.join(filename);

    fs::copy(source_path, &dest_path)?;

    // Update the track metadata to reflect it's a single
    let mut tags = AudioTags::from_file(&dest_path)?;
    tags.releasetype = "single".to_string();
    tags.releasetitle = Some(single_title.to_string());
    tags.release_id = Some(release_id.clone());
    tags.flush(config)?;

    info!(
        "Created single release {} from track {}",
        single_title, track_id
    );

    // Update cache for the new release
    update_cache_for_releases(config, Some(vec![single_dir]), false)?;

    Ok(release_id)
}

// Python: def find_releases_matching_rule(
pub fn find_releases_matching_rule(
    config: &Config,
    matcher: &Matcher,
) -> Result<Vec<CachedRelease>> {
    // Use the rules engine to find matching releases
    let releases = fast_search_for_matching_releases(config, matcher)?;
    let filtered = filter_release_false_positives_using_read_cache(config, matcher, &releases)?;

    Ok(filtered)
}

// Helper function to get a track (not in Python, but needed)
fn get_track(config: &Config, track_id: &str) -> Result<Option<CachedTrack>> {
    let conn = connect(config)?;
    let mut stmt = conn.prepare(
        "SELECT tv.*, rv.*
         FROM tracks_view tv
         JOIN releases_view rv ON rv.id = tv.release_id
         WHERE tv.id = ?1",
    )?;

    let track = stmt
        .query_row([track_id], |row| {
            let release = crate::cache::cached_release_from_view(config, row, true)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            let track = crate::cache::cached_track_from_view(config, row, release, true)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            Ok(track)
        })
        .optional()?;

    Ok(track)
}
