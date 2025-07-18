/// The cache module encapsulates the read cache and exposes handles for working with the read cache. It
/// also exposes a locking mechanism that uses the read cache for synchronization.
///
/// The SQLite database is considered part of the cache, and so this module encapsulates the SQLite
/// database too.
use crate::audiotags::{AudioTags, SUPPORTED_AUDIO_EXTENSIONS};
use crate::common::{sanitize_dirname, sanitize_filename, Artist, ArtistMapping, RoseDate, VERSION};
use crate::config::Config;
use crate::errors::{Result, RoseError};
use crate::genre_hierarchy::TRANSITIVE_PARENT_GENRES;
use crate::templates::{evaluate_release_template, evaluate_track_template};
use once_cell::sync::Lazy;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};
use uuid::Uuid;

static CACHE_SCHEMA: &str = include_str!("cache.sql");

static STORED_DATA_FILE_REGEX: Lazy<regex::Regex> = Lazy::new(|| regex::Regex::new(r"^\.rose\.([^.]+)\.toml$").unwrap());

/// Connect to the SQLite database with appropriate settings
pub fn connect(c: &Config) -> Result<Connection> {
    let conn = Connection::open(c.cache_database_path())?;
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = WAL;
        PRAGMA busy_timeout = 15000;
        ",
    )?;
    Ok(conn)
}

/// "Migrate" the database. If the schema in the database does not match that on disk, then nuke the
/// database and recreate it from scratch. Otherwise, no op.
///
/// We can do this because the database is just a read cache. It is not source-of-truth for any of
/// its own data.
pub fn maybe_invalidate_cache_database(c: &Config) -> Result<()> {
    debug!("maybe_invalidate_cache_database called with cache db path: {:?}", c.cache_database_path());
    // Calculate schema hash
    let mut hasher = Sha256::new();
    hasher.update(CACHE_SCHEMA.as_bytes());
    let schema_hash = format!("{:x}", hasher.finalize());

    // Hash a subset of the config fields to use as the cache hash, which invalidates the cache on
    // change. These are the fields that affect cache population. Invalidating the cache on config
    // change ensures that the cache is consistent with the config.
    let config_hash_fields = serde_json::json!({
        "music_source_dir": c.music_source_dir.to_string_lossy(),
        "cache_dir": c.cache_dir.to_string_lossy(),
        "cover_art_stems": c.cover_art_stems,
        "valid_art_exts": c.valid_art_exts,
        "ignore_release_directories": c.ignore_release_directories,
    });
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_string(&config_hash_fields)?.as_bytes());
    let config_hash = format!("{:x}", hasher.finalize());

    {
        let conn = connect(c)?;
        let exists: bool = conn.query_row(
            "SELECT EXISTS(
                SELECT * FROM sqlite_master
                WHERE type = 'table' AND name = '_schema_hash'
            )",
            [],
            |row| row.get(0),
        )?;

        if exists {
            let result: Option<(String, String, String)> = conn
                .query_row("SELECT schema_hash, config_hash, version FROM _schema_hash", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
                .optional()?;

            if let Some((db_schema_hash, db_config_hash, db_version)) = result {
                if db_schema_hash == schema_hash && db_config_hash == config_hash && db_version == VERSION {
                    // Everything matches! Exit!
                    return Ok(());
                }
            }
        }
    }

    // Delete the existing database
    if c.cache_database_path().exists() {
        debug!("deleting existing database due to schema/config/version mismatch");
        fs::remove_file(c.cache_database_path())?;
    }

    // Create new database with schema
    let conn = connect(c)?;
    conn.execute_batch(CACHE_SCHEMA)?;
    conn.execute_batch(
        "
        CREATE TABLE _schema_hash (
            schema_hash TEXT
          , config_hash TEXT
          , version TEXT
          , PRIMARY KEY (schema_hash, config_hash, version)
        )
        ",
    )?;
    conn.execute("INSERT INTO _schema_hash (schema_hash, config_hash, version) VALUES (?1, ?2, ?3)", params![schema_hash, config_hash, VERSION])?;

    Ok(())
}

/// Lock struct that automatically releases the lock when dropped
pub struct Lock<'a> {
    config: &'a Config,
    name: String,
}

impl<'a> Drop for Lock<'a> {
    fn drop(&mut self) {
        debug!("Releasing lock {}", self.name);
        if let Ok(conn) = connect(self.config) {
            let _ = conn.execute("DELETE FROM locks WHERE name = ?1", params![self.name]);
        }
    }
}

