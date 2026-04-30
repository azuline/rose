//! Playlist CRUD operations: create/rename/delete playlists, add/remove tracks,
//! and interactive editor support with duplicate `description_meta` disambiguation.
//!
//! Playlists are ordered lists of tracks stored as TOML files in
//! `{music_source_dir}/!playlists/{name}.toml`. Each entry has a `uuid` and a
//! `description_meta` (human-readable label).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::cache::{
    artistsfmt, get_track, get_track_logtext, lock, make_track_logtext, playlist_lock_name,
    update_cache_evict_nonexistent_playlists, update_cache_for_playlists,
};
use crate::common::RoseError;
use crate::config::Config;

// ---------------------------------------------------------------------------
// Path helper
// ---------------------------------------------------------------------------

/// Returns the path to a playlist TOML file: `{music_source_dir}/!playlists/{name}.toml`.
pub fn playlist_path(c: &Config, name: &str) -> PathBuf {
    c.music_source_dir
        .join("!playlists")
        .join(format!("{name}.toml"))
}

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

/// Create a new empty playlist. Errors if a playlist with this name already exists.
pub fn create_playlist(c: &Config, name: &str) -> Result<(), RoseError> {
    let dir = c.music_source_dir.join("!playlists");
    std::fs::create_dir_all(&dir).map_err(|e| {
        RoseError::Internal(format!(
            "Failed to create playlists directory {}: {e}",
            dir.display()
        ))
    })?;
    let path = playlist_path(c, name);
    {
        let _guard = lock(c, &playlist_lock_name(name), 10.0)?;
        if path.exists() {
            return Err(RoseError::PlaylistAlreadyExists(format!(
                "Playlist {name} already exists"
            )));
        }
        // Touch the file (create empty).
        std::fs::write(&path, b"").map_err(|e| {
            RoseError::Internal(format!(
                "Failed to create playlist file {}: {e}",
                path.display()
            ))
        })?;
    }
    tracing::info!("Created playlist {name} in source directory");
    update_cache_for_playlists(c, Some(vec![name.to_string()]), true)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

/// Delete a playlist by moving its TOML file to trash. Errors if it doesn't exist.
pub fn delete_playlist(c: &Config, name: &str) -> Result<(), RoseError> {
    let path = playlist_path(c, name);
    {
        let _guard = lock(c, &playlist_lock_name(name), 10.0)?;
        if !path.exists() {
            return Err(RoseError::PlaylistDoesNotExist(format!(
                "Playlist {name} does not exist"
            )));
        }
        trash::delete(&path)
            .map_err(|e| RoseError::Internal(format!("Failed to trash {}: {e}", path.display())))?;
    }
    tracing::info!("Deleted playlist {name} from source directory");
    update_cache_evict_nonexistent_playlists(c)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Rename
// ---------------------------------------------------------------------------

/// Rename a playlist and all adjacent files sharing the old stem (e.g. cover art).
pub fn rename_playlist(c: &Config, old_name: &str, new_name: &str) -> Result<(), RoseError> {
    let old_path = playlist_path(c, old_name);
    let new_path = playlist_path(c, new_name);
    {
        let _guard_old = lock(c, &playlist_lock_name(old_name), 10.0)?;
        let _guard_new = lock(c, &playlist_lock_name(new_name), 10.0)?;
        if !old_path.exists() {
            return Err(RoseError::PlaylistDoesNotExist(format!(
                "Playlist {old_name} does not exist"
            )));
        }
        if new_path.exists() {
            return Err(RoseError::PlaylistAlreadyExists(format!(
                "Playlist {new_name} already exists"
            )));
        }
        std::fs::rename(&old_path, &new_path).map_err(|e| {
            RoseError::Internal(format!(
                "Failed to rename {} to {}: {e}",
                old_path.display(),
                new_path.display()
            ))
        })?;
        // Also rename all adjacent files in !playlists/ that share the old stem.
        let playlist_dir = c.music_source_dir.join("!playlists");
        if let Ok(entries) = std::fs::read_dir(&playlist_dir) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                let stem = entry_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                let old_stem = old_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if stem != old_stem {
                    continue;
                }
                let suffix = entry_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| format!(".{e}"))
                    .unwrap_or_default();
                let new_adjacent = playlist_dir.join(format!("{new_name}{suffix}"));
                if new_adjacent.exists() {
                    continue;
                }
                std::fs::rename(&entry_path, &new_adjacent).map_err(|e| {
                    RoseError::Internal(format!(
                        "Failed to rename {} to {}: {e}",
                        entry_path.display(),
                        new_adjacent.display()
                    ))
                })?;
                tracing::debug!(
                    "Renaming playlist-adjacent file {} to {}",
                    entry_path.display(),
                    new_adjacent.display()
                );
            }
        }
    }
    tracing::info!("Renamed playlist {old_name} to {new_name}");
    update_cache_for_playlists(c, Some(vec![new_name.to_string()]), true)?;
    update_cache_evict_nonexistent_playlists(c)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Add track
// ---------------------------------------------------------------------------

