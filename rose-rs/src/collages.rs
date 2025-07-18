// """
// The collages module provides functions for interacting with collages.
// """
//
// import logging
// import tomllib
// from pathlib import Path
// from typing import Any
//
// import click
// import tomli_w
// from send2trash import send2trash
//
// from rose.cache import (
//     collage_lock_name,
//     get_release_logtext,
//     lock,
//     update_cache_evict_nonexistent_collages,
//     update_cache_for_collages,
// )
// from rose.common import RoseExpectedError
// from rose.config import Config
// from rose.releases import ReleaseDoesNotExistError
//
// logger = logging.getLogger(__name__)

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info};

use crate::cache::{collage_lock_name, get_release_logtext, lock, update_cache_evict_nonexistent_collages, update_cache_for_collages};
use crate::config::Config;
use crate::errors::{Result, RoseError, RoseExpectedError};
use crate::releases::ReleaseDoesNotExistError;

// class DescriptionMismatchError(RoseExpectedError):
//     pass
#[derive(Error, Debug)]
#[error("Release {0} does not match a known release in the collage. Was the line edited?")]
pub struct DescriptionMismatchError(pub String);

impl From<DescriptionMismatchError> for RoseExpectedError {
    fn from(err: DescriptionMismatchError) -> Self {
        RoseExpectedError::Generic(err.to_string())
    }
}

impl From<DescriptionMismatchError> for RoseError {
    fn from(err: DescriptionMismatchError) -> Self {
        RoseError::Expected(err.into())
    }
}

// class CollageDoesNotExistError(RoseExpectedError):
//     pass
#[derive(Error, Debug)]
#[error("Collage {0} does not exist")]
pub struct CollageDoesNotExistError(pub String);

impl From<CollageDoesNotExistError> for RoseExpectedError {
    fn from(err: CollageDoesNotExistError) -> Self {
        RoseExpectedError::CollageDoesNotExist { name: err.0 }
    }
}

impl From<CollageDoesNotExistError> for RoseError {
    fn from(err: CollageDoesNotExistError) -> Self {
        RoseError::Expected(err.into())
    }
}

// class CollageAlreadyExistsError(RoseExpectedError):
//     pass
#[derive(Error, Debug)]
#[error("Collage {0} already exists")]
pub struct CollageAlreadyExistsError(pub String);

impl From<CollageAlreadyExistsError> for RoseExpectedError {
    fn from(err: CollageAlreadyExistsError) -> Self {
        RoseExpectedError::Generic(err.to_string())
    }
}