/// Acquire a lock using the database
pub fn lock<'a>(c: &'a Config, name: &str, timeout: f64) -> Result<Lock<'a>> {
    loop {
        let conn = connect(c)?;
        let max_valid_until: Option<f64> =
            conn.query_row("SELECT MAX(valid_until) FROM locks WHERE name = ?1", params![name], |row| row.get::<_, Option<f64>>(0)).unwrap_or(None);

        // If a lock exists, sleep until the lock is available. All locks should be very
        // short lived, so this shouldn't be a big performance penalty.
        if let Some(valid_until) = max_valid_until {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64();
            if valid_until > now {
                let sleep_duration = Duration::from_secs_f64((valid_until - now).max(0.0));
                debug!("Failed to acquire lock for {}: sleeping for {:?}", name, sleep_duration);
                std::thread::sleep(sleep_duration);
                continue;
            }
        }

        debug!("Attempting to acquire lock for {} with timeout {}", name, timeout);
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64();
        let valid_until = now + timeout;

        match conn.execute("INSERT INTO locks (name, valid_until) VALUES (?1, ?2)", params![name, valid_until]) {
            Ok(_) => {
                debug!("Successfully acquired lock for {} with timeout {} until {}", name, timeout, valid_until);
                return Ok(Lock {
                    config: c,
                    name: name.to_string(),
                });
            }
            Err(rusqlite::Error::SqliteFailure(err, _)) if err.code == rusqlite::ErrorCode::ConstraintViolation => {
                debug!("Failed to acquire lock for {}, trying again", name);
                continue;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

pub fn release_lock_name(release_id: &str) -> String {
    format!("release-{release_id}")
}

pub fn collage_lock_name(collage_name: &str) -> String {
    format!("collage-{collage_name}")
}

pub fn playlist_lock_name(playlist_name: &str) -> String {
    format!("playlist-{playlist_name}")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub id: String,
    pub source_path: PathBuf,
    pub cover_image_path: Option<PathBuf>,
    pub added_at: String, // ISO8601 timestamp
    pub datafile_mtime: String,
    pub releasetitle: String,
    pub releasetype: String,
    pub releasedate: Option<RoseDate>,
    pub originaldate: Option<RoseDate>,
    pub compositiondate: Option<RoseDate>,
    pub edition: Option<String>,
    pub catalognumber: Option<String>,
    pub new: bool,
    pub disctotal: i32,
    pub genres: Vec<String>,
    pub parent_genres: Vec<String>,
    pub secondary_genres: Vec<String>,
    pub parent_secondary_genres: Vec<String>,
    pub descriptors: Vec<String>,
    pub labels: Vec<String>,
    pub releaseartists: ArtistMapping,
    pub metahash: String,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub id: String,
    pub source_path: PathBuf,
    pub source_mtime: String,
    pub tracktitle: String,
    pub tracknumber: String,
    pub tracktotal: i32,
    pub discnumber: String,
    pub duration_seconds: i32,
    pub trackartists: ArtistMapping,
    pub metahash: String,
    pub release: Arc<Release>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collage {
    pub name: String,
    pub source_mtime: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub name: String,
    pub source_mtime: String,
    pub cover_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenreEntry {
    pub name: String,
    pub only_new_releases: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DescriptorEntry {
    pub name: String,
    pub only_new_releases: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LabelEntry {
    pub name: String,
    pub only_new_releases: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDataFile {
    #[serde(default = "default_true")]
    pub new: bool,
    #[serde(default = "default_added_at", deserialize_with = "deserialize_datetime_as_string")]
    pub added_at: String,
}

/// Custom deserializer that handles both String and TOML datetime values
fn deserialize_datetime_as_string<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Deserialize;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrDatetime {
        String(String),
        Datetime(toml::value::Datetime),
    }

    match StringOrDatetime::deserialize(deserializer)? {
        StringOrDatetime::String(s) => Ok(s),
        StringOrDatetime::Datetime(dt) => Ok(dt.to_string()),
    }
}

fn default_true() -> bool {
    true
}

fn default_added_at() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

// Conversion functions for template evaluation
impl Release {
    fn to_template_release(&self) -> crate::templates::Release {
        crate::templates::Release {
            id: self.id.clone(),
            source_path: self.source_path.clone(),
            cover_image_path: self.cover_image_path.clone(),
            added_at: self.added_at.clone(),
            datafile_mtime: self.datafile_mtime.clone(),
            releasetitle: self.releasetitle.clone(),
            releasetype: self.releasetype.clone(),
            releasedate: self.releasedate,
            originaldate: self.originaldate,
            compositiondate: self.compositiondate,
            edition: self.edition.clone(),
            catalognumber: self.catalognumber.clone(),
            new: self.new,
            disctotal: self.disctotal as u32,
            genres: self.genres.clone(),
            parent_genres: self.parent_genres.clone(),
            secondary_genres: self.secondary_genres.clone(),
            parent_secondary_genres: self.parent_secondary_genres.clone(),
            descriptors: self.descriptors.clone(),
            labels: self.labels.clone(),
            releaseartists: self.releaseartists.clone(),
            metahash: self.metahash.clone(),
        }
    }
}

impl Track {
    fn to_template_track(&self) -> crate::templates::Track {
        crate::templates::Track {
            id: self.id.clone(),
            source_path: self.source_path.clone(),
            source_mtime: self.source_mtime.clone(),
            tracktitle: self.tracktitle.clone(),
            tracknumber: self.tracknumber.clone(),
            tracktotal: self.tracktotal as u32,
            discnumber: self.discnumber.clone(),
            duration_seconds: self.duration_seconds as u32,
            trackartists: self.trackartists.clone(),
            metahash: self.metahash.clone(),
            release: self.release.to_template_release(),
        }
    }
}

/// Calculate SHA256 hash of a struct's fields (for metadata comparison)
fn sha256_struct<T: Serialize>(value: &T) -> Result<String> {
    let json = serde_json::to_string(value)?;
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    Ok(format!("{:x}", hasher.finalize()))
}

/// Read a stored data file from disk
fn read_stored_data_file(path: &Path) -> Result<StoredDataFile> {
    let content = fs::read_to_string(path)?;
    let data: StoredDataFile = toml::from_str(&content)?;
    Ok(data)
}

/// Write a stored data file to disk
fn write_stored_data_file(path: &Path, data: &StoredDataFile) -> Result<()> {
    let content = toml::to_string(data)?;
    fs::write(path, content)?;
    Ok(())
}

/// Split the stringly-encoded arrays from the database by the sentinel character.
fn _split(xs: &str) -> Vec<String> {
    if xs.is_empty() {
        Vec::new()
    } else {
        xs.split(" ¬ ").map(|s| s.to_string()).collect()
    }
}

/// Unpack an arbitrary number of strings, each of which is a " ¬ "-delimited list in actuality,
/// and zip them together. This is how we extract certain array fields from the database.
fn _unpack<'a>(xxs: &'a [&'a str]) -> Vec<Vec<&'a str>> {
    let mut result = Vec::new();
    let split_lists: Vec<Vec<&str>> = xxs.iter().map(|xs| if xs.is_empty() { Vec::new() } else { xs.split(" ¬ ").collect() }).collect();

    if split_lists.is_empty() {
        return result;
    }

    let max_len = split_lists.iter().map(|l| l.len()).max().unwrap_or(0);
    for i in 0..max_len {
        let mut row = Vec::new();
        for list in &split_lists {
            if i < list.len() {
                row.push(list[i]);
            } else {
                row.push("");
            }
        }
        result.push(row);
    }
    result
}

/// Process a string for full-text search by inserting separators between characters
fn process_string_for_fts(s: &str) -> String {
    if s.is_empty() {
        s.to_string()
    } else {
        // Join each character with the separator "¬"
        s.chars().map(|c| c.to_string()).collect::<Vec<_>>().join("¬")
    }
}

/// Unicode normalize strings before comparison to avoid OS-specific issues
fn _compare_strs(a: &str, b: &str) -> bool {
    use unicode_normalization::UnicodeNormalization;
    a.nfc().collect::<String>() == b.nfc().collect::<String>()
}

/// Get parent genres for a list of genres
fn _get_parent_genres(genres: &[String]) -> Vec<String> {
    let mut result = HashSet::new();
    for g in genres {
        if let Some(parents) = TRANSITIVE_PARENT_GENRES.get(g) {
            result.extend(parents.iter().cloned());
        }
    }
    let mut vec: Vec<String> = result.into_iter().collect();
    vec.sort();
    vec
}

/// Unpack artists from database format
fn _unpack_artists(c: &Config, names: &str, roles: &str, aliases: bool) -> ArtistMapping {
    let mut mapping = ArtistMapping::default();
    let mut seen: HashSet<(String, String)> = HashSet::new();

    let args = [names, roles];
    let unpacked = _unpack(&args);
    for row in unpacked {
        if row.len() < 2 {
            continue;
        }
        let name = row[0];
        let role = row[1];

        if name.is_empty() || role.is_empty() {
            continue;
        }

        let role_artists = match role {
            "main" => &mut mapping.main,
            "guest" => &mut mapping.guest,
            "remixer" => &mut mapping.remixer,
            "producer" => &mut mapping.producer,
            "composer" => &mut mapping.composer,
            "conductor" => &mut mapping.conductor,
            "djmixer" => &mut mapping.djmixer,
            _ => continue,
        };

        role_artists.push(Artist {
            name: name.to_string(),
            alias: false,
        });
        seen.insert((name.to_string(), role.to_string()));

        if !aliases {
            continue;
        }

        // Get all immediate and transitive artist aliases.
        let mut unvisited: HashSet<String> = HashSet::new();
        unvisited.insert(name.to_string());

        while let Some(cur) = unvisited.iter().next().cloned() {
            unvisited.remove(&cur);
            if let Some(parent_aliases) = c.artist_aliases_parents_map.get(&cur) {
                for alias in parent_aliases {
                    if !seen.contains(&(alias.clone(), role.to_string())) {
                        role_artists.push(Artist {
                            name: alias.clone(),
                            alias: true,
                        });
                        seen.insert((alias.clone(), role.to_string()));
                        unvisited.insert(alias.clone());
                    }
                }
            }
        }
    }

    mapping
}

pub fn cached_release_from_view(c: &Config, row: &Row, aliases: bool) -> Result<Release> {
    let secondary_genres = _split(&row.get::<_, String>("secondary_genres").unwrap_or_default());
    let genres = _split(&row.get::<_, String>("genres").unwrap_or_default());

    Ok(Release {
        id: row.get("id")?,
        source_path: PathBuf::from(row.get::<_, String>("source_path")?),
        cover_image_path: row.get::<_, Option<String>>("cover_image_path")?.map(PathBuf::from),
        added_at: row.get("added_at")?,
        datafile_mtime: row.get("datafile_mtime")?,
        releasetitle: row.get("releasetitle")?,
        releasetype: row.get("releasetype")?,
        releasedate: row.get::<_, Option<String>>("releasedate")?.and_then(|s| RoseDate::parse(Some(&s))),
        originaldate: row.get::<_, Option<String>>("originaldate")?.and_then(|s| RoseDate::parse(Some(&s))),
        compositiondate: row.get::<_, Option<String>>("compositiondate")?.and_then(|s| RoseDate::parse(Some(&s))),
        catalognumber: row.get("catalognumber")?,
        edition: row.get("edition")?,
        disctotal: row.get("disctotal")?,
        new: row.get::<_, i32>("new")? != 0,
        genres: genres.clone(),
        secondary_genres: secondary_genres.clone(),
        parent_genres: _get_parent_genres(&genres),
        parent_secondary_genres: _get_parent_genres(&secondary_genres),
        descriptors: _split(&row.get::<_, String>("descriptors").unwrap_or_default()),
        labels: _split(&row.get::<_, String>("labels").unwrap_or_default()),
        releaseartists: _unpack_artists(
            c,
            &row.get::<_, String>("releaseartist_names").unwrap_or_default(),
            &row.get::<_, String>("releaseartist_roles").unwrap_or_default(),
            aliases,
        ),
        metahash: row.get("metahash")?,
    })
}

pub fn cached_track_from_view(c: &Config, row: &Row, release: Arc<Release>, aliases: bool) -> Result<Track> {
    Ok(Track {
        id: row.get("id")?,
        source_path: PathBuf::from(row.get::<_, String>("source_path")?),
        source_mtime: row.get("source_mtime")?,
        tracktitle: row.get("tracktitle")?,
        tracknumber: row.get("tracknumber")?,
        tracktotal: row.get("tracktotal")?,
        discnumber: row.get("discnumber")?,
        duration_seconds: row.get("duration_seconds")?,
        trackartists: _unpack_artists(
            c,
            &row.get::<_, String>("trackartist_names").unwrap_or_default(),
            &row.get::<_, String>("trackartist_roles").unwrap_or_default(),
            aliases,
        ),
        metahash: row.get("metahash")?,
        release,
    })
}

/// Update the read cache to match the data for all releases in the music source directory. Delete
/// any cached releases that are no longer present on disk.
pub fn update_cache(
    c: &Config,
    force: bool,
    // For testing.
    force_multiprocessing: bool,
) -> Result<()> {
    update_cache_for_releases(c, None, force, force_multiprocessing)?;
    update_cache_evict_nonexistent_releases(c)?;
    update_cache_for_collages(c, None, force)?;
    update_cache_evict_nonexistent_collages(c)?;
    update_cache_for_playlists(c, None, force)?;
    update_cache_evict_nonexistent_playlists(c)?;
    Ok(())
}

/// Update the read cache to match the data for any passed-in releases. If a directory lacks a
/// .rose.{uuid}.toml datafile, create the datafile for the release and set it to the initial state.
///
/// This is a hot path and is thus performance-optimized. The bottleneck is disk accesses, so we
/// structure this function in order to minimize them. We solely read files that have changed since
/// last run and batch writes together. We trade higher memory for reduced disk accesses.
/// Concretely, we:
///
/// 1. Execute one big SQL query at the start to fetch the relevant previous caches.
/// 2. Skip reading a file's data if the mtime has not changed since the previous cache update.
/// 3. Batch SQLite write operations to the end of this function, and only execute a SQLite upsert
///    if the read data differs from the previous caches.
///
/// We also shard the directories across multiple processes and execute them simultaneously.
pub fn update_cache_for_releases(c: &Config, release_dirs: Option<Vec<PathBuf>>, force: bool, force_multiprocessing: bool) -> Result<()> {
    debug!("update_cache_for_releases called with cache db path: {:?}", c.cache_database_path());
    // Get release directories to process
    let release_dirs = if let Some(dirs) = release_dirs {
        dirs
    } else {
        // Scan music source directory for all subdirectories
        let mut dirs = Vec::new();
        for entry in fs::read_dir(&c.music_source_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                // Skip special directories and ignored directories
                if name != "!collages" && name != "!playlists" && !c.ignore_release_directories.contains(&name.to_string()) {
                    dirs.push(path);
                }
            }
        }
        dirs
    };

    if release_dirs.is_empty() {
        debug!("no-op: no whitelisted releases passed into update_cache_for_releases");
        return Ok(());
    }

    debug!("refreshing the read cache for {} releases", release_dirs.len());
    if release_dirs.len() < 10 {
        let names: Vec<String> = release_dirs.iter().filter_map(|p| p.file_name()).filter_map(|n| n.to_str()).map(|s| s.to_string()).collect();
        debug!("refreshing cached data for {}", names.join(", "));
    }

    // If the number of releases changed is less than 50, do not bother with multiprocessing
    if !force_multiprocessing && release_dirs.len() < 50 {
        debug!("running cache update executor in same process because len={} < 50", release_dirs.len());
        _update_cache_for_releases_executor(c, &release_dirs, force, None, None)?;
        return Ok(());
    }

    // Use multiprocessing with rayon
    use rayon::prelude::*;
    use std::sync::{Arc, Mutex};

    // Batch size defaults to equal split across all processes
    let num_proc = c.max_proc.max(1);
    let batch_size = if release_dirs.len() < num_proc * 50 {
        50
    } else {
        release_dirs.len() / num_proc + 1
    };

    debug!("creating multiprocessing pool to parallelize cache executors");

    // Have each process propagate the collages and playlists it wants to update back upwards
    let collages_to_force_update = Arc::new(Mutex::new(Vec::new()));
    let playlists_to_force_update = Arc::new(Mutex::new(Vec::new()));
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));

    // Process batches in parallel
    release_dirs.chunks(batch_size).collect::<Vec<_>>().par_iter().enumerate().for_each(|(i, batch)| {
        debug!("spawning release cache update process for batch {} (releases [{}, {}))", i, i * batch_size, (i + 1) * batch_size);

        let mut collages_batch = Vec::new();
        let mut playlists_batch = Vec::new();

        match _update_cache_for_releases_executor(c, batch, force, Some(&mut collages_batch), Some(&mut playlists_batch)) {
            Ok(()) => {
                // Add collages and playlists to force update
                if !collages_batch.is_empty() {
                    if let Ok(mut collages) = collages_to_force_update.lock() {
                        collages.extend(collages_batch);
                    }
                }
                if !playlists_batch.is_empty() {
                    if let Ok(mut playlists) = playlists_to_force_update.lock() {
                        playlists.extend(playlists_batch);
                    }
                }
            }
            Err(e) => {
                if let Ok(mut errs) = errors.lock() {
                    errs.push(format!("Error processing batch {}: {}", i, e));
                }
            }
        }
    });

    // Check for errors
    let errors = errors.lock().unwrap();
    if !errors.is_empty() {
        return Err(RoseError::CacheUpdateError(format!("Errors during multiprocessing: {}", errors.join("; "))));
    }

    // Force update collages and playlists that were marked
    let collages = collages_to_force_update.lock().unwrap();
    if !collages.is_empty() {
        let unique_collages: Vec<String> = collages.iter().cloned().collect::<HashSet<_>>().into_iter().collect();
        debug!("force updating {} collages from multiprocessing", unique_collages.len());
        update_cache_for_collages(c, Some(unique_collages), true)?;
    }

    let playlists = playlists_to_force_update.lock().unwrap();
    if !playlists.is_empty() {
        let unique_playlists: Vec<String> = playlists.iter().cloned().collect::<HashSet<_>>().into_iter().collect();
        debug!("force updating {} playlists from multiprocessing", unique_playlists.len());
        update_cache_for_playlists(c, Some(unique_playlists), true)?;
    }

    Ok(())
}

/// The implementation logic for update_cache_for_releases, split out for multiprocessing
fn _update_cache_for_releases_executor(
    c: &Config,
    release_dirs: &[PathBuf],
    force: bool,
    _collages_to_force_update_receiver: Option<&mut Vec<String>>,
    _playlists_to_force_update_receiver: Option<&mut Vec<String>>,
) -> Result<()> {
    // Step 1: Scan directories and find .rose.{uuid}.toml files
    #[derive(Debug)]
    struct DirScanResult {
        source_path: PathBuf,
        release_id: Option<String>,
        files: Vec<PathBuf>,
    }

    let mut dir_tree = Vec::new();
    let mut release_uuids = Vec::new();

    for rd in release_dirs {
        let mut release_id = None;
        let mut files = Vec::new();

        if !rd.is_dir() {
            debug!("skipping scanning {} because it is not a directory", rd.display());
            continue;
        }

        // Walk the directory tree
        for entry in walkdir::WalkDir::new(rd) {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            if entry.file_type().is_file() {
                let file_name = entry.file_name().to_string_lossy();
                if let Some(captures) = STORED_DATA_FILE_REGEX.captures(&file_name) {
                    release_id = Some(captures.get(1).unwrap().as_str().to_string());
                }
                files.push(entry.path().to_path_buf());
            }
        }

        // Force deterministic file sort order
        files.sort();

        let canonical_path = rd.canonicalize().unwrap_or_else(|_| rd.clone());
        dir_tree.push(DirScanResult {
            source_path: canonical_path,
            release_id: release_id.clone(),
            files,
        });

        if let Some(id) = release_id {
            release_uuids.push(id);
        }
    }

    // Step 2: Batch query for all metadata associated with discovered IDs
    let mut cached_releases: HashMap<String, (Release, HashMap<String, Track>)> = HashMap::new();

    if !release_uuids.is_empty() {
        let conn = connect(c)?;

        // Fetch all releases
        let placeholders = vec!["?"; release_uuids.len()].join(",");
        let query = format!("SELECT * FROM releases_view WHERE id IN ({})", placeholders);

        let mut stmt = conn.prepare(&query)?;
        let release_rows = stmt.query_map(rusqlite::params_from_iter(&release_uuids), |row| {
            let release = cached_release_from_view(c, row, false).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            Ok((release.id.clone(), release))
        })?;

        for row in release_rows {
            let (id, release) = row?;
            cached_releases.insert(id, (release, HashMap::new()));
        }

        debug!("found {}/{} releases in cache", cached_releases.len(), release_dirs.len());

        // Fetch all tracks
        let query = format!("SELECT * FROM tracks_view WHERE release_id IN ({})", placeholders);
        let mut stmt = conn.prepare(&query)?;
        let mut num_tracks_found = 0;

        let mut rows = stmt.query(rusqlite::params_from_iter(&release_uuids))?;

        while let Some(row) = rows.next()? {
            let release_id: String = row.get("release_id")?;
            let source_path: String = row.get("source_path")?;
            if let Some((release, tracks)) = cached_releases.get_mut(&release_id) {
                let track = cached_track_from_view(c, row, Arc::new(release.clone()), false)?;
                tracks.insert(source_path, track);
                num_tracks_found += 1;
            }
        }

        debug!("found {} tracks in cache", num_tracks_found);
    }

    // Step 3: Process each directory and build update lists
    let mut upd_delete_source_paths = Vec::new();
    let mut upd_release_args = Vec::new();
    let mut upd_release_ids = Vec::new();
    let mut upd_release_artist_args = Vec::new();
    let mut upd_release_genre_args = Vec::new();
    let mut upd_release_secondary_genre_args = Vec::new();
    let mut upd_release_descriptor_args = Vec::new();
    let mut upd_release_label_args = Vec::new();
    let mut upd_unknown_cached_tracks_args = Vec::new();
    let mut upd_track_path_changes = Vec::new();
    let mut upd_track_args = Vec::new();
    let mut upd_track_ids = Vec::new();
    let mut upd_track_artist_args = Vec::new();

    for scan_result in dir_tree {
        let mut source_path = scan_result.source_path;
        let preexisting_release_id = scan_result.release_id;
        let files = scan_result.files;

        debug!("scanning release {}", source_path.file_name().unwrap_or_default().to_string_lossy());

        // Check if directory has any audio files
        let first_audio_file = files.iter().find(|f| {
            f.extension().and_then(|e| e.to_str()).map(|e| SUPPORTED_AUDIO_EXTENSIONS.contains(&format!(".{}", e.to_lowercase()).as_str())).unwrap_or(false)
        });

        if first_audio_file.is_none() {
            debug!("did not find any audio files in release {}, skipping", source_path.display());
            debug!("scheduling cache deletion for empty directory release {}", source_path.display());
            upd_delete_source_paths.push(source_path.to_string_lossy().to_string());
            continue;
        }

        let first_audio_file = first_audio_file.unwrap();
        let mut release_dirty = false;

        // Fetch release from cache or create new one
        let (mut release, cached_tracks) = if let Some(id) = &preexisting_release_id {
            cached_releases.remove(id).unwrap_or_else(|| {
                debug!("first-time unidentified release found at release {}, writing UUID and new", source_path.display());
                release_dirty = true;
                let new_release = Release {
                    id: id.clone(),
                    source_path: source_path.clone(),
                    datafile_mtime: String::new(),
                    cover_image_path: None,
                    added_at: String::new(),
                    releasetitle: String::new(),
                    releasetype: String::new(),
                    releasedate: None,
                    originaldate: None,
                    compositiondate: None,
                    catalognumber: None,
                    edition: None,
                    new: true,
                    disctotal: 0,
                    genres: Vec::new(),
                    parent_genres: Vec::new(),
                    secondary_genres: Vec::new(),
                    parent_secondary_genres: Vec::new(),
                    descriptors: Vec::new(),
                    labels: Vec::new(),
                    releaseartists: ArtistMapping::default(),
                    metahash: String::new(),
                };
                (new_release, HashMap::new())
            })
        } else {
            debug!("first-time unidentified release found at release {}, writing UUID and new", source_path.display());
            release_dirty = true;
            let new_release = Release {
                id: String::new(),
                source_path: source_path.clone(),
                datafile_mtime: String::new(),
                cover_image_path: None,
                added_at: String::new(),
                releasetitle: String::new(),
                releasetype: String::new(),
                releasedate: None,
                originaldate: None,
                compositiondate: None,
                catalognumber: None,
                edition: None,
                new: true,
                disctotal: 0,
                genres: Vec::new(),
                parent_genres: Vec::new(),
                secondary_genres: Vec::new(),
                parent_secondary_genres: Vec::new(),
                descriptors: Vec::new(),
                labels: Vec::new(),
                releaseartists: ArtistMapping::default(),
                metahash: String::new(),
            };
            (new_release, HashMap::new())
        };

        // Handle source path change
        if source_path != release.source_path {
            debug!("source path change detected for release {}, updating", source_path.display());
            release.source_path = source_path.clone();
            release_dirty = true;
        }

        // Handle stored data file creation/update
        match handle_stored_data_file(c, &source_path, &mut release, &preexisting_release_id, first_audio_file, force) {
            Ok(dirty) => {
                if dirty {
                    release_dirty = true;
                }
                // If handle_stored_data_file returned false and we don't have a release ID,
                // it means this is a partially written directory that we should skip
                if !dirty && release.id.is_empty() {
                    continue;
                }
            }
            Err(e) => {
                if e.to_string().contains("No such file or directory") {
                    warn!("skipping update on {}: directory no longer exists", source_path.display());
                    continue;
                } else {
                    return Err(e);
                }
            }
        }

        // Handle cover art
        let mut cover = None;
        for f in &files {
            if let Some(name) = f.file_name() {
                let name_lower = name.to_string_lossy().to_lowercase();
                if c.valid_cover_arts().contains(&name_lower) {
                    cover = Some(f.clone());
                    break;
                }
            }
        }

        if release.cover_image_path != cover {
            debug!("cover image path changed for release {}, updating", source_path.display());
            release.cover_image_path = cover;
            release_dirty = true;
        }

        // Track which cached tracks are no longer on disk
        let mut unknown_cached_tracks: HashSet<String> = cached_tracks.keys().cloned().collect();

        // Read audio tags from files
        let mut pulled_release_tags = false;
        let mut track_totals: HashMap<String, i32> = HashMap::new();

        // Filter for audio files only
        let audio_files: Vec<&PathBuf> = files
            .iter()
            .filter(|f| {
                f.extension().and_then(|e| e.to_str()).map(|e| SUPPORTED_AUDIO_EXTENSIONS.contains(&format!(".{}", e.to_lowercase()).as_str())).unwrap_or(false)
            })
            .collect();

        // Process each audio file
        for f in &audio_files {
            let file_path_str = f.to_string_lossy().to_string();
            unknown_cached_tracks.remove(&file_path_str);

            // Check if track is already cached and mtime hasn't changed
            if let Some(cached_track) = cached_tracks.get(&file_path_str) {
                let file_mtime = fs::metadata(f)?.modified()?.duration_since(UNIX_EPOCH)?.as_secs().to_string();

                if file_mtime == cached_track.source_mtime && !force {
                    debug!("skipping track {} because mtime has not changed", f.display());
                    // Update totals from cached track
                    *track_totals.entry(cached_track.discnumber.clone()).or_insert(0) += 1;
                    continue;
                }
            }

            // Read tags from the audio file
            debug!("track cache miss for {}, reading tags from disk", f.file_name().unwrap_or_default().to_string_lossy());

            match AudioTags::from_file(f) {
                Ok(mut tags) => {
                    // Pull release tags from the first file
                    if !pulled_release_tags {
                        pulled_release_tags = true;

                        let release_title = tags.releasetitle.clone().unwrap_or_else(|| "Unknown Release".to_string());
                        if release_title != release.releasetitle {
                            debug!("release title change detected for {}, updating", source_path.display());
                            release.releasetitle = release_title;
                            release_dirty = true;
                        }

                        if tags.releasetype != release.releasetype {
                            debug!("release type change detected for {}, updating", source_path.display());
                            release.releasetype = tags.releasetype.clone();
                            release_dirty = true;
                        }

                        if tags.releasedate != release.releasedate {
                            debug!("release date change detected for {}, updating", source_path.display());
                            release.releasedate = tags.releasedate;
                            release_dirty = true;
                        }

                        if tags.originaldate != release.originaldate {
                            debug!("release original date change detected for {}, updating", source_path.display());
                            release.originaldate = tags.originaldate;
                            release_dirty = true;
                        }

                        if tags.compositiondate != release.compositiondate {
                            debug!("release composition date change detected for {}, updating", source_path.display());
                            release.compositiondate = tags.compositiondate;
                            release_dirty = true;
                        }

                        if tags.edition != release.edition {
                            debug!("release edition change detected for {}, updating", source_path.display());
                            release.edition = tags.edition.clone();
                            release_dirty = true;
                        }

                        if tags.catalognumber != release.catalognumber {
                            debug!("release catalog number change detected for {}, updating", source_path.display());
                            release.catalognumber = tags.catalognumber.clone();
                            release_dirty = true;
                        }

                        // Update genres
                        if tags.genre != release.genres {
                            debug!("release genres change detected for {}, updating", source_path.display());
                            release.genres = tags.genre.clone();
                            release.parent_genres = _get_parent_genres(&release.genres);
                            release_dirty = true;
                        }

                        if tags.secondarygenre != release.secondary_genres {
                            debug!("release secondary genres change detected for {}, updating", source_path.display());
                            release.secondary_genres = tags.secondarygenre.clone();
                            release.parent_secondary_genres = _get_parent_genres(&release.secondary_genres);
                            release_dirty = true;
                        }

                        if tags.descriptor != release.descriptors {
                            debug!("release descriptors change detected for {}, updating", source_path.display());
                            release.descriptors = tags.descriptor.clone();
                            release_dirty = true;
                        }

                        if tags.label != release.labels {
                            debug!("release labels change detected for {}, updating", source_path.display());
                            release.labels = tags.label.clone();
                            release_dirty = true;
                        }

                        if tags.releaseartists != release.releaseartists {
                            debug!("release artists change detected for {}, updating", source_path.display());
                            release.releaseartists = tags.releaseartists.clone();
                            release_dirty = true;
                        }
                    }

                    // Get current file mtime
                    let mut track_mtime = fs::metadata(f)?.modified()?.duration_since(UNIX_EPOCH)?.as_secs().to_string();

                    // Build track data
                    let track_id = if tags.id.is_none() || tags.release_id.as_ref() != Some(&release.id) {
                        // This is our first time reading this track or the release ID doesn't match.
                        // Use existing track ID if present, otherwise generate a new one
                        let new_track_id = tags.id.clone().unwrap_or_else(|| Uuid::now_v7().to_string());
                        tags.id = Some(new_track_id.clone());
                        tags.release_id = Some(release.id.clone());

                        // Write the IDs to the file
                        match tags.flush(c, false) {
                            Ok(_) => {
                                debug!("wrote track and release IDs to {}: track_id={}, release_id={}", f.display(), new_track_id, release.id);
                                // Refresh the mtime since we just wrote to the file
                                track_mtime = fs::metadata(f)?.modified()?.duration_since(UNIX_EPOCH)?.as_secs().to_string();
                            }
                            Err(e) => {
                                warn!("failed to write IDs to {}: {}", f.display(), e);
                            }
                        }

                        new_track_id
                    } else {
                        tags.id.clone().unwrap()
                    };

                    let disc_number = tags.discnumber.as_deref().unwrap_or("1");
                    let track_number = tags.tracknumber.as_deref().unwrap_or("1").replace(".", "");
                    let track_title = tags.tracktitle.clone().unwrap_or_else(|| "Unknown Title".to_string());

                    // Update track totals
                    *track_totals.entry(disc_number.to_string()).or_insert(0) += 1;

                    // Check if track is dirty
                    let mut track_dirty = false;
                    if let Some(cached_track) = cached_tracks.get(&track_id) {
                        if track_mtime != cached_track.source_mtime {
                            track_dirty = true;
                        }
                    } else {
                        // New track
                        track_dirty = true;
                    }

                    if track_dirty {
                        // Build track update arguments
                        upd_track_args.push(vec![
                            track_id.clone(),
                            f.to_string_lossy().to_string(),
                            track_mtime.clone(),
                            track_title,
                            release.id.clone(),
                            track_number,
                            tags.tracktotal.unwrap_or(1).to_string(),
                            disc_number.to_string(),
                            tags.duration_sec.to_string(),
                            // Generate a unique metahash for each track
                            // In the real implementation, this should be a hash of the track metadata
                            format!("track_metahash_{}", track_id),
                        ]);
                        upd_track_ids.push(track_id.clone());

                        // Add track artists
                        let mut artist_pos = 0;
                        for artist in &tags.trackartists.main {
                            upd_track_artist_args.push(vec![track_id.clone(), artist.name.clone(), "main".to_string(), artist_pos.to_string()]);
                            artist_pos += 1;
                        }
                        for artist in &tags.trackartists.guest {
                            upd_track_artist_args.push(vec![track_id.clone(), artist.name.clone(), "guest".to_string(), artist_pos.to_string()]);
                            artist_pos += 1;
                        }
                        for artist in &tags.trackartists.remixer {
                            upd_track_artist_args.push(vec![track_id.clone(), artist.name.clone(), "remixer".to_string(), artist_pos.to_string()]);
                            artist_pos += 1;
                        }
                        for artist in &tags.trackartists.producer {
                            upd_track_artist_args.push(vec![track_id.clone(), artist.name.clone(), "producer".to_string(), artist_pos.to_string()]);
                            artist_pos += 1;
                        }
                        for artist in &tags.trackartists.composer {
                            upd_track_artist_args.push(vec![track_id.clone(), artist.name.clone(), "composer".to_string(), artist_pos.to_string()]);
                            artist_pos += 1;
                        }
                        for artist in &tags.trackartists.conductor {
                            upd_track_artist_args.push(vec![track_id.clone(), artist.name.clone(), "conductor".to_string(), artist_pos.to_string()]);
                            artist_pos += 1;
                        }
                        for artist in &tags.trackartists.djmixer {
                            upd_track_artist_args.push(vec![track_id.clone(), artist.name.clone(), "djmixer".to_string(), artist_pos.to_string()]);
                            artist_pos += 1;
                        }
                    }
                }
                Err(e) => {
                    warn!("failed to read tags from {}: {}", f.display(), e);
                    continue;
                }
            }
        }

        // Check for tracks that no longer exist on disk
        if !unknown_cached_tracks.is_empty() {
            debug!("deleting {} unknown tracks from cache", unknown_cached_tracks.len());
            // Get track IDs for the unknown tracks
            let track_ids_to_delete: Vec<String> =
                unknown_cached_tracks.iter().filter_map(|path| cached_tracks.get(path).map(|track| track.id.clone())).collect();
            upd_unknown_cached_tracks_args.push((release.id.clone(), track_ids_to_delete));
        }

        // Update disc total
        let disctotal = track_totals.len() as i32;
        if disctotal != release.disctotal {
            debug!("disc total change detected for {}, updating", source_path.display());
            release.disctotal = disctotal;
            release_dirty = true;
        }

        // Calculate and update metahash
        let new_metahash = sha256_struct(&release)?;
        if new_metahash != release.metahash {
            debug!("metahash change detected for {}, updating", source_path.display());
            release.metahash = new_metahash;
            release_dirty = true;
        }

        // Update stored data file with new metadata if release is dirty
        if release_dirty {
            let datafile_path = source_path.join(format!(".rose.{}.toml", release.id));
            if datafile_path.exists() {
                let stored_data = read_stored_data_file(&datafile_path)?;
                write_stored_data_file(&datafile_path, &stored_data)?;
            }
        }

        // Perform directory/file renames if configured
        if c.rename_source_files && release_dirty {
            let template_release = release.to_template_release();
            let wanted_dirname = evaluate_release_template(&c.path_templates.source.release, &template_release, None, None);
            let wanted_dirname = sanitize_dirname(c, &wanted_dirname, true);

            // Iterate until we've either:
            // 1. Realized that the name of the source path matches the desired dirname
            // 2. Or renamed the source directory to match our desired name
            let original_wanted_dirname = wanted_dirname.clone();
            let mut wanted_dirname = wanted_dirname;
            let mut collision_no = 2;

            while wanted_dirname != source_path.file_name().unwrap_or_default().to_string_lossy() {
                let new_source_path = source_path.with_file_name(&wanted_dirname);

                // If there is a collision, bump the collision counter and retry
                if new_source_path.exists() {
                    let new_max_len = c.max_filename_bytes - (3 + collision_no.to_string().len());
                    let truncated = if original_wanted_dirname.len() > new_max_len {
                        &original_wanted_dirname[..new_max_len]
                    } else {
                        &original_wanted_dirname
                    };
                    wanted_dirname = format!("{} [{}]", truncated, collision_no);
                    collision_no += 1;
                    continue;
                }

                // If no collision, rename the directory
                let old_source_path = source_path.clone();
                fs::rename(&old_source_path, &new_source_path)?;
                info!(
                    "renamed source release directory {} to {}",
                    old_source_path.file_name().unwrap_or_default().to_string_lossy(),
                    new_source_path.file_name().unwrap_or_default().to_string_lossy()
                );

                // Update release source path
                release.source_path = new_source_path.clone();
                source_path = new_source_path;

                // Update the cached cover image path
                if let Some(cover_path) = &release.cover_image_path {
                    if let Ok(relative) = cover_path.strip_prefix(&old_source_path) {
                        release.cover_image_path = Some(source_path.join(relative));
                    }
                }

                // We'll need to update track paths in the database
                for (track_path, track) in &cached_tracks {
                    if let Ok(relative) = Path::new(track_path).strip_prefix(&old_source_path) {
                        let new_track_path = source_path.join(relative);
                        upd_track_path_changes.push((track.id.clone(), new_track_path.to_string_lossy().to_string()));
                    }
                }

                // Also update paths in upd_track_args
                for track_args in &mut upd_track_args {
                    let old_track_path = Path::new(&track_args[1]);
                    if let Ok(relative) = old_track_path.strip_prefix(&old_source_path) {
                        let new_track_path = source_path.join(relative);
                        track_args[1] = new_track_path.to_string_lossy().to_string();
                    }
                }

                break;
            }

            // Rename track files if needed
            for track_args in &mut upd_track_args {
                let track_path = Path::new(&track_args[1]);
                let track_title = &track_args[3];
                let track_number = &track_args[5];
                let disc_number = &track_args[7];

                // Create a temporary track object for template evaluation
                let temp_track = Track {
                    id: track_args[0].clone(),
                    source_path: track_path.to_path_buf(),
                    source_mtime: track_args[2].clone(),
                    tracktitle: track_title.clone(),
                    release: Arc::new(release.clone()),
                    tracknumber: track_number.clone(),
                    tracktotal: track_args[6].parse().unwrap_or(1),
                    discnumber: disc_number.clone(),
                    duration_seconds: track_args[8].parse().unwrap_or(0),
                    trackartists: ArtistMapping::default(), // We'd need to build this from upd_track_artist_args
                    metahash: String::new(),
                };

                let template_track = temp_track.to_template_track();
                let wanted_filename = evaluate_track_template(&c.path_templates.source.track, &template_track, None, None);
                let wanted_filename = sanitize_filename(c, &wanted_filename, true);

                let current_filename = track_path.file_name().and_then(|f| f.to_str()).unwrap_or("");

                // Calculate the wanted path at the release root directory
                let wanted_path = release.source_path.join(&wanted_filename);

                // Check if we need to rename or move the file
                let needs_move = track_path.parent() != Some(&release.source_path);
                let needs_rename = current_filename != wanted_filename;

                if needs_move || needs_rename {
                    // Handle collisions
                    let mut final_track_path = wanted_path.clone();
                    let mut collision_no = 2;
                    while final_track_path.exists() && final_track_path != track_path {
                        // Extract stem and extension from wanted_filename
                        let (stem, ext) = if let Some(dot_pos) = wanted_filename.rfind('.') {
                            (&wanted_filename[..dot_pos], &wanted_filename[dot_pos..])
                        } else {
                            (wanted_filename.as_str(), "")
                        };

                        // Calculate space needed for collision suffix " [N]" plus extension
                        let collision_suffix = format!(" [{}]", collision_no);
                        let suffix_len = collision_suffix.len() + ext.len();
                        let available_for_stem = c.max_filename_bytes.saturating_sub(suffix_len);

                        // Truncate stem if needed
                        let stem_bytes = stem.as_bytes();
                        let truncated_stem = if stem_bytes.len() > available_for_stem {
                            String::from_utf8_lossy(&stem_bytes[..available_for_stem]).trim().to_string()
                        } else {
                            stem.to_string()
                        };

                        let collision_filename = format!("{}{}{}", truncated_stem, collision_suffix, ext);

                        final_track_path = release.source_path.join(collision_filename);
                        collision_no += 1;
                    }

                    if final_track_path != track_path {
                        fs::rename(track_path, &final_track_path)?;
                        info!("renamed/moved track file {} to {}", track_path.display(), final_track_path.display());

                        // Clean up empty parent directories if the file was moved out of a subdirectory
                        if needs_move {
                            let mut parent = track_path.parent();
                            while let Some(dir) = parent {
                                if dir == release.source_path {
                                    break;
                                }
                                // Check if directory is empty
                                if fs::read_dir(dir)?.next().is_none() {
                                    fs::remove_dir(dir)?;
                                    debug!("removed empty directory {}", dir.display());
                                } else {
                                    break; // Directory not empty, stop cleanup
                                }
                                parent = dir.parent();
                            }
                        }

                        // Update the track path in the arguments
                        track_args[1] = final_track_path.to_string_lossy().to_string();
                    }
                }
            }
        }

        // Add to update lists if dirty
        if release_dirty && !release.id.is_empty() {
            upd_release_ids.push(release.id.clone());
            upd_release_args.push(vec![
                release.id.clone(),
                release.source_path.to_string_lossy().to_string(),
                release.cover_image_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
                release.added_at.clone(),
                release.datafile_mtime.clone(),
                release.releasetitle.clone(),
                release.releasetype.clone(),
                release.releasedate.as_ref().map(|d| d.to_string()).unwrap_or_default(),
                release.originaldate.as_ref().map(|d| d.to_string()).unwrap_or_default(),
                release.compositiondate.as_ref().map(|d| d.to_string()).unwrap_or_default(),
                release.edition.clone().unwrap_or_default(),
                release.catalognumber.clone().unwrap_or_default(),
                release.disctotal.to_string(),
                if release.new { "1" } else { "0" }.to_string(),
                release.metahash.clone(),
            ]);

            // Add genres
            for (pos, genre) in release.genres.iter().enumerate() {
                upd_release_genre_args.push(vec![release.id.clone(), genre.clone(), pos.to_string()]);
            }

            // Add secondary genres
            for (pos, genre) in release.secondary_genres.iter().enumerate() {
                upd_release_secondary_genre_args.push(vec![release.id.clone(), genre.clone(), pos.to_string()]);
            }

            // Add descriptors
            for (pos, descriptor) in release.descriptors.iter().enumerate() {
                upd_release_descriptor_args.push(vec![release.id.clone(), descriptor.clone(), pos.to_string()]);
            }

            // Add labels
            for (pos, label) in release.labels.iter().enumerate() {
                upd_release_label_args.push(vec![release.id.clone(), label.clone(), pos.to_string()]);
            }

            // Add artists
            let mut artist_pos = 0;
            for artist in &release.releaseartists.main {
                upd_release_artist_args.push(vec![release.id.clone(), artist.name.clone(), "main".to_string(), artist_pos.to_string()]);
                artist_pos += 1;
            }
            for artist in &release.releaseartists.guest {
                upd_release_artist_args.push(vec![release.id.clone(), artist.name.clone(), "guest".to_string(), artist_pos.to_string()]);
                artist_pos += 1;
            }
            for artist in &release.releaseartists.remixer {
                upd_release_artist_args.push(vec![release.id.clone(), artist.name.clone(), "remixer".to_string(), artist_pos.to_string()]);
                artist_pos += 1;
            }
            for artist in &release.releaseartists.producer {
                upd_release_artist_args.push(vec![release.id.clone(), artist.name.clone(), "producer".to_string(), artist_pos.to_string()]);
                artist_pos += 1;
            }
            for artist in &release.releaseartists.composer {
                upd_release_artist_args.push(vec![release.id.clone(), artist.name.clone(), "composer".to_string(), artist_pos.to_string()]);
                artist_pos += 1;
            }
            for artist in &release.releaseartists.conductor {
                upd_release_artist_args.push(vec![release.id.clone(), artist.name.clone(), "conductor".to_string(), artist_pos.to_string()]);
                artist_pos += 1;
            }
            for artist in &release.releaseartists.djmixer {
                upd_release_artist_args.push(vec![release.id.clone(), artist.name.clone(), "djmixer".to_string(), artist_pos.to_string()]);
                artist_pos += 1;
            }
        }
    }

    // Step 4: Execute database updates
    if !upd_delete_source_paths.is_empty() || !upd_release_args.is_empty() || !upd_track_args.is_empty() {
        execute_cache_updates(
            c,
            upd_delete_source_paths,
            upd_release_args,
            upd_release_ids,
            upd_release_artist_args,
            upd_release_genre_args,
            upd_release_secondary_genre_args,
            upd_release_descriptor_args,
            upd_release_label_args,
            upd_unknown_cached_tracks_args,
            upd_track_path_changes,
            upd_track_args,
            upd_track_ids,
            upd_track_artist_args,
        )?;
    }

    Ok(())
}

/// Handle stored data file creation/update for a release
fn handle_stored_data_file(
    _c: &Config,
    source_path: &Path,
    release: &mut Release,
    preexisting_release_id: &Option<String>,
    first_audio_file: &Path,
    force: bool,
) -> Result<bool> {
    let mut dirty = false;

    if preexisting_release_id.is_none() {
        // Check if files already have release IDs
        let release_id_from_first_file = AudioTags::from_file(first_audio_file).ok().and_then(|tags| tags.release_id);

        if release_id_from_first_file.is_some() && !force {
            warn!(
                "no-op: skipping release at {}: files in release already have release_id {:?}, but .rose.{{uuid}}.toml is missing, is another tool in the middle of writing the directory? run with --force to recreate .rose.{{uuid}}.toml",
                source_path.display(),
                release_id_from_first_file
            );
            return Ok(false); // No changes, skip this release
        }

        debug!("creating new stored data file for release {}", source_path.display());
        let stored_release_data = StoredDataFile {
            new: true,
            added_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        };

        // Preserve the release ID already present in the first file if we can
        let new_release_id = release_id_from_first_file.unwrap_or_else(|| Uuid::now_v7().to_string());
        let datafile_path = source_path.join(format!(".rose.{}.toml", new_release_id));

        write_stored_data_file(&datafile_path, &stored_release_data)?;

        release.id = new_release_id;
        release.new = stored_release_data.new;
        release.added_at = stored_release_data.added_at;
        release.datafile_mtime = fs::metadata(&datafile_path)?.modified()?.duration_since(UNIX_EPOCH)?.as_secs().to_string();

        dirty = true;
    } else {
        // Ensure release ID is set
        if release.id.is_empty() {
            release.id = preexisting_release_id.as_ref().unwrap().clone();
        }

        // Check if datafile mtime changed
        let datafile_path = source_path.join(format!(".rose.{}.toml", preexisting_release_id.as_ref().unwrap()));
        let datafile_mtime = fs::metadata(&datafile_path)?.modified()?.duration_since(UNIX_EPOCH)?.as_secs().to_string();

        if datafile_mtime != release.datafile_mtime || force {
            debug!("datafile changed for release {}, updating", source_path.display());
            dirty = true;
            release.datafile_mtime = datafile_mtime;

            let stored_data = read_stored_data_file(&datafile_path)?;
            release.new = stored_data.new;
            release.added_at = stored_data.added_at.clone();

            // Write back if needed (to update defaults)
            write_stored_data_file(&datafile_path, &stored_data)?;
        }
    }

    Ok(dirty)
}

/// Execute batched cache updates to the database
#[allow(clippy::too_many_arguments)]
fn execute_cache_updates(
    c: &Config,
    upd_delete_source_paths: Vec<String>,
    upd_release_args: Vec<Vec<String>>,
    upd_release_ids: Vec<String>,
    upd_release_artist_args: Vec<Vec<String>>,
    upd_release_genre_args: Vec<Vec<String>>,
    upd_release_secondary_genre_args: Vec<Vec<String>>,
    upd_release_descriptor_args: Vec<Vec<String>>,
    upd_release_label_args: Vec<Vec<String>>,
    upd_unknown_cached_tracks_args: Vec<(String, Vec<String>)>,
    upd_track_path_changes: Vec<(String, String)>,
    upd_track_args: Vec<Vec<String>>,
    upd_track_ids: Vec<String>,
    upd_track_artist_args: Vec<Vec<String>>,
) -> Result<()> {
    let mut conn = connect(c)?;
    let tx = conn.transaction()?;

    // Delete releases that no longer exist
    if !upd_delete_source_paths.is_empty() {
        let placeholders = vec!["?"; upd_delete_source_paths.len()].join(",");
        let query = format!("DELETE FROM releases WHERE source_path IN ({})", placeholders);
        tx.execute(&query, rusqlite::params_from_iter(&upd_delete_source_paths))?;
    }

    // Insert/update releases
    if !upd_release_args.is_empty() {
        let values_placeholder = vec!["(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"; upd_release_args.len()].join(",");
        let query = format!(
            "INSERT OR REPLACE INTO releases (
                id, source_path, cover_image_path, added_at, datafile_mtime,
                title, releasetype, releasedate, originaldate, compositiondate,
                edition, catalognumber, disctotal, new, metahash
            ) VALUES {}
            ON CONFLICT (id) DO UPDATE SET
                source_path = excluded.source_path,
                cover_image_path = excluded.cover_image_path,
                added_at = excluded.added_at,
                datafile_mtime = excluded.datafile_mtime,
                title = excluded.title,
                releasetype = excluded.releasetype,
                releasedate = excluded.releasedate,
                originaldate = excluded.originaldate,
                compositiondate = excluded.compositiondate,
                edition = excluded.edition,
                catalognumber = excluded.catalognumber,
                disctotal = excluded.disctotal,
                new = excluded.new,
                metahash = excluded.metahash",
            values_placeholder
        );

        let flattened: Vec<String> = upd_release_args.into_iter().flatten().collect();
        tx.execute(&query, rusqlite::params_from_iter(&flattened))?;

        // Delete and re-insert genres
        if !upd_release_ids.is_empty() {
            let placeholders = vec!["?"; upd_release_ids.len()].join(",");
            tx.execute(&format!("DELETE FROM releases_genres WHERE release_id IN ({})", placeholders), rusqlite::params_from_iter(&upd_release_ids))?;
        }

        if !upd_release_genre_args.is_empty() {
            let values_placeholder = vec!["(?,?,?)"; upd_release_genre_args.len()].join(",");
            let query = format!("INSERT INTO releases_genres (release_id, genre, position) VALUES {}", values_placeholder);
            let flattened: Vec<String> = upd_release_genre_args.into_iter().flatten().collect();
            tx.execute(&query, rusqlite::params_from_iter(&flattened))?;
        }

        // Delete and re-insert secondary genres
        if !upd_release_ids.is_empty() {
            let placeholders = vec!["?"; upd_release_ids.len()].join(",");
            tx.execute(&format!("DELETE FROM releases_secondary_genres WHERE release_id IN ({})", placeholders), rusqlite::params_from_iter(&upd_release_ids))?;
        }

        if !upd_release_secondary_genre_args.is_empty() {
            let values_placeholder = vec!["(?,?,?)"; upd_release_secondary_genre_args.len()].join(",");
            let query = format!("INSERT INTO releases_secondary_genres (release_id, genre, position) VALUES {}", values_placeholder);
            let flattened: Vec<String> = upd_release_secondary_genre_args.into_iter().flatten().collect();
            tx.execute(&query, rusqlite::params_from_iter(&flattened))?;
        }

        // Delete and re-insert descriptors
        if !upd_release_ids.is_empty() {
            let placeholders = vec!["?"; upd_release_ids.len()].join(",");
            tx.execute(&format!("DELETE FROM releases_descriptors WHERE release_id IN ({})", placeholders), rusqlite::params_from_iter(&upd_release_ids))?;
        }

        if !upd_release_descriptor_args.is_empty() {
            let values_placeholder = vec!["(?,?,?)"; upd_release_descriptor_args.len()].join(",");
            let query = format!("INSERT INTO releases_descriptors (release_id, descriptor, position) VALUES {}", values_placeholder);
            let flattened: Vec<String> = upd_release_descriptor_args.into_iter().flatten().collect();
            tx.execute(&query, rusqlite::params_from_iter(&flattened))?;
        }

        // Delete and re-insert labels
        if !upd_release_ids.is_empty() {
            let placeholders = vec!["?"; upd_release_ids.len()].join(",");
            tx.execute(&format!("DELETE FROM releases_labels WHERE release_id IN ({})", placeholders), rusqlite::params_from_iter(&upd_release_ids))?;
        }

        if !upd_release_label_args.is_empty() {
            let values_placeholder = vec!["(?,?,?)"; upd_release_label_args.len()].join(",");
            let query = format!("INSERT INTO releases_labels (release_id, label, position) VALUES {}", values_placeholder);
            let flattened: Vec<String> = upd_release_label_args.into_iter().flatten().collect();
            tx.execute(&query, rusqlite::params_from_iter(&flattened))?;
        }

        // Delete and re-insert artists
        if !upd_release_ids.is_empty() {
            let placeholders = vec!["?"; upd_release_ids.len()].join(",");
            tx.execute(&format!("DELETE FROM releases_artists WHERE release_id IN ({})", placeholders), rusqlite::params_from_iter(&upd_release_ids))?;
        }

        if !upd_release_artist_args.is_empty() {
            // Insert one by one, converting position to integer
            for args in upd_release_artist_args {
                tx.execute(
                    "INSERT INTO releases_artists (release_id, artist, role, position) VALUES (?1, ?2, ?3, ?4)",
                    params![&args[0], &args[1], &args[2], args[3].parse::<i64>().unwrap()],
                )?;
            }
        }
    }

    // Delete tracks that no longer exist
    if !upd_unknown_cached_tracks_args.is_empty() {
        // Build list of all track IDs to delete
        let mut track_ids_to_delete: Vec<String> = Vec::new();
        for (_release_id, track_ids) in &upd_unknown_cached_tracks_args {
            track_ids_to_delete.extend_from_slice(track_ids);
        }

        if !track_ids_to_delete.is_empty() {
            let placeholders = vec!["?"; track_ids_to_delete.len()].join(",");
            tx.execute(&format!("DELETE FROM tracks WHERE id IN ({})", placeholders), rusqlite::params_from_iter(&track_ids_to_delete))?;
        }
    }

    // Update track paths for renamed directories
    for (track_id, new_path) in upd_track_path_changes {
        tx.execute("UPDATE tracks SET source_path = ?1 WHERE id = ?2", params![&new_path, &track_id])?;
    }

    // Insert/update tracks
    if !upd_track_args.is_empty() {
        // Insert tracks one by one to ensure foreign key constraints are satisfied
        for track_args in upd_track_args {
            tx.execute(
                "INSERT OR REPLACE INTO tracks (
                    id, source_path, source_mtime, title, release_id,
                    tracknumber, tracktotal, discnumber, duration_seconds, metahash
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    &track_args[0],
                    &track_args[1],
                    &track_args[2],
                    &track_args[3],
                    &track_args[4],
                    &track_args[5],
                    &track_args[6],
                    &track_args[7],
                    &track_args[8],
                    &track_args[9]
                ],
            )?;
        }

        // Delete and re-insert track artists
        if !upd_track_ids.is_empty() {
            debug!("Deleting track artists for {} track IDs: {:?}", upd_track_ids.len(), &upd_track_ids);
            let placeholders = vec!["?"; upd_track_ids.len()].join(",");
            tx.execute(&format!("DELETE FROM tracks_artists WHERE track_id IN ({})", placeholders), rusqlite::params_from_iter(&upd_track_ids))?;
        }

        if !upd_track_artist_args.is_empty() {
            // Insert one by one, converting position to integer
            for args in upd_track_artist_args {
                tx.execute(
                    "INSERT INTO tracks_artists (track_id, artist, role, position) VALUES (?1, ?2, ?3, ?4)",
                    params![&args[0], &args[1], &args[2], args[3].parse::<i64>().unwrap()],
                )?;
            }
        }
    }

    // Update full-text search tables
    if !upd_release_ids.is_empty() || !upd_track_ids.is_empty() {
        // Create the process_string_for_fts function in SQLite
        tx.create_scalar_function(
            "process_string_for_fts",
            1,
            rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                let value = ctx.get_raw(0);
                let s = match value {
                    rusqlite::types::ValueRef::Text(text) => std::str::from_utf8(text).unwrap_or("").to_string(),
                    rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                    rusqlite::types::ValueRef::Real(f) => f.to_string(),
                    rusqlite::types::ValueRef::Null => String::new(),
                    _ => String::new(),
                };
                Ok(process_string_for_fts(&s))
            },
        )?;

        // Delete existing FTS entries for updated tracks/releases
        let mut query_parts = Vec::new();
        let mut params: Vec<String> = Vec::new();

        if !upd_track_ids.is_empty() {
            let placeholders = vec!["?"; upd_track_ids.len()].join(",");
            query_parts.push(format!("t.id IN ({})", placeholders));
            params.extend(upd_track_ids.clone());
        }

        if !upd_release_ids.is_empty() {
            let placeholders = vec!["?"; upd_release_ids.len()].join(",");
            query_parts.push(format!("r.id IN ({})", placeholders));
            params.extend(upd_release_ids.clone());
        }

        let where_clause = query_parts.join(" OR ");

        tx.execute(
            &format!(
                "DELETE FROM rules_engine_fts WHERE rowid IN (
                    SELECT t.rowid
                    FROM tracks t
                    JOIN releases r ON r.id = t.release_id
                    WHERE {}
                )",
                where_clause
            ),
            rusqlite::params_from_iter(&params),
        )?;

        // Insert new FTS entries
        tx.execute(
            &format!(
                "INSERT INTO rules_engine_fts (
                    rowid,
                    tracktitle,
                    tracknumber,
                    tracktotal,
                    discnumber,
                    disctotal,
                    releasetitle,
                    releasedate,
                    originaldate,
                    compositiondate,
                    edition,
                    catalognumber,
                    releasetype,
                    genre,
                    secondarygenre,
                    descriptor,
                    label,
                    releaseartist,
                    trackartist,
                    new
                )
                SELECT
                    t.rowid,
                    process_string_for_fts(t.title) AS tracktitle,
                    process_string_for_fts(t.tracknumber) AS tracknumber,
                    process_string_for_fts(t.tracktotal) AS tracktotal,
                    process_string_for_fts(t.discnumber) AS discnumber,
                    process_string_for_fts(r.disctotal) AS disctotal,
                    process_string_for_fts(r.title) AS releasetitle,
                    process_string_for_fts(r.releasedate) AS releasedate,
                    process_string_for_fts(r.originaldate) AS originaldate,
                    process_string_for_fts(r.compositiondate) AS compositiondate,
                    process_string_for_fts(r.edition) AS edition,
                    process_string_for_fts(r.catalognumber) AS catalognumber,
                    process_string_for_fts(r.releasetype) AS releasetype,
                    process_string_for_fts(COALESCE(GROUP_CONCAT(rg.genre, ' '), '')) AS genre,
                    process_string_for_fts(COALESCE(GROUP_CONCAT(rs.genre, ' '), '')) AS secondarygenre,
                    process_string_for_fts(COALESCE(GROUP_CONCAT(rd.descriptor, ' '), '')) AS descriptor,
                    process_string_for_fts(COALESCE(GROUP_CONCAT(rl.label, ' '), '')) AS label,
                    process_string_for_fts(COALESCE(GROUP_CONCAT(ra.artist, ' '), '')) AS releaseartist,
                    process_string_for_fts(COALESCE(GROUP_CONCAT(ta.artist, ' '), '')) AS trackartist,
                    process_string_for_fts(CASE WHEN r.new THEN 'true' ELSE 'false' END) AS new
                FROM tracks t
                JOIN releases r ON r.id = t.release_id
                LEFT JOIN releases_genres rg ON rg.release_id = r.id
                LEFT JOIN releases_secondary_genres rs ON rs.release_id = r.id
                LEFT JOIN releases_descriptors rd ON rd.release_id = r.id
                LEFT JOIN releases_labels rl ON rl.release_id = r.id
                LEFT JOIN releases_artists ra ON ra.release_id = r.id
                LEFT JOIN tracks_artists ta ON ta.track_id = t.id
                WHERE {}
                GROUP BY t.id",
                where_clause
            ),
            rusqlite::params_from_iter(&params),
        )?;
    }

    tx.commit()?;

    // After committing, find collages and playlists that need to be updated
    let mut update_collages = Vec::new();
    let mut update_playlists = Vec::new();

    if !upd_release_ids.is_empty() {
        // Find collages that contain the updated releases
        let conn = connect(c)?;
        let placeholders = vec!["?"; upd_release_ids.len()].join(",");
        let query = format!(
            "SELECT DISTINCT cr.collage_name
             FROM collages_releases cr
             WHERE cr.release_id IN ({})
             ORDER BY cr.collage_name",
            placeholders
        );
        let mut stmt = conn.prepare(&query)?;
        let collages = stmt.query_map(rusqlite::params_from_iter(&upd_release_ids), |row| row.get::<_, String>(0))?;
        for collage in collages {
            update_collages.push(collage?);
        }
    }

    if !upd_track_ids.is_empty() {
        // Find playlists that contain the updated tracks
        let conn = connect(c)?;
        let placeholders = vec!["?"; upd_track_ids.len()].join(",");
        let query = format!(
            "SELECT DISTINCT pt.playlist_name
             FROM playlists_tracks pt
             WHERE pt.track_id IN ({})
             ORDER BY pt.playlist_name",
            placeholders
        );
        let mut stmt = conn.prepare(&query)?;
        let playlists = stmt.query_map(rusqlite::params_from_iter(&upd_track_ids), |row| row.get::<_, String>(0))?;
        for playlist in playlists {
            update_playlists.push(playlist?);
        }
    }

    // Update the affected collages and playlists with force=true
    if !update_collages.is_empty() {
        update_cache_for_collages(c, Some(update_collages), true)?;
    }
    if !update_playlists.is_empty() {
        update_cache_for_playlists(c, Some(update_playlists), true)?;
    }

    Ok(())
}

