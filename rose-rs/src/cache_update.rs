// Cache update logic migrated from Python cache.py

use crate::audiotags::{AudioTags, SUPPORTED_AUDIO_EXTENSIONS};
use crate::cache::{
    cached_release_from_view, cached_track_from_view, collage_lock_name, connect,
    playlist_lock_name, process_string_for_fts, release_lock_name, unlock, CachedCollage,
    CachedPlaylist, CachedRelease, CachedTrack, STORED_DATA_FILE_REGEX,
    SQL_ARRAY_DELIMITER,
};
use crate::common::{Artist, ArtistMapping};
use crate::config::Config;
use crate::datafiles::{find_release_datafile, read_or_create_datafile};
use crate::error::{Result, RoseError};
use crate::genre_hierarchy::get_transitive_parent_genres;
use chrono::{DateTime, Utc};
use rayon::prelude::*;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};
use uuid::Uuid;

// Python: def update_cache(
//     c: Config,
//     force: bool = False,
//     # For testing.
//     force_multiprocessing: bool = False,
// ) -> None:
pub fn update_cache(config: &Config, force: bool) -> Result<()> {
    // """
    // Update the read cache to match the data for all releases in the music source directory. Delete
    // any cached releases that are no longer present on disk.
    // """
    update_cache_for_releases(config, None, force)?;
    update_cache_evict_nonexistent_releases(config)?;
    update_cache_for_collages(config, None, force)?;
    update_cache_evict_nonexistent_collages(config)?;
    update_cache_for_playlists(config, None, force)?;
    update_cache_evict_nonexistent_playlists(config)?;
    Ok(())
}

// Python: def update_cache_evict_nonexistent_releases(c: Config) -> None:
pub fn update_cache_evict_nonexistent_releases(config: &Config) -> Result<()> {
    debug!("Evicting cached releases that are not on disk");
    
    let mut dirs = Vec::new();
    for entry in fs::read_dir(&config.music_source_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            dirs.push(entry.path().canonicalize()?);
        }
    }
    
    let conn = connect(config)?;
    
    // Python: cursor = conn.execute(
    //     f"""
    //     DELETE FROM releases
    //     WHERE source_path NOT IN ({",".join(["?"] * len(dirs))})
    //     RETURNING source_path
    //     """,
    //     [str(d) for d in dirs],
    // )
    let placeholders = vec!["?"; dirs.len()].join(",");
    let sql = format!(
        "DELETE FROM releases WHERE source_path NOT IN ({}) RETURNING source_path",
        placeholders
    );
    
    let mut stmt = conn.prepare(&sql)?;
    let dir_strs: Vec<String> = dirs.iter().map(|d| d.to_string_lossy().to_string()).collect();
    let params: Vec<&dyn rusqlite::ToSql> = dir_strs.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    
    let mut rows = stmt.query(&params[..])?;
    while let Some(row) = rows.next()? {
        let source_path: String = row.get(0)?;
        info!("Evicted missing release {} from cache", source_path);
    }
    
    Ok(())
}

