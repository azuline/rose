// """
// The playlists module provides functions for interacting with playlists.
// """
//
// import logging
// import shutil
// import tomllib
// from collections import Counter
// from pathlib import Path
// from typing import Any
//
// import click
// import tomli_w
// from send2trash import send2trash
//
// from rose.cache import (
//     get_track,
//     get_track_logtext,
//     lock,
//     make_track_logtext,
//     playlist_lock_name,
//     update_cache_evict_nonexistent_playlists,
//     update_cache_for_playlists,
// )
// from rose.collages import DescriptionMismatchError
// from rose.common import RoseExpectedError
// from rose.config import Config
// from rose.releases import InvalidCoverArtFileError
// from rose.templates import artistsfmt
// from rose.tracks import TrackDoesNotExistError
//
// logger = logging.getLogger(__name__)

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info};

use crate::cache::{get_track, get_track_logtext, lock, playlist_lock_name, update_cache_evict_nonexistent_playlists, update_cache_for_playlists};
use crate::collages::DescriptionMismatchError;
use crate::common::ArtistMapping;
use crate::config::Config;
use crate::errors::{Result, RoseError, RoseExpectedError};
use crate::releases::InvalidCoverArtFileError;
use crate::templates::format_artist_mapping as artistsfmt;

// class PlaylistDoesNotExistError(RoseExpectedError):
//     pass
#[derive(Error, Debug)]
#[error("Playlist {0} does not exist")]
pub struct PlaylistDoesNotExistError(pub String);

impl From<PlaylistDoesNotExistError> for RoseExpectedError {
    fn from(err: PlaylistDoesNotExistError) -> Self {
        RoseExpectedError::Generic(err.to_string())
    }
}

impl From<PlaylistDoesNotExistError> for RoseError {
    fn from(err: PlaylistDoesNotExistError) -> Self {
        RoseError::Expected(err.into())
    }
}

// class PlaylistAlreadyExistsError(RoseExpectedError):
//     pass
#[derive(Error, Debug)]
#[error("Playlist {0} already exists")]
pub struct PlaylistAlreadyExistsError(pub String);

impl From<PlaylistAlreadyExistsError> for RoseExpectedError {
    fn from(err: PlaylistAlreadyExistsError) -> Self {
        RoseExpectedError::Generic(err.to_string())
    }
}

impl From<PlaylistAlreadyExistsError> for RoseError {
    fn from(err: PlaylistAlreadyExistsError) -> Self {
        RoseError::Expected(err.into())
    }
}