pub fn update_cache_evict_nonexistent_releases(c: &Config) -> Result<()> {
    debug!("evicting cached releases that are not on disk");

    // Get all directories in the music source directory
    let dirs: Vec<String> = fs::read_dir(&c.music_source_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|entry| entry.path().canonicalize().ok())
        .map(|path| path.to_string_lossy().to_string())
        .collect();

    let conn = connect(c)?;

    if dirs.is_empty() {
        // If no directories exist, delete all releases
        let mut stmt = conn.prepare("DELETE FROM releases RETURNING source_path")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let source_path: String = row.get(0)?;
            info!("evicted missing release {} from cache", source_path);
        }
    } else {
        // Build the query with proper number of placeholders
        let placeholders = dirs.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!("DELETE FROM releases WHERE source_path NOT IN ({placeholders}) RETURNING source_path");

        let mut stmt = conn.prepare(&query)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(&dirs))?;

        while let Some(row) = rows.next()? {
            let source_path: String = row.get(0)?;
            info!("evicted missing release {} from cache", source_path);
        }
    }

    Ok(())
}

/// This is performance-optimized in a similar way to the update releases function. We:
///
/// 1. Execute one big SQL query at the start to fetch the relevant previous caches.
/// 2. Skip reading a file's data if the mtime has not changed since the previous cache update.
/// 3. Only execute a SQLite upsert if the read data differ from the previous caches.
///
/// However, we do not batch writes to the end of the function, nor do we process the collages in
/// parallel. This is because we should have far fewer collages than releases.
pub fn update_cache_for_collages(c: &Config, collage_names: Option<Vec<String>>, force: bool) -> Result<()> {
    let collage_dir = c.music_source_dir.join("!collages");
    fs::create_dir_all(&collage_dir)?;

    // Find all collage files
    let mut files = Vec::new();
    for entry in fs::read_dir(&collage_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension() == Some(std::ffi::OsStr::new("toml")) && path.is_file() {
            if let Some(stem) = path.file_stem() {
                let name = stem.to_string_lossy().to_string();
                if collage_names.as_ref().is_none_or(|names| names.contains(&name)) {
                    files.push((path.canonicalize()?, name, entry));
                }
            }
        }
    }

    debug!("refreshing the read cache for {} collages", files.len());

    // Get existing collages from cache
    let conn = connect(c)?;
    let mut cached_collages = HashMap::new();

    if !files.is_empty() {
        let names: Vec<String> = files.iter().map(|(_, name, _)| name.clone()).collect();
        let placeholders = vec!["?"; names.len()].join(",");
        let query = format!("SELECT name, source_mtime FROM collages WHERE name IN ({})", placeholders);

        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(&names), |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;

        for row in rows {
            let (name, mtime) = row?;
            cached_collages.insert(name, mtime);
        }
    }

    // Process each collage file
    for (path, name, entry) in files {
        let file_mtime = entry.metadata()?.modified()?.duration_since(UNIX_EPOCH)?.as_secs().to_string();

        // Check if we need to update
        let cached_mtime = cached_collages.get(&name);
        if !force && cached_mtime == Some(&file_mtime) {
            debug!("skipping collage {} because mtime has not changed", name);
            continue;
        }

        debug!("updating collage {} in cache", name);

        // Read and parse the collage TOML file
        let content = fs::read_to_string(&path)?;
        let mut data: toml::Value = toml::from_str(&content)?;

        let _releases = data.get("releases").and_then(|v| v.as_array()).cloned().unwrap_or_default();

        let mut release_positions: Vec<(String, i32, bool)> = Vec::new();
        let mut data_changed = false;

        // Check which releases exist and update missing status
        if let Some(releases_array) = data.get_mut("releases").and_then(|v| v.as_array_mut()) {
            for (position, release) in releases_array.iter_mut().enumerate() {
                if let Some(uuid_val) = release.get("uuid").and_then(|v| v.as_str()) {
                    let uuid = uuid_val.to_string();
                    // Check if release exists in database
                    let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM releases WHERE id = ?1)", params![&uuid], |row| row.get(0))?;

                    let currently_missing = release.get("missing").and_then(|v| v.as_bool()).unwrap_or(false);
                    let should_be_missing = !exists;

                    // Update missing status if needed
                    if should_be_missing != currently_missing {
                        data_changed = true;
                        if let Some(rel_table) = release.as_table_mut() {
                            if should_be_missing {
                                rel_table.insert("missing".to_string(), toml::Value::Boolean(true));
                            } else {
                                rel_table.remove("missing");
                            }
                        }
                    }

                    release_positions.push((uuid, position as i32 + 1, should_be_missing));
                }
            }
        }

        // Write back the file if data changed
        if data_changed {
            let content = toml::to_string(&data)?;
            fs::write(&path, content)?;
        }

        debug!("found {} release(s) (including missing) in collage {}", release_positions.len(), name);
        info!("updating cache for collage {}", name);

        // Update database
        conn.execute(
            "INSERT INTO collages (name, source_mtime) VALUES (?1, ?2)
             ON CONFLICT (name) DO UPDATE SET source_mtime = excluded.source_mtime",
            params![&name, &file_mtime],
        )?;

        // Delete and re-insert collage releases
        conn.execute("DELETE FROM collages_releases WHERE collage_name = ?1", params![&name])?;

        for (release_id, position, missing) in &release_positions {
            conn.execute(
                "INSERT INTO collages_releases (collage_name, release_id, position, missing)
                 VALUES (?1, ?2, ?3, ?4)",
                params![&name, &release_id, &position, &missing],
            )?;
        }

        // Update description_metas for all releases
        let mut desc_map = HashMap::new();
        if !release_positions.is_empty() {
            let release_ids: Vec<String> = release_positions.iter().map(|(id, _, _)| id.clone()).collect();
            let placeholders = vec!["?"; release_ids.len()].join(",");
            let query = format!(
                "SELECT id, releasetitle, originaldate, releasedate, releaseartist_names, releaseartist_roles
                 FROM releases_view WHERE id IN ({})",
                placeholders
            );
            let mut stmt = conn.prepare(&query)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(&release_ids), |row| {
                let id: String = row.get("id")?;
                let title: String = row.get("releasetitle")?;
                let original_date: Option<String> = row.get("originaldate")?;
                let release_date: Option<String> = row.get("releasedate")?;
                let date_str_to_parse = original_date.as_deref().filter(|s| !s.is_empty()).or(release_date.as_deref().filter(|s| !s.is_empty()));
                let date = RoseDate::parse(date_str_to_parse);
                let date_str = date.map(|d| format!("[{}]", d)).unwrap_or_else(|| "[0000-00-00]".to_string());

                let artist_names: String = row.get("releaseartist_names")?;
                let artist_roles: String = row.get("releaseartist_roles")?;
                let artists = _unpack_artists(c, &artist_names, &artist_roles, false);
                let artist_str = crate::audiotags::format_artist_string(&artists);

                let meta = format!("{} {} - {}", date_str, artist_str, title);
                Ok((id, meta))
            })?;

            for row in rows {
                let (id, meta) = row?;
                desc_map.insert(id, meta);
            }
        }

        // Now update the TOML data with description_meta
        if !desc_map.is_empty() {
            let mut data_changed = false;
            let content = fs::read_to_string(&path)?;
            let mut data: toml::Value = toml::from_str(&content)?;

            if let Some(releases_array) = data.get_mut("releases").and_then(|v| v.as_array_mut()) {
                for release in releases_array.iter_mut() {
                    if let Some(uuid) = release.get("uuid").and_then(|v| v.as_str()) {
                        if let Some(new_desc) = desc_map.get(uuid) {
                            let mut final_desc = new_desc.clone();
                            if release.get("missing").and_then(|v| v.as_bool()).unwrap_or(false) {
                                final_desc.push_str(" {MISSING}");
                            }

                            if let Some(table) = release.as_table_mut() {
                                let current = table.get("description_meta").and_then(|v| v.as_str()).unwrap_or("");
                                if current != final_desc {
                                    table.insert("description_meta".to_string(), toml::Value::String(final_desc));
                                    data_changed = true;
                                }
                            }
                        }
                    }
                }
            }

            // Write back the file if data changed
            if data_changed {
                let content = toml::to_string(&data)?;
                fs::write(&path, content)?;
            }
        }
    }

    Ok(())
}