// Python: def update_cache_for_releases(
//     c: Config,
//     # Leave as None to update all releases.
//     release_dirs: list[Path] | None = None,
//     force: bool = False,
//     # For testing.
//     force_multiprocessing: bool = False,
// ) -> None:
pub fn update_cache_for_releases(
    config: &Config,
    release_dirs: Option<Vec<PathBuf>>,
    force: bool,
) -> Result<()> {
    // """
    // Update the read cache to match the data for any passed-in releases. If a directory lacks a
    // .rose.{uuid}.toml datafile, create the datafile for the release and set it to the initial state.
    //
    // This is a hot path and is thus performance-optimized. The bottleneck is disk accesses, so we
    // structure this function in order to minimize them. We solely read files that have changed since
    // last run and batch writes together. We trade higher memory for reduced disk accesses.
    // Concretely, we:
    //
    // 1. Execute one big SQL query at the start to fetch the relevant previous caches.
    // 2. Skip reading a file's data if the mtime has not changed since the previous cache update.
    // 3. Batch SQLite write operations to the end of this function, and only execute a SQLite upsert
    //    if the read data differs from the previous caches.
    //
    // We also shard the directories across multiple processes and execute them simultaneously.
    // """
    
    let release_dirs = match release_dirs {
        Some(dirs) => dirs,
        None => {
            let mut dirs = Vec::new();
            for entry in fs::read_dir(&config.music_source_dir)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    dirs.push(entry.path());
                }
            }
            dirs
        }
    };
    
    // Python: release_dirs = [
    //     d
    //     for d in release_dirs
    //     if d.name != "!collages" and d.name != "!playlists" and d.name not in c.ignore_release_directories
    // ]
    let release_dirs: Vec<PathBuf> = release_dirs
        .into_iter()
        .filter(|d| {
            if let Some(name) = d.file_name().and_then(|n| n.to_str()) {
                name != "!collages" 
                    && name != "!playlists" 
                    && !config.ignore_release_directories.contains(&name.to_string())
            } else {
                true
            }
        })
        .collect();
        
    if release_dirs.is_empty() {
        debug!("No-Op: No whitelisted releases passed into update_cache_for_releases");
        return Ok(());
    }
    
    debug!("Refreshing the read cache for {} releases", release_dirs.len());
    if release_dirs.len() < 10 {
        let names: Vec<String> = release_dirs
            .iter()
            .filter_map(|r| r.file_name().and_then(|n| n.to_str()).map(String::from))
            .collect();
        debug!("Refreshing cached data for {}", names.join(", "));
    }
    
    // Python: If the number of releases changed is less than 50; do not bother with all that multiprocessing
    // gunk: instead, directly call the executor.
    //
    // This has an added benefit of not spawning processes from the virtual filesystem and watchdog
    // processes, as those processes always update the cache for one release at a time and are
    // multithreaded. Starting other processes from threads is bad!
    if release_dirs.len() < 50 {
        debug!("Running cache update executor in same process because len={} < 50", release_dirs.len());
        _update_cache_for_releases_executor(config, release_dirs, force)?;
        return Ok(());
    }
    
    // For larger numbers, use rayon for parallel processing
    let collages_to_update = Arc::new(Mutex::new(Vec::new()));
    let playlists_to_update = Arc::new(Mutex::new(Vec::new()));
    
    // Split work across threads
    let batch_size = (release_dirs.len() / num_cpus::get()).max(50);
    
    let results: Vec<Result<()>> = release_dirs
        .par_chunks(batch_size)
        .map(|batch| {
            _update_cache_for_releases_executor_with_receivers(
                config,
                batch.to_vec(),
                force,
                collages_to_update.clone(),
                playlists_to_update.clone(),
            )
        })
        .collect();
    
    // Check for errors
    for result in results {
        result?;
    }
    
    // Update collages and playlists if needed
    let collages = collages_to_update.lock().unwrap();
    if !collages.is_empty() {
        update_cache_for_collages(config, Some(collages.clone()), true)?;
    }
    
    let playlists = playlists_to_update.lock().unwrap();
    if !playlists.is_empty() {
        update_cache_for_playlists(config, Some(playlists.clone()), true)?;
    }
    
    Ok(())
}

fn _update_cache_for_releases_executor(
    config: &Config,
    release_dirs: Vec<PathBuf>,
    force: bool,
) -> Result<()> {
    _update_cache_for_releases_executor_with_receivers(
        config,
        release_dirs,
        force,
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(Vec::new())),
    )
}