// TrackDoesNotExistError is already defined in errors.rs as RoseExpectedError::TrackDoesNotExist

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistTrack {
    pub uuid: String,
    pub description_meta: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistData {
    #[serde(default)]
    pub tracks: Vec<PlaylistTrack>,
}

// Helper function to create track logtext - mimics Python's make_track_logtext
pub fn make_track_logtext(title: &str, artists: &ArtistMapping, releasedate: Option<&crate::common::RoseDate>, suffix: &str) -> String {
    let artists_str = artistsfmt(artists);

    let date_part = releasedate.and_then(|d| d.year).map(|y| format!(" [{}]", y)).unwrap_or_default();

    format!("{} - {}{}{}", artists_str, title, date_part, suffix)
}

// def create_playlist(c: Config, name: str) -> None:
//     (c.music_source_dir / "!playlists").mkdir(parents=True, exist_ok=True)
//     path = playlist_path(c, name)
//     with lock(c, playlist_lock_name(name)):
//         if path.exists():
//             raise PlaylistAlreadyExistsError(f"Playlist {name} already exists")
//         path.touch()
//     logger.info(f"Created playlist {name} in source directory")
//     update_cache_for_playlists(c, [name], force=True)
pub fn create_playlist(c: &Config, name: &str) -> Result<()> {
    let playlists_dir = c.music_source_dir.join("!playlists");
    fs::create_dir_all(&playlists_dir)?;

    let path = playlist_path(c, name);
    let _lock = lock(c, &playlist_lock_name(name), 5.0)?;

    if path.exists() {
        return Err(PlaylistAlreadyExistsError(name.to_string()).into());
    }

    fs::File::create(&path)?;
    info!("created playlist {} in source directory", name);
    update_cache_for_playlists(c, Some(vec![name.to_string()]), true)?;

    Ok(())
}

// def delete_playlist(c: Config, name: str) -> None:
//     path = playlist_path(c, name)
//     with lock(c, playlist_lock_name(name)):
//         if not path.exists():
//             raise PlaylistDoesNotExistError(f"Playlist {name} does not exist")
//         send2trash(path)
//     logger.info(f"Deleted playlist {name} from source directory")
//     update_cache_evict_nonexistent_playlists(c)
pub fn delete_playlist(c: &Config, name: &str) -> Result<()> {
    let path = playlist_path(c, name);
    let _lock = lock(c, &playlist_lock_name(name), 5.0)?;

    if !path.exists() {
        return Err(PlaylistDoesNotExistError(name.to_string()).into());
    }

    trash::delete(&path).map_err(|e| RoseError::Io(std::io::Error::other(e)))?;
    info!("deleted playlist {} from source directory", name);
    update_cache_evict_nonexistent_playlists(c)?;

    Ok(())
}

// def rename_playlist(c: Config, old_name: str, new_name: str) -> None:
//     logger.info(f"Renamed playlist {old_name} to {new_name}")
//     old_path = playlist_path(c, old_name)
//     new_path = playlist_path(c, new_name)
//     with lock(c, playlist_lock_name(old_name)), lock(c, playlist_lock_name(new_name)):
//         if not old_path.exists():
//             raise PlaylistDoesNotExistError(f"Playlist {old_name} does not exist")
//         if new_path.exists():
//             raise PlaylistAlreadyExistsError(f"Playlist {new_name} already exists")
//         old_path.rename(new_path)
//         # And also rename all files with the same stem (e.g. cover arts).
//         for old_adjacent_file in (c.music_source_dir / "!playlists").iterdir():
//             if old_adjacent_file.stem != old_path.stem:
//                 continue
//             new_adjacent_file = old_adjacent_file.with_name(new_path.stem + old_adjacent_file.suffix)
//             if new_adjacent_file.exists():
//                 continue
//             old_adjacent_file.rename(new_adjacent_file)
//             logger.debug("Renaming playlist-adjacent file {old_adjacent_file} to {new_adjacent_file}")
//     update_cache_for_playlists(c, [new_name], force=True)
//     update_cache_evict_nonexistent_playlists(c)
pub fn rename_playlist(c: &Config, old_name: &str, new_name: &str) -> Result<()> {
    info!("renamed playlist {} to {}", old_name, new_name);

    let old_path = playlist_path(c, old_name);
    let new_path = playlist_path(c, new_name);

    let _lock1 = lock(c, &playlist_lock_name(old_name), 5.0)?;
    let _lock2 = lock(c, &playlist_lock_name(new_name), 5.0)?;

    if !old_path.exists() {
        return Err(PlaylistDoesNotExistError(old_name.to_string()).into());
    }
    if new_path.exists() {
        return Err(PlaylistAlreadyExistsError(new_name.to_string()).into());
    }

    fs::rename(&old_path, &new_path)?;

    // And also rename all files with the same stem (e.g. cover arts).
    let playlists_dir = c.music_source_dir.join("!playlists");
    let old_stem = old_path.file_stem().unwrap().to_string_lossy();
    let new_stem = new_path.file_stem().unwrap().to_string_lossy();

    for entry in fs::read_dir(&playlists_dir)? {
        let entry = entry?;
        let path = entry.path();
        if let Some(stem) = path.file_stem() {
            if stem == old_stem.as_ref() {
                let extension = path.extension().map(|e| e.to_string_lossy()).unwrap_or_default();
                let new_adjacent_file = playlists_dir.join(format!("{}.{}", new_stem, extension));
                if !new_adjacent_file.exists() {
                    debug!("renaming playlist-adjacent file {:?} to {:?}", path, new_adjacent_file);
                    fs::rename(&path, &new_adjacent_file)?;
                }
            }
        }
    }

    update_cache_for_playlists(c, Some(vec![new_name.to_string()]), true)?;
    update_cache_evict_nonexistent_playlists(c)?;

    Ok(())
}

// def remove_track_from_playlist(
//     c: Config,
//     playlist_name: str,
//     track_id: str,
// ) -> None:
//     track_logtext = get_track_logtext(c, track_id)
//     if not track_logtext:
//         raise TrackDoesNotExistError(f"Track {track_id} does not exist")
//     path = playlist_path(c, playlist_name)
//     if not path.exists():
//         raise PlaylistDoesNotExistError(f"Playlist {playlist_name} does not exist")
//     with lock(c, playlist_lock_name(playlist_name)):
//         with path.open("rb") as fp:
//             data = tomllib.load(fp)
//         old_tracks = data.get("tracks", [])
//         new_tracks = [r for r in old_tracks if r["uuid"] != track_id]
//         if old_tracks == new_tracks:
//             logger.info(f"No-Op: Track {track_logtext} not in playlist {playlist_name}")
//             return
//         data["tracks"] = new_tracks
//         with path.open("wb") as fp:
//             tomli_w.dump(data, fp)
//     logger.info(f"Removed track {track_logtext} from playlist {playlist_name}")
//     update_cache_for_playlists(c, [playlist_name], force=True)
pub fn remove_track_from_playlist(c: &Config, playlist_name: &str, track_id: &str) -> Result<()> {
    let track_logtext = get_track_logtext(c, track_id)?;

    let path = playlist_path(c, playlist_name);
    if !path.exists() {
        return Err(PlaylistDoesNotExistError(playlist_name.to_string()).into());
    }

    let _lock = lock(c, &playlist_lock_name(playlist_name), 5.0)?;

    let contents = fs::read_to_string(&path)?;
    let mut data: PlaylistData = toml::from_str(&contents).unwrap_or(PlaylistData { tracks: vec![] });

    let old_tracks_len = data.tracks.len();
    data.tracks.retain(|t| t.uuid != track_id);

    if old_tracks_len == data.tracks.len() {
        info!("no-op: track {} not in playlist {}", track_logtext, playlist_name);
        return Ok(());
    }

    let toml_string = toml::to_string_pretty(&data)?;
    fs::write(&path, toml_string)?;

    info!("removed track {} from playlist {}", track_logtext, playlist_name);
    update_cache_for_playlists(c, Some(vec![playlist_name.to_string()]), true)?;

    Ok(())
}

// def add_track_to_playlist(
//     c: Config,
//     playlist_name: str,
//     track_id: str,
// ) -> None:
//     track = get_track(c, track_id)
//     if not track:
//         raise TrackDoesNotExistError(f"Track {track_id} does not exist")
//     path = playlist_path(c, playlist_name)
//     if not path.exists():
//         raise PlaylistDoesNotExistError(f"Playlist {playlist_name} does not exist")
//     with lock(c, playlist_lock_name(playlist_name)):
//         with path.open("rb") as fp:
//             data = tomllib.load(fp)
//         data["tracks"] = data.get("tracks", [])
//         # Check to see if track is already in the playlist. If so, no op. We don't support
//         # duplicate playlist entries.
//         for r in data["tracks"]:
//             if r["uuid"] == track_id:
//                 logger.info(f"No-Op: Track {track} already in playlist {playlist_name}")
//                 return
//
//         desc = f"{artistsfmt(track.trackartists)} - {track.tracktitle}"
//         data["tracks"].append({"uuid": track_id, "description_meta": desc})
//         with path.open("wb") as fp:
//             tomli_w.dump(data, fp)
//     track_logtext = make_track_logtext(
//         title=track.tracktitle,
//         artists=track.trackartists,
//         releasedate=track.release.releasedate,
//         suffix=track.source_path.suffix,
//     )
//     logger.info(f"Added track {track_logtext} to playlist {playlist_name}")
//     update_cache_for_playlists(c, [playlist_name], force=True)
pub fn add_track_to_playlist(c: &Config, playlist_name: &str, track_id: &str) -> Result<()> {
    let track = get_track(c, track_id)?.ok_or_else(|| RoseExpectedError::TrackDoesNotExist { id: track_id.to_string() })?;

    let path = playlist_path(c, playlist_name);
    if !path.exists() {
        return Err(PlaylistDoesNotExistError(playlist_name.to_string()).into());
    }

    let _lock = lock(c, &playlist_lock_name(playlist_name), 5.0)?;

    let contents = fs::read_to_string(&path)?;
    let mut data: PlaylistData = toml::from_str(&contents).unwrap_or(PlaylistData { tracks: vec![] });

    // Check to see if track is already in the playlist. If so, no op. We don't support
    // duplicate playlist entries.
    for t in &data.tracks {
        if t.uuid == track_id {
            let track_str = format!("{} - {}", artistsfmt(&track.trackartists), track.tracktitle);
            info!("no-op: track {} already in playlist {}", track_str, playlist_name);
            return Ok(());
        }
    }

    let desc = format!("{} - {}", artistsfmt(&track.trackartists), track.tracktitle);
    data.tracks.push(PlaylistTrack {
        uuid: track_id.to_string(),
        description_meta: desc,
        missing: None,
    });

    let toml_string = toml::to_string_pretty(&data)?;
    fs::write(&path, toml_string)?;

    let track_logtext = make_track_logtext(
        &track.tracktitle,
        &track.trackartists,
        track.release.releasedate.as_ref(),
        &track.source_path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default(),
    );
    info!("added track {} to playlist {}", track_logtext, playlist_name);
    update_cache_for_playlists(c, Some(vec![playlist_name.to_string()]), true)?;

    Ok(())
}

// def edit_playlist_in_editor(c: Config, playlist_name: str) -> None:
//     path = playlist_path(c, playlist_name)
//     if not path.exists():
//         raise PlaylistDoesNotExistError(f"Playlist {playlist_name} does not exist")
//     with lock(c, playlist_lock_name(playlist_name), timeout=60.0):
//         with path.open("rb") as fp:
//             data = tomllib.load(fp)
//         raw_tracks = data.get("tracks", [])
//
//         # Because tracks are not globally unique, we append the UUID if there are any conflicts.
//         # discriminator.
//         lines_to_edit: list[str] = []
//         uuid_mapping: dict[str, str] = {}
//         line_occurrences = Counter([r["description_meta"] for r in raw_tracks])
//         for r in raw_tracks:
//             if line_occurrences[r["description_meta"]] > 1:
//                 line = f'{r["description_meta"]} [{r["uuid"]}]'
//             else:
//                 line = r["description_meta"]
//             lines_to_edit.append(line)
//             uuid_mapping[line] = r["uuid"]
//
//         edited_track_descriptions = click.edit("\n".join(lines_to_edit))
//         if edited_track_descriptions is None:
//             logger.info("Aborting: metadata file not submitted.")
//             return
//
//         edited_tracks: list[dict[str, Any]] = []
//         for desc in edited_track_descriptions.strip().split("\n"):
//             try:
//                 uuid = uuid_mapping[desc]
//             except KeyError as e:
//                 raise DescriptionMismatchError(
//                     f"Track {desc} does not match a known track in the playlist. Was the line edited?"
//                 ) from e
//             edited_tracks.append({"uuid": uuid, "description_meta": desc})
//         data["tracks"] = edited_tracks
//
//         with path.open("wb") as fp:
//             tomli_w.dump(data, fp)
//     logger.info(f"Edited playlist {playlist_name} from EDITOR")
//     update_cache_for_playlists(c, [playlist_name], force=True)
pub fn edit_playlist_in_editor(c: &Config, playlist_name: &str, editor_fn: impl FnOnce(&str) -> Option<String>) -> Result<()> {
    let path = playlist_path(c, playlist_name);
    if !path.exists() {
        return Err(PlaylistDoesNotExistError(playlist_name.to_string()).into());
    }

    let _lock = lock(c, &playlist_lock_name(playlist_name), 60.0)?;

    let contents = fs::read_to_string(&path)?;
    let mut data: PlaylistData = toml::from_str(&contents)?;

    // Count occurrences of each description_meta
    let mut line_occurrences: HashMap<String, usize> = HashMap::new();
    for track in &data.tracks {
        *line_occurrences.entry(track.description_meta.clone()).or_insert(0) += 1;
    }

    // Build lines to edit and uuid mapping
    let mut lines_to_edit = Vec::new();
    let mut uuid_mapping = HashMap::new();

    for track in &data.tracks {
        let line = if line_occurrences[&track.description_meta] > 1 {
            format!("{} [{}]", track.description_meta, track.uuid)
        } else {
            track.description_meta.clone()
        };
        lines_to_edit.push(line.clone());
        uuid_mapping.insert(line, track.uuid.clone());
    }

    let edited_content = editor_fn(&lines_to_edit.join("\n"));

    if edited_content.is_none() {
        info!("aborting: metadata file not submitted.");
        return Ok(());
    }

    let edited_content = edited_content.unwrap();
    let mut edited_tracks = Vec::new();

    for desc in edited_content.trim().split('\n') {
        if desc.is_empty() {
            continue;
        }

        let uuid = uuid_mapping
            .get(desc)
            .ok_or_else(|| DescriptionMismatchError(format!("Track {} does not match a known track in the playlist. Was the line edited?", desc)))?;

        // Find the original track to preserve any extra fields like 'missing'
        let original_track = data.tracks.iter().find(|t| &t.uuid == uuid);
        let mut track = PlaylistTrack {
            uuid: uuid.clone(),
            description_meta: desc.to_string(),
            missing: None,
        };

        // Preserve the missing field if it exists
        if let Some(original) = original_track {
            track.missing = original.missing;
        }

        edited_tracks.push(track);
    }

    data.tracks = edited_tracks;

    let toml_string = toml::to_string_pretty(&data)?;
    fs::write(&path, toml_string)?;

    info!("edited playlist {} from editor", playlist_name);
    update_cache_for_playlists(c, Some(vec![playlist_name.to_string()]), true)?;

    Ok(())
}

// def set_playlist_cover_art(c: Config, playlist_name: str, new_cover_art_path: Path) -> None:
//     """
//     This function removes all potential cover arts for the playlist, and then copies the file
//     file located at the passed in path to be the playlist's art file.
//     """
//     suffix = new_cover_art_path.suffix.lower()
//     if suffix[1:] not in c.valid_art_exts:
//         raise InvalidCoverArtFileError(
//             f"File {new_cover_art_path.name}'s extension is not supported for cover images: "
//             "To change this, please read the configuration documentation"
//         )
//
//     path = playlist_path(c, playlist_name)
//     if not path.exists():
//         raise PlaylistDoesNotExistError(f"Playlist {playlist_name} does not exist")
//     for f in (c.music_source_dir / "!playlists").iterdir():
//         if f.stem == playlist_name and f.suffix[1:].lower() in c.valid_art_exts:
//             logger.debug(f"Deleting existing cover art {f.name} in playlists")
//             f.unlink()
//     shutil.copyfile(new_cover_art_path, path.with_suffix(suffix))
//     logger.info(f"Set the cover of playlist {playlist_name} to {new_cover_art_path.name}")
//     update_cache_for_playlists(c, [playlist_name])
pub fn set_playlist_cover_art(c: &Config, playlist_name: &str, new_cover_art_path: &Path) -> Result<()> {
    let extension = new_cover_art_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

    if !c.valid_art_exts.contains(&extension) {
        return Err(InvalidCoverArtFileError(format!(
            "File {}'s extension is not supported for cover images: \
             To change this, please read the configuration documentation",
            new_cover_art_path.file_name().unwrap_or_default().to_string_lossy()
        ))
        .into());
    }

    let path = playlist_path(c, playlist_name);
    if !path.exists() {
        return Err(PlaylistDoesNotExistError(playlist_name.to_string()).into());
    }

    let playlists_dir = c.music_source_dir.join("!playlists");

    // Remove existing cover arts
    for entry in fs::read_dir(&playlists_dir)? {
        let entry = entry?;
        let entry_path = entry.path();
        if let (Some(stem), Some(ext)) = (entry_path.file_stem(), entry_path.extension()) {
            if stem.to_string_lossy() == playlist_name {
                let ext_str = ext.to_string_lossy().to_lowercase();
                if c.valid_art_exts.contains(&ext_str) {
                    debug!("deleting existing cover art {} in playlists", entry_path.file_name().unwrap().to_string_lossy());
                    fs::remove_file(&entry_path)?;
                }
            }
        }
    }

    let new_path = path.with_extension(&extension);
    fs::copy(new_cover_art_path, &new_path)?;

    info!("set the cover of playlist {} to {}", playlist_name, new_cover_art_path.file_name().unwrap_or_default().to_string_lossy());
    update_cache_for_playlists(c, Some(vec![playlist_name.to_string()]), true)?;

    Ok(())
}

// def delete_playlist_cover_art(c: Config, playlist_name: str) -> None:
//     """This function removes all potential cover arts for the playlist."""
//     path = playlist_path(c, playlist_name)
//     if not path.exists():
//         raise PlaylistDoesNotExistError(f"Playlist {playlist_name} does not exist")
//     found = False
//     for f in (c.music_source_dir / "!playlists").iterdir():
//         if f.stem == playlist_name and f.suffix[1:].lower() in c.valid_art_exts:
//             logger.debug(f"Deleting existing cover art {f.name} in playlists")
//             f.unlink()
//             found = True
//     if found:
//         logger.info(f"Deleted cover arts of playlist {playlist_name}")
//     else:
//         logger.info(f"No-Op: No cover arts found for playlist {playlist_name}")
//     update_cache_for_playlists(c, [playlist_name])
pub fn delete_playlist_cover_art(c: &Config, playlist_name: &str) -> Result<()> {
    let path = playlist_path(c, playlist_name);
    if !path.exists() {
        return Err(PlaylistDoesNotExistError(playlist_name.to_string()).into());
    }

    let playlists_dir = c.music_source_dir.join("!playlists");
    let mut found = false;

    for entry in fs::read_dir(&playlists_dir)? {
        let entry = entry?;
        let entry_path = entry.path();
        if let (Some(stem), Some(ext)) = (entry_path.file_stem(), entry_path.extension()) {
            if stem.to_string_lossy() == playlist_name {
                let ext_str = ext.to_string_lossy().to_lowercase();
                if c.valid_art_exts.contains(&ext_str) {
                    debug!("deleting existing cover art {} in playlists", entry_path.file_name().unwrap().to_string_lossy());
                    fs::remove_file(&entry_path)?;
                    found = true;
                }
            }
        }
    }

    if found {
        info!("deleted cover arts of playlist {}", playlist_name);
    } else {
        info!("no-op: no cover arts found for playlist {}", playlist_name);
    }

    update_cache_for_playlists(c, Some(vec![playlist_name.to_string()]), true)?;

    Ok(())
}

// def playlist_path(c: Config, name: str) -> Path:
//     return c.music_source_dir / "!playlists" / f"{name}.toml"
pub fn playlist_path(c: &Config, name: &str) -> PathBuf {
    c.music_source_dir.join("!playlists").join(format!("{}.toml", name))
}

// TESTS

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{connect, update_cache};
    use crate::testing;
    use std::fs;

    // import shutil
    // import tomllib
    // from pathlib import Path
    // from typing import Any
    //
    // from conftest import TEST_PLAYLIST_1, TEST_RELEASE_1
    // from rose.cache import connect, update_cache
    // from rose.config import Config
    // from rose.playlists import (
    //     add_track_to_playlist,
    //     create_playlist,
    //     delete_playlist,
    //     delete_playlist_cover_art,
    //     edit_playlist_in_editor,
    //     remove_track_from_playlist,
    //     rename_playlist,
    //     set_playlist_cover_art,
    // )

    // def test_remove_track_from_playlist(config: Config, source_dir: Path) -> None:
    //     remove_track_from_playlist(config, "Lala Lisa", "iloveloona")
    //
    //     # Assert file is updated.
    //     with (source_dir / "!playlists" / "Lala Lisa.toml").open("rb") as fp:
    //         diskdata = tomllib.load(fp)
    //     assert len(diskdata["tracks"]) == 1
    //     assert diskdata["tracks"][0]["uuid"] == "ilovetwice"
    //
    //     # Assert cache is updated.
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'Lala Lisa'")
    //         ids = [r["track_id"] for r in cursor]
    //         assert ids == ["ilovetwice"]
    #[test]
    fn test_remove_track_from_playlist() {
        let (config, _temp_dir) = testing::seeded_cache();
        let source_dir = &config.music_source_dir;

        remove_track_from_playlist(&config, "Lala Lisa", "t1").unwrap();

        // Assert file is updated.
        let filepath = source_dir.join("!playlists").join("Lala Lisa.toml");
        let contents = fs::read_to_string(&filepath).unwrap();
        let data: PlaylistData = toml::from_str(&contents).unwrap();
        assert_eq!(data.tracks.len(), 1);
        assert_eq!(data.tracks[0].uuid, "t3");

        // Assert cache is updated.
        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'Lala Lisa'").unwrap();
        let ids: Vec<String> = stmt.query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        assert_eq!(ids, vec!["t3"]);
    }

    // def test_playlist_lifecycle(config: Config, source_dir: Path) -> None:
    //     filepath = source_dir / "!playlists" / "You & Me.toml"
    //
    //     # Create playlist.
    //     assert not filepath.exists()
    //     create_playlist(config, "You & Me")
    //     assert filepath.is_file()
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT EXISTS(SELECT * FROM playlists WHERE name = 'You & Me')")
    //         assert cursor.fetchone()[0]
    //
    //     # Add one track.
    //     add_track_to_playlist(config, "You & Me", "iloveloona")
    //     with filepath.open("rb") as fp:
    //         diskdata = tomllib.load(fp)
    //         assert {r["uuid"] for r in diskdata["tracks"]} == {"iloveloona"}
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'")
    //         assert {r["track_id"] for r in cursor} == {"iloveloona"}
    //
    //     # Add another track.
    //     add_track_to_playlist(config, "You & Me", "ilovetwice")
    //     with (source_dir / "!playlists" / "You & Me.toml").open("rb") as fp:
    //         diskdata = tomllib.load(fp)
    //         assert {r["uuid"] for r in diskdata["tracks"]} == {"iloveloona", "ilovetwice"}
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'")
    //         assert {r["track_id"] for r in cursor} == {"iloveloona", "ilovetwice"}
    //
    //     # Delete one track.
    //     remove_track_from_playlist(config, "You & Me", "ilovetwice")
    //     with filepath.open("rb") as fp:
    //         diskdata = tomllib.load(fp)
    //         assert {r["uuid"] for r in diskdata["tracks"]} == {"iloveloona"}
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'")
    //         assert {r["track_id"] for r in cursor} == {"iloveloona"}
    //
    //     # And delete the playlist.
    //     delete_playlist(config, "You & Me")
    //     assert not filepath.is_file()
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT EXISTS(SELECT * FROM playlists WHERE name = 'You & Me')")
    //         assert not cursor.fetchone()[0]
    #[test]
    fn test_playlist_lifecycle() {
        let (config, _temp_dir) = testing::seeded_cache();
        let source_dir = &config.music_source_dir;
        let filepath = source_dir.join("!playlists").join("You & Me.toml");

        // Create playlist.
        assert!(!filepath.exists());
        create_playlist(&config, "You & Me").unwrap();
        assert!(filepath.is_file());

        let conn = connect(&config).unwrap();
        let exists: bool = conn.query_row("SELECT EXISTS(SELECT * FROM playlists WHERE name = 'You & Me')", [], |row| row.get(0)).unwrap();
        assert!(exists);
        drop(conn);

        // Add one track.
        add_track_to_playlist(&config, "You & Me", "t1").unwrap();
        let contents = fs::read_to_string(&filepath).unwrap();
        let data: PlaylistData = toml::from_str(&contents).unwrap();
        let uuids: std::collections::HashSet<_> = data.tracks.iter().map(|t| t.uuid.as_str()).collect();
        assert_eq!(uuids, ["t1"].iter().copied().collect());

        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'").unwrap();
        let track_ids: std::collections::HashSet<String> =
            stmt.query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<std::collections::HashSet<_>>>().unwrap();
        assert_eq!(track_ids, ["t1"].iter().map(|s| s.to_string()).collect());
        drop(stmt);
        drop(conn);

        // Add another track.
        add_track_to_playlist(&config, "You & Me", "t3").unwrap();
        let contents = fs::read_to_string(&filepath).unwrap();
        let data: PlaylistData = toml::from_str(&contents).unwrap();
        let uuids: std::collections::HashSet<_> = data.tracks.iter().map(|t| t.uuid.as_str()).collect();
        assert_eq!(uuids, ["t1", "t3"].iter().copied().collect());

        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'").unwrap();
        let track_ids: std::collections::HashSet<String> =
            stmt.query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<std::collections::HashSet<_>>>().unwrap();
        assert_eq!(track_ids, ["t1", "t3"].iter().map(|s| s.to_string()).collect());
        drop(stmt);
        drop(conn);

        // Delete one track.
        remove_track_from_playlist(&config, "You & Me", "t3").unwrap();
        let contents = fs::read_to_string(&filepath).unwrap();
        let data: PlaylistData = toml::from_str(&contents).unwrap();
        let uuids: std::collections::HashSet<_> = data.tracks.iter().map(|t| t.uuid.as_str()).collect();
        assert_eq!(uuids, ["t1"].iter().copied().collect());

        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'").unwrap();
        let track_ids: std::collections::HashSet<String> =
            stmt.query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<std::collections::HashSet<_>>>().unwrap();
        assert_eq!(track_ids, ["t1"].iter().map(|s| s.to_string()).collect());
        drop(stmt);
        drop(conn);

        // And delete the playlist.
        delete_playlist(&config, "You & Me").unwrap();
        assert!(!filepath.is_file());

        let conn = connect(&config).unwrap();
        let exists: bool = conn.query_row("SELECT EXISTS(SELECT * FROM playlists WHERE name = 'You & Me')", [], |row| row.get(0)).unwrap();
        assert!(!exists);
    }

    // def test_playlist_add_duplicate(config: Config, source_dir: Path) -> None:
    //     create_playlist(config, "You & Me")
    //     add_track_to_playlist(config, "You & Me", "ilovetwice")
    //     add_track_to_playlist(config, "You & Me", "ilovetwice")
    //     with (source_dir / "!playlists" / "You & Me.toml").open("rb") as fp:
    //         diskdata = tomllib.load(fp)
    //         assert len(diskdata["tracks"]) == 1
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT * FROM playlists_tracks WHERE playlist_name = 'You & Me'")
    //         assert len(cursor.fetchall()) == 1
    #[test]
    fn test_playlist_add_duplicate() {
        let (config, _temp_dir) = testing::seeded_cache();
        let source_dir = &config.music_source_dir;

        create_playlist(&config, "You & Me").unwrap();
        add_track_to_playlist(&config, "You & Me", "t3").unwrap();
        add_track_to_playlist(&config, "You & Me", "t3").unwrap();

        let filepath = source_dir.join("!playlists").join("You & Me.toml");
        let contents = fs::read_to_string(&filepath).unwrap();
        let data: PlaylistData = toml::from_str(&contents).unwrap();
        assert_eq!(data.tracks.len(), 1);

        let conn = connect(&config).unwrap();
        let count: i32 = conn.query_row("SELECT COUNT(*) FROM playlists_tracks WHERE playlist_name = 'You & Me'", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    // def test_rename_playlist(config: Config, source_dir: Path) -> None:
    //     # And check that auxiliary files were renamed. Create an aux cover art here.
    //     (source_dir / "!playlists" / "Lala Lisa.jpg").touch(exist_ok=True)
    //
    //     rename_playlist(config, "Lala Lisa", "Turtle Rabbit")
    //     assert not (source_dir / "!playlists" / "Lala Lisa.toml").exists()
    //     assert not (source_dir / "!playlists" / "Lala Lisa.jpg").exists()
    //     assert (source_dir / "!playlists" / "Turtle Rabbit.toml").exists()
    //     assert (source_dir / "!playlists" / "Turtle Rabbit.jpg").exists()
    //
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT EXISTS(SELECT * FROM playlists WHERE name = 'Turtle Rabbit')")
    //         assert cursor.fetchone()[0]
    //         cursor = conn.execute("SELECT cover_path FROM playlists WHERE name = 'Turtle Rabbit'")
    //         assert Path(cursor.fetchone()[0]) == source_dir / "!playlists" / "Turtle Rabbit.jpg"
    //         cursor = conn.execute("SELECT EXISTS(SELECT * FROM playlists WHERE name = 'Lala Lisa')")
    //         assert not cursor.fetchone()[0]
    #[test]
    fn test_rename_playlist() {
        let (config, _temp_dir) = testing::seeded_cache();
        let source_dir = &config.music_source_dir;
        let playlists_dir = source_dir.join("!playlists");

        // Create an aux cover art
        fs::File::create(playlists_dir.join("Lala Lisa.jpg")).unwrap();

        rename_playlist(&config, "Lala Lisa", "Awesome Playlist").unwrap();

        assert!(!playlists_dir.join("Lala Lisa.toml").exists());
        assert!(!playlists_dir.join("Lala Lisa.jpg").exists());
        assert!(playlists_dir.join("Awesome Playlist.toml").exists());
        assert!(playlists_dir.join("Awesome Playlist.jpg").exists());

        let conn = connect(&config).unwrap();

        let exists: bool = conn.query_row("SELECT EXISTS(SELECT * FROM playlists WHERE name = 'Awesome Playlist')", [], |row| row.get(0)).unwrap();
        assert!(exists);

        let cover_path: Option<String> = conn.query_row("SELECT cover_path FROM playlists WHERE name = 'Awesome Playlist'", [], |row| row.get(0)).unwrap();
        assert_eq!(cover_path.map(PathBuf::from), Some(playlists_dir.join("Awesome Playlist.jpg")));

        let exists: bool = conn.query_row("SELECT EXISTS(SELECT * FROM playlists WHERE name = 'Lala Lisa')", [], |row| row.get(0)).unwrap();
        assert!(!exists);
    }

    // def test_edit_playlists_ordering(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    //     filepath = source_dir / "!playlists" / "Lala Lisa.toml"
    //     monkeypatch.setattr("rose.playlists.click.edit", lambda x: "\n".join(reversed(x.split("\n"))))
    //     edit_playlist_in_editor(config, "Lala Lisa")
    //
    //     with filepath.open("rb") as fp:
    //         data = tomllib.load(fp)
    //     assert data["tracks"][0]["uuid"] == "ilovetwice"
    //     assert data["tracks"][1]["uuid"] == "iloveloona"
    #[test]
    fn test_edit_playlists_ordering() {
        let (config, _temp_dir) = testing::seeded_cache();
        let source_dir = &config.music_source_dir;
        let filepath = source_dir.join("!playlists").join("Lala Lisa.toml");

        // Mock editor function that reverses the order of lines
        let editor_fn = |content: &str| -> Option<String> {
            let lines: Vec<&str> = content.split('\n').collect();
            Some(lines.into_iter().rev().collect::<Vec<_>>().join("\n"))
        };

        edit_playlist_in_editor(&config, "Lala Lisa", editor_fn).unwrap();

        let contents = fs::read_to_string(&filepath).unwrap();
        let data: PlaylistData = toml::from_str(&contents).unwrap();
        assert_eq!(data.tracks[0].uuid, "t3");
        assert_eq!(data.tracks[1].uuid, "t1");
    }

    // def test_edit_playlists_remove_track(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    //     filepath = source_dir / "!playlists" / "Lala Lisa.toml"
    //     monkeypatch.setattr("rose.playlists.click.edit", lambda x: x.split("\n")[0])
    //     edit_playlist_in_editor(config, "Lala Lisa")
    //
    //     with filepath.open("rb") as fp:
    //         data = tomllib.load(fp)
    //     assert len(data["tracks"]) == 1
    #[test]
    fn test_edit_playlists_remove_track() {
        let (config, _temp_dir) = testing::seeded_cache();
        let source_dir = &config.music_source_dir;
        let filepath = source_dir.join("!playlists").join("Lala Lisa.toml");

        // Mock editor function that returns only the first line
        let editor_fn = |content: &str| -> Option<String> { content.split('\n').next().map(|s| s.to_string()) };

        edit_playlist_in_editor(&config, "Lala Lisa", editor_fn).unwrap();

        let contents = fs::read_to_string(&filepath).unwrap();
        let data: PlaylistData = toml::from_str(&contents).unwrap();
        assert_eq!(data.tracks.len(), 1);
    }

    // def test_edit_playlists_duplicate_track_name(monkeypatch: Any, config: Config) -> None:
    //     """
    //     When there are duplicate virtual filenames, we append UUID. Check that it works by asserting on
    //     the seen text and checking that reversing the order works.
    //     """
    //     # Generate conflicting virtual tracknames by having two copies of a release in the library.
    //     shutil.copytree(TEST_RELEASE_1, config.music_source_dir / "a")
    //     shutil.copytree(TEST_RELEASE_1, config.music_source_dir / "b")
    //     update_cache(config)
    //
    //     with connect(config) as conn:
    //         # Get the first track of each release.
    //         cursor = conn.execute("SELECT id FROM tracks WHERE source_path LIKE '%01.m4a'")
    //         track_ids = [r["id"] for r in cursor]
    //         assert len(track_ids) == 2
    //
    //     create_playlist(config, "You & Me")
    //     for tid in track_ids:
    //         add_track_to_playlist(config, "You & Me", tid)
    //
    //     seen = ""
    //
    //     def editfn(x: str) -> str:
    //         nonlocal seen
    //         seen = x
    //         return "\n".join(reversed(x.split("\n")))
    //
    //     monkeypatch.setattr("rose.playlists.click.edit", editfn)
    //     edit_playlist_in_editor(config, "You & Me")
    //
    //     assert seen == "\n".join([f"[1990-02-05] BLACKPINK - Track 1 [{tid}]" for tid in track_ids])
    //
    //     with (config.music_source_dir / "!playlists" / "You & Me.toml").open("rb") as fp:
    //         data = tomllib.load(fp)
    //     assert data["tracks"][0]["uuid"] == track_ids[1]
    //     assert data["tracks"][1]["uuid"] == track_ids[0]
    #[test]
    fn test_edit_playlists_duplicate_track_name() {
        let (config, _temp_dir) = testing::config();

        // Create test releases in two different locations
        let testdata_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
        let test_release = testdata_dir.join("Test Release 1");

        testing::copy_dir_all(&test_release, &config.music_source_dir.join("a")).unwrap();
        testing::copy_dir_all(&test_release, &config.music_source_dir.join("b")).unwrap();

        update_cache(&config, false, false).unwrap();

        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT id FROM tracks WHERE source_path LIKE '%01.m4a'").unwrap();
        let track_ids: Vec<String> = stmt.query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        assert_eq!(track_ids.len(), 2);
        drop(stmt);
        drop(conn);

        create_playlist(&config, "You & Me").unwrap();
        for tid in &track_ids {
            add_track_to_playlist(&config, "You & Me", tid).unwrap();
        }

        use std::cell::RefCell;

        let seen = RefCell::new(String::new());
        let track_ids_clone = track_ids.clone();
        let editor_fn = |content: &str| -> Option<String> {
            *seen.borrow_mut() = content.to_string();
            let lines: Vec<&str> = content.split('\n').collect();
            Some(lines.into_iter().rev().collect::<Vec<_>>().join("\n"))
        };

        edit_playlist_in_editor(&config, "You & Me", editor_fn).unwrap();

        // The actual format includes the release date
        let expected_lines: Vec<String> = track_ids_clone.iter().map(|tid| format!("[1990-02-05] BLACKPINK - Track 1 [{}]", tid)).collect();
        assert_eq!(*seen.borrow(), expected_lines.join("\n"));

        let filepath = config.music_source_dir.join("!playlists").join("You & Me.toml");
        let contents = fs::read_to_string(&filepath).unwrap();
        let data: PlaylistData = toml::from_str(&contents).unwrap();
        assert_eq!(data.tracks[0].uuid, track_ids[1]);
        assert_eq!(data.tracks[1].uuid, track_ids[0]);
    }

    // def test_playlist_handle_missing_track(config: Config, source_dir: Path) -> None:
    //     """Test that the lifecycle of the playlist remains unimpeded despite a missing track."""
    //     filepath = source_dir / "!playlists" / "You & Me.toml"
    //     with filepath.open("w") as fp:
    //         fp.write(
    //             """\
    // [[tracks]]
    // uuid = "iloveloona"
    // description_meta = "lalala"
    // [[tracks]]
    // uuid = "ghost"
    // description_meta = "lalala {MISSING}"
    // missing = true
    // """
    //         )
    //     update_cache(config)
    //
    //     # Assert that adding another track works.
    //     add_track_to_playlist(config, "You & Me", "ilovetwice")
    //     with (source_dir / "!playlists" / "You & Me.toml").open("rb") as fp:
    //         diskdata = tomllib.load(fp)
    //         assert {r["uuid"] for r in diskdata["tracks"]} == {"ghost", "iloveloona", "ilovetwice"}
    //         assert next(r for r in diskdata["tracks"] if r["uuid"] == "ghost")["missing"]
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'")
    //         assert {r["track_id"] for r in cursor} == {"ghost", "iloveloona", "ilovetwice"}
    //
    //     # Delete that track.
    //     remove_track_from_playlist(config, "You & Me", "ilovetwice")
    //     with filepath.open("rb") as fp:
    //         diskdata = tomllib.load(fp)
    //         assert {r["uuid"] for r in diskdata["tracks"]} == {"ghost", "iloveloona"}
    //         assert next(r for r in diskdata["tracks"] if r["uuid"] == "ghost")["missing"]
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'")
    //         assert {r["track_id"] for r in cursor} == {"ghost", "iloveloona"}
    //
    //     # And delete the playlist.
    //     delete_playlist(config, "You & Me")
    //     assert not filepath.is_file()
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT EXISTS(SELECT * FROM playlists WHERE name = 'You & Me')")
    //         assert not cursor.fetchone()[0]
    #[test]
    fn test_playlist_handle_missing_track() {
        let (config, _temp_dir) = testing::seeded_cache();
        let source_dir = &config.music_source_dir;
        let playlists_dir = source_dir.join("!playlists");
        fs::create_dir_all(&playlists_dir).unwrap();

        let filepath = playlists_dir.join("You & Me.toml");
        fs::write(
            &filepath,
            r#"[[tracks]]
uuid = "t1"
description_meta = "lalala"
[[tracks]]
uuid = "ghost"
description_meta = "lalala {MISSING}"
missing = true
"#,
        )
        .unwrap();

        // Don't update cache so we keep the original track IDs
        // update_cache(&config, false, false).unwrap();

        // Assert that adding another track works using the seeded track ID
        add_track_to_playlist(&config, "You & Me", "t2").unwrap();

        let contents = fs::read_to_string(&filepath).unwrap();
        let data: PlaylistData = toml::from_str(&contents).unwrap();
        let uuids: std::collections::HashSet<_> = data.tracks.iter().map(|t| t.uuid.as_str()).collect();
        // Check that we have the ghost track, t1, and t2
        assert_eq!(uuids, ["ghost", "t1", "t2"].iter().copied().collect());

        let ghost_track = data.tracks.iter().find(|t| t.uuid == "ghost").unwrap();
        assert_eq!(ghost_track.missing, Some(true));

        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'").unwrap();
        let track_ids: std::collections::HashSet<String> =
            stmt.query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<std::collections::HashSet<_>>>().unwrap();
        assert_eq!(track_ids, ["ghost", "t1", "t2"].iter().map(|s| s.to_string()).collect());
        drop(stmt);
        drop(conn);

        // Delete that track.
        remove_track_from_playlist(&config, "You & Me", "t2").unwrap();

        let contents = fs::read_to_string(&filepath).unwrap();
        let data: PlaylistData = toml::from_str(&contents).unwrap();
        let uuids: std::collections::HashSet<_> = data.tracks.iter().map(|t| t.uuid.as_str()).collect();
        assert_eq!(uuids, ["ghost", "t1"].iter().copied().collect());

        let ghost_track = data.tracks.iter().find(|t| t.uuid == "ghost").unwrap();
        assert_eq!(ghost_track.missing, Some(true));

        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT track_id FROM playlists_tracks WHERE playlist_name = 'You & Me'").unwrap();
        let track_ids: std::collections::HashSet<String> =
            stmt.query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<std::collections::HashSet<_>>>().unwrap();
        assert_eq!(track_ids, ["ghost", "t1"].iter().map(|s| s.to_string()).collect());
        drop(stmt);
        drop(conn);

        // And delete the playlist.
        delete_playlist(&config, "You & Me").unwrap();
        assert!(!filepath.is_file());

        let conn = connect(&config).unwrap();
        let exists: bool = conn.query_row("SELECT EXISTS(SELECT * FROM playlists WHERE name = 'You & Me')", [], |row| row.get(0)).unwrap();
        assert!(!exists);
    }

    // def test_set_playlist_cover_art(isolated_dir: Path, config: Config) -> None:
    //     imagepath = isolated_dir / "folder.png"
    //     with imagepath.open("w") as fp:
    //         fp.write("lalala")
    //
    //     playlists_dir = config.music_source_dir / "!playlists"
    //     shutil.copytree(TEST_PLAYLIST_1, playlists_dir)
    //     (playlists_dir / "Turtle Rabbit.toml").touch()
    //     (playlists_dir / "Turtle Rabbit.jpg").touch()
    //     (playlists_dir / "Lala Lisa.txt").touch()
    //     update_cache(config)
    //
    //     set_playlist_cover_art(config, "Lala Lisa", imagepath)
    //     assert (playlists_dir / "Lala Lisa.png").is_file()
    //     assert not (playlists_dir / "Lala Lisa.jpg").exists()
    //     assert (playlists_dir / "Lala Lisa.txt").is_file()
    //     assert len(list(playlists_dir.iterdir())) == 5
    //
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT cover_path FROM playlists WHERE name = 'Lala Lisa'")
    //         assert Path(cursor.fetchone()["cover_path"]) == playlists_dir / "Lala Lisa.png"
    #[test]
    fn test_set_playlist_cover_art() {
        let (config, temp_dir) = testing::config();
        let imagepath = temp_dir.path().join("folder.png");
        fs::write(&imagepath, "lalala").unwrap();

        let playlists_dir = config.music_source_dir.join("!playlists");

        // Create test playlist directory structure
        let testdata_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
        let test_playlist_dir = testdata_dir.join("Playlist 1");
        testing::copy_dir_all(&test_playlist_dir, &playlists_dir).unwrap();

        // Create additional files
        fs::File::create(playlists_dir.join("Turtle Rabbit.toml")).unwrap();
        fs::File::create(playlists_dir.join("Turtle Rabbit.jpg")).unwrap();
        fs::File::create(playlists_dir.join("Lala Lisa.txt")).unwrap();

        update_cache(&config, false, false).unwrap();

        set_playlist_cover_art(&config, "Lala Lisa", &imagepath).unwrap();

        assert!(playlists_dir.join("Lala Lisa.png").is_file());
        assert!(!playlists_dir.join("Lala Lisa.jpg").exists());
        assert!(playlists_dir.join("Lala Lisa.txt").is_file());
        assert_eq!(fs::read_dir(&playlists_dir).unwrap().count(), 5);

        let conn = connect(&config).unwrap();
        let cover_path: Option<String> = conn.query_row("SELECT cover_path FROM playlists WHERE name = 'Lala Lisa'", [], |row| row.get(0)).unwrap();
        assert_eq!(cover_path.map(PathBuf::from), Some(playlists_dir.join("Lala Lisa.png")));
    }

    // def test_remove_playlist_cover_art(config: Config) -> None:
    //     playlists_dir = config.music_source_dir / "!playlists"
    //     playlists_dir.mkdir()
    //     (playlists_dir / "Turtle Rabbit.toml").touch()
    //     (playlists_dir / "Turtle Rabbit.jpg").touch()
    //     update_cache(config)
    //
    //     delete_playlist_cover_art(config, "Turtle Rabbit")
    //     assert not (playlists_dir / "Turtle Rabbit.jpg").exists()
    //     with connect(config) as conn:
    //         cursor = conn.execute("SELECT cover_path FROM playlists")
    //         assert not cursor.fetchone()["cover_path"]
    #[test]
    fn test_remove_playlist_cover_art() {
        let (config, _temp_dir) = testing::config();
        let playlists_dir = config.music_source_dir.join("!playlists");
        fs::create_dir_all(&playlists_dir).unwrap();

        fs::File::create(playlists_dir.join("Turtle Rabbit.toml")).unwrap();
        fs::File::create(playlists_dir.join("Turtle Rabbit.jpg")).unwrap();

        update_cache(&config, false, false).unwrap();

        // Verify the cover is initially present in the cache
        let conn = connect(&config).unwrap();
        let initial_cover_path: Option<String> =
            conn.query_row("SELECT cover_path FROM playlists WHERE name = ?", ["Turtle Rabbit"], |row| row.get(0)).unwrap();
        assert_eq!(initial_cover_path.map(PathBuf::from), Some(playlists_dir.join("Turtle Rabbit.jpg")));
        drop(conn);

        delete_playlist_cover_art(&config, "Turtle Rabbit").unwrap();

        assert!(!playlists_dir.join("Turtle Rabbit.jpg").exists());

        let conn = connect(&config).unwrap();
        let cover_path: Option<String> = conn.query_row("SELECT cover_path FROM playlists WHERE name = ?", ["Turtle Rabbit"], |row| row.get(0)).unwrap();
        assert_eq!(cover_path, None);
    }
}