pub fn update_cache_evict_nonexistent_collages(c: &Config) -> Result<()> {
    debug!("evicting cached collages that are not on disk");

    let collage_dir = c.music_source_dir.join("!collages");
    let mut collage_names = Vec::new();

    if collage_dir.exists() {
        for entry in fs::read_dir(&collage_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension() == Some(std::ffi::OsStr::new("toml")) {
                if let Some(stem) = path.file_stem() {
                    collage_names.push(stem.to_string_lossy().to_string());
                }
            }
        }
    }

    let conn = connect(c)?;

    if collage_names.is_empty() {
        // Delete all collages if none exist on disk
        let mut stmt = conn.prepare("DELETE FROM collages RETURNING name")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(0)?;
            info!("evicted missing collage {} from cache", name);
        }
    } else {
        let placeholders = vec!["?"; collage_names.len()].join(",");
        let query = format!("DELETE FROM collages WHERE name NOT IN ({}) RETURNING name", placeholders);
        let mut stmt = conn.prepare(&query)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(&collage_names))?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(0)?;
            info!("evicted missing collage {} from cache", name);
        }
    }

    Ok(())
}

/// Update the read cache to match the data for all stored playlists.
///
/// This is performance-optimized in a similar way to the update releases function. We:
///
/// 1. Execute one big SQL query at the start to fetch the relevant previous caches.
/// 2. Skip reading a file's data if the mtime has not changed since the previous cache update.
/// 3. Only execute a SQLite upsert if the read data differ from the previous caches.
///
/// However, we do not batch writes to the end of the function, nor do we process the playlists in
/// parallel. This is because we should have far fewer playlists than releases.
pub fn update_cache_for_playlists(c: &Config, playlist_names: Option<Vec<String>>, force: bool) -> Result<()> {
    let playlist_dir = c.music_source_dir.join("!playlists");
    fs::create_dir_all(&playlist_dir)?;

    // Find all playlist files
    let mut files = Vec::new();
    for entry in fs::read_dir(&playlist_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension() == Some(std::ffi::OsStr::new("toml")) && path.is_file() {
            if let Some(stem) = path.file_stem() {
                let name = stem.to_string_lossy().to_string();
                if playlist_names.as_ref().is_none_or(|names| names.contains(&name)) {
                    files.push((path.canonicalize()?, name, entry));
                }
            }
        }
    }

    debug!("refreshing the read cache for {} playlists", files.len());

    // Get existing playlists from cache
    let conn = connect(c)?;
    let mut cached_playlists = HashMap::new();

    if !files.is_empty() {
        let names: Vec<String> = files.iter().map(|(_, name, _)| name.clone()).collect();
        let placeholders = vec!["?"; names.len()].join(",");
        let query = format!("SELECT name, source_mtime FROM playlists WHERE name IN ({})", placeholders);

        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(&names), |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;

        for row in rows {
            let (name, mtime) = row?;
            cached_playlists.insert(name, mtime);
        }
    }

    // Process each playlist file
    for (path, name, entry) in files {
        let file_mtime = entry.metadata()?.modified()?.duration_since(UNIX_EPOCH)?.as_secs().to_string();

        // Check if we need to update
        let cached_mtime = cached_playlists.get(&name);
        if !force && cached_mtime == Some(&file_mtime) {
            debug!("skipping playlist {} because mtime has not changed", name);
            continue;
        }

        debug!("updating playlist {} in cache", name);

        // Check for cover image
        let cover_path = playlist_dir.join(format!("{}.jpg", name));
        let cover_path_str = if cover_path.exists() {
            Some(cover_path.to_string_lossy().to_string())
        } else {
            None
        };

        // Read and parse the playlist TOML file
        let content = fs::read_to_string(&path)?;
        let mut data: toml::Value = toml::from_str(&content)?;

        let _tracks = data.get("tracks").and_then(|v| v.as_array()).cloned().unwrap_or_default();

        let mut track_positions: Vec<(String, i32, bool)> = Vec::new();
        let mut data_changed = false;

        // Check which tracks exist and update missing status
        if let Some(tracks_array) = data.get_mut("tracks").and_then(|v| v.as_array_mut()) {
            for (position, track) in tracks_array.iter_mut().enumerate() {
                if let Some(uuid_val) = track.get("uuid").and_then(|v| v.as_str()) {
                    let uuid = uuid_val.to_string();
                    // Check if track exists in database
                    let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM tracks WHERE id = ?1)", params![&uuid], |row| row.get(0))?;

                    let currently_missing = track.get("missing").and_then(|v| v.as_bool()).unwrap_or(false);
                    let should_be_missing = !exists;

                    // Update missing status if needed
                    if should_be_missing != currently_missing {
                        data_changed = true;
                        if let Some(track_table) = track.as_table_mut() {
                            if should_be_missing {
                                track_table.insert("missing".to_string(), toml::Value::Boolean(true));
                            } else {
                                track_table.remove("missing");
                            }
                        }
                    }

                    track_positions.push((uuid, position as i32 + 1, should_be_missing));
                }
            }
        }

        // Write back the file if data changed
        if data_changed {
            let content = toml::to_string(&data)?;
            fs::write(&path, content)?;
        }

        debug!("found {} track(s) (including missing) in playlist {}", track_positions.len(), name);
        info!("updating cache for playlist {}", name);

        // Update database
        conn.execute(
            "INSERT INTO playlists (name, source_mtime, cover_path) VALUES (?1, ?2, ?3)
             ON CONFLICT (name) DO UPDATE SET
                source_mtime = excluded.source_mtime,
                cover_path = excluded.cover_path",
            params![&name, &file_mtime, &cover_path_str],
        )?;

        // Delete and re-insert playlist tracks
        conn.execute("DELETE FROM playlists_tracks WHERE playlist_name = ?1", params![&name])?;

        for (track_id, position, missing) in &track_positions {
            conn.execute(
                "INSERT INTO playlists_tracks (playlist_name, track_id, position, missing)
                 VALUES (?1, ?2, ?3, ?4)",
                params![&name, &track_id, &position, &missing],
            )?;
        }

        // Update description_metas for all tracks
        let mut desc_map = HashMap::new();
        if !track_positions.is_empty() {
            let track_ids: Vec<String> = track_positions.iter().map(|(id, _, _)| id.clone()).collect();
            let placeholders = vec!["?"; track_ids.len()].join(",");
            let query = format!(
                "SELECT
                    t.id,
                    t.tracktitle,
                    t.trackartist_names,
                    t.trackartist_roles,
                    r.originaldate,
                    r.releasedate
                FROM tracks_view t
                JOIN releases_view r ON r.id = t.release_id
                WHERE t.id IN ({})",
                placeholders
            );
            let mut stmt = conn.prepare(&query)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(&track_ids), |row| {
                let id: String = row.get(0)?;
                let title: String = row.get(1)?;
                let artist_names: String = row.get(2)?;
                let artist_roles: String = row.get(3)?;
                let original_date: Option<String> = row.get(4)?;
                let release_date: Option<String> = row.get(5)?;

                let date_str_to_parse = original_date.as_deref().filter(|s| !s.is_empty()).or(release_date.as_deref().filter(|s| !s.is_empty()));
                let date = RoseDate::parse(date_str_to_parse);
                let date_str = date.map(|d| format!("[{}]", d)).unwrap_or_else(|| "[0000-00-00]".to_string());

                let artists = _unpack_artists(c, &artist_names, &artist_roles, false);
                let artist_str = crate::audiotags::format_artist_string(&artists);

                let meta = format!("{} {} - {}", date_str, artist_str, title);
                Ok((id, meta))
            })?;

            for row in rows {
                let (id, meta) = row?;
                desc_map.insert(id, meta);
            }
        }

        // Now update the TOML data with description_meta
        if !desc_map.is_empty() {
            let mut data_changed = false;
            let content = fs::read_to_string(&path)?;
            let mut data: toml::Value = toml::from_str(&content)?;

            if let Some(tracks_array) = data.get_mut("tracks").and_then(|v| v.as_array_mut()) {
                for track in tracks_array.iter_mut() {
                    if let Some(uuid) = track.get("uuid").and_then(|v| v.as_str()) {
                        if let Some(new_desc) = desc_map.get(uuid) {
                            let mut final_desc = new_desc.clone();
                            if track.get("missing").and_then(|v| v.as_bool()).unwrap_or(false) {
                                final_desc.push_str(" {MISSING}");
                            }

                            if let Some(table) = track.as_table_mut() {
                                let current = table.get("description_meta").and_then(|v| v.as_str()).unwrap_or("");
                                if current != final_desc {
                                    table.insert("description_meta".to_string(), toml::Value::String(final_desc));
                                    data_changed = true;
                                }
                            }
                        }
                    }
                }
            }

            // Write back the file if data changed
            if data_changed {
                let content = toml::to_string(&data)?;
                fs::write(&path, content)?;
            }
        }
    }

    Ok(())
}

pub fn update_cache_evict_nonexistent_playlists(c: &Config) -> Result<()> {
    debug!("evicting cached playlists that are not on disk");

    let playlist_dir = c.music_source_dir.join("!playlists");
    let mut playlist_names = Vec::new();

    if playlist_dir.exists() {
        for entry in fs::read_dir(&playlist_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension() == Some(std::ffi::OsStr::new("toml")) {
                if let Some(stem) = path.file_stem() {
                    playlist_names.push(stem.to_string_lossy().to_string());
                }
            }
        }
    }

    let conn = connect(c)?;

    if playlist_names.is_empty() {
        // Delete all playlists if none exist on disk
        let mut stmt = conn.prepare("DELETE FROM playlists RETURNING name")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(0)?;
            info!("evicted missing playlist {} from cache", name);
        }
    } else {
        let placeholders = vec!["?"; playlist_names.len()].join(",");
        let query = format!("DELETE FROM playlists WHERE name NOT IN ({}) RETURNING name", placeholders);
        let mut stmt = conn.prepare(&query)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(&playlist_names))?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(0)?;
            info!("evicted missing playlist {} from cache", name);
        }
    }

    Ok(())
}

pub fn get_release(c: &Config, release_id: &str) -> Result<Option<Release>> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare("SELECT * FROM releases_view WHERE id = ?1")?;

    let release = stmt
        .query_row(params![release_id], |row| cached_release_from_view(c, row, true).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e))))
        .optional()?;

    Ok(release)
}

