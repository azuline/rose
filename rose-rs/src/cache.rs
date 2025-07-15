/// The cache module encapsulates the read cache and exposes handles for working with the read cache. It
/// also exposes a locking mechanism that uses the read cache for synchronization.
///
/// The SQLite database is considered part of the cache, and so this module encapsulates the SQLite
/// database too.
use crate::common::{Artist, ArtistMapping, Result, RoseDate, VERSION};
use crate::config::Config;
use crate::genre_hierarchy::TRANSITIVE_PARENT_GENRES;
use once_cell::sync::Lazy;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::debug;

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
                .query_row("SELECT schema_hash, config_hash, version FROM _schema_hash", [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
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
    conn.execute(
        "INSERT INTO _schema_hash (schema_hash, config_hash, version) VALUES (?1, ?2, ?3)",
        params![schema_hash, config_hash, VERSION],
    )?;

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
        let max_valid_until: Option<f64> = conn
            .query_row("SELECT MAX(valid_until) FROM locks WHERE name = ?1", params![name], |row| row.get(0))
            .optional()?;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDataFile {
    #[serde(default = "default_true")]
    pub new: bool,
    #[serde(default = "default_added_at")]
    pub added_at: String,
}

fn default_true() -> bool {
    true
}

fn default_added_at() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
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
    let split_lists: Vec<Vec<&str>> = xxs
        .iter()
        .map(|xs| if xs.is_empty() { Vec::new() } else { xs.split(" ¬ ").collect() })
        .collect();

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

// Placeholder functions - to be implemented
pub fn update_cache_for_releases(_c: &Config, _release_dirs: Option<Vec<PathBuf>>, _force: bool, _force_multiprocessing: bool) -> Result<()> {
    // TODO: Implement - see cache_py.rs for Python implementation
    Ok(())
}

pub fn update_cache_evict_nonexistent_releases(_c: &Config) -> Result<()> {
    // TODO: Implement - see cache_py.rs for Python implementation
    Ok(())
}

pub fn update_cache_for_collages(_c: &Config, _collage_names: Option<Vec<String>>, _force: bool) -> Result<()> {
    // TODO: Implement - see cache_py.rs for Python implementation
    Ok(())
}

pub fn update_cache_evict_nonexistent_collages(_c: &Config) -> Result<()> {
    // TODO: Implement - see cache_py.rs for Python implementation
    Ok(())
}

pub fn update_cache_for_playlists(_c: &Config, _playlist_names: Option<Vec<String>>, _force: bool) -> Result<()> {
    // TODO: Implement - see cache_py.rs for Python implementation
    Ok(())
}

pub fn update_cache_evict_nonexistent_playlists(_c: &Config) -> Result<()> {
    // TODO: Implement - see cache_py.rs for Python implementation
    Ok(())
}

// Additional placeholder functions needed by lib.rs
pub fn get_release(_c: &Config, _id: &str) -> Result<Option<Release>> {
    // TODO: Implement - see cache_py.rs for Python implementation
    Ok(None)
}

pub fn list_releases(_c: &Config) -> Result<Vec<Release>> {
    // TODO: Implement - see cache_py.rs for Python implementation
    Ok(Vec::new())
}

pub fn get_track(_c: &Config, _id: &str) -> Result<Option<Track>> {
    // TODO: Implement - see cache_py.rs for Python implementation
    Ok(None)
}

pub fn list_tracks(_c: &Config) -> Result<Vec<Track>> {
    // TODO: Implement - see cache_py.rs for Python implementation
    Ok(Vec::new())
}

pub fn list_collages(_c: &Config) -> Result<Vec<Collage>> {
    // TODO: Implement - see cache_py.rs for Python implementation
    Ok(Vec::new())
}

pub fn list_playlists(_c: &Config) -> Result<Vec<Playlist>> {
    // TODO: Implement - see cache_py.rs for Python implementation
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

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
    #[ignore = "Needs proper Config type - testing module returns common::Config not config::Config"]
    fn test_maybe_invalidate_cache_database() {
        // TODO: Fix this test once we have proper test utilities that return config::Config
        // let _ = testing::init();
        // let (config, _temp_dir) = testing::seeded_cache();
        //
        // // First call should create the database
        // maybe_invalidate_cache_database(&config).unwrap();
        // assert!(config.cache_database_path().exists());
        //
        // // Second call should not recreate it since nothing changed
        // let mtime_before = std::fs::metadata(config.cache_database_path()).unwrap().modified().unwrap();
        // maybe_invalidate_cache_database(&config).unwrap();
        // let mtime_after = std::fs::metadata(config.cache_database_path()).unwrap().modified().unwrap();
        // assert_eq!(mtime_before, mtime_after);
    }
}