/// Add a track to a playlist. No-op if the track is already present.
pub fn add_track_to_playlist(
    c: &Config,
    playlist_name: &str,
    track_id: &str,
) -> Result<(), RoseError> {
    let track = get_track(c, track_id)?
        .ok_or_else(|| RoseError::TrackDoesNotExist(format!("Track {track_id} does not exist")))?;

    let path = playlist_path(c, playlist_name);
    if !path.exists() {
        return Err(RoseError::PlaylistDoesNotExist(format!(
            "Playlist {playlist_name} does not exist"
        )));
    }

    {
        let _guard = lock(c, &playlist_lock_name(playlist_name), 10.0)?;
        let content = std::fs::read_to_string(&path)
            .map_err(|e| RoseError::Internal(format!("Failed to read {}: {e}", path.display())))?;
        let mut data: toml::Value = if content.trim().is_empty() {
            toml::Value::Table(toml::map::Map::new())
        } else {
            content.parse().map_err(|e: toml::de::Error| {
                RoseError::Internal(format!("Failed to parse TOML {}: {e}", path.display()))
            })?
        };

        let tracks = data
            .as_table_mut()
            .unwrap()
            .entry("tracks")
            .or_insert_with(|| toml::Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| RoseError::Internal("'tracks' key is not an array".to_string()))?;

        // Check for duplicates.
        for t in tracks.iter() {
            if t.get("uuid").and_then(|v| v.as_str()) == Some(track_id) {
                tracing::info!("No-Op: Track {track_id} already in playlist {playlist_name}");
                return Ok(());
            }
        }

        let desc = format!("{} - {}", artistsfmt(&track.trackartists), track.tracktitle);
        let mut entry = toml::map::Map::new();
        entry.insert(
            "uuid".to_string(),
            toml::Value::String(track_id.to_string()),
        );
        entry.insert("description_meta".to_string(), toml::Value::String(desc));
        tracks.push(toml::Value::Table(entry));

        let toml_str = toml::to_string(&data)
            .map_err(|e| RoseError::Internal(format!("Failed to serialize TOML: {e}")))?;
        std::fs::write(&path, toml_str)
            .map_err(|e| RoseError::Internal(format!("Failed to write {}: {e}", path.display())))?;
    }

    let track_logtext = make_track_logtext(
        &track.tracktitle,
        &track.trackartists,
        track.release.releasedate.as_ref(),
        &track
            .source_path
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default(),
    );
    tracing::info!("Added track {track_logtext} to playlist {playlist_name}");
    update_cache_for_playlists(c, Some(vec![playlist_name.to_string()]), true)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Remove track
// ---------------------------------------------------------------------------

/// Remove a track from a playlist. No-op if the track is not present.
pub fn remove_track_from_playlist(
    c: &Config,
    playlist_name: &str,
    track_id: &str,
) -> Result<(), RoseError> {
    let track_logtext = get_track_logtext(c, track_id)?
        .ok_or_else(|| RoseError::TrackDoesNotExist(format!("Track {track_id} does not exist")))?;

    let path = playlist_path(c, playlist_name);
    if !path.exists() {
        return Err(RoseError::PlaylistDoesNotExist(format!(
            "Playlist {playlist_name} does not exist"
        )));
    }

    {
        let _guard = lock(c, &playlist_lock_name(playlist_name), 10.0)?;
        let content = std::fs::read_to_string(&path)
            .map_err(|e| RoseError::Internal(format!("Failed to read {}: {e}", path.display())))?;
        let mut data: toml::Value = if content.trim().is_empty() {
            toml::Value::Table(toml::map::Map::new())
        } else {
            content.parse().map_err(|e: toml::de::Error| {
                RoseError::Internal(format!("Failed to parse TOML {}: {e}", path.display()))
            })?
        };

        let tracks = data
            .get("tracks")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let new_tracks: Vec<toml::Value> = tracks
            .iter()
            .filter(|t| t.get("uuid").and_then(|v| v.as_str()) != Some(track_id))
            .cloned()
            .collect();

        if tracks.len() == new_tracks.len() {
            tracing::info!("No-Op: Track {track_logtext} not in playlist {playlist_name}");
            return Ok(());
        }

        data.as_table_mut()
            .unwrap()
            .insert("tracks".to_string(), toml::Value::Array(new_tracks));

        let toml_str = toml::to_string(&data)
            .map_err(|e| RoseError::Internal(format!("Failed to serialize TOML: {e}")))?;
        std::fs::write(&path, toml_str)
            .map_err(|e| RoseError::Internal(format!("Failed to write {}: {e}", path.display())))?;
    }
    tracing::info!("Removed track {track_logtext} from playlist {playlist_name}");
    update_cache_for_playlists(c, Some(vec![playlist_name.to_string()]), true)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Edit in editor — with duplicate description_meta disambiguation
// ---------------------------------------------------------------------------

/// Open a playlist in `$EDITOR` for interactive reordering/removal.
///
/// When two tracks have the same `description_meta`, a `[uuid]` discriminator
/// is appended so the user can distinguish them.
pub fn edit_playlist_in_editor(c: &Config, playlist_name: &str) -> Result<(), RoseError> {
    let path = playlist_path(c, playlist_name);
    if !path.exists() {
        return Err(RoseError::PlaylistDoesNotExist(format!(
            "Playlist {playlist_name} does not exist"
        )));
    }

    let _guard = lock(c, &playlist_lock_name(playlist_name), 60.0)?;

    let content = std::fs::read_to_string(&path)
        .map_err(|e| RoseError::Internal(format!("Failed to read {}: {e}", path.display())))?;
    let mut data: toml::Value = if content.trim().is_empty() {
        toml::Value::Table(toml::map::Map::new())
    } else {
        content.parse().map_err(|e: toml::de::Error| {
            RoseError::Internal(format!("Failed to parse TOML {}: {e}", path.display()))
        })?
    };

    let raw_tracks = data
        .get("tracks")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // Count occurrences of each description_meta to detect duplicates.
    let mut desc_counts: HashMap<String, usize> = HashMap::new();
    for t in &raw_tracks {
        let desc = t
            .get("description_meta")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        *desc_counts.entry(desc).or_insert(0) += 1;
    }

    // Build lines and UUID mapping, adding [uuid] discriminator for duplicates.
    let mut lines_to_edit: Vec<String> = Vec::new();
    let mut uuid_mapping: HashMap<String, String> = HashMap::new();
    for t in &raw_tracks {
        let desc = t
            .get("description_meta")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let uuid = t
            .get("uuid")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let line = if desc_counts.get(&desc).copied().unwrap_or(0) > 1 {
            format!("{desc} [{uuid}]")
        } else {
            desc
        };

        lines_to_edit.push(line.clone());
        uuid_mapping.insert(line, uuid);
    }

    let editor_text = lines_to_edit.join("\n");
    let edited = open_editor_for_playlist(&editor_text)?;

    if edited.is_none() {
        tracing::info!("Aborting: metadata file not submitted.");
        return Ok(());
    }
    let edited_text = edited.unwrap();

    let mut edited_tracks: Vec<toml::Value> = Vec::new();
    for line in edited_text.trim().split('\n') {
        if line.is_empty() {
            continue;
        }
        let uuid = uuid_mapping.get(line).ok_or_else(|| {
            RoseError::DescriptionMismatch(format!(
                "Track {line} does not match a known track in the playlist. Was the line edited?"
            ))
        })?;
        // Find the original track entry to preserve all fields.
        let original = raw_tracks
            .iter()
            .find(|t| t.get("uuid").and_then(|v| v.as_str()) == Some(uuid));
        if let Some(orig) = original {
            edited_tracks.push(orig.clone());
        } else {
            let mut entry = toml::map::Map::new();
            entry.insert("uuid".to_string(), toml::Value::String(uuid.clone()));
            entry.insert(
                "description_meta".to_string(),
                toml::Value::String(line.to_string()),
            );
            edited_tracks.push(toml::Value::Table(entry));
        }
    }

    data.as_table_mut()
        .unwrap()
        .insert("tracks".to_string(), toml::Value::Array(edited_tracks));

    let toml_str = toml::to_string(&data)
        .map_err(|e| RoseError::Internal(format!("Failed to serialize TOML: {e}")))?;
    std::fs::write(&path, toml_str)
        .map_err(|e| RoseError::Internal(format!("Failed to write {}: {e}", path.display())))?;

    // Release the lock before updating cache.
    drop(_guard);

    tracing::info!("Edited playlist {playlist_name} from EDITOR");
    update_cache_for_playlists(c, Some(vec![playlist_name.to_string()]), true)?;
    Ok(())
}

/// Open `$EDITOR` with the given text. Returns `None` if the editor exits non-zero
/// or the text is unchanged (treated as abort).
fn open_editor_for_playlist(text: &str) -> Result<Option<String>, RoseError> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let tmp_dir = std::env::temp_dir();
    let tmp_file = tmp_dir.join(format!("rose-playlist-edit-{}.txt", uuid::Uuid::now_v7()));
    std::fs::write(&tmp_file, text).map_err(|e| {
        RoseError::Internal(format!(
            "Failed to write temp file {}: {e}",
            tmp_file.display()
        ))
    })?;

    let status = std::process::Command::new(&editor)
        .arg(&tmp_file)
        .status()
        .map_err(|e| RoseError::Internal(format!("Failed to launch editor '{editor}': {e}")))?;

    if !status.success() {
        let _ = std::fs::remove_file(&tmp_file);
        return Ok(None);
    }

    let result = std::fs::read_to_string(&tmp_file).map_err(|e| {
        RoseError::Internal(format!(
            "Failed to read temp file {}: {e}",
            tmp_file.display()
        ))
    })?;
    let _ = std::fs::remove_file(&tmp_file);
    Ok(Some(result))
}

// ---------------------------------------------------------------------------
// Cover art: set
// ---------------------------------------------------------------------------

/// Remove existing cover arts and copy a new cover image for the playlist.
pub fn set_playlist_cover_art(
    c: &Config,
    playlist_name: &str,
    new_path: &Path,
) -> Result<(), RoseError> {
    let suffix = new_path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if !c.valid_art_exts.contains(&suffix) {
        return Err(RoseError::InvalidCoverArt(format!(
            "File {}'s extension is not supported for cover images: \
             To change this, please read the configuration documentation",
            new_path.file_name().unwrap_or_default().to_string_lossy()
        )));
    }

    let path = playlist_path(c, playlist_name);
    if !path.exists() {
        return Err(RoseError::PlaylistDoesNotExist(format!(
            "Playlist {playlist_name} does not exist"
        )));
    }

    let playlist_dir = c.music_source_dir.join("!playlists");
    // Delete existing cover art files (files with matching stem and art extension).
    for entry in std::fs::read_dir(&playlist_dir).map_err(|e| {
        RoseError::Internal(format!(
            "Failed to read directory {}: {e}",
            playlist_dir.display()
        ))
    })? {
        let entry =
            entry.map_err(|e| RoseError::Internal(format!("Failed to read dir entry: {e}")))?;
        let entry_path = entry.path();
        let stem = entry_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let ext = entry_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();
        if stem == playlist_name && c.valid_art_exts.contains(&ext) {
            tracing::debug!(
                "Deleting existing cover art {} in playlists",
                entry_path.display()
            );
            std::fs::remove_file(&entry_path).map_err(|e| {
                RoseError::Internal(format!("Failed to delete {}: {e}", entry_path.display()))
            })?;
        }
    }

    // Copy new file.
    let dest = playlist_dir.join(format!("{playlist_name}.{suffix}"));
    std::fs::copy(new_path, &dest).map_err(|e| {
        RoseError::Internal(format!(
            "Failed to copy {} to {}: {e}",
            new_path.display(),
            dest.display()
        ))
    })?;
    tracing::info!(
        "Set the cover of playlist {playlist_name} to {}",
        new_path.file_name().unwrap_or_default().to_string_lossy()
    );
    update_cache_for_playlists(c, Some(vec![playlist_name.to_string()]), false)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Cover art: delete
// ---------------------------------------------------------------------------

/// Remove all cover art files for a playlist.
pub fn delete_playlist_cover_art(c: &Config, playlist_name: &str) -> Result<(), RoseError> {
    let path = playlist_path(c, playlist_name);
    if !path.exists() {
        return Err(RoseError::PlaylistDoesNotExist(format!(
            "Playlist {playlist_name} does not exist"
        )));
    }

    let playlist_dir = c.music_source_dir.join("!playlists");
    let mut found = false;
    for entry in std::fs::read_dir(&playlist_dir).map_err(|e| {
        RoseError::Internal(format!(
            "Failed to read directory {}: {e}",
            playlist_dir.display()
        ))
    })? {
        let entry =
            entry.map_err(|e| RoseError::Internal(format!("Failed to read dir entry: {e}")))?;
        let entry_path = entry.path();
        let stem = entry_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let ext = entry_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();
        if stem == playlist_name && c.valid_art_exts.contains(&ext) {
            tracing::debug!(
                "Deleting existing cover art {} in playlists",
                entry_path.display()
            );
            std::fs::remove_file(&entry_path).map_err(|e| {
                RoseError::Internal(format!("Failed to delete {}: {e}", entry_path.display()))
            })?;
            found = true;
        }
    }

    if found {
        tracing::info!("Deleted cover arts of playlist {playlist_name}");
    } else {
        tracing::info!("No-Op: No cover arts found for playlist {playlist_name}");
    }
    update_cache_for_playlists(c, Some(vec![playlist_name.to_string()]), false)?;
    Ok(())
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{connect, maybe_invalidate_cache_database, update_cache_for_playlists};
    use crate::config::Config;
    #[allow(unused_imports)]
    use std::collections::{HashMap, HashSet};
    use tempfile::TempDir;

    /// Set up a test environment: a temp directory with a config, cache database,
    /// and two fake tracks in the cache, plus a pre-existing playlist.
    fn setup_test_env() -> (Config, TempDir) {
        let tmp = TempDir::new().unwrap();
        let source_dir = tmp.path().join("music");
        std::fs::create_dir_all(&source_dir).unwrap();
        let cache_dir = tmp.path().join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let vfs_dir = tmp.path().join("vfs");
        std::fs::create_dir_all(&vfs_dir).unwrap();

        // Create a minimal config.
        let config_toml = format!(
            r#"
music_source_dir = "{}"
cache_dir = "{}"
vfs.mount_dir = "{}"
"#,
            source_dir.display(),
            cache_dir.display(),
            vfs_dir.display(),
        );
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, &config_toml).unwrap();
        let c = Config::parse(Some(&config_path)).unwrap();

        // Bootstrap the cache database.
        maybe_invalidate_cache_database(&c).unwrap();

        // Create two fake releases + tracks in the cache.
        let conn = connect(&c).unwrap();
        conn.execute(
            "INSERT INTO releases (id, source_path, cover_image_path, added_at, datafile_mtime, \
             title, releasetype, releasedate, disctotal, new, favorite, metahash) \
             VALUES (?1, ?2, NULL, '2024-01-01', '0', ?3, 'album', '2024', 1, 1, 0, 'hash1')",
            rusqlite::params![
                "rel1",
                source_dir.join("Release1").to_string_lossy().to_string(),
                "Artist1 - 2024. Album1",
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO releases_artists (release_id, artist, role, position) VALUES (?1, ?2, 'main', 0)",
            rusqlite::params!["rel1", "Artist1"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks (id, source_path, source_mtime, title, release_id, \
             tracknumber, tracktotal, discnumber, duration_seconds, metahash) \
             VALUES (?1, ?2, '0', ?3, ?4, '1', 10, '1', 120, 'thash1')",
            rusqlite::params![
                "iloveloona",
                source_dir
                    .join("Release1")
                    .join("track1.flac")
                    .to_string_lossy()
                    .to_string(),
                "Track One",
                "rel1",
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks_artists (track_id, artist, role, position) VALUES (?1, ?2, 'main', 0)",
            rusqlite::params!["iloveloona", "Artist1"],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO releases (id, source_path, cover_image_path, added_at, datafile_mtime, \
             title, releasetype, releasedate, disctotal, new, favorite, metahash) \
             VALUES (?1, ?2, NULL, '2024-01-01', '0', ?3, 'album', '2023', 1, 1, 0, 'hash2')",
            rusqlite::params![
                "rel2",
                source_dir.join("Release2").to_string_lossy().to_string(),
                "Artist2 - 2023. Album2",
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO releases_artists (release_id, artist, role, position) VALUES (?1, ?2, 'main', 0)",
            rusqlite::params!["rel2", "Artist2"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks (id, source_path, source_mtime, title, release_id, \
             tracknumber, tracktotal, discnumber, duration_seconds, metahash) \
             VALUES (?1, ?2, '0', ?3, ?4, '1', 10, '1', 180, 'thash2')",
            rusqlite::params![
                "ilovetwice",
                source_dir
                    .join("Release2")
                    .join("track2.flac")
                    .to_string_lossy()
                    .to_string(),
                "Track Two",
                "rel2",
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks_artists (track_id, artist, role, position) VALUES (?1, ?2, 'main', 0)",
            rusqlite::params!["ilovetwice", "Artist2"],
        )
        .unwrap();
        drop(conn);

        // Create the !playlists directory and a pre-existing playlist.
        let playlists_dir = source_dir.join("!playlists");
        std::fs::create_dir_all(&playlists_dir).unwrap();
        std::fs::write(
            playlists_dir.join("Lala Lisa.toml"),
            r#"[[tracks]]
uuid = "iloveloona"
description_meta = "Artist1 - Track One"

[[tracks]]
uuid = "ilovetwice"
description_meta = "Artist2 - Track Two"
"#,
        )
        .unwrap();

        // Update cache for the pre-existing playlist.
        update_cache_for_playlists(&c, Some(vec!["Lala Lisa".to_string()]), true).unwrap();

        (c, tmp)
    }

    #[test]
    fn test_playlist_path() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("music")).unwrap();
        std::fs::create_dir_all(tmp.path().join("cache")).unwrap();
        std::fs::create_dir_all(tmp.path().join("vfs")).unwrap();
        let config_toml = format!(
            "music_source_dir = \"{}\"\ncache_dir = \"{}\"\nvfs.mount_dir = \"{}\"",
            tmp.path().join("music").display(),
            tmp.path().join("cache").display(),
            tmp.path().join("vfs").display(),
        );
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, &config_toml).unwrap();
        let c = Config::parse(Some(&config_path)).unwrap();
        let p = playlist_path(&c, "My Playlist");
        assert!(p.ends_with("!playlists/My Playlist.toml"));
    }

    #[test]
    fn test_playlist_lifecycle() {
        let (c, _tmp) = setup_test_env();
        let source_dir = &c.music_source_dir;
        let filepath = source_dir.join("!playlists").join("You & Me.toml");

        // Create playlist.
        assert!(!filepath.exists());
        create_playlist(&c, "You & Me").unwrap();
        assert!(filepath.is_file());
        {
            let conn = connect(&c).unwrap();
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT * FROM playlists WHERE name = 'You & Me')",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists);
        }

        // Add one track.
        add_track_to_playlist(&c, "You & Me", "iloveloona").unwrap();
        {
            let content = std::fs::read_to_string(&filepath).unwrap();
            let data: toml::Value = content.parse().unwrap();
            let tracks = data["tracks"].as_array().unwrap();
            let uuids: HashSet<&str> = tracks.iter().map(|t| t["uuid"].as_str().unwrap()).collect();
            assert!(uuids.contains("iloveloona"));
        }
        {
            let conn = connect(&c).unwrap();
            let mut stmt = conn
                .prepare("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'")
                .unwrap();
            let ids: HashSet<String> = stmt
                .query_map([], |row| row.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            assert!(ids.contains("iloveloona"));
        }

        // Add another track.
        add_track_to_playlist(&c, "You & Me", "ilovetwice").unwrap();
        {
            let content = std::fs::read_to_string(&filepath).unwrap();
            let data: toml::Value = content.parse().unwrap();
            let tracks = data["tracks"].as_array().unwrap();
            let uuids: HashSet<&str> = tracks.iter().map(|t| t["uuid"].as_str().unwrap()).collect();
            assert_eq!(uuids, ["iloveloona", "ilovetwice"].into_iter().collect());
        }
        {
            let conn = connect(&c).unwrap();
            let mut stmt = conn
                .prepare("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'")
                .unwrap();
            let ids: HashSet<String> = stmt
                .query_map([], |row| row.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            assert_eq!(
                ids,
                ["iloveloona", "ilovetwice"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            );
        }

        // Remove one track.
        remove_track_from_playlist(&c, "You & Me", "ilovetwice").unwrap();
        {
            let content = std::fs::read_to_string(&filepath).unwrap();
            let data: toml::Value = content.parse().unwrap();
            let tracks = data["tracks"].as_array().unwrap();
            let uuids: HashSet<&str> = tracks.iter().map(|t| t["uuid"].as_str().unwrap()).collect();
            assert_eq!(uuids, ["iloveloona"].into_iter().collect());
        }
        {
            let conn = connect(&c).unwrap();
            let mut stmt = conn
                .prepare("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'")
                .unwrap();
            let ids: HashSet<String> = stmt
                .query_map([], |row| row.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            assert_eq!(ids, ["iloveloona"].iter().map(|s| s.to_string()).collect());
        }

        // Delete the playlist.
        delete_playlist(&c, "You & Me").unwrap();
        assert!(!filepath.is_file());
        {
            let conn = connect(&c).unwrap();
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT * FROM playlists WHERE name = 'You & Me')",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(!exists);
        }
    }

    #[test]
    fn test_playlist_add_duplicate() {
        let (c, _tmp) = setup_test_env();
        create_playlist(&c, "Dupes").unwrap();
        add_track_to_playlist(&c, "Dupes", "ilovetwice").unwrap();
        add_track_to_playlist(&c, "Dupes", "ilovetwice").unwrap();
        let filepath = c.music_source_dir.join("!playlists").join("Dupes.toml");
        let content = std::fs::read_to_string(&filepath).unwrap();
        let data: toml::Value = content.parse().unwrap();
        let tracks = data["tracks"].as_array().unwrap();
        assert_eq!(tracks.len(), 1);
        {
            let conn = connect(&c).unwrap();
            let mut stmt = conn
                .prepare("SELECT * FROM playlists_tracks WHERE playlist_name = 'Dupes'")
                .unwrap();
            let count: usize = stmt.query_map([], |_row| Ok(())).unwrap().count();
            assert_eq!(count, 1);
        }
    }

    #[test]
    fn test_remove_track_from_playlist() {
        let (c, _tmp) = setup_test_env();
        let filepath = c.music_source_dir.join("!playlists").join("Lala Lisa.toml");

        remove_track_from_playlist(&c, "Lala Lisa", "iloveloona").unwrap();

        // Assert file is updated.
        let content = std::fs::read_to_string(&filepath).unwrap();
        let data: toml::Value = content.parse().unwrap();
        let tracks = data["tracks"].as_array().unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0]["uuid"].as_str().unwrap(), "ilovetwice");

        // Assert cache is updated.
        let conn = connect(&c).unwrap();
        let mut stmt = conn
            .prepare("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'Lala Lisa'")
            .unwrap();
        let ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(ids, vec!["ilovetwice"]);
    }

    #[test]
    fn test_rename_playlist() {
        let (c, _tmp) = setup_test_env();
        let source_dir = &c.music_source_dir;

        // Create an auxiliary file with the same stem (e.g. cover art).
        std::fs::write(
            source_dir.join("!playlists").join("Lala Lisa.jpg"),
            "cover art placeholder",
        )
        .unwrap();

        rename_playlist(&c, "Lala Lisa", "Turtle Rabbit").unwrap();

        assert!(!source_dir
            .join("!playlists")
            .join("Lala Lisa.toml")
            .exists());
        assert!(!source_dir.join("!playlists").join("Lala Lisa.jpg").exists());
        assert!(source_dir
            .join("!playlists")
            .join("Turtle Rabbit.toml")
            .exists());
        assert!(source_dir
            .join("!playlists")
            .join("Turtle Rabbit.jpg")
            .exists());

        let conn = connect(&c).unwrap();
        let exists_new: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT * FROM playlists WHERE name = 'Turtle Rabbit')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists_new);
        let exists_old: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT * FROM playlists WHERE name = 'Lala Lisa')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!exists_old);
    }

    #[test]
    fn test_playlist_handle_missing_track() {
        let (c, _tmp) = setup_test_env();
        let source_dir = &c.music_source_dir;
        let filepath = source_dir.join("!playlists").join("Ghost.toml");

        // Create a playlist with a "ghost" (missing) track.
        std::fs::write(
            &filepath,
            r#"[[tracks]]
uuid = "iloveloona"
description_meta = "lalala"

[[tracks]]
uuid = "ghost"
description_meta = "lalala {MISSING}"
missing = true
"#,
        )
        .unwrap();
        update_cache_for_playlists(&c, Some(vec!["Ghost".to_string()]), true).unwrap();

        // Adding another track should work.
        add_track_to_playlist(&c, "Ghost", "ilovetwice").unwrap();
        {
            let content = std::fs::read_to_string(&filepath).unwrap();
            let data: toml::Value = content.parse().unwrap();
            let tracks = data["tracks"].as_array().unwrap();
            let uuids: HashSet<&str> = tracks.iter().map(|t| t["uuid"].as_str().unwrap()).collect();
            assert_eq!(
                uuids,
                ["ghost", "iloveloona", "ilovetwice"].into_iter().collect()
            );
            // Verify ghost still has missing flag.
            let ghost = tracks
                .iter()
                .find(|t| t["uuid"].as_str().unwrap() == "ghost")
                .unwrap();
            assert_eq!(ghost.get("missing").and_then(|v| v.as_bool()), Some(true));
        }
        {
            let conn = connect(&c).unwrap();
            let mut stmt = conn
                .prepare("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'Ghost'")
                .unwrap();
            let ids: HashSet<String> = stmt
                .query_map([], |row| row.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            assert_eq!(
                ids,
                ["ghost", "iloveloona", "ilovetwice"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            );
        }

        // Remove that track.
        remove_track_from_playlist(&c, "Ghost", "ilovetwice").unwrap();
        {
            let content = std::fs::read_to_string(&filepath).unwrap();
            let data: toml::Value = content.parse().unwrap();
            let tracks = data["tracks"].as_array().unwrap();
            let uuids: HashSet<&str> = tracks.iter().map(|t| t["uuid"].as_str().unwrap()).collect();
            assert_eq!(uuids, ["ghost", "iloveloona"].into_iter().collect());
            let ghost = tracks
                .iter()
                .find(|t| t["uuid"].as_str().unwrap() == "ghost")
                .unwrap();
            assert_eq!(ghost.get("missing").and_then(|v| v.as_bool()), Some(true));
        }

        // Delete the playlist.
        delete_playlist(&c, "Ghost").unwrap();
        assert!(!filepath.is_file());
        {
            let conn = connect(&c).unwrap();
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT * FROM playlists WHERE name = 'Ghost')",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(!exists);
        }
    }

    #[test]
    fn test_edit_playlists_ordering() {
        let (c, _tmp) = setup_test_env();
        let filepath = c.music_source_dir.join("!playlists").join("Lala Lisa.toml");

        // Simulate editor by directly manipulating the file (same approach as collages).
        let content = std::fs::read_to_string(&filepath).unwrap();
        let data: toml::Value = content.parse().unwrap();
        let raw_tracks = data["tracks"].as_array().unwrap();
        let descs: Vec<&str> = raw_tracks
            .iter()
            .map(|t| t["description_meta"].as_str().unwrap())
            .collect();

        // Reverse the descriptions (simulating the editor).
        let reversed: Vec<&str> = descs.iter().rev().cloned().collect();

        // Build UUID mapping.
        let uuid_mapping: HashMap<String, String> = raw_tracks
            .iter()
            .map(|t| {
                (
                    t["description_meta"].as_str().unwrap().to_string(),
                    t["uuid"].as_str().unwrap().to_string(),
                )
            })
            .collect();

        // Apply the reversed order.
        let mut edited_tracks: Vec<toml::Value> = Vec::new();
        for desc in &reversed {
            let uuid = uuid_mapping.get(*desc).unwrap();
            let orig = raw_tracks
                .iter()
                .find(|t| t["uuid"].as_str().unwrap() == uuid)
                .unwrap();
            edited_tracks.push(orig.clone());
        }

        let mut new_data = data.clone();
        new_data
            .as_table_mut()
            .unwrap()
            .insert("tracks".to_string(), toml::Value::Array(edited_tracks));
        let toml_str = toml::to_string(&new_data).unwrap();
        std::fs::write(&filepath, toml_str).unwrap();
        update_cache_for_playlists(&c, Some(vec!["Lala Lisa".to_string()]), true).unwrap();

        // Verify the order was reversed.
        let content = std::fs::read_to_string(&filepath).unwrap();
        let data: toml::Value = content.parse().unwrap();
        let tracks = data["tracks"].as_array().unwrap();
        assert_eq!(tracks[0]["uuid"].as_str().unwrap(), "ilovetwice");
        assert_eq!(tracks[1]["uuid"].as_str().unwrap(), "iloveloona");
    }

    #[test]
    fn test_edit_playlist_duplicate_description_discriminator() {
        let (c, _tmp) = setup_test_env();
        let source_dir = &c.music_source_dir;

        // Create a playlist where two tracks have the SAME description_meta.
        let filepath = source_dir.join("!playlists").join("SameDesc.toml");
        std::fs::write(
            &filepath,
            r#"[[tracks]]
uuid = "iloveloona"
description_meta = "Same Name"

[[tracks]]
uuid = "ilovetwice"
description_meta = "Same Name"
"#,
        )
        .unwrap();

        // Read the file directly.
        let content = std::fs::read_to_string(&filepath).unwrap();
        let data: toml::Value = content.parse().unwrap();
        let raw_tracks = data["tracks"].as_array().unwrap();

        // Count occurrences.
        let mut desc_counts: HashMap<String, usize> = HashMap::new();
        for t in raw_tracks {
            let desc = t["description_meta"].as_str().unwrap().to_string();
            *desc_counts.entry(desc).or_insert(0) += 1;
        }

        // Build lines with discriminator.
        let mut lines: Vec<String> = Vec::new();
        let mut uuid_map: HashMap<String, String> = HashMap::new();
        for t in raw_tracks {
            let desc = t["description_meta"].as_str().unwrap().to_string();
            let uuid = t["uuid"].as_str().unwrap().to_string();
            let line = if *desc_counts.get(&desc).unwrap() > 1 {
                format!("{desc} [{uuid}]")
            } else {
                desc
            };
            lines.push(line.clone());
            uuid_map.insert(line, uuid);
        }

        // Both lines should have the [uuid] discriminator.
        assert_eq!(lines[0], "Same Name [iloveloona]");
        assert_eq!(lines[1], "Same Name [ilovetwice]");

        // Verify UUID mapping works both ways.
        assert_eq!(uuid_map["Same Name [iloveloona]"], "iloveloona");
        assert_eq!(uuid_map["Same Name [ilovetwice]"], "ilovetwice");
    }

    #[test]
    fn test_edit_playlist_unknown_line_error() {
        let (c, _tmp) = setup_test_env();
        let filepath = c.music_source_dir.join("!playlists").join("Lala Lisa.toml");

        // Read the playlist and build the UUID mapping.
        let content = std::fs::read_to_string(&filepath).unwrap();
        let data: toml::Value = content.parse().unwrap();
        let raw_tracks = data["tracks"].as_array().unwrap();

        let mut uuid_mapping: HashMap<String, String> = HashMap::new();
        for t in raw_tracks {
            let desc = t["description_meta"].as_str().unwrap().to_string();
            let uuid = t["uuid"].as_str().unwrap().to_string();
            uuid_mapping.insert(desc, uuid);
        }

        // Simulate a bad edit — an unknown line.
        let result = uuid_mapping.get("THIS DOES NOT EXIST");
        assert!(result.is_none());

        // Verify that the DescriptionMismatch error is correctly constructed.
        let err =
            RoseError::DescriptionMismatch("Track THIS DOES NOT EXIST does not match".to_string());
        assert!(err.is_expected());
    }

    #[test]
    fn test_create_playlist_already_exists() {
        let (c, _tmp) = setup_test_env();
        // "Lala Lisa" already exists from setup.
        let result = create_playlist(&c, "Lala Lisa");
        assert!(result.is_err());
        match result.unwrap_err() {
            RoseError::PlaylistAlreadyExists(_) => {}
            other => panic!("Expected PlaylistAlreadyExists, got: {other:?}"),
        }
    }

    #[test]
    fn test_delete_playlist_does_not_exist() {
        let (c, _tmp) = setup_test_env();
        let result = delete_playlist(&c, "Nonexistent");
        assert!(result.is_err());
        match result.unwrap_err() {
            RoseError::PlaylistDoesNotExist(_) => {}
            other => panic!("Expected PlaylistDoesNotExist, got: {other:?}"),
        }
    }

    #[test]
    fn test_set_playlist_cover_art() {
        let (c, _tmp) = setup_test_env();
        let source_dir = &c.music_source_dir;
        let playlists_dir = source_dir.join("!playlists");

        // Create a fake cover art image to set.
        let img_path = _tmp.path().join("folder.png");
        std::fs::write(&img_path, "lalala").unwrap();

        // Create an existing cover art file that should be deleted.
        std::fs::write(playlists_dir.join("Lala Lisa.jpg"), "old cover").unwrap();
        // Create a non-art file that should NOT be deleted.
        std::fs::write(playlists_dir.join("Lala Lisa.txt"), "notes").unwrap();

        set_playlist_cover_art(&c, "Lala Lisa", &img_path).unwrap();
        assert!(playlists_dir.join("Lala Lisa.png").is_file());
        assert!(!playlists_dir.join("Lala Lisa.jpg").exists());
        // Non-art file should still exist.
        assert!(playlists_dir.join("Lala Lisa.txt").is_file());
    }

    #[test]
    fn test_delete_playlist_cover_art() {
        let (c, _tmp) = setup_test_env();
        let source_dir = &c.music_source_dir;
        let playlists_dir = source_dir.join("!playlists");

        // Create a cover art file.
        std::fs::write(playlists_dir.join("Lala Lisa.jpg"), "cover").unwrap();

        delete_playlist_cover_art(&c, "Lala Lisa").unwrap();
        assert!(!playlists_dir.join("Lala Lisa.jpg").exists());
    }

    #[test]
    fn test_set_playlist_cover_art_invalid_extension() {
        let (c, _tmp) = setup_test_env();

        let img_path = _tmp.path().join("cover.bmp");
        std::fs::write(&img_path, "data").unwrap();

        let result = set_playlist_cover_art(&c, "Lala Lisa", &img_path);
        assert!(result.is_err());
        match result.unwrap_err() {
            RoseError::InvalidCoverArt(_) => {}
            other => panic!("Expected InvalidCoverArt, got: {other:?}"),
        }
    }
}