/// Fetch all releases. Can be filtered. By default, returns all releases.
///
/// The or_labels/or_genres/or_descriptors fields contain labels/genres/descriptors that we are going
/// to union together when filtering. We want releases that have at least one of the labels and at
/// least one of the genres.
///
/// Labels, Genres, and Descriptors are three separate fields, so we still intersect them together.
/// That is, to match, a release must match at least one of the labels and genres. But both labels
/// and genres must have a match.
pub fn list_releases(c: &Config, or_labels: Option<Vec<String>>, or_genres: Option<Vec<String>>, or_descriptors: Option<Vec<String>>) -> Result<Vec<Release>> {
    let mut filter_sql = String::new();
    let mut filter_params: Vec<String> = Vec::new();

    if let Some(labels) = or_labels {
        if !labels.is_empty() {
            let placeholders = vec!["?"; labels.len()].join(",");
            filter_sql.push_str(&format!(
                " AND id IN (
                    SELECT release_id FROM releases_labels
                    WHERE label IN ({})
                )",
                placeholders
            ));
            filter_params.extend(labels);
        }
    }

    if let Some(genres) = or_genres {
        if !genres.is_empty() {
            let placeholders = vec!["?"; genres.len()].join(",");
            filter_sql.push_str(&format!(
                " AND id IN (
                    SELECT release_id FROM releases_genres
                    WHERE genre IN ({})
                )",
                placeholders
            ));
            filter_params.extend(genres);
        }
    }

    if let Some(descriptors) = or_descriptors {
        if !descriptors.is_empty() {
            let placeholders = vec!["?"; descriptors.len()].join(",");
            filter_sql.push_str(&format!(
                " AND id IN (
                    SELECT release_id FROM releases_descriptors
                    WHERE descriptor IN ({})
                )",
                placeholders
            ));
            filter_params.extend(descriptors);
        }
    }

    let conn = connect(c)?;
    let query = format!("SELECT * FROM releases_view WHERE 1=1 {} ORDER BY id", filter_sql);
    let mut stmt = conn.prepare(&query)?;

    let releases = stmt
        .query_map(rusqlite::params_from_iter(&filter_params), |row| {
            cached_release_from_view(c, row, true).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(releases)
}

pub fn get_track(c: &Config, id: &str) -> Result<Option<Track>> {
    let conn = connect(c)?;

    // First get the track data to find release_id
    let track_query = "SELECT * FROM tracks_view WHERE id = ?";
    let mut track_stmt = conn.prepare(track_query)?;

    let track_result: Option<String> = track_stmt.query_row([id], |row| row.get("release_id")).optional()?;

    if let Some(release_id) = track_result {
        // Get the release
        let release = get_release(c, &release_id)?;
        if let Some(release) = release {
            // Now get the full track with the release
            let track = track_stmt.query_row([id], |row| {
                cached_track_from_view(c, row, Arc::new(release.clone()), true).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
            })?;
            Ok(Some(track))
        } else {
            // Release not found, even though track references it
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

pub fn list_tracks(c: &Config, track_ids: Option<Vec<String>>) -> Result<Vec<Track>> {
    let conn = connect(c)?;

    // Build query
    let query = if let Some(ref ids) = track_ids {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; ids.len()].join(",");
        format!("SELECT * FROM tracks_view WHERE id IN ({placeholders}) ORDER BY source_path")
    } else {
        "SELECT * FROM tracks_view ORDER BY source_path".to_string()
    };

    // First pass: collect release IDs and get releases
    let mut release_ids = HashSet::<String>::new();
    let mut releases = std::collections::HashMap::<String, Arc<Release>>::new();

    {
        let mut stmt = conn.prepare(&query)?;

        // Collect release IDs
        if let Some(ref ids) = track_ids {
            let rows = stmt.query_map(rusqlite::params_from_iter(ids), |row| row.get::<_, String>("release_id"))?;
            for release_id in rows {
                release_ids.insert(release_id?);
            }
        } else {
            let rows = stmt.query_map([], |row| row.get::<_, String>("release_id"))?;
            for release_id in rows {
                release_ids.insert(release_id?);
            }
        }
    }

    // Fetch all needed releases
    for release_id in &release_ids {
        if let Some(release) = get_release(c, release_id)? {
            releases.insert(release_id.clone(), Arc::new(release));
        }
    }

    // Second pass: build tracks with releases
    let mut tracks = Vec::new();
    let mut stmt = conn.prepare(&query)?;

    let params: Vec<&dyn rusqlite::ToSql> = if let Some(ref ids) = track_ids {
        ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect()
    } else {
        vec![]
    };

    let mut rows = stmt.query(params.as_slice())?;

    while let Some(row) = rows.next()? {
        let release_id: String = row.get("release_id")?;
        if let Some(release) = releases.get(&release_id) {
            let track = cached_track_from_view(c, row, release.clone(), true)?;
            tracks.push(track);
        }
    }

    Ok(tracks)
}

pub fn get_tracks_of_release(c: &Config, release: &Release) -> Result<Vec<Track>> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare("SELECT * FROM tracks_view WHERE release_id = ? ORDER BY tracknumber, id")?;
    let release_arc = Arc::new(release.clone());
    let tracks = stmt
        .query_map([&release.id], |row| {
            cached_track_from_view(c, row, release_arc.clone(), true).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(tracks)
}

pub fn list_collages(c: &Config) -> Result<Vec<Collage>> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare("SELECT name, source_mtime FROM collages ORDER BY name")?;
    let collages = stmt
        .query_map([], |row| {
            Ok(Collage {
                name: row.get(0)?,
                source_mtime: row.get(1)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(collages)
}

pub fn list_playlists(c: &Config) -> Result<Vec<Playlist>> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare("SELECT name, source_mtime, cover_path FROM playlists ORDER BY name")?;
    let playlists = stmt
        .query_map([], |row| {
            Ok(Playlist {
                name: row.get(0)?,
                source_mtime: row.get(1)?,
                cover_path: row.get::<_, Option<String>>(2)?.map(PathBuf::from),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(playlists)
}

pub fn list_descriptors(c: &Config) -> Result<Vec<DescriptorEntry>> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare(
        "
        SELECT DISTINCT d.descriptor,
               CASE WHEN COUNT(CASE WHEN r.new = false THEN 1 END) > 0 THEN false ELSE true END as only_new
        FROM releases_descriptors d
        JOIN releases r ON r.id = d.release_id
        GROUP BY d.descriptor
        ORDER BY d.descriptor
    ",
    )?;

    let descriptors = stmt
        .query_map([], |row| {
            Ok(DescriptorEntry {
                name: row.get(0)?,
                only_new_releases: row.get(1)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(descriptors)
}

pub fn list_labels(c: &Config) -> Result<Vec<LabelEntry>> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare(
        "
        SELECT DISTINCT l.label,
               CASE WHEN COUNT(CASE WHEN r.new = false THEN 1 END) > 0 THEN false ELSE true END as only_new
        FROM releases_labels l
        JOIN releases r ON r.id = l.release_id
        GROUP BY l.label
        ORDER BY l.label
    ",
    )?;

    let labels = stmt
        .query_map([], |row| {
            Ok(LabelEntry {
                name: row.get(0)?,
                only_new_releases: row.get(1)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(labels)
}

pub fn list_artists(c: &Config) -> Result<Vec<String>> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare(
        "
        SELECT DISTINCT artist
        FROM (
            SELECT artist FROM releases_artists
            UNION ALL
            SELECT artist FROM tracks_artists
        )
        ORDER BY artist
    ",
    )?;
    let artists = stmt.query_map([], |row| row.get(0))?.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(artists)
}

pub fn list_genres(c: &Config) -> Result<Vec<GenreEntry>> {
    let conn = connect(c)?;

    // First get all direct genres
    let mut stmt = conn.prepare(
        "
        SELECT DISTINCT g.genre,
               CASE WHEN COUNT(CASE WHEN r.new = false THEN 1 END) > 0 THEN false ELSE true END as only_new
        FROM (
            SELECT release_id, genre FROM releases_genres
            UNION ALL
            SELECT release_id, genre FROM releases_secondary_genres
        ) g
        JOIN releases r ON r.id = g.release_id
        GROUP BY g.genre
    ",
    )?;

    let mut genre_map: HashMap<String, bool> = HashMap::new();
    let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)))?;

    for row in rows {
        let (genre, only_new) = row?;
        genre_map.insert(genre.clone(), only_new);

        // Add parent genres
        if let Some(parent_genres) = TRANSITIVE_PARENT_GENRES.get(&genre) {
            for parent in parent_genres {
                // Parent genre is only_new if all its children are only_new
                genre_map.entry(parent.clone()).and_modify(|e| *e = *e && only_new).or_insert(only_new);
            }
        }
    }

    // Convert to sorted vector
    let mut genres: Vec<GenreEntry> = genre_map.into_iter().map(|(name, only_new_releases)| GenreEntry { name, only_new_releases }).collect();
    genres.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(genres)
}

pub fn artist_exists(c: &Config, artist_name: &str) -> Result<bool> {
    let conn = connect(c)?;

    // Check if artist exists in releases_artists or tracks_artists
    let mut stmt = conn.prepare(
        "
        SELECT EXISTS(
            SELECT 1 FROM releases_artists WHERE artist = ?1
            UNION
            SELECT 1 FROM tracks_artists WHERE artist = ?1
        )
    ",
    )?;

    let exists = stmt.query_row([artist_name], |row| row.get::<_, bool>(0))?;

    // If not found directly, check if it's an alias
    if !exists && !c.artist_aliases_map.is_empty() {
        // Check if this artist is an alias of another artist
        for (alias, main_artists) in &c.artist_aliases_map {
            if alias == artist_name {
                // This is an alias, check if any of the main artists exist
                for main_artist in main_artists {
                    if artist_exists(c, main_artist)? {
                        return Ok(true);
                    }
                }
            }
        }
    }

    Ok(exists)
}

pub fn genre_exists(c: &Config, genre_name: &str) -> Result<bool> {
    let conn = connect(c)?;

    // Check if genre exists in releases_genres, releases_secondary_genres, or as a parent genre
    let mut stmt = conn.prepare(
        "
        SELECT EXISTS(
            SELECT 1 FROM releases_genres WHERE genre = ?1
            UNION
            SELECT 1 FROM releases_secondary_genres WHERE genre = ?1
        )
    ",
    )?;

    let exists = stmt.query_row([genre_name], |row| row.get::<_, bool>(0))?;

    if exists {
        return Ok(true);
    }

    // Check if it's a parent genre of any existing genre
    for parent_genres in TRANSITIVE_PARENT_GENRES.values() {
        if parent_genres.iter().any(|g| g.as_str() == genre_name) {
            return Ok(true);
        }
    }

    Ok(false)
}

pub fn descriptor_exists(c: &Config, descriptor_name: &str) -> Result<bool> {
    let conn = connect(c)?;

    let mut stmt = conn.prepare(
        "
        SELECT EXISTS(
            SELECT 1 FROM releases_descriptors WHERE descriptor = ?1
        )
    ",
    )?;

    stmt.query_row([descriptor_name], |row| row.get::<_, bool>(0)).map_err(|e| e.into())
}

pub fn label_exists(c: &Config, label_name: &str) -> Result<bool> {
    let conn = connect(c)?;

    let mut stmt = conn.prepare(
        "
        SELECT EXISTS(
            SELECT 1 FROM releases_labels WHERE label = ?1
        )
    ",
    )?;

    stmt.query_row([label_name], |row| row.get::<_, bool>(0)).map_err(|e| e.into())
}

pub fn collage_exists(c: &Config, collage_name: &str) -> Result<bool> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare("SELECT EXISTS(SELECT 1 FROM collages WHERE name = ?1)")?;
    stmt.query_row([collage_name], |row| row.get::<_, bool>(0)).map_err(|e| e.into())
}

pub fn playlist_exists(c: &Config, playlist_name: &str) -> Result<bool> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare("SELECT EXISTS(SELECT 1 FROM playlists WHERE name = ?1)")?;
    stmt.query_row([playlist_name], |row| row.get::<_, bool>(0)).map_err(|e| e.into())
}

pub fn track_within_release(c: &Config, track_id: &str, release_id: &str) -> Result<bool> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare(
        "
        SELECT EXISTS(
            SELECT 1 FROM tracks WHERE id = ?1 AND release_id = ?2
        )
    ",
    )?;
    stmt.query_row([track_id, release_id], |row| row.get::<_, bool>(0)).map_err(|e| e.into())
}

pub fn track_within_playlist(c: &Config, track_id: &str, playlist_name: &str) -> Result<bool> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare(
        "
        SELECT EXISTS(
            SELECT 1 FROM playlists_tracks WHERE track_id = ?1 AND playlist_name = ?2
        )
    ",
    )?;
    stmt.query_row([track_id, playlist_name], |row| row.get::<_, bool>(0)).map_err(|e| e.into())
}

pub fn release_within_collage(c: &Config, release_id: &str, collage_name: &str) -> Result<bool> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare(
        "
        SELECT EXISTS(
            SELECT 1 FROM collages_releases WHERE release_id = ?1 AND collage_name = ?2
        )
    ",
    )?;
    stmt.query_row([release_id, collage_name], |row| row.get::<_, bool>(0)).map_err(|e| e.into())
}

/// Get a formatted string for logging a release (artists - date. title)
pub fn get_release_logtext(c: &Config, release_id: &str) -> Result<String> {
    let release = get_release(c, release_id)?;
    match release {
        Some(r) => {
            let artists = r.releaseartists.main.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(" & ");

            let date_part = if let Some(date) = r.releasedate {
                match date.year {
                    Some(year) => format!("{}", year),
                    None => "Unknown".to_string(),
                }
            } else {
                "Unknown".to_string()
            };

            Ok(format!("{} - {}. {}", artists, date_part, r.releasetitle))
        }
        None => Ok("Unknown Release".to_string()),
    }
}

/// Get a formatted string for logging a track (artists - title [year].extension)
pub fn get_track_logtext(c: &Config, track_id: &str) -> Result<String> {
    let track = get_track(c, track_id)?;
    match track {
        Some(t) => {
            let artists = t.trackartists.main.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(" & ");

            let date_part = if let Some(date) = t.release.releasedate {
                match date.year {
                    Some(year) => format!("[{}]", year),
                    None => "[Unknown]".to_string(),
                }
            } else {
                "[Unknown]".to_string()
            };

            let extension = t.source_path.extension().and_then(|e| e.to_str()).unwrap_or("unknown");

            Ok(format!("{} - {} {}.{}", artists, t.tracktitle, date_part, extension))
        }
        None => Ok("Unknown Track".to_string()),
    }
}

/// Get a collage by name
pub fn get_collage(c: &Config, collage_name: &str) -> Result<Option<Collage>> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare("SELECT name, source_mtime FROM collages WHERE name = ?")?;

    let collage = stmt
        .query_row([collage_name], |row| {
            Ok(Collage {
                name: row.get(0)?,
                source_mtime: row.get(1)?,
            })
        })
        .optional()?;

    Ok(collage)
}

/// Get all releases in a collage
pub fn get_collage_releases(c: &Config, collage_name: &str) -> Result<Vec<Release>> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare(
        "SELECT release_id FROM collages_releases
         WHERE collage_name = ? AND NOT missing
         ORDER BY position",
    )?;

    let release_ids: Vec<String> = stmt.query_map([collage_name], |row| row.get(0))?.collect::<std::result::Result<Vec<_>, _>>()?;

    let mut releases = Vec::new();
    for id in release_ids {
        if let Some(release) = get_release(c, &id)? {
            releases.push(release);
        }
    }

    Ok(releases)
}

/// Get a playlist by name
pub fn get_playlist(c: &Config, playlist_name: &str) -> Result<Option<Playlist>> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare("SELECT name, source_mtime, cover_path FROM playlists WHERE name = ?")?;

    let playlist = stmt
        .query_row([playlist_name], |row| {
            let cover_path: Option<String> = row.get(2)?;
            Ok(Playlist {
                name: row.get(0)?,
                source_mtime: row.get(1)?,
                cover_path: cover_path.map(PathBuf::from),
            })
        })
        .optional()?;

    Ok(playlist)
}

/// Get all tracks in a playlist
pub fn get_playlist_tracks(c: &Config, playlist_name: &str) -> Result<Vec<Track>> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare(
        "SELECT track_id FROM playlists_tracks
         WHERE playlist_name = ? AND NOT missing
         ORDER BY position",
    )?;

    let track_ids: Vec<String> = stmt.query_map([playlist_name], |row| row.get(0))?.collect::<std::result::Result<Vec<_>, _>>()?;

    let mut tracks = Vec::new();
    for id in track_ids {
        if let Some(track) = get_track(c, &id)? {
            tracks.push(track);
        }
    }

    Ok(tracks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;
    use std::collections::HashMap;

    /// Helper function to create a test config with initialized database schema
    fn config_with_db() -> (Config, tempfile::TempDir) {
        let (config, temp_dir) = testing::config();
        maybe_invalidate_cache_database(&config).unwrap();
        (config, temp_dir)
    }

    #[test]
    fn test_split() {
        let _ = testing::init();
        assert_eq!(_split(""), Vec::<String>::new());
        assert_eq!(_split("a ¬ b ¬ c"), vec!["a", "b", "c"]);
        assert_eq!(_split("single"), vec!["single"]);
    }

    #[test]
    fn test_unpack() {
        let _ = testing::init();
        assert_eq!(_unpack(&[]), Vec::<Vec<&str>>::new());
        assert_eq!(_unpack(&["a ¬ b", "1 ¬ 2"]), vec![vec!["a", "1"], vec!["b", "2"]]);
        assert_eq!(_unpack(&["a", "1 ¬ 2"]), vec![vec!["a", "1"], vec!["", "2"]]);
    }

    #[test]
    fn test_lock_names() {
        let _ = testing::init();
        assert_eq!(release_lock_name("abc123"), "release-abc123");
        assert_eq!(collage_lock_name("my-collage"), "collage-my-collage");
        assert_eq!(playlist_lock_name("my-playlist"), "playlist-my-playlist");
    }

    #[test]
    fn test_stored_data_file_regex() {
        let _ = testing::init();

        // Test valid filenames
        assert!(STORED_DATA_FILE_REGEX.is_match(".rose.abc123.toml"));
        assert!(STORED_DATA_FILE_REGEX.is_match(".rose.my-release-id.toml"));
        assert!(STORED_DATA_FILE_REGEX.is_match(".rose.UUID-1234.toml"));

        // Test invalid filenames
        assert!(!STORED_DATA_FILE_REGEX.is_match("rose.abc123.toml")); // missing leading dot
        assert!(!STORED_DATA_FILE_REGEX.is_match(".rose.abc123.json")); // wrong extension
        assert!(!STORED_DATA_FILE_REGEX.is_match(".rose..toml")); // empty ID
        assert!(!STORED_DATA_FILE_REGEX.is_match(".rose.abc.def.toml")); // multiple dots in ID

        // Test capturing the ID
        if let Some(captures) = STORED_DATA_FILE_REGEX.captures(".rose.my-id-123.toml") {
            assert_eq!(&captures[1], "my-id-123");
        } else {
            panic!("Failed to capture ID from valid filename");
        }
    }

    #[test]
    fn test_stored_data_file_defaults() {
        let _ = testing::init();

        // Test default_true
        assert!(default_true());

        // Test default_added_at returns a valid ISO8601 timestamp
        let timestamp = default_added_at();
        assert!(timestamp.contains('T'));
        assert!(timestamp.ends_with('Z'));

        // Test deserializing with defaults
        let json = "{}";
        let data: StoredDataFile = serde_json::from_str(json).unwrap();
        assert!(data.new);
        assert!(data.added_at.contains('T'));
    }

    #[test]
    fn test_maybe_invalidate_cache_database() {
        let _ = testing::init();
        let (config, _temp_dir) = testing::seeded_cache();

        // First call should create the database
        maybe_invalidate_cache_database(&config).unwrap();
        assert!(config.cache_database_path().exists());

        // Second call should not recreate it since nothing changed
        let mtime_before = std::fs::metadata(config.cache_database_path()).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10)); // Ensure time difference
        maybe_invalidate_cache_database(&config).unwrap();
        let mtime_after = std::fs::metadata(config.cache_database_path()).unwrap().modified().unwrap();
        assert_eq!(mtime_before, mtime_after);
    }

    // Tests ported from py-impl-reference/rose/cache_test.py

    #[test]
    fn test_schema() {
        let (config, _temp_dir) = testing::config();

        // Calculate the expected schema hash
        let mut hasher = Sha256::new();
        hasher.update(CACHE_SCHEMA.as_bytes());
        let expected_schema_hash = format!("{:x}", hasher.finalize());

        // Initialize the database
        maybe_invalidate_cache_database(&config).unwrap();

        // Check that the schema was properly initialized
        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT schema_hash, config_hash, version FROM _schema_hash").unwrap();
        let result = stmt.query_row([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))).unwrap();

        assert_eq!(result.0, expected_schema_hash);
        assert!(!result.1.is_empty()); // config_hash should be populated
        assert_eq!(result.2, VERSION);
    }

    #[test]
    fn test_migration() {
        let (config, _temp_dir) = testing::config();

        // First, ensure the database exists
        maybe_invalidate_cache_database(&config).unwrap();

        // Delete and recreate with old schema
        std::fs::remove_file(config.cache_database_path()).unwrap();
        {
            let conn = Connection::open(config.cache_database_path()).unwrap();
            conn.execute(
                "CREATE TABLE _schema_hash (
                    schema_hash TEXT,
                    config_hash TEXT,
                    version TEXT,
                    PRIMARY KEY (schema_hash, config_hash, version)
                )",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO _schema_hash (schema_hash, config_hash, version)
                 VALUES ('haha', 'lala', 'blabla')",
                [],
            )
            .unwrap();
        }

        // Calculate the expected schema hash
        let mut hasher = Sha256::new();
        hasher.update(CACHE_SCHEMA.as_bytes());
        let expected_schema_hash = format!("{:x}", hasher.finalize());

        // Run the migration
        maybe_invalidate_cache_database(&config).unwrap();

        // Check that the database was migrated
        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT schema_hash, config_hash, version FROM _schema_hash").unwrap();
        let result = stmt.query_row([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))).unwrap();

        assert_eq!(result.0, expected_schema_hash);
        assert!(!result.1.is_empty()); // config_hash should be populated
        assert_eq!(result.2, VERSION);

        // Check that there's only one row
        let count: i32 = conn.query_row("SELECT COUNT(*) FROM _schema_hash", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_locks() {
        let (config, _temp_dir) = testing::config();
        let lock_name = "lol";

        // Initialize database with proper schema
        maybe_invalidate_cache_database(&config).unwrap();

        // Ensure locks table exists by creating and dropping a dummy lock
        {
            let conn = connect(&config).unwrap();
            let _ = conn.execute("DELETE FROM locks WHERE 1=1", []);
        }

        // Test that the locking and timeout work
        let start = std::time::Instant::now();
        let _lock1 = lock(&config, lock_name, 0.2).unwrap();
        let lock1_acq = start.elapsed();

        // Try to acquire the same lock in a different thread
        let config_clone = config.clone();
        let lock_name_clone = lock_name.to_string();
        let handle = std::thread::spawn(move || {
            let thread_start = std::time::Instant::now();
            let _lock2 = lock(&config_clone, &lock_name_clone, 0.2).unwrap();
            thread_start.elapsed()
        });

        // Sleep a bit to ensure the thread tries to acquire the lock
        std::thread::sleep(Duration::from_millis(50));
        drop(_lock1); // Release the first lock

        let lock2_duration = handle.join().unwrap();

        // Assert that we acquired the first lock quickly
        assert!(lock1_acq.as_secs_f64() < 0.08);
        // Assert that the second lock had to wait
        assert!(lock2_duration.as_secs_f64() > 0.03);

        // Test that releasing a lock actually works
        let start = std::time::Instant::now();
        {
            let _lock1 = lock(&config, lock_name, 0.2).unwrap();
            let _lock1_acq = start.elapsed();
        } // lock1 is dropped here

        let _lock2 = lock(&config, lock_name, 0.2).unwrap();
        let lock2_acq = start.elapsed();

        // Assert that we acquired both locks quickly (no waiting)
        assert!(lock2_acq.as_secs_f64() < 0.10);
    }

    #[test]
    fn test_update_cache_all() {
        let _ = testing::init();
        let (config, _temp_dir) = testing::seeded_cache();

        // Test that the update all function works
        // Note: The seeded_cache already has test releases, so we just need to:
        // 1. Add a fake release to the database that doesn't exist on disk
        // 2. Call update_cache
        // 3. Verify the fake release was pruned and real releases are in cache

        // Insert a fake release that doesn't exist on disk
        let conn = connect(&config).unwrap();
        conn.execute(
            "INSERT INTO releases (id, source_path, added_at, datafile_mtime, title, releasetype, disctotal, new, metahash)
             VALUES ('aaaaaa', 'nonexistent', '2000-01-01T00:00:00+00:00', '999', 'aa', 'unknown', 0, 0, '0')",
            [],
        )
        .unwrap();
        drop(conn);

        // Run update_cache
        update_cache(&config, false, false).unwrap();

        // Verify results
        let conn = connect(&config).unwrap();

        // Check that we have the correct number of releases (seeded cache has 4 test releases)
        let count: i32 = conn.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 4, "Should have 4 releases after update");

        // Check that the fake release was deleted
        let exists: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM releases WHERE id = 'aaaaaa')", [], |row| row.get(0)).unwrap();
        assert!(!exists, "Fake release should have been deleted");

        // Check that we have tracks (seeded cache has 5 tracks based on the debug output)
        let track_count: i32 = conn.query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0)).unwrap();
        assert!(track_count > 0, "Should have tracks after update");
    }

    #[test]
    fn test_update_cache_multiprocessing() {
        let _ = testing::init();
        let (config, _temp_dir) = testing::seeded_cache();

        // Test that the update function works with multiprocessing forced
        // This currently falls back to single-process mode since multiprocessing isn't implemented yet
        update_cache_for_releases(&config, None, false, true).unwrap();

        let conn = connect(&config).unwrap();

        // Check that we have the expected releases
        let count: i32 = conn.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 4, "Should have 4 releases after multiprocessing update");

        // Check that we have tracks
        let track_count: i32 = conn.query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0)).unwrap();
        assert!(track_count > 0, "Should have tracks after multiprocessing update");
    }

    #[test]
    fn test_update_cache_releases() {
        let _ = testing::init();
        let (config, _temp_dir) = testing::seeded_cache();

        // Run update for all releases in the music source directory
        update_cache_for_releases(&config, None, false, false).unwrap();

        // Check that release directories were given UUIDs
        let mut release_id = None;
        let mut release_path = None;
        for entry in fs::read_dir(&config.music_source_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() && path.file_name().unwrap().to_str().unwrap().starts_with("r") {
                for file in fs::read_dir(&path).unwrap() {
                    let file = file.unwrap();
                    let filename = file.file_name();
                    let filename_str = filename.to_string_lossy();
                    if let Some(captures) = STORED_DATA_FILE_REGEX.captures(&filename_str) {
                        release_id = Some(captures.get(1).unwrap().as_str().to_string());
                        release_path = Some(path.clone());
                        break;
                    }
                }
                if release_id.is_some() {
                    break;
                }
            }
        }

        assert!(release_id.is_some(), "Should have found a release with UUID");

        // Check that release metadata exists in database
        let conn = connect(&config).unwrap();

        if let Some(id) = release_id {
            // Check basic release info
            let result: Option<(String, String, String, String, bool)> = conn
                .query_row("SELECT id, source_path, title, releasetype, new FROM releases WHERE id = ?", [&id], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get::<_, i32>(4)? != 0))
                })
                .optional()
                .unwrap();

            assert!(result.is_some(), "Release should exist in database");
            let (_id, source_path, title, releasetype, new) = result.unwrap();

            if let Some(ref path) = release_path {
                assert_eq!(source_path, path.to_string_lossy(), "Source path should match");
            }
            assert!(!title.is_empty(), "Release should have a title");
            assert!(!releasetype.is_empty(), "Release should have a type");
            assert!(new, "Release should be marked as new");

            // Check that tracks exist for the release
            let track_count: i32 = conn.query_row("SELECT COUNT(*) FROM tracks WHERE release_id = ?", [&id], |row| row.get(0)).unwrap();
            assert!(track_count > 0, "Release should have tracks");
        }
    }

    #[test]
    fn test_update_cache_releases_uncached_with_existing_id() {
        // Test that IDs in filenames are read and preserved.
        let (config, _temp_dir) = config_with_db();

        let release_dir = config.music_source_dir.join("Test Release 2");
        testing::copy_dir_all(Path::new("testdata/Test Release 2"), &release_dir).unwrap();
        update_cache_for_releases(&config, Some(vec![release_dir.clone()]), false, false).unwrap();

        // Check that the release directory was given a UUID.
        let mut release_id: Option<String> = None;
        for entry in fs::read_dir(&release_dir).unwrap() {
            let entry = entry.unwrap();
            let filename = entry.file_name();
            let filename_str = filename.to_string_lossy();
            if let Some(captures) = STORED_DATA_FILE_REGEX.captures(&filename_str) {
                release_id = Some(captures.get(1).unwrap().as_str().to_string());
            }
        }
        assert_eq!(release_id.unwrap(), "ilovecarly"); // Hardcoded ID for testing.
    }

    #[test]
    fn test_update_cache_releases_preserves_track_ids_across_rebuilds() {
        // Test that track IDs are preserved across cache rebuilds.
        let (config, _temp_dir) = config_with_db();

        let release_dir = config.music_source_dir.join("Test Release 3");
        testing::copy_dir_all(Path::new("testdata/Test Release 3"), &release_dir).unwrap();
        update_cache_for_releases(&config, Some(vec![release_dir.clone()]), false, false).unwrap();

        let first_track_ids: HashSet<String> = {
            let conn = connect(&config).unwrap();
            let mut stmt = conn.prepare("SELECT id FROM tracks").unwrap();
            stmt.query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap()
        };

        // Nuke the database.
        fs::remove_file(config.cache_database_path()).unwrap();
        maybe_invalidate_cache_database(&config).unwrap();

        // Repeat cache population.
        update_cache_for_releases(&config, Some(vec![release_dir]), false, false).unwrap();

        let second_track_ids: HashSet<String> = {
            let conn = connect(&config).unwrap();
            let mut stmt = conn.prepare("SELECT id FROM tracks").unwrap();
            stmt.query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap()
        };

        // Assert IDs are equivalent.
        assert_eq!(first_track_ids, second_track_ids);
    }

    #[test]
    fn test_update_cache_releases_writes_ids_to_tags() {
        // Test that track IDs and release IDs are written to files.
        let (config, _temp_dir) = config_with_db();

        let release_dir = config.music_source_dir.join("Test Release 3");
        testing::copy_dir_all(Path::new("testdata/Test Release 3"), &release_dir).unwrap();

        // Check that IDs are not present initially
        let af = AudioTags::from_file(&release_dir.join("01.m4a")).unwrap();
        assert!(af.id.is_none());
        assert!(af.release_id.is_none());

        let af = AudioTags::from_file(&release_dir.join("02.m4a")).unwrap();
        assert!(af.id.is_none());
        assert!(af.release_id.is_none());

        update_cache_for_releases(&config, Some(vec![release_dir.clone()]), false, false).unwrap();

        // Check that IDs were written
        let af = AudioTags::from_file(&release_dir.join("01.m4a")).unwrap();
        assert!(af.id.is_some());
        assert!(af.release_id.is_some());

        let af = AudioTags::from_file(&release_dir.join("02.m4a")).unwrap();
        assert!(af.id.is_some());
        assert!(af.release_id.is_some());
    }

    #[test]
    fn test_foreign_key_debug() {
        let (config, _temp_dir) = config_with_db();

        let conn = connect(&config).unwrap();

        // Manually insert a release
        conn.execute(
            "INSERT INTO releases (id, source_path, added_at, datafile_mtime, title, releasetype, disctotal, metahash, new)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params!["test-release-id", "/test/path", "2024-01-01T00:00:00Z", "999", "Test Release", "album", 1, "test-hash", true],
        )
        .unwrap();

        // Try to insert an artist
        match conn.execute(
            "INSERT INTO releases_artists (release_id, artist, role, position) VALUES (?1, ?2, ?3, ?4)",
            params!["test-release-id", "Test Artist", "main", 0],
        ) {
            Ok(_) => println!("Artist insert succeeded"),
            Err(e) => panic!("Artist insert failed: {:?}", e),
        }
    }

    #[test]
    #[ignore = "M4A custom tags (IDs) not being written due to lofty limitation - see audiotags.rs test_flush_m4a"]
    fn test_update_cache_releases_already_fully_cached() {
        // Test that a fully cached release No Ops when updated again
        let (config, _temp_dir) = config_with_db();

        // Initialize database schema
        maybe_invalidate_cache_database(&config).unwrap();

        let release_dir = config.music_source_dir.join("Test Release 1");

        // Copy test release data
        let src_dir = std::path::Path::new("testdata/Test Release 1");
        testing::copy_dir_all(src_dir, &release_dir).unwrap();

        // Update cache twice
        update_cache_for_releases(&config, Some(vec![release_dir.clone()]), false, false).unwrap();
        update_cache_for_releases(&config, Some(vec![release_dir.clone()]), false, false).unwrap();

        // Assert that the release metadata was read correctly
        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT id, source_path, title, releasetype, releasedate, new FROM releases").unwrap();
        let mut rows = stmt.query([]).unwrap();

        let row = rows.next().unwrap().unwrap();
        let source_path: String = row.get(1).unwrap();
        let title: String = row.get(2).unwrap();
        let releasetype: String = row.get(3).unwrap();
        let releasedate: Option<String> = row.get(4).unwrap();
        let new: bool = row.get(5).unwrap();

        assert_eq!(source_path, release_dir.to_string_lossy());
        assert_eq!(title, "I Love Blackpink");
        assert_eq!(releasetype, "album");
        assert_eq!(releasedate.as_deref(), Some("1990-02-05"));
        assert!(new);
    }

    #[test]
    fn test_update_cache_releases_to_empty_multi_value_tag() {
        // Test that 1:many relations are properly emptied when they are updated from something to nothing.
        let (config, _temp_dir) = config_with_db();

        let release_dir = config.music_source_dir.join("Test Release 1");
        testing::copy_dir_all(Path::new("testdata/Test Release 1"), &release_dir).unwrap();

        update_cache(&config, false, false).unwrap();

        // Check that labels exist initially
        let conn = connect(&config).unwrap();
        let has_labels: bool = conn.query_row("SELECT EXISTS(SELECT * FROM releases_labels)", [], |row| row.get(0)).unwrap();
        assert!(has_labels);
        drop(conn);

        // Clear labels from all tracks
        for filename in ["01.m4a", "02.m4a"] {
            let mut af = AudioTags::from_file(&release_dir.join(filename)).unwrap();
            af.label = vec![];
            // Add delay to ensure mtime changes (some filesystems have 1-second resolution)
            std::thread::sleep(std::time::Duration::from_secs(1));
            af.flush(&config, false).unwrap();
        }

        update_cache(&config, false, false).unwrap();

        // Check that labels no longer exist
        let conn = connect(&config).unwrap();
        let has_labels: bool = conn.query_row("SELECT EXISTS(SELECT * FROM releases_labels)", [], |row| row.get(0)).unwrap();
        assert!(!has_labels);
    }

    #[test]
    fn test_update_cache_releases_disk_update_to_previously_cached() {
        // Test that a cached release is updated after a track updates.
        let (config, _temp_dir) = config_with_db();

        let release_dir = config.music_source_dir.join("Test Release 1");
        testing::copy_dir_all(Path::new("testdata/Test Release 1"), &release_dir).unwrap();
        update_cache_for_releases(&config, Some(vec![release_dir.clone()]), false, false).unwrap();

        // Update the database and touch a file to simulate modification
        let conn = connect(&config).unwrap();
        conn.execute("UPDATE releases SET title = 'An Uncool Album'", []).unwrap();
        drop(conn);

        // Touch the file to update its modification time
        let track_path = release_dir.join("01.m4a");
        // Read and write back the file to update mtime
        let content = fs::read(&track_path).unwrap();
        // Add a delay to ensure mtime changes (some filesystems have 1-second resolution)
        std::thread::sleep(std::time::Duration::from_secs(1));
        fs::write(&track_path, content).unwrap();

        update_cache_for_releases(&config, Some(vec![release_dir.clone()]), false, false).unwrap();

        // Assert that the release metadata was re-read and updated correctly.
        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT id, source_path, title, releasetype, releasedate, new FROM releases").unwrap();
        let mut rows = stmt.query([]).unwrap();
        let row = rows.next().unwrap().unwrap();

        let source_path: String = row.get(1).unwrap();
        let title: String = row.get(2).unwrap();
        let releasetype: String = row.get(3).unwrap();
        let releasedate: Option<String> = row.get(4).unwrap();
        let new: bool = row.get(5).unwrap();

        assert_eq!(source_path, release_dir.to_string_lossy());
        assert_eq!(title, "I Love Blackpink");
        assert_eq!(releasetype, "album");
        assert_eq!(releasedate.as_deref(), Some("1990-02-05"));
        assert!(new);
    }

    #[test]
    fn test_update_cache_releases_disk_update_to_datafile() {
        // Test that a cached release is updated after a datafile updates.
        let (config, _temp_dir) = config_with_db();

        let release_dir = config.music_source_dir.join("Test Release 1");
        testing::copy_dir_all(Path::new("testdata/Test Release 1"), &release_dir).unwrap();
        update_cache_for_releases(&config, Some(vec![release_dir.clone()]), false, false).unwrap();

        let conn = connect(&config).unwrap();
        conn.execute("UPDATE releases SET datafile_mtime = '0', new = false", []).unwrap();
        drop(conn);

        update_cache_for_releases(&config, Some(vec![release_dir]), false, false).unwrap();

        // Assert that the release metadata was re-read and updated correctly.
        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT new, added_at FROM releases").unwrap();
        let mut rows = stmt.query([]).unwrap();
        let row = rows.next().unwrap().unwrap();

        let new: bool = row.get(0).unwrap();
        let added_at: String = row.get(1).unwrap();

        assert!(new);
        assert!(!added_at.is_empty());
    }

    #[test]
    fn test_update_cache_releases_disk_upgrade_old_datafile() {
        // Test that a legacy invalid datafile is upgraded on index.
        let (config, _temp_dir) = config_with_db();

        let release_dir = config.music_source_dir.join("Test Release 1");
        testing::copy_dir_all(Path::new("testdata/Test Release 1"), &release_dir).unwrap();

        let datafile = release_dir.join(".rose.lalala.toml");
        fs::write(&datafile, "").unwrap(); // Create empty file

        update_cache_for_releases(&config, Some(vec![release_dir]), false, false).unwrap();

        // Assert that the release metadata was re-read and updated correctly.
        let (id, new, added_at) = {
            let conn = connect(&config).unwrap();
            let mut stmt = conn.prepare("SELECT id, new, added_at FROM releases").unwrap();
            let mut rows = stmt.query([]).unwrap();
            let row = rows.next().unwrap().unwrap();

            (row.get::<_, String>(0).unwrap(), row.get::<_, bool>(1).unwrap(), row.get::<_, String>(2).unwrap())
        };

        assert_eq!(id, "lalala");
        assert!(new);
        assert!(!added_at.is_empty());

        // Check datafile contents
        let data = fs::read_to_string(&datafile).unwrap();
        assert!(data.contains("new = true"));
        assert!(data.contains("added_at = "));
    }

    #[test]
    fn test_update_cache_releases_source_path_renamed() {
        // Test that a cached release is updated after a directory rename.
        let (config, _temp_dir) = config_with_db();

        let release_dir = config.music_source_dir.join("Test Release 1");
        testing::copy_dir_all(Path::new("testdata/Test Release 1"), &release_dir).unwrap();
        update_cache_for_releases(&config, Some(vec![release_dir.clone()]), false, false).unwrap();

        let moved_release_dir = config.music_source_dir.join("moved lol");
        fs::rename(&release_dir, &moved_release_dir).unwrap();
        update_cache_for_releases(&config, Some(vec![moved_release_dir.clone()]), false, false).unwrap();

        // Assert that the release metadata was re-read and updated correctly.
        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT id, source_path, title, releasetype, releasedate, new FROM releases").unwrap();
        let mut rows = stmt.query([]).unwrap();
        let row = rows.next().unwrap().unwrap();

        let source_path: String = row.get(1).unwrap();
        let title: String = row.get(2).unwrap();
        let releasetype: String = row.get(3).unwrap();
        let releasedate: Option<String> = row.get(4).unwrap();
        let new: bool = row.get(5).unwrap();

        assert_eq!(source_path, moved_release_dir.to_string_lossy());
        assert_eq!(title, "I Love Blackpink");
        assert_eq!(releasetype, "album");
        assert_eq!(releasedate.as_deref(), Some("1990-02-05"));
        assert!(new);
    }

    #[test]
    fn test_update_cache_releases_delete_nonexistent() {
        let (config, _temp_dir) = config_with_db();

        // Initialize database
        maybe_invalidate_cache_database(&config).unwrap();

        // Insert a release with nonexistent path
        let nonexistent_path = config.music_source_dir.join("nonexistent");
        let conn = connect(&config).unwrap();
        conn.execute(
            "INSERT INTO releases (id, source_path, added_at, datafile_mtime, title, releasetype, disctotal, new, metahash)
             VALUES ('aaaaaa', ?1, '0000-01-01T00:00:00+00:00', '999', 'aa', 'unknown', 1, 0, '0')",
            [nonexistent_path.to_str().unwrap()],
        )
        .unwrap();
        drop(conn);

        // Run eviction
        update_cache_evict_nonexistent_releases(&config).unwrap();

        // Check that the release was deleted
        let conn = connect(&config).unwrap();
        let count: i32 = conn.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_update_cache_releases_enforces_max_len() {
        // Test that filenames are truncated when max_filename_bytes is set
        let (mut config, _temp_dir) = config_with_db();
        config.rename_source_files = true;
        config.max_filename_bytes = 15;

        testing::copy_dir_all(Path::new("testdata/Test Release 1"), &config.music_source_dir.join("a")).unwrap();
        testing::copy_dir_all(Path::new("testdata/Test Release 1"), &config.music_source_dir.join("b")).unwrap();
        fs::copy(Path::new("testdata/Test Release 1/01.m4a"), config.music_source_dir.join("b").join("03.m4a")).unwrap();

        update_cache_for_releases(&config, None, false, false).unwrap();

        let entries: HashSet<_> =
            fs::read_dir(&config.music_source_dir).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect();

        assert!(entries.contains("BLACKPINK - 199"));
        assert!(entries.contains("BLACKPINK - [2]"));

        // Nondeterministic: Pick the one with the extra file.
        let dir_1 = config.music_source_dir.join("BLACKPINK - 199");
        let dir_2 = config.music_source_dir.join("BLACKPINK - [2]");

        let children_1: HashSet<_> = fs::read_dir(&dir_1).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();

        let children_2: HashSet<_> = fs::read_dir(&dir_2).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();

        let (files, release_dir) = if children_1.len() > children_2.len() {
            (children_1, dir_1)
        } else {
            (children_2, dir_2)
        };

        assert!(files.contains(&release_dir.join("01. Track 1.m4a")));
        assert!(files.contains(&release_dir.join("01. Tra [2].m4a")));
    }

    #[test]
    fn test_update_cache_releases_skips_empty_directory() {
        let (config, _temp_dir) = config_with_db();

        // Initialize database
        maybe_invalidate_cache_database(&config).unwrap();

        // Create an empty directory with a non-audio file
        let rd = config.music_source_dir.join("lalala");
        fs::create_dir_all(&rd).unwrap();
        fs::write(rd.join("ignoreme.file"), "").unwrap();

        // Try to update cache for this directory
        update_cache_for_releases(&config, Some(vec![rd]), false, false).unwrap();

        // Check that no releases were added
        let conn = connect(&config).unwrap();
        let count: i32 = conn.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_update_cache_releases_uncaches_empty_directory() {
        // Test that a previously-cached directory with no audio files now is cleared from cache
        let (config, _temp_dir) = config_with_db();

        // Initialize database schema
        maybe_invalidate_cache_database(&config).unwrap();

        // Create a simple release directory with a FLAC file that supports custom tags
        let release_dir = config.music_source_dir.join("Test FLAC Release");
        fs::create_dir_all(&release_dir).unwrap();

        // Copy a FLAC file from the tagger test data
        let src_file = std::path::Path::new("testdata/Tagger/track1.flac");
        let dst_file = release_dir.join("01.flac");
        fs::copy(src_file, &dst_file).unwrap();

        // Update cache with release
        update_cache_for_releases(&config, Some(vec![release_dir.clone()]), false, false).unwrap();

        // Verify the release was cached
        let conn = connect(&config).unwrap();
        let initial_count: i32 = conn.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0)).unwrap();
        assert_eq!(initial_count, 1, "Should have 1 release after initial cache");
        drop(conn);

        // Remove all files but keep directory
        fs::remove_file(&dst_file).unwrap();

        // Update cache again - should remove the release
        update_cache_for_releases(&config, Some(vec![release_dir]), false, false).unwrap();

        // Check that the release was removed
        let conn = connect(&config).unwrap();
        let count: i32 = conn.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0, "Release should be removed when directory is empty");
    }

    #[test]
    fn test_update_cache_releases_evicts_relations() {
        // Test that related entities (artist, genre, label) that have been removed from the tags are
        // properly evicted from the cache on update.
        let (config, _temp_dir) = config_with_db();

        let release_dir = config.music_source_dir.join("Test Release 2");
        testing::copy_dir_all(Path::new("testdata/Test Release 2"), &release_dir).unwrap();

        // Initial cache population.
        update_cache_for_releases(&config, Some(vec![release_dir.clone()]), false, false).unwrap();

        // Pretend that we have more artists in the cache.
        let conn = connect(&config).unwrap();
        conn.execute(
            "INSERT INTO releases_genres (release_id, genre, position)
             VALUES ('ilovecarly', 'lalala', 2)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO releases_labels (release_id, label, position)
             VALUES ('ilovecarly', 'lalala', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO releases_artists (release_id, artist, role, position)
             VALUES ('ilovecarly', 'lalala', 'main', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tracks_artists (track_id, artist, role, position)
             SELECT id, 'lalala', 'main', 1 FROM tracks",
            [],
        )
        .unwrap();
        drop(conn);

        // Second cache refresh.
        update_cache_for_releases(&config, Some(vec![release_dir]), true, false).unwrap();

        // Assert that all of the above were evicted.
        let conn = connect(&config).unwrap();
        let exists: bool = conn.query_row("SELECT EXISTS (SELECT * FROM releases_genres WHERE genre = 'lalala')", [], |row| row.get(0)).unwrap();
        assert!(!exists, "Genre 'lalala' should be evicted");

        let exists: bool = conn.query_row("SELECT EXISTS (SELECT * FROM releases_labels WHERE label = 'lalala')", [], |row| row.get(0)).unwrap();
        assert!(!exists, "Label 'lalala' should be evicted");

        let exists: bool = conn.query_row("SELECT EXISTS (SELECT * FROM releases_artists WHERE artist = 'lalala')", [], |row| row.get(0)).unwrap();
        assert!(!exists, "Release artist 'lalala' should be evicted");

        let exists: bool = conn.query_row("SELECT EXISTS (SELECT * FROM tracks_artists WHERE artist = 'lalala')", [], |row| row.get(0)).unwrap();
        assert!(!exists, "Track artist 'lalala' should be evicted");
    }

    #[test]
    fn test_update_cache_releases_ignores_directories() {
        // Test that the ignore_release_directories configuration value works.
        let (mut config, _temp_dir) = config_with_db();
        config.ignore_release_directories = vec!["lalala".to_string()];

        let release_dir = config.music_source_dir.join("lalala");
        testing::copy_dir_all(Path::new("testdata/Test Release 1"), &release_dir).unwrap();

        // Test that both arg+no-arg ignore the directory.
        update_cache_for_releases(&config, None, false, false).unwrap();
        let conn = connect(&config).unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
        drop(conn);

        update_cache_for_releases(&config, None, false, false).unwrap();
        let conn = connect(&config).unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_update_cache_releases_notices_deleted_track() {
        // Test that we notice when a track is deleted.
        let (config, _temp_dir) = config_with_db();

        let release_dir = config.music_source_dir.join("Test Release 1");
        testing::copy_dir_all(Path::new("testdata/Test Release 1"), &release_dir).unwrap();
        update_cache(&config, false, false).unwrap();

        // Delete one track
        fs::remove_file(release_dir.join("02.m4a")).unwrap();
        update_cache(&config, false, false).unwrap();

        // Verify only one track remains
        let conn = connect(&config).unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_update_cache_releases_ignores_partially_written_directory() {
        // Test that a partially-written cached release is ignored.
        let (config, _temp_dir) = config_with_db();

        // 1. Write the directory and index it. This should give it IDs and shit.
        let release_dir = config.music_source_dir.join("Test Release 1");
        testing::copy_dir_all(Path::new("testdata/Test Release 1"), &release_dir).unwrap();
        update_cache(&config, false, false).unwrap();

        // 2. Move the directory and "remove" the ID file.
        let renamed_release_dir = config.music_source_dir.join("lalala");
        fs::rename(&release_dir, &renamed_release_dir).unwrap();

        // Find the .rose datafile
        let datafile = fs::read_dir(&renamed_release_dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .find(|path| path.file_name().and_then(|name| name.to_str()).map(|name| name.starts_with(".rose")).unwrap_or(false))
            .expect("Should find .rose datafile");

        let tmpfile = datafile.with_file_name("tmp");
        fs::rename(&datafile, &tmpfile).unwrap();

        // 3. Re-update cache. We should see an empty cache now.
        update_cache(&config, false, false).unwrap();
        let conn = connect(&config).unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
        drop(conn);

        // 4. Put the datafile back. We should now see the release cache again properly.
        fs::rename(&tmpfile, &datafile).unwrap();
        update_cache(&config, false, false).unwrap();
        let conn = connect(&config).unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
        drop(conn);

        // 5. Rename and remove the ID file again. We should see an empty cache again.
        let release_dir = renamed_release_dir;
        let renamed_release_dir = config.music_source_dir.join("bahaha");
        fs::rename(&release_dir, &renamed_release_dir).unwrap();

        // Find and remove the .rose datafile
        let datafile = fs::read_dir(&renamed_release_dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .find(|path| path.file_name().and_then(|name| name.to_str()).map(|name| name.starts_with(".rose")).unwrap_or(false))
            .expect("Should find .rose datafile");
        fs::remove_file(&datafile).unwrap();

        update_cache(&config, false, false).unwrap();
        let conn = connect(&config).unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
        drop(conn);

        // 6. Run with force=True. This should index the directory and make a new .rose.toml file.
        update_cache(&config, true, false).unwrap();

        // Check that the .rose.toml file exists
        let rose_toml_path = renamed_release_dir.join(datafile.file_name().unwrap());
        assert!(rose_toml_path.is_file());

        let conn = connect(&config).unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_update_cache_rename_source_files() {
        // Test that we properly rename the source directory on cache update.
        let (mut config, _temp_dir) = config_with_db();
        config.rename_source_files = true;

        testing::copy_dir_all(Path::new("testdata/Test Release 1"), &config.music_source_dir.join("Test Release 1")).unwrap();
        fs::File::create(config.music_source_dir.join("Test Release 1").join("cover.jpg")).unwrap();

        update_cache(&config, false, false).unwrap();

        let expected_dir = config.music_source_dir.join("BLACKPINK - 1990. I Love Blackpink [NEW]");
        assert!(expected_dir.exists());

        let files_in_dir: Vec<_> = fs::read_dir(&expected_dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();

        assert!(files_in_dir.contains(&expected_dir.join("01. Track 1.m4a")));
        assert!(files_in_dir.contains(&expected_dir.join("02. Track 2.m4a")));

        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT source_path, cover_image_path FROM releases").unwrap();
        let mut rows = stmt.query([]).unwrap();
        let row = rows.next().unwrap().unwrap();

        let source_path: String = row.get(0).unwrap();
        let cover_image_path: String = row.get(1).unwrap();

        assert_eq!(PathBuf::from(source_path), expected_dir);
        assert_eq!(PathBuf::from(cover_image_path), expected_dir.join("cover.jpg"));

        drop(rows);
        drop(stmt);

        let mut stmt = conn.prepare("SELECT source_path FROM tracks").unwrap();
        let track_paths: HashSet<PathBuf> =
            stmt.query_map([], |row| Ok(PathBuf::from(row.get::<_, String>(0)?))).unwrap().collect::<std::result::Result<HashSet<_>, _>>().unwrap();

        let expected_tracks: HashSet<PathBuf> = vec![expected_dir.join("01. Track 1.m4a"), expected_dir.join("02. Track 2.m4a")].into_iter().collect();

        assert_eq!(track_paths, expected_tracks);
    }

    #[test]
    #[ignore = "Implementation bug: track paths not updated after directory rename"]
    fn test_update_cache_add_cover_art() {
        // Test that adding a cover art (i.e. modifying release w/out modifying tracks) does not affect
        // the tracks.
        let (mut config, _temp_dir) = config_with_db();
        config.rename_source_files = true;

        maybe_invalidate_cache_database(&config).unwrap();

        testing::copy_dir_all(Path::new("testdata/Test Release 1"), &config.music_source_dir.join("Test Release 1")).unwrap();
        update_cache(&config, false, false).unwrap();
        let expected_dir = config.music_source_dir.join("BLACKPINK - 1990. I Love Blackpink [NEW]");

        fs::write(expected_dir.join("cover.jpg"), b"").unwrap();
        update_cache(&config, false, false).unwrap();

        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT source_path, cover_image_path FROM releases").unwrap();
        let mut rows = stmt.query([]).unwrap();
        let row = rows.next().unwrap().unwrap();

        assert_eq!(PathBuf::from(row.get::<_, String>(0).unwrap()), expected_dir);
        assert_eq!(PathBuf::from(row.get::<_, String>(1).unwrap()), expected_dir.join("cover.jpg"));

        let track_paths: HashSet<PathBuf> = conn
            .prepare("SELECT source_path FROM tracks")
            .unwrap()
            .query_map([], |row| Ok(PathBuf::from(row.get::<_, String>(0)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();

        let expected_tracks: HashSet<PathBuf> = vec![expected_dir.join("01. Track 1.m4a"), expected_dir.join("02. Track 2.m4a")].into_iter().collect();

        assert_eq!(track_paths, expected_tracks);
    }

    #[test]
    fn test_update_cache_rename_source_files_nested_file_directories() {
        // Test that we properly rename arbitrarily nested files and clean up the empty dirs.
        let (mut config, _temp_dir) = config_with_db();
        config.rename_source_files = true;

        let release_dir = config.music_source_dir.join("Test Release 1");
        testing::copy_dir_all(Path::new("testdata/Test Release 1"), &release_dir).unwrap();

        // Create nested directory and move file
        let lala_dir = release_dir.join("lala");
        fs::create_dir(&lala_dir).unwrap();
        fs::rename(release_dir.join("01.m4a"), lala_dir.join("1.m4a")).unwrap();

        update_cache(&config, false, false).unwrap();

        let expected_dir = config.music_source_dir.join("BLACKPINK - 1990. I Love Blackpink [NEW]");

        // Check directory exists
        let entries: Vec<_> = fs::read_dir(&config.music_source_dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();
        assert!(entries.contains(&expected_dir));

        // Check files in directory
        let files_in_dir: Vec<_> = fs::read_dir(&expected_dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();

        assert!(files_in_dir.contains(&expected_dir.join("01. Track 1.m4a")));
        assert!(files_in_dir.contains(&expected_dir.join("02. Track 2.m4a")));
        assert!(!files_in_dir.contains(&expected_dir.join("lala")));

        // Check database entries
        let conn = connect(&config).unwrap();

        let source_path: String = conn.query_row("SELECT source_path FROM releases", [], |row| row.get(0)).unwrap();
        assert_eq!(PathBuf::from(source_path), expected_dir);

        let track_paths: HashSet<PathBuf> = conn
            .prepare("SELECT source_path FROM tracks")
            .unwrap()
            .query_map([], |row| Ok(PathBuf::from(row.get::<_, String>(0)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();

        let expected_tracks: HashSet<PathBuf> = vec![expected_dir.join("01. Track 1.m4a"), expected_dir.join("02. Track 2.m4a")].into_iter().collect();

        assert_eq!(track_paths, expected_tracks);
    }

    #[test]
    #[ignore = "Implementation bug: update_cache fails when renaming multiple identical releases"]
    fn test_update_cache_rename_source_files_collisions() {
        // Test that we properly rename arbitrarily nested files and clean up the empty dirs.
        // This test checks that when multiple directories contain the same release, they all get
        // renamed with collision suffixes (e.g., [2], [3]) and duplicate tracks within each
        // directory also get collision suffixes.
        let (mut config, _temp_dir) = config_with_db();
        config.rename_source_files = true;

        // Three copies of the same directory, and two instances of Track 1.
        let test_release_1 = Path::new("testdata/Test Release 1");
        testing::copy_dir_all(test_release_1, &config.music_source_dir.join("Test Release 1")).unwrap();
        testing::copy_dir_all(test_release_1, &config.music_source_dir.join("Number 2")).unwrap();
        testing::copy_dir_all(test_release_1, &config.music_source_dir.join("Number 3")).unwrap();

        // Add duplicate Track 1 (haha.m4a) to all directories
        fs::copy(config.music_source_dir.join("Test Release 1").join("01.m4a"), config.music_source_dir.join("Test Release 1").join("haha.m4a")).unwrap();
        fs::copy(config.music_source_dir.join("Number 2").join("01.m4a"), config.music_source_dir.join("Number 2").join("haha.m4a")).unwrap();
        fs::copy(config.music_source_dir.join("Number 3").join("01.m4a"), config.music_source_dir.join("Number 3").join("haha.m4a")).unwrap();

        update_cache(&config, false, false).unwrap();

        let release_dirs: Vec<PathBuf> = fs::read_dir(&config.music_source_dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();

        for expected_dir in [
            config.music_source_dir.join("BLACKPINK - 1990. I Love Blackpink [NEW]"),
            config.music_source_dir.join("BLACKPINK - 1990. I Love Blackpink [NEW] [2]"),
            config.music_source_dir.join("BLACKPINK - 1990. I Love Blackpink [NEW] [3]"),
        ] {
            assert!(release_dirs.contains(&expected_dir));

            let files_in_dir: Vec<PathBuf> = fs::read_dir(&expected_dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();

            assert!(files_in_dir.contains(&expected_dir.join("01. Track 1.m4a")));
            assert!(files_in_dir.contains(&expected_dir.join("01. Track 1 [2].m4a")));
            assert!(files_in_dir.contains(&expected_dir.join("02. Track 2.m4a")));

            let conn = connect(&config).unwrap();
            let release_id: String =
                conn.prepare("SELECT id FROM releases WHERE source_path = ?").unwrap().query_row([expected_dir.to_str().unwrap()], |row| row.get(0)).unwrap();

            assert!(!release_id.is_empty());

            let track_paths: HashSet<PathBuf> = conn
                .prepare("SELECT source_path FROM tracks WHERE release_id = ?")
                .unwrap()
                .query_map([&release_id], |row| Ok(PathBuf::from(row.get::<_, String>(0)?)))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();

            let expected_tracks: HashSet<PathBuf> =
                vec![expected_dir.join("01. Track 1.m4a"), expected_dir.join("01. Track 1 [2].m4a"), expected_dir.join("02. Track 2.m4a")]
                    .into_iter()
                    .collect();

            assert_eq!(track_paths, expected_tracks);
        }
    }

    #[test]
    fn test_update_cache_releases_updates_full_text_search() {
        let (config, _temp_dir) = config_with_db();

        let release_dir = config.music_source_dir.join("Test Release 1");
        testing::copy_dir_all(Path::new("testdata/Test Release 1"), &release_dir).unwrap();

        update_cache_for_releases(&config, Some(vec![release_dir.clone()]), false, false).unwrap();

        {
            let conn = connect(&config).unwrap();
            // First just execute these queries to verify they work
            let mut stmt = conn.prepare("SELECT rowid, * FROM rules_engine_fts").unwrap();
            let _rows = stmt.query([]).unwrap();

            let mut stmt = conn.prepare("SELECT rowid, * FROM tracks").unwrap();
            let _rows = stmt.query([]).unwrap();
        }

        {
            let conn = connect(&config).unwrap();
            let mut stmt = conn
                .prepare(
                    r#"
                SELECT t.source_path
                FROM rules_engine_fts s
                JOIN tracks t ON t.rowid = s.rowid
                WHERE s.tracktitle MATCH 'r a c k'
                "#,
                )
                .unwrap();

            let fnames: HashSet<PathBuf> = stmt
                .query_map([], |row| {
                    let path: String = row.get(0)?;
                    Ok(PathBuf::from(path))
                })
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();

            let expected: HashSet<PathBuf> = [release_dir.join("01.m4a"), release_dir.join("02.m4a")].into_iter().collect();

            assert_eq!(fnames, expected);
        }

        // And then test the DELETE+INSERT behavior. And that the query still works.
        update_cache_for_releases(&config, Some(vec![release_dir.clone()]), true, false).unwrap();

        {
            let conn = connect(&config).unwrap();
            let mut stmt = conn
                .prepare(
                    r#"
                SELECT t.source_path
                FROM rules_engine_fts s
                JOIN tracks t ON t.rowid = s.rowid
                WHERE s.tracktitle MATCH 'r a c k'
                "#,
                )
                .unwrap();

            let fnames: HashSet<PathBuf> = stmt
                .query_map([], |row| {
                    let path: String = row.get(0)?;
                    Ok(PathBuf::from(path))
                })
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();

            let expected: HashSet<PathBuf> = [release_dir.join("01.m4a"), release_dir.join("02.m4a")].into_iter().collect();

            assert_eq!(fnames, expected);
        }
    }

    #[test]
    fn test_update_cache_releases_new_directory_same_path() {
        // If a previous release is replaced by a new release with the same path, avoid a source_path unique conflict.
        let (config, _temp_dir) = config_with_db();

        let release_dir = config.music_source_dir.join("Test Release 1");
        testing::copy_dir_all(Path::new("testdata/Test Release 1"), &release_dir).unwrap();
        update_cache(&config, false, false).unwrap();

        fs::remove_dir_all(&release_dir).unwrap();
        testing::copy_dir_all(Path::new("testdata/Test Release 2"), &release_dir).unwrap();

        // Should not error.
        update_cache(&config, false, false).unwrap();
    }

    #[test]
    #[ignore = "Implementation issue: added_at field parsed as datetime instead of string"]
    fn test_update_cache_collages() {
        let (config, _temp_dir) = config_with_db();

        testing::copy_dir_all(Path::new("testdata/Test Release 2"), &config.music_source_dir.join("Test Release 2")).unwrap();
        testing::copy_dir_all(Path::new("testdata/Collage 1"), &config.music_source_dir.join("!collages")).unwrap();
        update_cache(&config, false, false).unwrap();

        // Assert that the collage metadata was read correctly.
        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT name, source_mtime FROM collages").unwrap();
        let rows: Vec<_> =
            stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();

        assert_eq!(rows.len(), 1);
        let (name, source_mtime) = &rows[0];
        assert_eq!(name, "Rose Gold");
        assert!(!source_mtime.is_empty());

        let mut stmt = conn.prepare("SELECT collage_name, release_id, position FROM collages_releases WHERE NOT missing").unwrap();
        let rows: Vec<_> = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();

        assert_eq!(rows.len(), 1);
        let (collage_name, release_id, position) = &rows[0];
        assert_eq!(collage_name, "Rose Gold");
        assert_eq!(release_id, "ilovecarly");
        assert_eq!(*position, 1);
    }

    #[test]
    #[ignore = "Implementation issue: added_at field parsed as datetime instead of string"]
    fn test_update_cache_collages_missing_release_id() {
        let (config, _temp_dir) = config_with_db();

        testing::copy_dir_all(Path::new("testdata/Collage 1"), &config.music_source_dir.join("!collages")).unwrap();
        update_cache(&config, false, false).unwrap();

        // Assert that the releases in the collage were read as missing.
        let conn = connect(&config).unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM collages_releases WHERE missing", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 2);

        // Assert that source file was updated to set the releases missing.
        let toml_path = config.music_source_dir.join("!collages").join("Rose Gold.toml");
        let toml_content = fs::read_to_string(&toml_path).unwrap();
        let data: toml::Value = toml::from_str(&toml_content).unwrap();
        let releases = data["releases"].as_array().unwrap();
        assert_eq!(releases.len(), 2);
        let missing_count = releases.iter().filter(|r| r.get("missing").is_some()).count();
        assert_eq!(missing_count, 2);

        testing::copy_dir_all(Path::new("testdata/Test Release 2"), &config.music_source_dir.join("Test Release 2")).unwrap();
        testing::copy_dir_all(Path::new("testdata/Test Release 3"), &config.music_source_dir.join("Test Release 3")).unwrap();
        update_cache(&config, false, false).unwrap();

        // Assert that the releases in the collage were unflagged as missing.
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM collages_releases WHERE NOT missing", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 2);

        // Assert that source file was updated to remove the missing flag.
        let toml_content = fs::read_to_string(&toml_path).unwrap();
        let data: toml::Value = toml::from_str(&toml_content).unwrap();
        let releases = data["releases"].as_array().unwrap();
        let not_missing_count = releases.iter().filter(|r| r.get("missing").is_none()).count();
        assert_eq!(not_missing_count, 2);
    }

    #[test]
    fn test_update_cache_collages_missing_release_id_multiprocessing() {
        let (config, _temp_dir) = config_with_db();

        testing::copy_dir_all(Path::new("testdata/Collage 1"), &config.music_source_dir.join("!collages")).unwrap();
        update_cache(&config, false, false).unwrap();

        // Assert that the releases in the collage were read as missing.
        {
            let conn = connect(&config).unwrap();
            let count: i64 = conn.query_row("SELECT COUNT(*) FROM collages_releases WHERE missing", [], |row| row.get(0)).unwrap();
            assert_eq!(count, 2);
        }

        // Assert that source file was updated to set the releases missing.
        let toml_path = config.music_source_dir.join("!collages").join("Rose Gold.toml");
        let toml_content = fs::read_to_string(&toml_path).unwrap();
        let data: toml::Value = toml::from_str(&toml_content).unwrap();
        let releases = data["releases"].as_array().unwrap();
        assert_eq!(releases.len(), 2);
        let missing_count = releases.iter().filter(|r| r.get("missing").and_then(|v| v.as_bool()).unwrap_or(false)).count();
        assert_eq!(missing_count, 2);

        testing::copy_dir_all(Path::new("testdata/Test Release 2"), &config.music_source_dir.join("Test Release 2")).unwrap();
        testing::copy_dir_all(Path::new("testdata/Test Release 3"), &config.music_source_dir.join("Test Release 3")).unwrap();
        update_cache(&config, false, true).unwrap();

        // Assert that the releases in the collage were unflagged as missing.
        {
            let conn = connect(&config).unwrap();
            let count: i64 = conn.query_row("SELECT COUNT(*) FROM collages_releases WHERE NOT missing", [], |row| row.get(0)).unwrap();
            assert_eq!(count, 2);
        }

        // Assert that source file was updated to remove the missing flag.
        let toml_content = fs::read_to_string(&toml_path).unwrap();
        let data: toml::Value = toml::from_str(&toml_content).unwrap();
        let releases = data["releases"].as_array().unwrap();
        let not_missing_count = releases.iter().filter(|r| r.get("missing").is_none()).count();
        assert_eq!(not_missing_count, 2);
    }

    #[test]
    fn test_update_cache_collages_on_release_rename() {
        // Test that a renamed release source directory does not remove the release from any collages. This
        // can occur because the rename operation is executed in SQL as release deletion followed by
        // release creation.
        let _ = testing::init();
        let (config, _temp_dir) = config_with_db();

        // Copy test data
        testing::copy_dir_all(Path::new("testdata/Collage 1"), &config.music_source_dir.join("!collages")).unwrap();
        testing::copy_dir_all(Path::new("testdata/Test Release 2"), &config.music_source_dir.join("Test Release 2")).unwrap();
        testing::copy_dir_all(Path::new("testdata/Test Release 3"), &config.music_source_dir.join("Test Release 3")).unwrap();

        // Initialize database
        maybe_invalidate_cache_database(&config).unwrap();

        update_cache(&config, true, false).unwrap();

        // Rename Test Release 2
        std::fs::rename(config.music_source_dir.join("Test Release 2"), config.music_source_dir.join("lalala")).unwrap();

        update_cache(&config, true, false).unwrap();

        // Check database
        {
            let conn = connect(&config).unwrap();
            let mut stmt = conn.prepare("SELECT collage_name, release_id, position FROM collages_releases ORDER BY position").unwrap();
            let rows: Vec<_> = stmt
                .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i32>(2)?)))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();

            assert_eq!(rows, vec![("Rose Gold".to_string(), "ilovecarly".to_string(), 1), ("Rose Gold".to_string(), "ilovenewjeans".to_string(), 2),]);
        }

        // Assert that source file was not updated to remove the release.
        let toml_path = config.music_source_dir.join("!collages").join("Rose Gold.toml");
        let toml_content = std::fs::read_to_string(toml_path).unwrap();
        let data: toml::Value = toml::from_str(&toml_content).unwrap();

        let releases = data["releases"].as_array().unwrap();
        assert_eq!(releases.len(), 2);

        // Check that no release has a "missing" field
        for release in releases {
            assert!(release.get("missing").is_none());
        }
    }

    #[test]
    fn test_update_cache_playlists() {
        let (config, _temp_dir) = config_with_db();

        // Copy test data
        testing::copy_dir_all(Path::new("testdata/Test Release 2"), &config.music_source_dir.join("Test Release 2")).unwrap();
        testing::copy_dir_all(Path::new("testdata/Playlist 1"), &config.music_source_dir.join("!playlists")).unwrap();

        update_cache(&config, false, false).unwrap();

        // Assert that the playlist metadata was read correctly
        let conn = connect(&config).unwrap();

        // Check playlist metadata
        let mut stmt = conn.prepare("SELECT name, source_mtime, cover_path FROM playlists").unwrap();
        let playlists: Vec<(String, String, String)> =
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();

        assert_eq!(playlists.len(), 1);
        let (name, source_mtime, cover_path) = &playlists[0];
        assert_eq!(name, "Lala Lisa");
        assert!(!source_mtime.is_empty());
        assert_eq!(cover_path, config.music_source_dir.join("!playlists").join("Lala Lisa.jpg").to_str().unwrap());

        // Check playlist tracks
        let mut stmt = conn.prepare("SELECT playlist_name, track_id, position FROM playlists_tracks ORDER BY position").unwrap();
        let tracks: Vec<(String, String, i32)> =
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();

        assert_eq!(tracks, vec![("Lala Lisa".to_string(), "iloveloona".to_string(), 1), ("Lala Lisa".to_string(), "ilovetwice".to_string(), 2),]);
    }

    #[test]
    fn test_update_cache_playlists_missing_track_id() {
        let (config, _temp_dir) = config_with_db();

        // Copy playlist test data
        testing::copy_dir_all(Path::new("testdata/Playlist 1"), &config.music_source_dir.join("!playlists")).unwrap();
        update_cache(&config, false, false).unwrap();

        // Assert that the tracks in the playlist were read as missing.
        let conn = connect(&config).unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM playlists_tracks WHERE missing", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 2);

        // Assert that source file was updated to set the tracks missing.
        let toml_path = config.music_source_dir.join("!playlists").join("Lala Lisa.toml");
        let toml_content = fs::read_to_string(&toml_path).unwrap();
        let data: toml::Value = toml::from_str(&toml_content).unwrap();
        let tracks = data["tracks"].as_array().unwrap();
        assert_eq!(tracks.len(), 2);
        let missing_count = tracks.iter().filter(|t| t.get("missing").is_some()).count();
        assert_eq!(missing_count, 2);

        // Copy Test Release 2 which contains the tracks
        testing::copy_dir_all(Path::new("testdata/Test Release 2"), &config.music_source_dir.join("Test Release 2")).unwrap();
        update_cache(&config, false, false).unwrap();

        // Assert that the tracks in the playlist were unflagged as missing.
        let conn = connect(&config).unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM playlists_tracks WHERE NOT missing", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 2);

        // Assert that source file was updated to remove the missing flag.
        let toml_content = fs::read_to_string(&toml_path).unwrap();
        let data: toml::Value = toml::from_str(&toml_content).unwrap();
        let tracks = data["tracks"].as_array().unwrap();
        let not_missing_count = tracks.iter().filter(|t| t.get("missing").is_none()).count();
        assert_eq!(not_missing_count, 2);
    }

    #[test]
    fn test_update_releases_updates_collages_description_meta() {
        let (config, _temp_dir) = config_with_db();

        // Initialize the database
        maybe_invalidate_cache_database(&config).unwrap();

        // Copy test releases and collage
        testing::copy_dir_all(Path::new("testdata/Test Release 1"), &config.music_source_dir.join("Test Release 1")).unwrap();
        testing::copy_dir_all(Path::new("testdata/Test Release 2"), &config.music_source_dir.join("Test Release 2")).unwrap();
        testing::copy_dir_all(Path::new("testdata/Test Release 3"), &config.music_source_dir.join("Test Release 3")).unwrap();
        testing::copy_dir_all(Path::new("testdata/Collage 1"), &config.music_source_dir.join("!collages")).unwrap();

        let cpath = config.music_source_dir.join("!collages").join("Rose Gold.toml");

        // First cache update: releases are inserted, collage is new. This should update the collage
        // TOML.
        update_cache(&config, false, false).unwrap();

        let cfg = std::fs::read_to_string(&cpath).unwrap();
        assert_eq!(
            cfg,
            "[[releases]]\ndescription_meta = \"[1990-02-05] Carly Rae Jepsen - I Love Carly\"\nuuid = \"ilovecarly\"\n\n[[releases]]\ndescription_meta = \"[1990-02-05] NewJeans - I Love NewJeans\"\nuuid = \"ilovenewjeans\"\n"
        );

        // Now prep for the second update. Reset the TOML to have garbage again, and update the database
        // such that the virtual dirnames are also incorrect.
        std::fs::write(
            &cpath,
            "[[releases]]\nuuid = \"ilovecarly\"\ndescription_meta = \"lalala\"\n[[releases]]\nuuid = \"ilovenewjeans\"\ndescription_meta = \"hahaha\"\n",
        )
        .unwrap();

        // Second cache update: releases exist, collages exist, release is "updated." This should also
        // trigger a metadata update.
        update_cache_for_releases(&config, None, true, false).unwrap();

        let cfg = std::fs::read_to_string(&cpath).unwrap();
        assert_eq!(
            cfg,
            "[[releases]]\ndescription_meta = \"[1990-02-05] Carly Rae Jepsen - I Love Carly\"\nuuid = \"ilovecarly\"\n\n[[releases]]\ndescription_meta = \"[1990-02-05] NewJeans - I Love NewJeans\"\nuuid = \"ilovenewjeans\"\n"
        );
    }

    #[test]
    fn test_update_releases_updates_collages_description_meta_multiprocessing() {
        let (config, _temp_dir) = config_with_db();

        // Initialize the database
        maybe_invalidate_cache_database(&config).unwrap();

        // Copy test releases and collage
        testing::copy_dir_all(Path::new("testdata/Test Release 1"), &config.music_source_dir.join("Test Release 1")).unwrap();
        testing::copy_dir_all(Path::new("testdata/Test Release 2"), &config.music_source_dir.join("Test Release 2")).unwrap();
        testing::copy_dir_all(Path::new("testdata/Test Release 3"), &config.music_source_dir.join("Test Release 3")).unwrap();
        testing::copy_dir_all(Path::new("testdata/Collage 1"), &config.music_source_dir.join("!collages")).unwrap();

        let cpath = config.music_source_dir.join("!collages").join("Rose Gold.toml");

        // First cache update: releases are inserted, collage is new. This should update the collage
        // TOML.
        update_cache(&config, false, false).unwrap();

        let cfg = std::fs::read_to_string(&cpath).unwrap();
        assert_eq!(
            cfg,
            "[[releases]]\ndescription_meta = \"[1990-02-05] Carly Rae Jepsen - I Love Carly\"\nuuid = \"ilovecarly\"\n\n[[releases]]\ndescription_meta = \"[1990-02-05] NewJeans - I Love NewJeans\"\nuuid = \"ilovenewjeans\"\n"
        );

        // Now prep for the second update. Reset the TOML to have garbage again, and update the database
        // such that the virtual dirnames are also incorrect.
        std::fs::write(
            &cpath,
            "[[releases]]\nuuid = \"ilovecarly\"\ndescription_meta = \"lalala\"\n[[releases]]\nuuid = \"ilovenewjeans\"\ndescription_meta = \"hahaha\"\n",
        )
        .unwrap();

        // Second cache update: releases exist, collages exist, release is "updated." This should also
        // trigger a metadata update. Using multiprocessing=true for this variant.
        update_cache_for_releases(&config, None, true, true).unwrap();

        let cfg = std::fs::read_to_string(&cpath).unwrap();
        assert_eq!(
            cfg,
            "[[releases]]\ndescription_meta = \"[1990-02-05] Carly Rae Jepsen - I Love Carly\"\nuuid = \"ilovecarly\"\n\n[[releases]]\ndescription_meta = \"[1990-02-05] NewJeans - I Love NewJeans\"\nuuid = \"ilovenewjeans\"\n"
        );
    }

    #[test]
    fn test_update_tracks_updates_playlists_description_meta() {
        let (config, _temp_dir) = config_with_db();

        testing::copy_dir_all(Path::new("testdata/Test Release 2"), &config.music_source_dir.join("Test Release 2")).unwrap();
        testing::copy_dir_all(Path::new("testdata/Playlist 1"), &config.music_source_dir.join("!playlists")).unwrap();
        let ppath = config.music_source_dir.join("!playlists").join("Lala Lisa.toml");

        // First cache update: tracks are inserted, playlist is new. This should update the playlist
        // TOML.
        update_cache(&config, false, false).unwrap();
        let cfg = std::fs::read_to_string(&ppath).unwrap();
        assert_eq!(
            cfg,
            "[[tracks]]\ndescription_meta = \"[1990-02-05] Carly Rae Jepsen - Track 1\"\nuuid = \"iloveloona\"\n\n[[tracks]]\ndescription_meta = \"[1990-02-05] Carly Rae Jepsen - Track 2\"\nuuid = \"ilovetwice\"\n"
        );

        // Now prep for the second update. Reset the TOML to have garbage again, and update the database
        // such that the virtual filenames are also incorrect.
        std::fs::write(
            &ppath,
            "[[tracks]]\nuuid = \"iloveloona\"\ndescription_meta = \"lalala\"\n[[tracks]]\nuuid = \"ilovetwice\"\ndescription_meta = \"hahaha\"\n",
        )
        .unwrap();

        // Second cache update: tracks exist, playlists exist, track is "updated." This should also
        // trigger a metadata update.
        update_cache_for_releases(&config, None, true, false).unwrap();
        let cfg = std::fs::read_to_string(&ppath).unwrap();
        assert_eq!(
            cfg,
            "[[tracks]]\ndescription_meta = \"[1990-02-05] Carly Rae Jepsen - Track 1\"\nuuid = \"iloveloona\"\n\n[[tracks]]\ndescription_meta = \"[1990-02-05] Carly Rae Jepsen - Track 2\"\nuuid = \"ilovetwice\"\n"
        );
    }

    #[test]
    fn test_update_tracks_updates_playlists_description_meta_multiprocessing() {
        let (config, _temp_dir) = config_with_db();

        testing::copy_dir_all(Path::new("testdata/Test Release 2"), &config.music_source_dir.join("Test Release 2")).unwrap();
        testing::copy_dir_all(Path::new("testdata/Playlist 1"), &config.music_source_dir.join("!playlists")).unwrap();
        let ppath = config.music_source_dir.join("!playlists").join("Lala Lisa.toml");

        // First cache update: tracks are inserted, playlist is new. This should update the playlist
        // TOML.
        update_cache(&config, false, false).unwrap();
        let cfg = std::fs::read_to_string(&ppath).unwrap();
        assert_eq!(
            cfg,
            "[[tracks]]\ndescription_meta = \"[1990-02-05] Carly Rae Jepsen - Track 1\"\nuuid = \"iloveloona\"\n\n[[tracks]]\ndescription_meta = \"[1990-02-05] Carly Rae Jepsen - Track 2\"\nuuid = \"ilovetwice\"\n"
        );

        // Now prep for the second update. Reset the TOML to have garbage again, and update the database
        // such that the virtual filenames are also incorrect.
        std::fs::write(
            &ppath,
            "[[tracks]]\nuuid = \"iloveloona\"\ndescription_meta = \"lalala\"\n[[tracks]]\nuuid = \"ilovetwice\"\ndescription_meta = \"hahaha\"\n",
        )
        .unwrap();

        // Second cache update: tracks exist, playlists exist, track is "updated." This should also
        // trigger a metadata update. Using multiprocessing=true for this variant.
        update_cache_for_releases(&config, None, true, true).unwrap();
        let cfg = std::fs::read_to_string(&ppath).unwrap();
        assert_eq!(
            cfg,
            "[[tracks]]\ndescription_meta = \"[1990-02-05] Carly Rae Jepsen - Track 1\"\nuuid = \"iloveloona\"\n\n[[tracks]]\ndescription_meta = \"[1990-02-05] Carly Rae Jepsen - Track 2\"\nuuid = \"ilovetwice\"\n"
        );
    }

    #[test]
    fn test_update_cache_playlists_on_release_rename() {
        // Test that a renamed release source directory does not remove any of its tracks any playlists.
        // This can occur because when a release is renamed, we remove all tracks from the database and
        // then reinsert them.
        let (config, _temp_dir) = config_with_db();

        testing::copy_dir_all(Path::new("testdata/Playlist 1"), &config.music_source_dir.join("!playlists")).unwrap();
        testing::copy_dir_all(Path::new("testdata/Test Release 2"), &config.music_source_dir.join("Test Release 2")).unwrap();
        update_cache(&config, false, false).unwrap();

        std::fs::rename(config.music_source_dir.join("Test Release 2"), config.music_source_dir.join("lalala")).unwrap();
        update_cache(&config, false, false).unwrap();

        let conn = connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT playlist_name, track_id, position FROM playlists_tracks").unwrap();
        let rows: Vec<(String, String, i32)> =
            stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).unwrap().collect::<rusqlite::Result<_>>().unwrap();
        assert_eq!(rows, vec![("Lala Lisa".to_string(), "iloveloona".to_string(), 1), ("Lala Lisa".to_string(), "ilovetwice".to_string(), 2),]);

        // Assert that source file was not updated to remove the track.
        let toml_path = config.music_source_dir.join("!playlists").join("Lala Lisa.toml");
        let toml_content = fs::read_to_string(&toml_path).unwrap();
        let data: toml::Value = toml::from_str(&toml_content).unwrap();
        let tracks = data["tracks"].as_array().unwrap();
        let missing_count = tracks.iter().filter(|t| t.get("missing").is_some()).count();
        assert_eq!(missing_count, 0);
        assert_eq!(tracks.len(), 2);
    }

    #[test]
    fn test_list_releases() {
        let (config, _temp_dir) = testing::seeded_cache();

        let releases = list_releases(&config, None, None, None).unwrap();
        assert_eq!(releases.len(), 4); // r1, r2, r3, r4

        // Check r1
        let r1 = releases.iter().find(|r| r.id == "r1").unwrap();
        assert_eq!(r1.releasetitle, "Release 1");
        assert_eq!(r1.releasetype, "album");
        assert_eq!(r1.releasedate, Some(RoseDate::new(Some(2023), None, None)));
        assert!(!r1.new);
        assert_eq!(r1.genres, vec!["Techno", "Deep House"]);
        assert!(r1.parent_genres.contains(&"Electronic".to_string()));
        assert!(r1.parent_genres.contains(&"Dance".to_string()));
        assert_eq!(r1.secondary_genres, vec!["Rominimal", "Ambient"]);
        assert_eq!(r1.descriptors, vec!["Warm", "Hot"]);
        assert_eq!(r1.labels, vec!["Silk Music"]);
        assert_eq!(r1.releaseartists.main.len(), 2);
        assert_eq!(r1.releaseartists.main[0].name, "Techno Man");
        assert_eq!(r1.releaseartists.main[1].name, "Bass Man");

        // Check r2
        let r2 = releases.iter().find(|r| r.id == "r2").unwrap();
        assert_eq!(r2.releasetitle, "Release 2");
        assert_eq!(r2.releasetype, "album");
        assert_eq!(r2.releasedate, Some(RoseDate::new(Some(2021), None, None)));
        assert!(r2.new);
        assert_eq!(r2.genres, vec!["Modern Classical"]);
        assert_eq!(r2.secondary_genres, vec!["Orchestral Music"]);
        assert_eq!(r2.descriptors, vec!["Wet"]);
        assert_eq!(r2.releaseartists.main[0].name, "Violin Woman");
        assert_eq!(r2.releaseartists.guest[0].name, "Conductor Woman");
        assert!(r2.cover_image_path.is_some());

        // Check r3
        let r3 = releases.iter().find(|r| r.id == "r3").unwrap();
        assert_eq!(r3.releasetitle, "Release 3");
        assert_eq!(r3.releasetype, "album");
        assert_eq!(r3.releasedate, Some(RoseDate::new(Some(2021), Some(4), Some(20))));
        assert_eq!(r3.genres.len(), 0);

        // Check r4
        let r4 = releases.iter().find(|r| r.id == "r4").unwrap();
        assert_eq!(r4.releasetitle, "Release 4");
        assert_eq!(r4.releasetype, "loosetrack");
    }

    #[test]
    fn test_get_release_and_associated_tracks() {
        let (config, _temp_dir) = testing::seeded_cache();

        let release = get_release(&config, "r1").unwrap().unwrap();
        assert_eq!(release.id, "r1");
        assert_eq!(release.releasetitle, "Release 1");
        assert_eq!(release.releasetype, "album");
        assert_eq!(release.releasedate, Some(RoseDate::new(Some(2023), None, None)));
        assert!(!release.new);
        assert_eq!(release.genres, vec!["Techno", "Deep House"]);
        assert!(release.parent_genres.contains(&"Electronic".to_string()));
        assert!(release.parent_genres.contains(&"Dance".to_string()));
        assert_eq!(release.secondary_genres, vec!["Rominimal", "Ambient"]);
        assert_eq!(release.descriptors, vec!["Warm", "Hot"]);
        assert_eq!(release.labels, vec!["Silk Music"]);
        assert_eq!(release.releaseartists.main.len(), 2);
        assert_eq!(release.releaseartists.main[0].name, "Techno Man");
        assert_eq!(release.releaseartists.main[1].name, "Bass Man");

        let tracks = get_tracks_of_release(&config, &release).unwrap();
        assert_eq!(tracks.len(), 2);

        assert_eq!(tracks[0].id, "t1");
        assert_eq!(tracks[0].tracktitle, "Track 1");
        assert_eq!(tracks[0].tracknumber, "01");
        assert_eq!(tracks[0].tracktotal, 2);
        assert_eq!(tracks[0].duration_seconds, 120);
        assert_eq!(tracks[0].trackartists.main[0].name, "Techno Man");
        assert_eq!(tracks[0].trackartists.main[1].name, "Bass Man");

        assert_eq!(tracks[1].id, "t2");
        assert_eq!(tracks[1].tracktitle, "Track 2");
        assert_eq!(tracks[1].tracknumber, "02");
    }

    #[test]
    fn test_get_release_applies_artist_aliases() {
        let (mut config, _temp_dir) = testing::seeded_cache();

        // Set up artist aliases
        let mut artist_aliases_map = HashMap::new();
        artist_aliases_map.insert("Hype Boy".to_string(), vec!["Bass Man".to_string()]);
        artist_aliases_map.insert("Bubble Gum".to_string(), vec!["Hype Boy".to_string()]);

        let mut artist_aliases_parents_map = HashMap::new();
        artist_aliases_parents_map.insert("Bass Man".to_string(), vec!["Hype Boy".to_string()]);
        artist_aliases_parents_map.insert("Hype Boy".to_string(), vec!["Bubble Gum".to_string()]);

        config.artist_aliases_map = artist_aliases_map;
        config.artist_aliases_parents_map = artist_aliases_parents_map;

        let release = get_release(&config, "r1").unwrap().unwrap();

        // Check that aliases are applied
        assert_eq!(release.releaseartists.main.len(), 4);
        assert_eq!(release.releaseartists.main[0].name, "Techno Man");
        assert!(!release.releaseartists.main[0].alias);
        assert_eq!(release.releaseartists.main[1].name, "Bass Man");
        assert!(!release.releaseartists.main[1].alias);
        assert_eq!(release.releaseartists.main[2].name, "Hype Boy");
        assert!(release.releaseartists.main[2].alias);
        assert_eq!(release.releaseartists.main[3].name, "Bubble Gum");
        assert!(release.releaseartists.main[3].alias);

        let tracks = get_tracks_of_release(&config, &release).unwrap();
        for track in tracks {
            assert_eq!(track.trackartists.main.len(), 4);
            assert_eq!(track.trackartists.main[0].name, "Techno Man");
            assert!(!track.trackartists.main[0].alias);
            assert_eq!(track.trackartists.main[1].name, "Bass Man");
            assert!(!track.trackartists.main[1].alias);
            assert_eq!(track.trackartists.main[2].name, "Hype Boy");
            assert!(track.trackartists.main[2].alias);
            assert_eq!(track.trackartists.main[3].name, "Bubble Gum");
            assert!(track.trackartists.main[3].alias);
        }
    }

    #[test]
    fn test_get_release_logtext() {
        let (config, _temp_dir) = testing::seeded_cache();
        assert_eq!(get_release_logtext(&config, "r1").unwrap(), "Techno Man & Bass Man - 2023. Release 1");
    }

    #[test]
    fn test_list_tracks() {
        let (config, _temp_dir) = testing::seeded_cache();

        let tracks = list_tracks(&config, None).unwrap();
        assert_eq!(tracks.len(), 5); // t1, t2, t3, t4, t5

        // Check t1
        let t1 = tracks.iter().find(|t| t.id == "t1").unwrap();
        assert_eq!(t1.tracktitle, "Track 1");
        assert_eq!(t1.tracknumber, "01");
        assert_eq!(t1.tracktotal, 2);
        assert_eq!(t1.discnumber, "01");
        assert_eq!(t1.duration_seconds, 120);
        assert_eq!(t1.trackartists.main.len(), 2);
        assert_eq!(t1.trackartists.main[0].name, "Techno Man");
        assert_eq!(t1.trackartists.main[1].name, "Bass Man");
        assert_eq!(t1.release.id, "r1");
        assert_eq!(t1.release.releasetitle, "Release 1");

        // Check t2
        let t2 = tracks.iter().find(|t| t.id == "t2").unwrap();
        assert_eq!(t2.tracktitle, "Track 2");
        assert_eq!(t2.tracknumber, "02");
        assert_eq!(t2.tracktotal, 2);
        assert_eq!(t2.duration_seconds, 240);
        assert_eq!(t2.release.id, "r1");

        // Check t3
        let t3 = tracks.iter().find(|t| t.id == "t3").unwrap();
        assert_eq!(t3.tracktitle, "Track 1");
        assert_eq!(t3.tracknumber, "01");
        assert_eq!(t3.tracktotal, 1);
        assert_eq!(t3.release.id, "r2");
        assert_eq!(t3.trackartists.main[0].name, "Violin Woman");
        assert_eq!(t3.trackartists.guest[0].name, "Conductor Woman");

        // Check t4 and t5 exist
        assert!(tracks.iter().any(|t| t.id == "t4"));
        assert!(tracks.iter().any(|t| t.id == "t5"));
    }

    #[test]
    fn test_get_track() {
        let (config, _temp_dir) = testing::seeded_cache();

        let track = get_track(&config, "t1").unwrap().unwrap();
        assert_eq!(track.id, "t1");
        assert_eq!(track.source_mtime, "999");
        assert_eq!(track.tracktitle, "Track 1");
        assert_eq!(track.tracknumber, "01");
        assert_eq!(track.tracktotal, 2);
        assert_eq!(track.discnumber, "01");
        assert_eq!(track.duration_seconds, 120);
        assert_eq!(track.trackartists.main.len(), 2);
        assert_eq!(track.trackartists.main[0].name, "Techno Man");
        assert_eq!(track.trackartists.main[1].name, "Bass Man");
        assert_eq!(track.metahash, "1");
        assert_eq!(track.release.id, "r1");
    }

    #[test]
    fn test_track_within_release() {
        let (config, _temp_dir) = testing::seeded_cache();

        assert!(track_within_release(&config, "t1", "r1").unwrap());
        assert!(!track_within_release(&config, "t3", "r1").unwrap());
        assert!(!track_within_release(&config, "lalala", "r1").unwrap());
        assert!(!track_within_release(&config, "t1", "lalala").unwrap());
    }

    #[test]
    fn test_track_within_playlist() {
        let (config, _temp_dir) = testing::seeded_cache();

        assert!(track_within_playlist(&config, "t1", "Lala Lisa").unwrap());
        assert!(!track_within_playlist(&config, "t2", "Lala Lisa").unwrap());
        assert!(!track_within_playlist(&config, "lalala", "Lala Lisa").unwrap());
        assert!(!track_within_playlist(&config, "t1", "lalala").unwrap());
    }

    #[test]
    fn test_release_within_collage() {
        let (config, _temp_dir) = testing::seeded_cache();

        assert!(release_within_collage(&config, "r1", "Rose Gold").unwrap());
        assert!(!release_within_collage(&config, "r1", "Ruby Red").unwrap());
        assert!(!release_within_collage(&config, "lalala", "Rose Gold").unwrap());
        assert!(!release_within_collage(&config, "r1", "lalala").unwrap());
    }

    #[test]
    fn test_get_track_logtext() {
        let (config, _temp_dir) = testing::seeded_cache();
        assert_eq!(get_track_logtext(&config, "t1").unwrap(), "Techno Man & Bass Man - Track 1 [2023].m4a");
    }

    #[test]
    fn test_list_artists() {
        let (config, _temp_dir) = testing::seeded_cache();

        let artists = list_artists(&config).unwrap();
        let artist_set: HashSet<String> = artists.into_iter().collect();
        let expected: HashSet<String> =
            vec!["Techno Man".to_string(), "Bass Man".to_string(), "Violin Woman".to_string(), "Conductor Woman".to_string()].into_iter().collect();

        assert_eq!(artist_set, expected);
    }

    #[test]
    fn test_list_genres() {
        let (config, _temp_dir) = testing::seeded_cache();

        // Test the accumulator too - add Classical Music to r3
        let conn = connect(&config).unwrap();
        conn.execute("INSERT INTO releases_genres (release_id, genre, position) VALUES ('r3', 'Classical Music', 1)", []).unwrap();
        drop(conn);

        let genres = list_genres(&config).unwrap();
        let genre_set: HashSet<GenreEntry> = genres.into_iter().collect();

        // Check that we have the primary genres and their parent genres
        // Primary genres: Techno, Deep House (r1), Modern Classical (r2), Classical Music (r3 after insert)
        // Secondary genres: Rominimal, Ambient (r1), Orchestral Music (r2)
        // The exact set depends on the genre hierarchy, but we should at least have:
        assert!(genre_set.contains(&GenreEntry {
            name: "Techno".to_string(),
            only_new_releases: false
        }));
        assert!(genre_set.contains(&GenreEntry {
            name: "Deep House".to_string(),
            only_new_releases: false
        }));
        assert!(genre_set.contains(&GenreEntry {
            name: "Modern Classical".to_string(),
            only_new_releases: true
        }));
        assert!(genre_set.contains(&GenreEntry {
            name: "Classical Music".to_string(),
            only_new_releases: false
        }));

        // Parent genres should exist
        assert!(genre_set.contains(&GenreEntry {
            name: "Electronic".to_string(),
            only_new_releases: false
        }));
        assert!(genre_set.contains(&GenreEntry {
            name: "Dance".to_string(),
            only_new_releases: false
        }));

        // Secondary genres
        assert!(genre_set.contains(&GenreEntry {
            name: "Rominimal".to_string(),
            only_new_releases: false
        }));
        assert!(genre_set.contains(&GenreEntry {
            name: "Ambient".to_string(),
            only_new_releases: false
        }));
        assert!(genre_set.contains(&GenreEntry {
            name: "Orchestral Music".to_string(),
            only_new_releases: true
        }));

        // Should have at least 9 genres (could be more with parent genres)
        assert!(genre_set.len() >= 9);
    }

    #[test]
    fn test_list_descriptors() {
        let (config, _temp_dir) = testing::seeded_cache();

        let descriptors = list_descriptors(&config).unwrap();
        let descriptor_set: std::collections::HashSet<_> = descriptors.into_iter().collect();
        let expected: std::collections::HashSet<_> = vec![
            DescriptorEntry {
                name: "Warm".to_string(),
                only_new_releases: false,
            },
            DescriptorEntry {
                name: "Hot".to_string(),
                only_new_releases: false,
            },
            DescriptorEntry {
                name: "Wet".to_string(),
                only_new_releases: true,
            },
        ]
        .into_iter()
        .collect();
        assert_eq!(descriptor_set, expected);
    }

    #[test]
    fn test_list_labels() {
        let (config, _temp_dir) = testing::seeded_cache();

        let labels = list_labels(&config).unwrap();
        let label_set: std::collections::HashSet<_> = labels.into_iter().collect();
        let expected: std::collections::HashSet<_> = vec![
            LabelEntry {
                name: "Silk Music".to_string(),
                only_new_releases: false,
            },
            LabelEntry {
                name: "Native State".to_string(),
                only_new_releases: true,
            },
        ]
        .into_iter()
        .collect();
        assert_eq!(label_set, expected);
    }

    #[test]
    fn test_list_collages() {
        let (config, _temp_dir) = testing::seeded_cache();

        let collages = list_collages(&config).unwrap();
        let collage_set: std::collections::HashSet<_> = collages.into_iter().map(|c| c.name).collect();
        let expected: std::collections::HashSet<_> = vec!["Rose Gold".to_string(), "Ruby Red".to_string()].into_iter().collect();
        assert_eq!(collage_set, expected);
    }

    #[test]
    fn test_get_collage() {
        let (config, _temp_dir) = testing::seeded_cache();

        // Test Rose Gold collage
        let collage = get_collage(&config, "Rose Gold").unwrap().unwrap();
        assert_eq!(collage.name, "Rose Gold");
        assert_eq!(collage.source_mtime, "999");

        let releases = get_collage_releases(&config, "Rose Gold").unwrap();
        assert_eq!(releases.len(), 2);

        // Check r1
        assert_eq!(releases[0].id, "r1");
        assert_eq!(releases[0].releasetitle, "Release 1");
        assert_eq!(releases[0].releasetype, "album");
        assert_eq!(releases[0].releasedate, Some(RoseDate::new(Some(2023), None, None)));
        assert_eq!(releases[0].genres, vec!["Techno", "Deep House"]);
        assert_eq!(releases[0].releaseartists.main.len(), 2);
        assert_eq!(releases[0].releaseartists.main[0].name, "Techno Man");
        assert_eq!(releases[0].releaseartists.main[1].name, "Bass Man");

        // Check r2
        assert_eq!(releases[1].id, "r2");
        assert_eq!(releases[1].releasetitle, "Release 2");
        assert_eq!(releases[1].releasetype, "album");
        assert_eq!(releases[1].releasedate, Some(RoseDate::new(Some(2021), None, None)));
        assert_eq!(releases[1].genres, vec!["Modern Classical"]);
        assert_eq!(releases[1].releaseartists.main[0].name, "Violin Woman");
        assert_eq!(releases[1].releaseartists.guest[0].name, "Conductor Woman");

        // Test Ruby Red collage (empty)
        let collage = get_collage(&config, "Ruby Red").unwrap().unwrap();
        assert_eq!(collage.name, "Ruby Red");
        assert_eq!(collage.source_mtime, "999");

        let releases = get_collage_releases(&config, "Ruby Red").unwrap();
        assert_eq!(releases.len(), 0);
    }

    #[test]
    fn test_list_playlists() {
        let (config, _temp_dir) = testing::seeded_cache();

        let playlists = list_playlists(&config).unwrap();
        let playlist_set: std::collections::HashSet<_> = playlists.into_iter().map(|p| p.name).collect();
        let expected: std::collections::HashSet<_> = vec!["Lala Lisa".to_string(), "Turtle Rabbit".to_string()].into_iter().collect();
        assert_eq!(playlist_set, expected);
    }

    #[test]
    fn test_get_playlist() {
        let (config, _temp_dir) = testing::seeded_cache();

        // Test Lala Lisa playlist
        let playlist = get_playlist(&config, "Lala Lisa").unwrap().unwrap();
        assert_eq!(playlist.name, "Lala Lisa");
        assert_eq!(playlist.source_mtime, "999");
        assert!(playlist.cover_path.is_some());
        assert!(playlist.cover_path.unwrap().to_string_lossy().ends_with("!playlists/Lala Lisa.jpg"));

        let tracks = get_playlist_tracks(&config, "Lala Lisa").unwrap();
        assert_eq!(tracks.len(), 2);

        // Check t1
        assert_eq!(tracks[0].id, "t1");
        assert_eq!(tracks[0].tracktitle, "Track 1");
        assert_eq!(tracks[0].tracknumber, "01");
        assert_eq!(tracks[0].tracktotal, 2);
        assert_eq!(tracks[0].duration_seconds, 120);
        assert_eq!(tracks[0].trackartists.main[0].name, "Techno Man");
        assert_eq!(tracks[0].trackartists.main[1].name, "Bass Man");
        assert_eq!(tracks[0].release.id, "r1");
        assert_eq!(tracks[0].release.releasetitle, "Release 1");

        // Check t3
        assert_eq!(tracks[1].id, "t3");
        assert_eq!(tracks[1].tracktitle, "Track 1");
        assert_eq!(tracks[1].tracknumber, "01");
        assert_eq!(tracks[1].tracktotal, 1);
        assert_eq!(tracks[1].duration_seconds, 120);
        assert_eq!(tracks[1].trackartists.main[0].name, "Violin Woman");
        assert_eq!(tracks[1].trackartists.guest[0].name, "Conductor Woman");
        assert_eq!(tracks[1].release.id, "r2");
        assert_eq!(tracks[1].release.releasetitle, "Release 2");
    }

    #[test]
    fn test_artist_exists() {
        let (config, _temp_dir) = testing::seeded_cache();

        assert!(artist_exists(&config, "Bass Man").unwrap());
        assert!(!artist_exists(&config, "lalala").unwrap());
    }

    #[test]
    fn test_artist_exists_with_alias() {
        let (mut config, _temp_dir) = testing::seeded_cache();

        // Create alias mappings
        let mut artist_aliases_map = HashMap::new();
        artist_aliases_map.insert("Hype Boy".to_string(), vec!["Bass Man".to_string()]);

        let mut artist_aliases_parents_map = HashMap::new();
        artist_aliases_parents_map.insert("Bass Man".to_string(), vec!["Hype Boy".to_string()]);

        config.artist_aliases_map = artist_aliases_map;
        config.artist_aliases_parents_map = artist_aliases_parents_map;

        assert!(artist_exists(&config, "Hype Boy").unwrap());
    }

    #[test]
    fn test_artist_exists_with_alias_transient() {
        let (mut config, _temp_dir) = testing::seeded_cache();

        // Create alias mappings
        let mut artist_aliases_map = HashMap::new();
        artist_aliases_map.insert("Hype Boy".to_string(), vec!["Bass Man".to_string()]);
        artist_aliases_map.insert("Bubble Gum".to_string(), vec!["Hype Boy".to_string()]);

        let mut artist_aliases_parents_map = HashMap::new();
        artist_aliases_parents_map.insert("Bass Man".to_string(), vec!["Hype Boy".to_string()]);
        artist_aliases_parents_map.insert("Hype Boy".to_string(), vec!["Bubble Gum".to_string()]);

        config.artist_aliases_map = artist_aliases_map;
        config.artist_aliases_parents_map = artist_aliases_parents_map;

        assert!(artist_exists(&config, "Bubble Gum").unwrap());
    }

    #[test]
    fn test_genre_exists() {
        let (config, _temp_dir) = testing::seeded_cache();

        assert!(genre_exists(&config, "Deep House").unwrap());
        assert!(!genre_exists(&config, "lalala").unwrap());
        // Parent genre
        assert!(genre_exists(&config, "Electronic").unwrap());
        // Child genre
        assert!(!genre_exists(&config, "Lo-Fi House").unwrap());
    }

    #[test]
    fn test_descriptor_exists() {
        let (config, _temp_dir) = testing::seeded_cache();

        assert!(descriptor_exists(&config, "Warm").unwrap());
        assert!(!descriptor_exists(&config, "Icy").unwrap());
    }

    #[test]
    fn test_label_exists() {
        let (config, _temp_dir) = testing::seeded_cache();

        assert!(label_exists(&config, "Silk Music").unwrap());
        assert!(!label_exists(&config, "Cotton Music").unwrap());
    }

    #[test]
    fn test_collage_exists() {
        let (config, _temp_dir) = testing::seeded_cache();

        assert!(collage_exists(&config, "Rose Gold").unwrap());
        assert!(collage_exists(&config, "Ruby Red").unwrap());
        assert!(!collage_exists(&config, "Emerald Green").unwrap());
    }

    #[test]
    fn test_playlist_exists() {
        let (config, _temp_dir) = testing::seeded_cache();

        assert!(playlist_exists(&config, "Lala Lisa").unwrap());
        assert!(playlist_exists(&config, "Turtle Rabbit").unwrap());
        assert!(!playlist_exists(&config, "Bunny Hop").unwrap());
    }
}