impl From<CollageAlreadyExistsError> for RoseError {
    fn from(err: CollageAlreadyExistsError) -> Self {
        RoseError::Expected(err.into())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct CollageRelease {
    uuid: String,
    description_meta: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    missing: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct CollageData {
    #[serde(default)]
    releases: Vec<CollageRelease>,
}

// def create_collage(c: Config, name: str) -> None:
//     (c.music_source_dir / "!collages").mkdir(parents=True, exist_ok=True)
//     path = collage_path(c, name)
//     with lock(c, collage_lock_name(name)):
//         if path.exists():
//             raise CollageAlreadyExistsError(f"Collage {name} already exists")
//         path.touch()
//     logger.info(f"Created collage {name} in source directory")
//     update_cache_for_collages(c, [name], force=True)
pub fn create_collage(c: &Config, name: &str) -> Result<()> {
    let collages_dir = c.music_source_dir.join("!collages");
    fs::create_dir_all(&collages_dir)?;

    let path = collage_path(c, name);
    let _lock = lock(c, &collage_lock_name(name), 5.0)?;

    if path.exists() {
        return Err(CollageAlreadyExistsError(name.to_string()).into());
    }

    fs::File::create(&path)?;

    info!("created collage {} in source directory", name);
    update_cache_for_collages(c, Some(vec![name.to_string()]), true)?;

    Ok(())
}

// def delete_collage(c: Config, name: str) -> None:
//     path = collage_path(c, name)
//     with lock(c, collage_lock_name(name)):
//         if not path.exists():
//             raise CollageDoesNotExistError(f"Collage {name} does not exist")
//         send2trash(path)
//     logger.info(f"Deleted collage {name} from source directory")
//     update_cache_evict_nonexistent_collages(c)
pub fn delete_collage(c: &Config, name: &str) -> Result<()> {
    let path = collage_path(c, name);
    let _lock = lock(c, &collage_lock_name(name), 5.0)?;

    if !path.exists() {
        return Err(CollageDoesNotExistError(name.to_string()).into());
    }

    // Use trash crate to move to trash
    trash::delete(&path).map_err(|e| RoseError::Io(std::io::Error::other(e.to_string())))?;

    info!("deleted collage {} from source directory", name);
    update_cache_evict_nonexistent_collages(c)?;

    Ok(())
}

// def rename_collage(c: Config, old_name: str, new_name: str) -> None:
//     old_path = collage_path(c, old_name)
//     new_path = collage_path(c, new_name)
//     with lock(c, collage_lock_name(old_name)), lock(c, collage_lock_name(new_name)):
//         if not old_path.exists():
//             raise CollageDoesNotExistError(f"Collage {old_name} does not exist")
//         if new_path.exists():
//             raise CollageAlreadyExistsError(f"Collage {new_name} already exists")
//         old_path.rename(new_path)
//         # And also rename all files with the same stem (e.g. cover arts).
//         for old_adjacent_file in (c.music_source_dir / "!collages").iterdir():
//             if old_adjacent_file.stem != old_path.stem:
//                 continue
//             new_adjacent_file = old_adjacent_file.with_name(new_path.stem + old_adjacent_file.suffix)
//             if new_adjacent_file.exists():
//                 continue
//             old_adjacent_file.rename(new_adjacent_file)
//             logger.debug("Renaming collage-adjacent file {old_adjacent_file} to {new_adjacent_file}")
//     logger.info(f"Renamed collage {old_name} to {new_name}")
//     update_cache_for_collages(c, [new_name], force=True)
//     update_cache_evict_nonexistent_collages(c)
pub fn rename_collage(c: &Config, old_name: &str, new_name: &str) -> Result<()> {
    let old_path = collage_path(c, old_name);
    let new_path = collage_path(c, new_name);

    let _lock1 = lock(c, &collage_lock_name(old_name), 5.0)?;
    let _lock2 = lock(c, &collage_lock_name(new_name), 5.0)?;

    if !old_path.exists() {
        return Err(CollageDoesNotExistError(old_name.to_string()).into());
    }

    if new_path.exists() {
        return Err(CollageAlreadyExistsError(new_name.to_string()).into());
    }

    fs::rename(&old_path, &new_path)?;

    // And also rename all files with the same stem (e.g. cover arts).
    let collages_dir = c.music_source_dir.join("!collages");
    let old_stem = old_path.file_stem().unwrap();
    let new_stem = new_path.file_stem().unwrap();

    for entry in fs::read_dir(&collages_dir)? {
        let entry = entry?;
        let old_adjacent_file = entry.path();

        if old_adjacent_file.file_stem() != Some(old_stem) {
            continue;
        }

        let extension = old_adjacent_file.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        let new_adjacent_file = collages_dir.join(format!("{}{}", new_stem.to_string_lossy(), extension));

        if new_adjacent_file.exists() {
            continue;
        }

        fs::rename(&old_adjacent_file, &new_adjacent_file)?;
        debug!("renaming collage-adjacent file {:?} to {:?}", old_adjacent_file, new_adjacent_file);
    }

    info!("renamed collage {} to {}", old_name, new_name);
    update_cache_for_collages(c, Some(vec![new_name.to_string()]), true)?;
    update_cache_evict_nonexistent_collages(c)?;

    Ok(())
}

// def remove_release_from_collage(c: Config, collage_name: str, release_id: str) -> None:
//     release_logtext = get_release_logtext(c, release_id)
//     if not release_logtext:
//         raise ReleaseDoesNotExistError(f"Release {release_id} does not exist")
//
//     path = collage_path(c, collage_name)
//     if not path.exists():
//         raise CollageDoesNotExistError(f"Collage {collage_name} does not exist")
//     with lock(c, collage_lock_name(collage_name)):
//         with path.open("rb") as fp:
//             data = tomllib.load(fp)
//         old_releases = data.get("releases", [])
//         releases_new = [r for r in old_releases if r["uuid"] != release_id]
//         if old_releases == releases_new:
//             logger.info(f"No-Op: Release {release_logtext} not in collage {collage_name}")
//             return
//         data["releases"] = releases_new
//         with path.open("wb") as fp:
//             tomli_w.dump(data, fp)
//     logger.info(f"Removed release {release_logtext} from collage {collage_name}")
//     update_cache_for_collages(c, [collage_name], force=True)
pub fn remove_release_from_collage(c: &Config, collage_name: &str, release_id: &str) -> Result<()> {
    let release_logtext = match get_release_logtext(c, release_id) {
        Ok(text) => text,
        Err(_) => return Err(ReleaseDoesNotExistError(format!("Release {} does not exist", release_id)).into()),
    };

    let path = collage_path(c, collage_name);
    if !path.exists() {
        return Err(CollageDoesNotExistError(collage_name.to_string()).into());
    }

    let _lock = lock(c, &collage_lock_name(collage_name), 5.0)?;

    let contents = fs::read_to_string(&path)?;
    let mut data: CollageData = toml::from_str(&contents)?;

    let old_len = data.releases.len();
    data.releases.retain(|r| r.uuid != release_id);

    if old_len == data.releases.len() {
        info!("no-op: release {} not in collage {}", release_logtext, collage_name);
        return Ok(());
    }

    let toml_string = toml::to_string_pretty(&data)?;
    fs::write(&path, toml_string)?;

    info!("removed release {} from collage {}", release_logtext, collage_name);
    update_cache_for_collages(c, Some(vec![collage_name.to_string()]), true)?;

    Ok(())
}

// def add_release_to_collage(
//     c: Config,
//     collage_name: str,
//     release_id: str,
// ) -> None:
//     release_logtext = get_release_logtext(c, release_id)
//     if not release_logtext:
//         raise ReleaseDoesNotExistError(f"Release {release_id} does not exist")
//
//     path = collage_path(c, collage_name)
//     if not path.exists():
//         raise CollageDoesNotExistError(f"Collage {collage_name} does not exist")
//
//     with lock(c, collage_lock_name(collage_name)):
//         with path.open("rb") as fp:
//             data = tomllib.load(fp)
//         data["releases"] = data.get("releases", [])
//         # Check to see if release is already in the collage. If so, no op. We don't support
//         # duplicate collage entries.
//         for r in data["releases"]:
//             if r["uuid"] == release_id:
//                 logger.info(f"No-Op: Release {release_logtext} already in collage {collage_name}")
//                 return
//         data["releases"].append({"uuid": release_id, "description_meta": release_logtext})
//         with path.open("wb") as fp:
//             tomli_w.dump(data, fp)
//     logger.info(f"Added release {release_logtext} to collage {collage_name}")
//     update_cache_for_collages(c, [collage_name], force=True)
pub fn add_release_to_collage(c: &Config, collage_name: &str, release_id: &str) -> Result<()> {
    let release_logtext = match get_release_logtext(c, release_id) {
        Ok(text) => text,
        Err(_) => return Err(ReleaseDoesNotExistError(format!("Release {} does not exist", release_id)).into()),
    };

    let path = collage_path(c, collage_name);
    if !path.exists() {
        return Err(CollageDoesNotExistError(collage_name.to_string()).into());
    }

    let _lock = lock(c, &collage_lock_name(collage_name), 5.0)?;

    let contents = fs::read_to_string(&path)?;
    let mut data: CollageData = if contents.is_empty() {
        CollageData::default()
    } else {
        toml::from_str(&contents)?
    };

    // Check to see if release is already in the collage. If so, no op. We don't support
    // duplicate collage entries.
    for r in &data.releases {
        if r.uuid == release_id {
            info!("no-op: release {} already in collage {}", release_logtext, collage_name);
            return Ok(());
        }
    }

    data.releases.push(CollageRelease {
        uuid: release_id.to_string(),
        description_meta: release_logtext.clone(),
        missing: None,
    });

    let toml_string = toml::to_string_pretty(&data)?;
    fs::write(&path, toml_string)?;

    info!("added release {} to collage {}", release_logtext, collage_name);
    update_cache_for_collages(c, Some(vec![collage_name.to_string()]), true)?;

    Ok(())
}

// def edit_collage_in_editor(c: Config, collage_name: str) -> None:
//     path = collage_path(c, collage_name)
//     if not path.exists():
//         raise CollageDoesNotExistError(f"Collage {collage_name} does not exist")
//     with lock(c, collage_lock_name(collage_name), timeout=60.0):
//         with path.open("rb") as fp:
//             data = tomllib.load(fp)
//         raw_releases = data.get("releases", [])
//         edited_release_descriptions = click.edit("\n".join([r["description_meta"] for r in raw_releases]))
//         if edited_release_descriptions is None:
//             logger.info("Aborting: metadata file not submitted.")
//             return
//         uuid_mapping = {r["description_meta"]: r["uuid"] for r in raw_releases}
//
//         edited_releases: list[dict[str, Any]] = []
//         for desc in edited_release_descriptions.strip().split("\n"):
//             try:
//                 uuid = uuid_mapping[desc]
//             except KeyError as e:
//                 raise DescriptionMismatchError(
//                     f"Release {desc} does not match a known release in the collage. " "Was the line edited?"
//                 ) from e
//             edited_releases.append({"uuid": uuid, "description_meta": desc})
//         data["releases"] = edited_releases
//
//         with path.open("wb") as fp:
//             tomli_w.dump(data, fp)
//     logger.info(f"Edited collage {collage_name} from EDITOR")
//     update_cache_for_collages(c, [collage_name], force=True)
pub fn edit_collage_in_editor(c: &Config, collage_name: &str, editor_fn: impl FnOnce(&str) -> Option<String>) -> Result<()> {
    let path = collage_path(c, collage_name);
    if !path.exists() {
        return Err(CollageDoesNotExistError(collage_name.to_string()).into());
    }

    let _lock = lock(c, &collage_lock_name(collage_name), 60.0)?;

    let contents = fs::read_to_string(&path)?;
    let mut data: CollageData = toml::from_str(&contents)?;

    let raw_releases = &data.releases;
    let release_descriptions: Vec<String> = raw_releases.iter().map(|r| r.description_meta.clone()).collect();
    let editor_content = release_descriptions.join("\n");

    let edited_content = editor_fn(&editor_content);
    if edited_content.is_none() {
        info!("aborting: metadata file not submitted.");
        return Ok(());
    }

    let edited_release_descriptions = edited_content.unwrap();
    let uuid_mapping: HashMap<&str, &str> = raw_releases.iter().map(|r| (r.description_meta.as_str(), r.uuid.as_str())).collect();

    let mut edited_releases = Vec::new();
    for desc in edited_release_descriptions.trim().split('\n') {
        if desc.is_empty() {
            continue;
        }

        let uuid = uuid_mapping.get(desc).ok_or_else(|| DescriptionMismatchError(desc.to_string()))?;

        // Preserve the missing field if it exists
        let missing = raw_releases.iter().find(|r| r.uuid == *uuid).and_then(|r| r.missing);

        edited_releases.push(CollageRelease {
            uuid: uuid.to_string(),
            description_meta: desc.to_string(),
            missing,
        });
    }

    data.releases = edited_releases;

    let toml_string = toml::to_string_pretty(&data)?;
    fs::write(&path, toml_string)?;

    info!("edited collage {} from editor", collage_name);
    update_cache_for_collages(c, Some(vec![collage_name.to_string()]), true)?;

    Ok(())
}

// def collage_path(c: Config, name: str) -> Path:
//     return c.music_source_dir / "!collages" / f"{name}.toml"
pub fn collage_path(c: &Config, name: &str) -> PathBuf {
    c.music_source_dir.join("!collages").join(format!("{}.toml", name))
}

// TESTS
//
// import tomllib
// from pathlib import Path
// from typing import Any
//
// from rose.cache import connect, update_cache
// from rose.collages import (
//     add_release_to_collage,
//     create_collage,
//     delete_collage,
//     edit_collage_in_editor,
//     remove_release_from_collage,
//     rename_collage,
// )
// from rose.config import Config

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{connect, update_cache};
    use crate::testing;
    use std::collections::HashSet;
    use std::fs;

    // def test_remove_release_from_collage(config: Config, source_dir: Path) -> None:
    //     remove_release_from_collage(config, "Rose Gold", "ilovecarly")
    //
    //     # Assert file is updated.
    //     with (source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
    //         diskdata = tomllib.load(fp)
    //     assert len(diskdata["releases"]) == 1
    //     assert diskdata["releases"][0]["uuid"] == "ilovenewjeans"
    //
    //     # Assert cache is updated.
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT release_id FROM collages_releases WHERE collage_name = 'Rose Gold'")
    //         ids = [r["release_id"] for r in cursor]
    //         assert ids == ["ilovenewjeans"]
    #[test]
    fn test_remove_release_from_collage() {
        let (config, _temp_dir) = testing::source_dir();
        let source_dir = &config.music_source_dir;

        remove_release_from_collage(&config, "Rose Gold", "ilovecarly").unwrap();

        // Assert file is updated.
        let contents = fs::read_to_string(source_dir.join("!collages").join("Rose Gold.toml")).unwrap();
        let diskdata: CollageData = toml::from_str(&contents).unwrap();
        assert_eq!(diskdata.releases.len(), 1);
        assert_eq!(diskdata.releases[0].uuid, "ilovenewjeans");

        // Assert cache is updated.
        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT release_id FROM collages_releases WHERE collage_name = 'Rose Gold'").unwrap();
        let ids: Vec<String> = stmt.query_map([], |row| row.get(0)).unwrap().map(|r| r.unwrap()).collect();
        assert_eq!(ids, vec!["ilovenewjeans"]);
    }

    // def test_collage_lifecycle(config: Config, source_dir: Path) -> None:
    //     filepath = source_dir / "!collages" / "All Eyes.toml"
    //
    //     # Create collage.
    //     assert not filepath.exists()
    //     create_collage(config, "All Eyes")
    //     assert filepath.is_file()
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT EXISTS(SELECT * FROM collages WHERE name = 'All Eyes')")
    //         assert cursor.fetchone()[0]
    //
    //     # Add one release.
    //     add_release_to_collage(config, "All Eyes", "ilovecarly")
    //     with filepath.open("rb") as fp:
    //         diskdata = tomllib.load(fp)
    //         assert {r["uuid"] for r in diskdata["releases"]} == {"ilovecarly"}
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT release_id FROM collages_releases WHERE collage_name = 'All Eyes'")
    //         assert {r["release_id"] for r in cursor} == {"ilovecarly"}
    //
    //     # Add another release.
    //     add_release_to_collage(config, "All Eyes", "ilovenewjeans")
    //     with (source_dir / "!collages" / "All Eyes.toml").open("rb") as fp:
    //         diskdata = tomllib.load(fp)
    //         assert {r["uuid"] for r in diskdata["releases"]} == {"ilovecarly", "ilovenewjeans"}
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT release_id FROM collages_releases WHERE collage_name = 'All Eyes'")
    //         assert {r["release_id"] for r in cursor} == {"ilovecarly", "ilovenewjeans"}
    //
    //     # Delete one release.
    //     remove_release_from_collage(config, "All Eyes", "ilovenewjeans")
    //     with filepath.open("rb") as fp:
    //         diskdata = tomllib.load(fp)
    //         assert {r["uuid"] for r in diskdata["releases"]} == {"ilovecarly"}
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT release_id FROM collages_releases WHERE collage_name = 'All Eyes'")
    //         assert {r["release_id"] for r in cursor} == {"ilovecarly"}
    //
    //     # And delete the collage.
    //     delete_collage(config, "All Eyes")
    //     assert not filepath.is_file()
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT EXISTS(SELECT * FROM collages WHERE name = 'All Eyes')")
    //         assert not cursor.fetchone()[0]
    #[test]
    fn test_collage_lifecycle() {
        let (config, _temp_dir) = testing::seeded_cache();
        let source_dir = &config.music_source_dir;
        let filepath = source_dir.join("!collages").join("All Eyes.toml");

        // Create collage.
        assert!(!filepath.exists());
        create_collage(&config, "All Eyes").unwrap();
        assert!(filepath.is_file());

        let conn = connect(&config).unwrap();
        let exists: bool = conn.query_row("SELECT EXISTS(SELECT * FROM collages WHERE name = 'All Eyes')", [], |row| row.get(0)).unwrap();
        assert!(exists);

        // Add one release.
        add_release_to_collage(&config, "All Eyes", "ilovecarly").unwrap();
        let contents = fs::read_to_string(&filepath).unwrap();
        let diskdata: CollageData = toml::from_str(&contents).unwrap();
        let uuids: std::collections::HashSet<String> = diskdata.releases.iter().map(|r| r.uuid.clone()).collect();
        assert_eq!(uuids, ["ilovecarly"].into_iter().map(|s| s.to_string()).collect::<HashSet<_>>());

        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT release_id FROM collages_releases WHERE collage_name = 'All Eyes'").unwrap();
        let ids: std::collections::HashSet<String> = stmt.query_map([], |row| row.get(0)).unwrap().map(|r| r.unwrap()).collect();
        assert_eq!(ids, ["ilovecarly"].into_iter().map(|s| s.to_string()).collect::<HashSet<_>>());

        // Add another release.
        add_release_to_collage(&config, "All Eyes", "ilovenewjeans").unwrap();
        let contents = fs::read_to_string(&filepath).unwrap();
        let diskdata: CollageData = toml::from_str(&contents).unwrap();
        let uuids: std::collections::HashSet<String> = diskdata.releases.iter().map(|r| r.uuid.clone()).collect();
        assert_eq!(uuids, ["ilovecarly", "ilovenewjeans"].into_iter().map(|s| s.to_string()).collect::<HashSet<_>>());

        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT release_id FROM collages_releases WHERE collage_name = 'All Eyes'").unwrap();
        let ids: std::collections::HashSet<String> = stmt.query_map([], |row| row.get(0)).unwrap().map(|r| r.unwrap()).collect();
        assert_eq!(ids, ["ilovecarly", "ilovenewjeans"].into_iter().map(|s| s.to_string()).collect::<HashSet<_>>());

        // Delete one release.
        remove_release_from_collage(&config, "All Eyes", "ilovenewjeans").unwrap();
        let contents = fs::read_to_string(&filepath).unwrap();
        let diskdata: CollageData = toml::from_str(&contents).unwrap();
        let uuids: std::collections::HashSet<String> = diskdata.releases.iter().map(|r| r.uuid.clone()).collect();
        assert_eq!(uuids, ["ilovecarly"].into_iter().map(|s| s.to_string()).collect::<HashSet<_>>());

        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT release_id FROM collages_releases WHERE collage_name = 'All Eyes'").unwrap();
        let ids: std::collections::HashSet<String> = stmt.query_map([], |row| row.get(0)).unwrap().map(|r| r.unwrap()).collect();
        assert_eq!(ids, ["ilovecarly"].into_iter().map(|s| s.to_string()).collect::<HashSet<_>>());

        // And delete the collage.
        delete_collage(&config, "All Eyes").unwrap();
        assert!(!filepath.is_file());

        let conn = connect(&config).unwrap();
        let exists: bool = conn.query_row("SELECT EXISTS(SELECT * FROM collages WHERE name = 'All Eyes')", [], |row| row.get(0)).unwrap();
        assert!(!exists);
    }

    // def test_collage_add_duplicate(config: Config, source_dir: Path) -> None:
    //     create_collage(config, "All Eyes")
    //     add_release_to_collage(config, "All Eyes", "ilovenewjeans")
    //     add_release_to_collage(config, "All Eyes", "ilovenewjeans")
    //     with (source_dir / "!collages" / "All Eyes.toml").open("rb") as fp:
    //         diskdata = tomllib.load(fp)
    //         assert len(diskdata["releases"]) == 1
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT * FROM collages_releases WHERE collage_name = 'All Eyes'")
    //         assert len(cursor.fetchall()) == 1
    #[test]
    fn test_collage_add_duplicate() {
        let (config, _temp_dir) = testing::seeded_cache();
        let source_dir = &config.music_source_dir;

        create_collage(&config, "All Eyes").unwrap();
        add_release_to_collage(&config, "All Eyes", "ilovenewjeans").unwrap();
        add_release_to_collage(&config, "All Eyes", "ilovenewjeans").unwrap();

        let contents = fs::read_to_string(source_dir.join("!collages").join("All Eyes.toml")).unwrap();
        let diskdata: CollageData = toml::from_str(&contents).unwrap();
        assert_eq!(diskdata.releases.len(), 1);

        let conn = connect(&config).unwrap();
        let count: i32 = conn.query_row("SELECT COUNT(*) FROM collages_releases WHERE collage_name = 'All Eyes'", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    // def test_rename_collage(config: Config, source_dir: Path) -> None:
    //     # And check that auxiliary files were renamed. Create an aux .txt file here.
    //     (source_dir / "!collages" / "Rose Gold.txt").touch()
    //
    //     rename_collage(config, "Rose Gold", "Black Pink")
    //     assert not (source_dir / "!collages" / "Rose Gold.toml").exists()
    //     assert not (source_dir / "!collages" / "Rose Gold.txt").exists()
    //     assert (source_dir / "!collages" / "Black Pink.toml").exists()
    //     assert (source_dir / "!collages" / "Black Pink.txt").exists()
    //
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT EXISTS(SELECT * FROM collages WHERE name = 'Black Pink')")
    //         assert cursor.fetchone()[0]
    //         cursor = conn.execute("SELECT EXISTS(SELECT * FROM collages WHERE name = 'Rose Gold')")
    //         assert not cursor.fetchone()[0]
    #[test]
    fn test_rename_collage() {
        let (config, _temp_dir) = testing::source_dir();
        let source_dir = &config.music_source_dir;

        // And check that auxiliary files were renamed. Create an aux .txt file here.
        fs::File::create(source_dir.join("!collages").join("Rose Gold.txt")).unwrap();

        rename_collage(&config, "Rose Gold", "Black Pink").unwrap();
        assert!(!source_dir.join("!collages").join("Rose Gold.toml").exists());
        assert!(!source_dir.join("!collages").join("Rose Gold.txt").exists());
        assert!(source_dir.join("!collages").join("Black Pink.toml").exists());
        assert!(source_dir.join("!collages").join("Black Pink.txt").exists());

        let conn = connect(&config).unwrap();
        let exists: bool = conn.query_row("SELECT EXISTS(SELECT * FROM collages WHERE name = 'Black Pink')", [], |row| row.get(0)).unwrap();
        assert!(exists);

        let exists: bool = conn.query_row("SELECT EXISTS(SELECT * FROM collages WHERE name = 'Rose Gold')", [], |row| row.get(0)).unwrap();
        assert!(!exists);
    }

    // def test_edit_collages_ordering(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    //     filepath = source_dir / "!collages" / "Rose Gold.toml"
    //     monkeypatch.setattr("rose.collages.click.edit", lambda x: "\n".join(reversed(x.split("\n"))))
    //     edit_collage_in_editor(config, "Rose Gold")
    //
    //     with filepath.open("rb") as fp:
    //         data = tomllib.load(fp)
    //     assert data["releases"][0]["uuid"] == "ilovenewjeans"
    //     assert data["releases"][1]["uuid"] == "ilovecarly"
    #[test]
    fn test_edit_collages_ordering() {
        let (config, _temp_dir) = testing::source_dir();
        let source_dir = &config.music_source_dir;
        let filepath = source_dir.join("!collages").join("Rose Gold.toml");

        // Mock editor function that reverses the order
        let editor_fn = |content: &str| -> Option<String> {
            let lines: Vec<&str> = content.split('\n').collect();
            Some(lines.into_iter().rev().collect::<Vec<_>>().join("\n"))
        };

        edit_collage_in_editor(&config, "Rose Gold", editor_fn).unwrap();

        let contents = fs::read_to_string(&filepath).unwrap();
        let data: CollageData = toml::from_str(&contents).unwrap();
        assert_eq!(data.releases[0].uuid, "ilovenewjeans");
        assert_eq!(data.releases[1].uuid, "ilovecarly");
    }

    // def test_edit_collages_remove_release(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    //     filepath = source_dir / "!collages" / "Rose Gold.toml"
    //     monkeypatch.setattr("rose.collages.click.edit", lambda x: x.split("\n")[0])
    //     edit_collage_in_editor(config, "Rose Gold")
    //
    //     with filepath.open("rb") as fp:
    //         data = tomllib.load(fp)
    //     assert len(data["releases"]) == 1
    #[test]
    fn test_edit_collages_remove_release() {
        let (config, _temp_dir) = testing::source_dir();
        let source_dir = &config.music_source_dir;
        let filepath = source_dir.join("!collages").join("Rose Gold.toml");

        // Mock editor function that returns only the first line
        let editor_fn = |content: &str| -> Option<String> { content.split('\n').next().map(|s| s.to_string()) };

        edit_collage_in_editor(&config, "Rose Gold", editor_fn).unwrap();

        let contents = fs::read_to_string(&filepath).unwrap();
        let data: CollageData = toml::from_str(&contents).unwrap();
        assert_eq!(data.releases.len(), 1);
    }

    // def test_collage_handle_missing_release(config: Config, source_dir: Path) -> None:
    //     """Test that the lifecycle of the collage remains unimpeded despite a missing release."""
    //     filepath = source_dir / "!collages" / "Black Pink.toml"
    //     with filepath.open("w") as fp:
    //         fp.write(
    //             """\
    // [[releases]]
    // uuid = "ilovecarly"
    // description_meta = "lalala"
    // [[releases]]
    // uuid = "ghost"
    // description_meta = "lalala {MISSING}"
    // missing = true
    // """
    //         )
    //     update_cache(config)
    //
    //     # Assert that adding another release works.
    //     add_release_to_collage(config, "Black Pink", "ilovenewjeans")
    //     with (source_dir / "!collages" / "Black Pink.toml").open("rb") as fp:
    //         diskdata = tomllib.load(fp)
    //         assert {r["uuid"] for r in diskdata["releases"]} == {"ghost", "ilovecarly", "ilovenewjeans"}
    //         assert next(r for r in diskdata["releases"] if r["uuid"] == "ghost")["missing"]
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT release_id FROM collages_releases WHERE collage_name = 'Black Pink'")
    //         assert {r["release_id"] for r in cursor} == {"ghost", "ilovecarly", "ilovenewjeans"}
    //
    //     # Delete that release.
    //     remove_release_from_collage(config, "Black Pink", "ilovenewjeans")
    //     with filepath.open("rb") as fp:
    //         diskdata = tomllib.load(fp)
    //         assert {r["uuid"] for r in diskdata["releases"]} == {"ghost", "ilovecarly"}
    //         assert next(r for r in diskdata["releases"] if r["uuid"] == "ghost")["missing"]
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT release_id FROM collages_releases WHERE collage_name = 'Black Pink'")
    //         assert {r["release_id"] for r in cursor} == {"ghost", "ilovecarly"}
    //
    //     # And delete the collage.
    //     delete_collage(config, "Black Pink")
    //     assert not filepath.is_file()
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT EXISTS(SELECT * FROM collages WHERE name = 'Black Pink')")
    //         assert not cursor.fetchone()[0]
    #[test]
    fn test_collage_handle_missing_release() {
        let (config, _temp_dir) = testing::seeded_cache();
        let source_dir = &config.music_source_dir;
        let filepath = source_dir.join("!collages").join("Black Pink.toml");

        fs::write(
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

        update_cache(&config, false, false).unwrap();

        // Assert that adding another release works.
        add_release_to_collage(&config, "Black Pink", "ilovenewjeans").unwrap();
        let contents = fs::read_to_string(&filepath).unwrap();
        let diskdata: CollageData = toml::from_str(&contents).unwrap();
        let uuids: std::collections::HashSet<String> = diskdata.releases.iter().map(|r| r.uuid.clone()).collect();
        assert_eq!(uuids, ["ghost", "ilovecarly", "ilovenewjeans"].into_iter().map(|s| s.to_string()).collect::<HashSet<_>>());

        let ghost_release = diskdata.releases.iter().find(|r| r.uuid == "ghost").unwrap();
        assert_eq!(ghost_release.missing, Some(true));

        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT release_id FROM collages_releases WHERE collage_name = 'Black Pink'").unwrap();
        let ids: std::collections::HashSet<String> = stmt.query_map([], |row| row.get(0)).unwrap().map(|r| r.unwrap()).collect();
        assert_eq!(ids, ["ghost", "ilovecarly", "ilovenewjeans"].into_iter().map(|s| s.to_string()).collect::<HashSet<_>>());

        // Delete that release.
        remove_release_from_collage(&config, "Black Pink", "ilovenewjeans").unwrap();
        let contents = fs::read_to_string(&filepath).unwrap();
        let diskdata: CollageData = toml::from_str(&contents).unwrap();
        let uuids: std::collections::HashSet<String> = diskdata.releases.iter().map(|r| r.uuid.clone()).collect();
        assert_eq!(uuids, ["ghost", "ilovecarly"].into_iter().map(|s| s.to_string()).collect::<HashSet<_>>());

        let ghost_release = diskdata.releases.iter().find(|r| r.uuid == "ghost").unwrap();
        assert_eq!(ghost_release.missing, Some(true));

        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT release_id FROM collages_releases WHERE collage_name = 'Black Pink'").unwrap();
        let ids: std::collections::HashSet<String> = stmt.query_map([], |row| row.get(0)).unwrap().map(|r| r.unwrap()).collect();
        assert_eq!(ids, ["ghost", "ilovecarly"].into_iter().map(|s| s.to_string()).collect::<HashSet<_>>());

        // And delete the collage.
        delete_collage(&config, "Black Pink").unwrap();
        assert!(!filepath.is_file());

        let conn = connect(&config).unwrap();
        let exists: bool = conn.query_row("SELECT EXISTS(SELECT * FROM collages WHERE name = 'Black Pink')", [], |row| row.get(0)).unwrap();
        assert!(!exists);
    }
}