fn _update_cache_for_releases_executor_with_receivers(
    config: &Config,
    release_dirs: Vec<PathBuf>,
    force: bool,
    collages_to_force_update_receiver: Arc<Mutex<Vec<String>>>,
    playlists_to_force_update_receiver: Arc<Mutex<Vec<String>>>,
) -> Result<()> {
    use crate::cache::{Release, Track, lock, unlock};
    
    // First, call readdir on every release directory
    let dir_scan_start = Instant::now();
    let mut dir_tree: Vec<(PathBuf, Option<String>, Vec<PathBuf>)> = Vec::new();
    let mut release_uuids: Vec<String> = Vec::new();
    let release_dirs_count = release_dirs.len();
    
    for rd in release_dirs {
        let mut release_id = None;
        let mut files: Vec<PathBuf> = Vec::new();
        
        if !rd.is_dir() {
            debug!("Skipping scanning {:?} because it is not a directory", rd);
            continue;
        }
        
        // Walk the directory tree
        for entry in walkdir::WalkDir::new(&rd).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let filename = entry.file_name().to_string_lossy();
                if let Some(captures) = STORED_DATA_FILE_REGEX.captures(&filename) {
                    if let Some(uuid_str) = captures.get(1) {
                        release_id = Some(uuid_str.as_str().to_string());
                    }
                }
                files.push(entry.path().to_path_buf());
            }
        }
        
        // Force a deterministic file sort order
        files.sort();
        dir_tree.push((rd.canonicalize()?, release_id.clone(), files));
        if let Some(id) = release_id {
            release_uuids.push(id);
        }
    }
    
    debug!("Release update source dir scan time {:?}", dir_scan_start.elapsed());
    
    // Then batch query for all metadata associated with the discovered IDs
    let cache_read_start = Instant::now();
    let mut cached_releases: HashMap<String, (Release, HashMap<String, Track>)> = HashMap::new();
    
    {
        let conn = connect(config)?;
        
        // Fetch all releases
        if !release_uuids.is_empty() {
            let placeholders = vec!["?"; release_uuids.len()].join(",");
            let sql = format!(
                "SELECT * FROM releases_view WHERE id IN ({})",
                placeholders
            );
            
            let mut stmt = conn.prepare(&sql)?;
            let params: Vec<&dyn rusqlite::ToSql> = release_uuids
                .iter()
                .map(|s| s as &dyn rusqlite::ToSql)
                .collect();
                
            let rows = stmt.query_map(&params[..], |row| {
                cached_release_from_view(config, row, false).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
            })?;
            
            for row in rows {
                let release = row?;
                let release_mutable = Release {
                    id: release.id.clone(),
                    source_path: release.source_path.clone(),
                    cover_image_path: release.cover_image_path.clone(),
                    added_at: release.added_at.clone(),
                    datafile_mtime: release.datafile_mtime.clone(),
                    releasetitle: release.releasetitle.clone(),
                    releasetype: release.releasetype.clone(),
                    releasedate: release.releasedate.clone(),
                    originaldate: release.originaldate.clone(),
                    compositiondate: release.compositiondate.clone(),
                    edition: release.edition.clone(),
                    catalognumber: release.catalognumber.clone(),
                    new: release.new,
                    disctotal: release.disctotal,
                    genres: release.genres.clone(),
                    parent_genres: release.parent_genres.clone(),
                    secondary_genres: release.secondary_genres.clone(),
                    parent_secondary_genres: release.parent_secondary_genres.clone(),
                    descriptors: release.descriptors.clone(),
                    labels: release.labels.clone(),
                    releaseartists: release.releaseartists.clone(),
                    metahash: release.metahash.clone(),
                };
                cached_releases.insert(release.id.clone(), (release_mutable, HashMap::new()));
            }
            
            debug!("Found {}/{} releases in cache", cached_releases.len(), release_dirs_count);
            
            // Fetch all tracks
            let sql = format!(
                "SELECT * FROM tracks_view WHERE release_id IN ({})",
                placeholders
            );
            
            let mut stmt = conn.prepare(&sql)?;
            let mut num_tracks_found = 0;
            
            // Read and convert tracks in a simpler way
            let mut rows = stmt.query(&params[..])?;
            
            while let Some(row) = rows.next()? {
                let release_id: String = row.get("release_id")?;
                let source_path: String = row.get("source_path")?;
                
                // Skip if we don't have this release cached
                let Some(cached_release) = cached_releases.get(&release_id) else {
                    continue;
                };
                
                // Create a CachedRelease for the track
                let cached_release_for_track = CachedRelease {
                    id: cached_release.0.id.clone(),
                    source_path: cached_release.0.source_path.clone(),
                    cover_image_path: cached_release.0.cover_image_path.clone(),
                    added_at: cached_release.0.added_at.clone(),
                    datafile_mtime: cached_release.0.datafile_mtime.clone(),
                    releasetitle: cached_release.0.releasetitle.clone(),
                    releasetype: cached_release.0.releasetype.clone(),
                    releasedate: cached_release.0.releasedate.clone(),
                    originaldate: cached_release.0.originaldate.clone(),
                    compositiondate: cached_release.0.compositiondate.clone(),
                    edition: cached_release.0.edition.clone(),
                    catalognumber: cached_release.0.catalognumber.clone(),
                    new: cached_release.0.new,
                    disctotal: cached_release.0.disctotal,
                    genres: cached_release.0.genres.clone(),
                    parent_genres: cached_release.0.parent_genres.clone(),
                    secondary_genres: cached_release.0.secondary_genres.clone(),
                    parent_secondary_genres: cached_release.0.parent_secondary_genres.clone(),
                    descriptors: cached_release.0.descriptors.clone(),
                    labels: cached_release.0.labels.clone(),
                    releaseartists: cached_release.0.releaseartists.clone(),
                    metahash: cached_release.0.metahash.clone(),
                };
                
                let track = cached_track_from_view(config, row, cached_release_for_track, false)?;
                
                // Convert CachedTrack to Track
                let track_simple = Track {
                    id: track.id,
                    source_path: track.source_path,
                    source_mtime: track.source_mtime,
                    tracktitle: track.tracktitle,
                    tracknumber: track.tracknumber,
                    tracktotal: track.tracktotal,
                    discnumber: track.discnumber,
                    duration_seconds: track.duration_seconds,
                    trackartists: track.trackartists,
                    metahash: track.metahash,
                };
                
                // Insert the track
                if let Some((_, tracks)) = cached_releases.get_mut(&release_id) {
                    tracks.insert(source_path, track_simple);
                    num_tracks_found += 1;
                }
            }
            
            debug!("Found {} tracks in cache", num_tracks_found);
        }
    }
    
    debug!("Release update cache read time {:?}", cache_read_start.elapsed());
    
    // Now iterate over all releases in the source directory
    let loop_start = Instant::now();
    let mut upd_delete_source_paths: Vec<String> = Vec::new();
    let mut upd_release_args: Vec<Vec<Box<dyn rusqlite::ToSql>>> = Vec::new();
    let mut upd_release_ids: Vec<String> = Vec::new();
    let mut upd_release_artist_args: Vec<Vec<Box<dyn rusqlite::ToSql>>> = Vec::new();
    let mut upd_release_genre_args: Vec<Vec<Box<dyn rusqlite::ToSql>>> = Vec::new();
    let mut upd_release_secondary_genre_args: Vec<Vec<Box<dyn rusqlite::ToSql>>> = Vec::new();
    let mut upd_release_descriptor_args: Vec<Vec<Box<dyn rusqlite::ToSql>>> = Vec::new();
    let mut upd_release_label_args: Vec<Vec<Box<dyn rusqlite::ToSql>>> = Vec::new();
    let mut upd_unknown_cached_tracks_args: Vec<(String, Vec<String>)> = Vec::new();
    let mut upd_track_args: Vec<Vec<Box<dyn rusqlite::ToSql>>> = Vec::new();
    let mut upd_track_ids: Vec<String> = Vec::new();
    let mut upd_track_artist_args: Vec<Vec<Box<dyn rusqlite::ToSql>>> = Vec::new();
    
    // TODO: Continue implementing the main processing loop
    // This is a large and complex function that processes each release directory
    // For now, we'll stop here and continue in the next iteration
    
    debug!("Release update scheduling loop time {:?}", loop_start.elapsed());
    
    Ok(())
}

// Placeholder implementations for collage and playlist functions
pub fn update_cache_for_collages(
    _config: &Config,
    _collage_names: Option<Vec<String>>,
    _force: bool,
) -> Result<()> {
    // TODO: Implement
    Ok(())
}

pub fn update_cache_evict_nonexistent_collages(_config: &Config) -> Result<()> {
    // TODO: Implement
    Ok(())
}

pub fn update_cache_for_playlists(
    _config: &Config,
    _playlist_names: Option<Vec<String>>,
    _force: bool,
) -> Result<()> {
    // TODO: Implement
    Ok(())
}

pub fn update_cache_evict_nonexistent_playlists(_config: &Config) -> Result<()> {
    // TODO: Implement
    Ok(())
}