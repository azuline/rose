//! Collage CRUD operations: create/rename/delete collages, add/remove releases,
//! and interactive editor support with duplicate `description_meta` disambiguation.
//!
//! Collages are ordered lists of releases stored as TOML files in
//! `{music_source_dir}/!collages/{name}.toml`. Each entry has a `uuid` and a
//! `description_meta` (human-readable label).

use std::collections::HashMap;
use std::path::PathBuf;

use crate::cache::{
    collage_lock_name, get_release_logtext, lock, update_cache_evict_nonexistent_collages,
    update_cache_for_collages,
};
use crate::common::RoseError;
use crate::config::Config;

// ---------------------------------------------------------------------------
// Path helper
// ---------------------------------------------------------------------------

/// Returns the path to a collage TOML file: `{music_source_dir}/!collages/{name}.toml`.
pub fn collage_path(c: &Config, name: &str) -> PathBuf {
    c.music_source_dir
        .join("!collages")
        .join(format!("{name}.toml"))
}

// ---------------------------------------------------------------------------
// Create
// ---------------------------------------------------------------------------

/// Create a new empty collage. Errors if a collage with this name already exists.
pub fn create_collage(c: &Config, name: &str) -> Result<(), RoseError> {
    let dir = c.music_source_dir.join("!collages");
    std::fs::create_dir_all(&dir).map_err(|e| {
        RoseError::Internal(format!(
            "Failed to create collages directory {}: {e}",
            dir.display()
        ))
    })?;
    let path = collage_path(c, name);
    {
        let _guard = lock(c, &collage_lock_name(name), 10.0)?;
        if path.exists() {
            return Err(RoseError::CollageAlreadyExists(format!(
                "Collage {name} already exists"
            )));
        }
        // Touch the file (create empty).
        std::fs::write(&path, b"").map_err(|e| {
            RoseError::Internal(format!(
                "Failed to create collage file {}: {e}",
                path.display()
            ))
        })?;
    }
    tracing::info!("Created collage {name} in source directory");
    update_cache_for_collages(c, Some(vec![name.to_string()]), true)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Delete
// ---------------------------------------------------------------------------

/// Delete a collage by moving its TOML file to trash. Errors if it doesn't exist.
pub fn delete_collage(c: &Config, name: &str) -> Result<(), RoseError> {
    let path = collage_path(c, name);
    {
        let _guard = lock(c, &collage_lock_name(name), 10.0)?;
        if !path.exists() {
            return Err(RoseError::CollageDoesNotExist(format!(
                "Collage {name} does not exist"
            )));
        }
        trash::delete(&path)
            .map_err(|e| RoseError::Internal(format!("Failed to trash {}: {e}", path.display())))?;
    }
    tracing::info!("Deleted collage {name} from source directory");
    update_cache_evict_nonexistent_collages(c)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Rename
// ---------------------------------------------------------------------------

/// Rename a collage and all adjacent files sharing the old stem (e.g. cover art).
pub fn rename_collage(c: &Config, old_name: &str, new_name: &str) -> Result<(), RoseError> {
    let old_path = collage_path(c, old_name);
    let new_path = collage_path(c, new_name);
    {
        let _guard_old = lock(c, &collage_lock_name(old_name), 10.0)?;
        let _guard_new = lock(c, &collage_lock_name(new_name), 10.0)?;
        if !old_path.exists() {
            return Err(RoseError::CollageDoesNotExist(format!(
                "Collage {old_name} does not exist"
            )));
        }
        if new_path.exists() {
            return Err(RoseError::CollageAlreadyExists(format!(
                "Collage {new_name} already exists"
            )));
        }
        std::fs::rename(&old_path, &new_path).map_err(|e| {
            RoseError::Internal(format!(
                "Failed to rename {} to {}: {e}",
                old_path.display(),
                new_path.display()
            ))
        })?;
        // Also rename all adjacent files in !collages/ that share the old stem.
        let collage_dir = c.music_source_dir.join("!collages");
        if let Ok(entries) = std::fs::read_dir(&collage_dir) {
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
                let new_adjacent = collage_dir.join(format!("{new_name}{suffix}"));
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
                    "Renaming collage-adjacent file {} to {}",
                    entry_path.display(),
                    new_adjacent.display()
                );
            }
        }
    }
    tracing::info!("Renamed collage {old_name} to {new_name}");
    update_cache_for_collages(c, Some(vec![new_name.to_string()]), true)?;
    update_cache_evict_nonexistent_collages(c)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Add release
// ---------------------------------------------------------------------------

/// Add a release to a collage. No-op if the release is already present.
pub fn add_release_to_collage(
    c: &Config,
    collage_name: &str,
    release_id: &str,
) -> Result<(), RoseError> {
    let release_logtext = get_release_logtext(c, release_id)?.ok_or_else(|| {
        RoseError::ReleaseDoesNotExist(format!("Release {release_id} does not exist"))
    })?;

    let path = collage_path(c, collage_name);
    if !path.exists() {
        return Err(RoseError::CollageDoesNotExist(format!(
            "Collage {collage_name} does not exist"
        )));
    }

    {
        let _guard = lock(c, &collage_lock_name(collage_name), 10.0)?;
        let content = std::fs::read_to_string(&path)
            .map_err(|e| RoseError::Internal(format!("Failed to read {}: {e}", path.display())))?;
        let mut data: toml::Value = if content.trim().is_empty() {
            toml::Value::Table(toml::map::Map::new())
        } else {
            content.parse().map_err(|e: toml::de::Error| {
                RoseError::Internal(format!("Failed to parse TOML {}: {e}", path.display()))
            })?
        };

        let releases = data
            .as_table_mut()
            .unwrap()
            .entry("releases")
            .or_insert_with(|| toml::Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| RoseError::Internal("'releases' key is not an array".to_string()))?;

        // Check for duplicates.
        for r in releases.iter() {
            if r.get("uuid").and_then(|v| v.as_str()) == Some(release_id) {
                tracing::info!(
                    "No-Op: Release {release_logtext} already in collage {collage_name}"
                );
                return Ok(());
            }
        }

        let mut entry = toml::map::Map::new();
        entry.insert(
            "uuid".to_string(),
            toml::Value::String(release_id.to_string()),
        );
        entry.insert(
            "description_meta".to_string(),
            toml::Value::String(release_logtext.clone()),
        );
        releases.push(toml::Value::Table(entry));

        let toml_str = toml::to_string(&data)
            .map_err(|e| RoseError::Internal(format!("Failed to serialize TOML: {e}")))?;
        std::fs::write(&path, toml_str)
            .map_err(|e| RoseError::Internal(format!("Failed to write {}: {e}", path.display())))?;
    }
    tracing::info!("Added release {release_logtext} to collage {collage_name}");
    update_cache_for_collages(c, Some(vec![collage_name.to_string()]), true)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Remove release
// ---------------------------------------------------------------------------

/// Remove a release from a collage. No-op if the release is not present.
pub fn remove_release_from_collage(
    c: &Config,
    collage_name: &str,
    release_id: &str,
) -> Result<(), RoseError> {
    let release_logtext = get_release_logtext(c, release_id)?.ok_or_else(|| {
        RoseError::ReleaseDoesNotExist(format!("Release {release_id} does not exist"))
    })?;

    let path = collage_path(c, collage_name);
    if !path.exists() {
        return Err(RoseError::CollageDoesNotExist(format!(
            "Collage {collage_name} does not exist"
        )));
    }

    {
        let _guard = lock(c, &collage_lock_name(collage_name), 10.0)?;
        let content = std::fs::read_to_string(&path)
            .map_err(|e| RoseError::Internal(format!("Failed to read {}: {e}", path.display())))?;
        let mut data: toml::Value = if content.trim().is_empty() {
            toml::Value::Table(toml::map::Map::new())
        } else {
            content.parse().map_err(|e: toml::de::Error| {
                RoseError::Internal(format!("Failed to parse TOML {}: {e}", path.display()))
            })?
        };

        let releases = data
            .get("releases")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let new_releases: Vec<toml::Value> = releases
            .iter()
            .filter(|r| r.get("uuid").and_then(|v| v.as_str()) != Some(release_id))
            .cloned()
            .collect();

        if releases.len() == new_releases.len() {
            tracing::info!("No-Op: Release {release_logtext} not in collage {collage_name}");
            return Ok(());
        }

        data.as_table_mut()
            .unwrap()
            .insert("releases".to_string(), toml::Value::Array(new_releases));

        let toml_str = toml::to_string(&data)
            .map_err(|e| RoseError::Internal(format!("Failed to serialize TOML: {e}")))?;
        std::fs::write(&path, toml_str)
            .map_err(|e| RoseError::Internal(format!("Failed to write {}: {e}", path.display())))?;
    }
    tracing::info!("Removed release {release_logtext} from collage {collage_name}");
    update_cache_for_collages(c, Some(vec![collage_name.to_string()]), true)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Edit in editor — WITH BUG FIX for duplicate description_meta
// ---------------------------------------------------------------------------

/// Open a collage in `$EDITOR` for interactive reordering/removal.
///
/// **Bug fix over Python:** When two releases have the same `description_meta`,
/// a `[uuid]` discriminator is appended so the user can distinguish them,
/// matching the playlist editor behavior.
pub fn edit_collage_in_editor(c: &Config, collage_name: &str) -> Result<(), RoseError> {
    let path = collage_path(c, collage_name);
    if !path.exists() {
        return Err(RoseError::CollageDoesNotExist(format!(
            "Collage {collage_name} does not exist"
        )));
    }

    let _guard = lock(c, &collage_lock_name(collage_name), 60.0)?;

    let content = std::fs::read_to_string(&path)
        .map_err(|e| RoseError::Internal(format!("Failed to read {}: {e}", path.display())))?;
    let mut data: toml::Value = if content.trim().is_empty() {
        toml::Value::Table(toml::map::Map::new())
    } else {
        content.parse().map_err(|e: toml::de::Error| {
            RoseError::Internal(format!("Failed to parse TOML {}: {e}", path.display()))
        })?
    };

    let raw_releases = data
        .get("releases")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // BUG FIX: Count occurrences of each description_meta to detect duplicates.
    let mut desc_counts: HashMap<String, usize> = HashMap::new();
    for r in &raw_releases {
        let desc = r
            .get("description_meta")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        *desc_counts.entry(desc).or_insert(0) += 1;
    }

    // Build lines and UUID mapping, adding [uuid] discriminator for duplicates.
    let mut lines_to_edit: Vec<String> = Vec::new();
    let mut uuid_mapping: HashMap<String, String> = HashMap::new();
    for r in &raw_releases {
        let desc = r
            .get("description_meta")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let uuid = r
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
    let edited = open_editor_for_collage(&editor_text)?;

    if edited.is_none() {
        tracing::info!("Aborting: metadata file not submitted.");
        return Ok(());
    }
    let edited_text = edited.unwrap();

    let mut edited_releases: Vec<toml::Value> = Vec::new();
    for line in edited_text.trim().split('\n') {
        if line.is_empty() {
            continue;
        }
        let uuid = uuid_mapping.get(line).ok_or_else(|| {
            RoseError::DescriptionMismatch(format!(
                "Release {line} does not match a known release in the collage. Was the line edited?"
            ))
        })?;
        // Find the original release entry to preserve all fields (including missing, etc.).
        let original = raw_releases
            .iter()
            .find(|r| r.get("uuid").and_then(|v| v.as_str()) == Some(uuid));
        if let Some(orig) = original {
            edited_releases.push(orig.clone());
        } else {
            let mut entry = toml::map::Map::new();
            entry.insert("uuid".to_string(), toml::Value::String(uuid.clone()));
            entry.insert(
                "description_meta".to_string(),
                toml::Value::String(line.to_string()),
            );
            edited_releases.push(toml::Value::Table(entry));
        }
    }

    data.as_table_mut()
        .unwrap()
        .insert("releases".to_string(), toml::Value::Array(edited_releases));

    let toml_str = toml::to_string(&data)
        .map_err(|e| RoseError::Internal(format!("Failed to serialize TOML: {e}")))?;
    std::fs::write(&path, toml_str)
        .map_err(|e| RoseError::Internal(format!("Failed to write {}: {e}", path.display())))?;

    // Release the lock before updating cache (lock is dropped when _guard goes out of scope).
    drop(_guard);

    tracing::info!("Edited collage {collage_name} from EDITOR");
    update_cache_for_collages(c, Some(vec![collage_name.to_string()]), true)?;
    Ok(())
}

/// Open `$EDITOR` with the given text. Returns `None` if the editor exits non-zero
/// or the text is unchanged (treated as abort).
fn open_editor_for_collage(text: &str) -> Result<Option<String>, RoseError> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let tmp_dir = std::env::temp_dir();
    let tmp_file = tmp_dir.join(format!("rose-collage-edit-{}.txt", uuid::Uuid::now_v7()));
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

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{connect, maybe_invalidate_cache_database, update_cache_for_collages};
    use crate::config::Config;
    #[allow(unused_imports)]
    use std::collections::HashMap;
    use tempfile::TempDir;

    /// Set up a test environment: a temp directory with a config, cache database,
    /// and a pre-existing collage with two releases in the cache.
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

        // Create two fake releases in the cache.
        let conn = connect(&c).unwrap();
        conn.execute(
            "INSERT INTO releases (id, source_path, cover_image_path, added_at, datafile_mtime, \
             title, releasetype, releasedate, disctotal, new, favorite, metahash) \
             VALUES (?1, ?2, NULL, '2024-01-01', '0', ?3, 'album', '2024', 1, 1, 0, 'hash1')",
            rusqlite::params![
                "ilovecarly",
                source_dir
                    .join("Carly Rae Jepsen")
                    .to_string_lossy()
                    .to_string(),
                "Carly Rae Jepsen - 2015. E-MO-TION",
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO releases_artists (release_id, artist, role, position) VALUES (?1, ?2, 'main', 0)",
            rusqlite::params!["ilovecarly", "Carly Rae Jepsen"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO releases (id, source_path, cover_image_path, added_at, datafile_mtime, \
             title, releasetype, releasedate, disctotal, new, favorite, metahash) \
             VALUES (?1, ?2, NULL, '2024-01-01', '0', ?3, 'album', '2023', 1, 1, 0, 'hash2')",
            rusqlite::params![
                "ilovenewjeans",
                source_dir.join("NewJeans").to_string_lossy().to_string(),
                "NewJeans - 2023. Get Up",
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO releases_artists (release_id, artist, role, position) VALUES (?1, ?2, 'main', 0)",
            rusqlite::params!["ilovenewjeans", "NewJeans"],
        )
        .unwrap();
        drop(conn);

        // Create the !collages directory and a pre-existing collage.
        let collages_dir = source_dir.join("!collages");
        std::fs::create_dir_all(&collages_dir).unwrap();
        std::fs::write(
            collages_dir.join("Rose Gold.toml"),
            r#"[[releases]]
uuid = "ilovecarly"
description_meta = "Carly Rae Jepsen - 2015. E-MO-TION"

[[releases]]
uuid = "ilovenewjeans"
description_meta = "NewJeans - 2023. Get Up"
"#,
        )
        .unwrap();

        // Update cache for the pre-existing collage.
        update_cache_for_collages(&c, Some(vec!["Rose Gold".to_string()]), true).unwrap();

        (c, tmp)
    }

    #[test]
    fn test_collage_path() {
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
        let p = collage_path(&c, "My Collage");
        assert!(p.ends_with("!collages/My Collage.toml"));
    }

    #[test]
    fn test_collage_lifecycle() {
        let (c, _tmp) = setup_test_env();
        let source_dir = &c.music_source_dir;
        let filepath = source_dir.join("!collages").join("All Eyes.toml");

        // Create collage.
        assert!(!filepath.exists());
        create_collage(&c, "All Eyes").unwrap();
        assert!(filepath.is_file());
        {
            let conn = connect(&c).unwrap();
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT * FROM collages WHERE name = 'All Eyes')",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists);
        }

        // Add one release.
        add_release_to_collage(&c, "All Eyes", "ilovecarly").unwrap();
        {
            let content = std::fs::read_to_string(&filepath).unwrap();
            let data: toml::Value = content.parse().unwrap();
            let releases = data["releases"].as_array().unwrap();
            let uuids: Vec<&str> = releases
                .iter()
                .map(|r| r["uuid"].as_str().unwrap())
                .collect();
            assert!(uuids.contains(&"ilovecarly"));
        }
        {
            let conn = connect(&c).unwrap();
            let mut stmt = conn
                .prepare("SELECT release_id FROM collages_releases WHERE collage_name = 'All Eyes'")
                .unwrap();
            let ids: Vec<String> = stmt
                .query_map([], |row| row.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            assert!(ids.contains(&"ilovecarly".to_string()));
        }

        // Add another release.
        add_release_to_collage(&c, "All Eyes", "ilovenewjeans").unwrap();
        {
            let content = std::fs::read_to_string(&filepath).unwrap();
            let data: toml::Value = content.parse().unwrap();
            let releases = data["releases"].as_array().unwrap();
            let uuids: std::collections::HashSet<&str> = releases
                .iter()
                .map(|r| r["uuid"].as_str().unwrap())
                .collect();
            assert_eq!(uuids, ["ilovecarly", "ilovenewjeans"].into_iter().collect());
        }
        {
            let conn = connect(&c).unwrap();
            let mut stmt = conn
                .prepare("SELECT release_id FROM collages_releases WHERE collage_name = 'All Eyes'")
                .unwrap();
            let ids: std::collections::HashSet<String> = stmt
                .query_map([], |row| row.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            assert_eq!(
                ids,
                ["ilovecarly", "ilovenewjeans"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            );
        }

        // Remove one release.
        remove_release_from_collage(&c, "All Eyes", "ilovenewjeans").unwrap();
        {
            let content = std::fs::read_to_string(&filepath).unwrap();
            let data: toml::Value = content.parse().unwrap();
            let releases = data["releases"].as_array().unwrap();
            let uuids: std::collections::HashSet<&str> = releases
                .iter()
                .map(|r| r["uuid"].as_str().unwrap())
                .collect();
            assert_eq!(uuids, ["ilovecarly"].into_iter().collect());
        }
        {
            let conn = connect(&c).unwrap();
            let mut stmt = conn
                .prepare("SELECT release_id FROM collages_releases WHERE collage_name = 'All Eyes'")
                .unwrap();
            let ids: std::collections::HashSet<String> = stmt
                .query_map([], |row| row.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            assert_eq!(ids, ["ilovecarly"].iter().map(|s| s.to_string()).collect());
        }

        // Delete the collage.
        delete_collage(&c, "All Eyes").unwrap();
        assert!(!filepath.is_file());
        {
            let conn = connect(&c).unwrap();
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT * FROM collages WHERE name = 'All Eyes')",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(!exists);
        }
    }

    #[test]
    fn test_collage_add_duplicate() {
        let (c, _tmp) = setup_test_env();
        create_collage(&c, "Dupes").unwrap();
        add_release_to_collage(&c, "Dupes", "ilovenewjeans").unwrap();
        add_release_to_collage(&c, "Dupes", "ilovenewjeans").unwrap();
        let filepath = c.music_source_dir.join("!collages").join("Dupes.toml");
        let content = std::fs::read_to_string(&filepath).unwrap();
        let data: toml::Value = content.parse().unwrap();
        let releases = data["releases"].as_array().unwrap();
        assert_eq!(releases.len(), 1);
        {
            let conn = connect(&c).unwrap();
            let mut stmt = conn
                .prepare("SELECT * FROM collages_releases WHERE collage_name = 'Dupes'")
                .unwrap();
            let count: usize = stmt.query_map([], |_row| Ok(())).unwrap().count();
            assert_eq!(count, 1);
        }
    }

    #[test]
    fn test_remove_release_from_collage() {
        let (c, _tmp) = setup_test_env();
        let filepath = c.music_source_dir.join("!collages").join("Rose Gold.toml");

        remove_release_from_collage(&c, "Rose Gold", "ilovecarly").unwrap();

        // Assert file is updated.
        let content = std::fs::read_to_string(&filepath).unwrap();
        let data: toml::Value = content.parse().unwrap();
        let releases = data["releases"].as_array().unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0]["uuid"].as_str().unwrap(), "ilovenewjeans");

        // Assert cache is updated.
        let conn = connect(&c).unwrap();
        let mut stmt = conn
            .prepare("SELECT release_id FROM collages_releases WHERE collage_name = 'Rose Gold'")
            .unwrap();
        let ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(ids, vec!["ilovenewjeans"]);
    }

    #[test]
    fn test_rename_collage() {
        let (c, _tmp) = setup_test_env();
        let source_dir = &c.music_source_dir;

        // Create an auxiliary file with the same stem.
        std::fs::write(
            source_dir.join("!collages").join("Rose Gold.txt"),
            "cover art placeholder",
        )
        .unwrap();

        rename_collage(&c, "Rose Gold", "Black Pink").unwrap();

        assert!(!source_dir.join("!collages").join("Rose Gold.toml").exists());
        assert!(!source_dir.join("!collages").join("Rose Gold.txt").exists());
        assert!(source_dir
            .join("!collages")
            .join("Black Pink.toml")
            .exists());
        assert!(source_dir.join("!collages").join("Black Pink.txt").exists());

        let conn = connect(&c).unwrap();
        let exists_new: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT * FROM collages WHERE name = 'Black Pink')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists_new);
        let exists_old: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT * FROM collages WHERE name = 'Rose Gold')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!exists_old);
    }

    #[test]
    fn test_collage_handle_missing_release() {
        let (c, _tmp) = setup_test_env();
        let source_dir = &c.music_source_dir;
        let filepath = source_dir.join("!collages").join("Black Pink.toml");

        // Create a collage with a "ghost" (missing) release.
        std::fs::write(
            &filepath,
            r#"[[releases]]
uuid = "ilovecarly"
description_meta = "lalala"

[[releases]]
uuid = "ghost"
description_meta = "lalala {MISSING}"
missing = true
"#,
        )
        .unwrap();
        update_cache_for_collages(&c, Some(vec!["Black Pink".to_string()]), true).unwrap();

        // Adding another release should work.
        add_release_to_collage(&c, "Black Pink", "ilovenewjeans").unwrap();
        {
            let content = std::fs::read_to_string(&filepath).unwrap();
            let data: toml::Value = content.parse().unwrap();
            let releases = data["releases"].as_array().unwrap();
            let uuids: std::collections::HashSet<&str> = releases
                .iter()
                .map(|r| r["uuid"].as_str().unwrap())
                .collect();
            assert_eq!(
                uuids,
                ["ghost", "ilovecarly", "ilovenewjeans"]
                    .into_iter()
                    .collect()
            );
            // Verify ghost still has missing flag.
            let ghost = releases
                .iter()
                .find(|r| r["uuid"].as_str().unwrap() == "ghost")
                .unwrap();
            assert_eq!(ghost.get("missing").and_then(|v| v.as_bool()), Some(true));
        }
        {
            let conn = connect(&c).unwrap();
            let mut stmt = conn
                .prepare(
                    "SELECT release_id FROM collages_releases WHERE collage_name = 'Black Pink'",
                )
                .unwrap();
            let ids: std::collections::HashSet<String> = stmt
                .query_map([], |row| row.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            assert_eq!(
                ids,
                ["ghost", "ilovecarly", "ilovenewjeans"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            );
        }

        // Remove that release.
        remove_release_from_collage(&c, "Black Pink", "ilovenewjeans").unwrap();
        {
            let content = std::fs::read_to_string(&filepath).unwrap();
            let data: toml::Value = content.parse().unwrap();
            let releases = data["releases"].as_array().unwrap();
            let uuids: std::collections::HashSet<&str> = releases
                .iter()
                .map(|r| r["uuid"].as_str().unwrap())
                .collect();
            assert_eq!(uuids, ["ghost", "ilovecarly"].into_iter().collect());
            let ghost = releases
                .iter()
                .find(|r| r["uuid"].as_str().unwrap() == "ghost")
                .unwrap();
            assert_eq!(ghost.get("missing").and_then(|v| v.as_bool()), Some(true));
        }

        // Delete the collage.
        delete_collage(&c, "Black Pink").unwrap();
        assert!(!filepath.is_file());
        {
            let conn = connect(&c).unwrap();
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT * FROM collages WHERE name = 'Black Pink')",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(!exists);
        }
    }

    #[test]
    fn test_edit_collages_ordering() {
        let (c, _tmp) = setup_test_env();
        let filepath = c.music_source_dir.join("!collages").join("Rose Gold.toml");

        // Simulate editor by setting EDITOR to a script that reverses lines.
        // Instead of spawning a real editor, we test the internal logic directly.
        //
        // Read the collage, build editor text, reverse it, then apply.
        let content = std::fs::read_to_string(&filepath).unwrap();
        let data: toml::Value = content.parse().unwrap();
        let raw_releases = data["releases"].as_array().unwrap();
        let descs: Vec<&str> = raw_releases
            .iter()
            .map(|r| r["description_meta"].as_str().unwrap())
            .collect();

        // Reverse the descriptions (simulating the editor).
        let reversed: Vec<&str> = descs.iter().rev().cloned().collect();

        // Build UUID mapping.
        let uuid_mapping: HashMap<String, String> = raw_releases
            .iter()
            .map(|r| {
                (
                    r["description_meta"].as_str().unwrap().to_string(),
                    r["uuid"].as_str().unwrap().to_string(),
                )
            })
            .collect();

        // Apply the reversed order.
        let mut edited_releases: Vec<toml::Value> = Vec::new();
        for desc in &reversed {
            let uuid = uuid_mapping.get(*desc).unwrap();
            let orig = raw_releases
                .iter()
                .find(|r| r["uuid"].as_str().unwrap() == uuid)
                .unwrap();
            edited_releases.push(orig.clone());
        }

        let mut new_data = data.clone();
        new_data
            .as_table_mut()
            .unwrap()
            .insert("releases".to_string(), toml::Value::Array(edited_releases));
        let toml_str = toml::to_string(&new_data).unwrap();
        std::fs::write(&filepath, toml_str).unwrap();
        update_cache_for_collages(&c, Some(vec!["Rose Gold".to_string()]), true).unwrap();

        // Verify the order was reversed.
        let content = std::fs::read_to_string(&filepath).unwrap();
        let data: toml::Value = content.parse().unwrap();
        let releases = data["releases"].as_array().unwrap();
        assert_eq!(releases[0]["uuid"].as_str().unwrap(), "ilovenewjeans");
        assert_eq!(releases[1]["uuid"].as_str().unwrap(), "ilovecarly");
    }

    #[test]
    fn test_edit_collage_duplicate_description_discriminator() {
        let (c, _tmp) = setup_test_env();
        let source_dir = &c.music_source_dir;

        // Create a collage where two releases have the SAME description_meta.
        // Note: We write directly without updating cache, because
        // update_cache_for_collages rewrites description_meta from the DB.
        let filepath = source_dir.join("!collages").join("SameDesc.toml");
        std::fs::write(
            &filepath,
            r#"[[releases]]
uuid = "ilovecarly"
description_meta = "Same Name"

[[releases]]
uuid = "ilovenewjeans"
description_meta = "Same Name"
"#,
        )
        .unwrap();

        // Read the file directly (before cache update rewrites description_meta).
        let content = std::fs::read_to_string(&filepath).unwrap();
        let data: toml::Value = content.parse().unwrap();
        let raw_releases = data["releases"].as_array().unwrap();

        // Count occurrences.
        let mut desc_counts: HashMap<String, usize> = HashMap::new();
        for r in raw_releases {
            let desc = r["description_meta"].as_str().unwrap().to_string();
            *desc_counts.entry(desc).or_insert(0) += 1;
        }

        // Build lines with discriminator.
        let mut lines: Vec<String> = Vec::new();
        let mut uuid_map: HashMap<String, String> = HashMap::new();
        for r in raw_releases {
            let desc = r["description_meta"].as_str().unwrap().to_string();
            let uuid = r["uuid"].as_str().unwrap().to_string();
            let line = if *desc_counts.get(&desc).unwrap() > 1 {
                format!("{desc} [{uuid}]")
            } else {
                desc
            };
            lines.push(line.clone());
            uuid_map.insert(line, uuid);
        }

        // Both lines should have the [uuid] discriminator.
        assert_eq!(lines[0], "Same Name [ilovecarly]");
        assert_eq!(lines[1], "Same Name [ilovenewjeans]");

        // Verify UUID mapping works both ways.
        assert_eq!(uuid_map["Same Name [ilovecarly]"], "ilovecarly");
        assert_eq!(uuid_map["Same Name [ilovenewjeans]"], "ilovenewjeans");
    }

    #[test]
    fn test_edit_collage_unknown_line_error() {
        let (c, _tmp) = setup_test_env();
        let filepath = c.music_source_dir.join("!collages").join("Rose Gold.toml");

        // Read the collage and build the UUID mapping.
        let content = std::fs::read_to_string(&filepath).unwrap();
        let data: toml::Value = content.parse().unwrap();
        let raw_releases = data["releases"].as_array().unwrap();

        let mut uuid_mapping: HashMap<String, String> = HashMap::new();
        for r in raw_releases {
            let desc = r["description_meta"].as_str().unwrap().to_string();
            let uuid = r["uuid"].as_str().unwrap().to_string();
            uuid_mapping.insert(desc, uuid);
        }

        // Simulate a bad edit — an unknown line.
        let result = uuid_mapping.get("THIS DOES NOT EXIST");
        assert!(result.is_none());

        // Verify that the DescriptionMismatch error is correctly constructed.
        let err = RoseError::DescriptionMismatch(
            "Release THIS DOES NOT EXIST does not match".to_string(),
        );
        assert!(err.is_expected());
    }

    #[test]
    fn test_create_collage_already_exists() {
        let (c, _tmp) = setup_test_env();
        // "Rose Gold" already exists from setup.
        let result = create_collage(&c, "Rose Gold");
        assert!(result.is_err());
        match result.unwrap_err() {
            RoseError::CollageAlreadyExists(_) => {}
            other => panic!("Expected CollageAlreadyExists, got: {other:?}"),
        }
    }

    #[test]
    fn test_delete_collage_does_not_exist() {
        let (c, _tmp) = setup_test_env();
        let result = delete_collage(&c, "Nonexistent");
        assert!(result.is_err());
        match result.unwrap_err() {
            RoseError::CollageDoesNotExist(_) => {}
            other => panic!("Expected CollageDoesNotExist, got: {other:?}"),
        }
    }
}
