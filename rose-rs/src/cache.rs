/// The cache module encapsulates the read cache and exposes handles for working with the read cache. It
/// also exposes a locking mechanism that uses the read cache for synchronization.
///
/// The SQLite database is considered part of the cache, and so this module encapsulates the SQLite
/// database too.
use crate::common::{Artist, ArtistMapping, RoseDate, VERSION};
use crate::config::Config;
use crate::errors::Result;
use crate::genre_hierarchy::TRANSITIVE_PARENT_GENRES;
use once_cell::sync::Lazy;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info};

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
            .query_row("SELECT MAX(valid_until) FROM locks WHERE name = ?1", params![name], |row| {
                row.get::<_, Option<f64>>(0)
            })
            .unwrap_or(None);

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

// def update_cache_for_releases(
//     c: Config,
//     # Leave as None to update all releases.
//     release_dirs: list[Path] | None = None,
//     force: bool = False,
//     # For testing.
//     force_multiprocessing: bool = False,
// ) -> None:
//     """
//     Update the read cache to match the data for any passed-in releases. If a directory lacks a
//     .rose.{uuid}.toml datafile, create the datafile for the release and set it to the initial state.
//
//     This is a hot path and is thus performance-optimized. The bottleneck is disk accesses, so we
//     structure this function in order to minimize them. We solely read files that have changed since
//     last run and batch writes together. We trade higher memory for reduced disk accesses.
//     Concretely, we:
//
//     1. Execute one big SQL query at the start to fetch the relevant previous caches.
//     2. Skip reading a file's data if the mtime has not changed since the previous cache update.
//     3. Batch SQLite write operations to the end of this function, and only execute a SQLite upsert
//        if the read data differs from the previous caches.
//
//     We also shard the directories across multiple processes and execute them simultaneously.
//     """
//     release_dirs = release_dirs or [Path(d.path) for d in os.scandir(c.music_source_dir) if d.is_dir()]
//     release_dirs = [
//         d
//         for d in release_dirs
//         if d.name != "!collages" and d.name != "!playlists" and d.name not in c.ignore_release_directories
//     ]
//     if not release_dirs:
//         logger.debug("No-Op: No whitelisted releases passed into update_cache_for_releases")
//         return
//     logger.debug(f"Refreshing the read cache for {len(release_dirs)} releases")
//     if len(release_dirs) < 10:
//         logger.debug(f"Refreshing cached data for {', '.join([r.name for r in release_dirs])}")
//
//     # If the number of releases changed is less than 50; do not bother with all that multiprocessing
//     # gunk: instead, directly call the executor.
//     #
//     # This has an added benefit of not spawning processes from the virtual filesystem and watchdog
//     # processes, as those processes always update the cache for one release at a time and are
//     # multithreaded. Starting other processes from threads is bad!
//     if not force_multiprocessing and len(release_dirs) < 50:
//         logger.debug(f"Running cache update executor in same process because {len(release_dirs)=} < 50")
//         _update_cache_for_releases_executor(c, release_dirs, force)
//         return
//
//     # Batch size defaults to equal split across all processes. However, if the number of directories
//     # is small, we shrink the # of processes to save on overhead.
//     num_proc = c.max_proc
//     if len(release_dirs) < c.max_proc * 50:
//         num_proc = max(1, math.ceil(len(release_dirs) // 50))
//     batch_size = len(release_dirs) // num_proc + 1
//
//     manager = multiprocessing.Manager()
//     # Have each process propagate the collages and playlists it wants to update back upwards. We
//     # will dispatch the force updater only once in the main process, instead of many times in each
//     # process.
//     collages_to_force_update = manager.list()
//     playlists_to_force_update = manager.list()
//
//     errors: list[BaseException] = []
//
//     logger.debug("Creating multiprocessing pool to parallelize cache executors.")
//     with multiprocessing.Pool(processes=c.max_proc) as pool:
//         # At 0, no batch. At 1, 1 batch. At 49, 1 batch. At 50, 1 batch. At 51, 2 batches.
//         for i in range(0, len(release_dirs), batch_size):
//             logger.debug(f"Spawning release cache update process for releases [{i}, {i + batch_size})")
//             pool.apply_async(
//                 _update_cache_for_releases_executor,
//                 (
//                     c,
//                     release_dirs[i : i + batch_size],
//                     force,
//                     collages_to_force_update,
//                     playlists_to_force_update,
//                 ),
//                 error_callback=lambda e: errors.append(e),
//             )
//         pool.close()
//         pool.join()
//
//     if errors:
//         raise ExceptionGroup("Exception occurred in cache update subprocesses", errors)  # type: ignore
//
//     if collages_to_force_update:
//         update_cache_for_collages(c, uniq(list(collages_to_force_update)), force=True)
//     if playlists_to_force_update:
//         update_cache_for_playlists(c, uniq(list(playlists_to_force_update)), force=True)
//
// def _update_cache_for_releases_executor(
//     c: Config,
//     release_dirs: list[Path],
//     force: bool,
//     # If these are not None, we will store the collages and playlists to update in here instead of
//     # invoking the update functions directly. If these are None, we will not put anything in them
//     # and instead invoke update_cache_for_{collages,playlists} directly. This is a Bad Pattern, but
//     # good enough.
//     collages_to_force_update_receiver: list[str] | None = None,
//     playlists_to_force_update_receiver: list[str] | None = None,
// ) -> None:
//     """The implementation logic, split out for multiprocessing."""
//     # NOTE: This is a very large function (~850 lines) that handles the actual cache update logic.
//     # It performs the following steps:
//     # 1. Scans directories and reads .rose.{uuid}.toml files
//     # 2. Batch queries existing cache data
//     # 3. Compares mtimes and metadata to determine what needs updating
//     # 4. Reads audio file tags for new/changed tracks
//     # 5. Batch inserts/updates the database
//     # 6. Updates full-text search tables
//     # 7. Handles collage and playlist references
//     # The full implementation can be found in cache_py.rs lines 986-1833
pub fn update_cache_for_releases(_c: &Config, _release_dirs: Option<Vec<PathBuf>>, _force: bool, _force_multiprocessing: bool) -> Result<()> {
    // TODO: Implement
    Ok(())
}

// def update_cache_evict_nonexistent_releases(c: Config) -> None:
//     logger.debug("Evicting cached releases that are not on disk")
//     dirs = [Path(d.path).resolve() for d in os.scandir(c.music_source_dir) if d.is_dir()]
//     with connect(c) as conn:
//         cursor = conn.execute(
//             f"""
//             DELETE FROM releases
//             WHERE source_path NOT IN ({','.join(['?'] * len(dirs))})
//             RETURNING source_path
//             """,
//             [str(d) for d in dirs],
//         )
//         for row in cursor:
//             logger.info(f"Evicted missing release {row['source_path']} from cache")
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

// def update_cache_for_collages(
//     c: Config,
//     # Leave as None to update all collages.
//     collage_names: list[str] | None = None,
//     force: bool = False,
// ) -> None:
//     """
//     Update the read cache to match the data for all stored collages.
//
//     This is performance-optimized in a similar way to the update releases function. We:
//
//     1. Execute one big SQL query at the start to fetch the relevant previous caches.
//     2. Skip reading a file's data if the mtime has not changed since the previous cache update.
//     3. Only execute a SQLite upsert if the read data differ from the previous caches.
//
//     However, we do not batch writes to the end of the function, nor do we process the collages in
//     parallel. This is because we should have far fewer collages than releases.
//     """
//     collage_dir = c.music_source_dir / "!collages"
//     collage_dir.mkdir(exist_ok=True)
//
//     files: list[tuple[Path, str, os.DirEntry[str]]] = []
//     for f in os.scandir(str(collage_dir)):
//         path = Path(f.path)
//         if path.suffix != ".toml":
//             continue
//         if not path.is_file():
//             logger.debug(f"Skipping processing collage {path.name} because it is not a file")
//             continue
//         if collage_names is None or path.stem in collage_names:
//             files.append((path.resolve(), path.stem, f))
//     logger.debug(f"Refreshing the read cache for {len(files)} collages")
pub fn update_cache_for_collages(_c: &Config, _collage_names: Option<Vec<String>>, _force: bool) -> Result<()> {
    // TODO: Implement
    Ok(())
}

// def update_cache_evict_nonexistent_collages(c: Config) -> None:
//     logger.debug("Evicting cached collages that are not on disk")
//     collage_names: list[str] = []
//     collage_dir = c.music_source_dir / "!collages"
//     for f in os.scandir(str(collage_dir)):
//         path = Path(f.path)
//         if path.suffix == ".toml" and path.is_file():
//             collage_names.append(path.stem)
//
//     with connect(c) as conn:
//         cursor = conn.execute(
//             f"""
//             DELETE FROM collages
//             WHERE name NOT IN ({','.join(['?'] * len(collage_names))})
//             RETURNING name
//             """
//             if collage_names
//             else "DELETE FROM collages RETURNING name",
//             collage_names,
//         )
//         for row in cursor:
//             logger.info(f"Evicted missing collage {row['name']} from cache")
pub fn update_cache_evict_nonexistent_collages(c: &Config) -> Result<()> {
    debug!("Evicting cached collages that are not on disk");

    let collages_dir = c.music_source_dir.join("!collages");
    let mut collage_names = Vec::new();

    if collages_dir.exists() {
        for entry in fs::read_dir(&collages_dir)? {
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
        let deleted_names: Vec<String> = stmt.query_map([], |row| row.get(0))?.collect::<std::result::Result<Vec<_>, _>>()?;

        for name in deleted_names {
            info!("Evicted missing collage {} from cache", name);
        }
    } else {
        // Delete collages not in the list
        let placeholders = vec!["?"; collage_names.len()].join(",");
        let query = format!("DELETE FROM collages WHERE name NOT IN ({placeholders}) RETURNING name");

        let mut stmt = conn.prepare(&query)?;
        let deleted_names: Vec<String> = stmt
            .query_map(rusqlite::params_from_iter(&collage_names), |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        for name in deleted_names {
            info!("Evicted missing collage {} from cache", name);
        }
    }

    Ok(())
}

// def update_cache_for_playlists(
//     c: Config,
//     # Leave as None to update all playlists.
//     playlist_names: list[str] | None = None,
//     force: bool = False,
// ) -> None:
//     """
//     Update the read cache to match the data for all stored playlists.
//
//     This is performance-optimized in a similar way to the update releases function. We:
//
//     1. Execute one big SQL query at the start to fetch the relevant previous caches.
//     2. Skip reading a file's data if the mtime has not changed since the previous cache update.
//     3. Only execute a SQLite upsert if the read data differ from the previous caches.
//
//     However, we do not batch writes to the end of the function, nor do we process the playlists in
//     parallel. This is because we should have far fewer playlists than releases.
//     """
//     playlist_dir = c.music_source_dir / "!playlists"
//     playlist_dir.mkdir(exist_ok=True)
//
//     files: list[tuple[Path, str, os.DirEntry[str]]] = []
//     for f in os.scandir(str(playlist_dir)):
//         path = Path(f.path)
//         if path.suffix != ".toml":
//             continue
//         if not path.is_file():
//             logger.debug(f"Skipping processing playlist {path.name} because it is not a file")
//             continue
//         if playlist_names is None or path.stem in playlist_names:
//             files.append((path.resolve(), path.stem, f))
//     logger.debug(f"Refreshing the read cache for {len(files)} playlists")
pub fn update_cache_for_playlists(_c: &Config, _playlist_names: Option<Vec<String>>, _force: bool) -> Result<()> {
    // TODO: Implement
    Ok(())
}

// def update_cache_evict_nonexistent_playlists(c: Config) -> None:
//     logger.debug("Evicting cached playlists that are not on disk")
//     playlist_names: list[str] = []
//     playlist_dir = c.music_source_dir / "!playlists"
//     for f in os.scandir(str(playlist_dir)):
//         path = Path(f.path)
//         if path.suffix == ".toml" and path.is_file():
//             playlist_names.append(path.stem)
//
//     with connect(c) as conn:
//         cursor = conn.execute(
//             f"""
//             DELETE FROM playlists
//             WHERE name NOT IN ({','.join(['?'] * len(playlist_names))})
//             RETURNING name
//             """
//             if playlist_names
//             else "DELETE FROM playlists RETURNING name",
//             playlist_names,
//         )
//         for row in cursor:
//             logger.info(f"Evicted missing playlist {row['name']} from cache")
pub fn update_cache_evict_nonexistent_playlists(c: &Config) -> Result<()> {
    debug!("Evicting cached playlists that are not on disk");

    let playlists_dir = c.music_source_dir.join("!playlists");
    let mut playlist_names = Vec::new();

    if playlists_dir.exists() {
        for entry in fs::read_dir(&playlists_dir)? {
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
        let deleted_names: Vec<String> = stmt.query_map([], |row| row.get(0))?.collect::<std::result::Result<Vec<_>, _>>()?;

        for name in deleted_names {
            info!("Evicted missing playlist {} from cache", name);
        }
    } else {
        // Delete playlists not in the list
        let placeholders = vec!["?"; playlist_names.len()].join(",");
        let query = format!("DELETE FROM playlists WHERE name NOT IN ({placeholders}) RETURNING name");

        let mut stmt = conn.prepare(&query)?;
        let deleted_names: Vec<String> = stmt
            .query_map(rusqlite::params_from_iter(&playlist_names), |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        for name in deleted_names {
            info!("Evicted missing playlist {} from cache", name);
        }
    }

    Ok(())
}

// Additional placeholder functions needed by lib.rs
// def get_release(c: Config, release_id: str) -> Release | None:
//     with connect(c) as conn:
//         cursor = conn.execute(
//             "SELECT * FROM releases_view WHERE id = ?",
//             (release_id,),
//         )
//         row = cursor.fetchone()
//         if not row:
//             return None
//         return cached_release_from_view(c, row)
pub fn get_release(c: &Config, release_id: &str) -> Result<Option<Release>> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare("SELECT * FROM releases_view WHERE id = ?1")?;

    let release = stmt
        .query_row(params![release_id], |row| {
            cached_release_from_view(c, row, true).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        })
        .optional()?;

    Ok(release)
}

// def list_releases(
//     c: Config,
//     # The or_labels/or_genres/or_descriptors fields contain labels/genres/descriptors that we are going
//     # to union together when filtering. We want releases that have at least one of the labels and at
//     # least one of the genres.
//     #
//     # Labels, Genres, and Descriptors are three separate fields, so we still intersect them together.
//     # That is, to match, a release must match at least one of the labels and genres. But both labels
//     # and genres must have a match.
//     or_labels: list[str] | None = None,
//     or_genres: list[str] | None = None,
//     or_descriptors: list[str] | None = None,
// ) -> list[Release]:
//     """Fetch all releases. Can be filtered. By default, returns all releases."""
//     filter_sql = ""
//     filter_params: list[str] = []
//     if or_labels:
//         filter_sql += f"""
//             AND id IN (
//               SELECT release_id FROM releases_labels
//               WHERE label IN ({','.join(['?'] * len(or_labels))})
//             )
//         """
//         filter_params.extend(or_labels)
//     if or_genres:
//         filter_sql += f"""
//             AND id IN (
//               SELECT release_id FROM releases_genres
//               WHERE genre IN ({','.join(['?'] * len(or_genres))})
//             )
//         """
//         filter_params.extend(or_genres)
//     if or_descriptors:
//         filter_sql += f"""
//             AND id IN (
//               SELECT release_id FROM releases_descriptors
//               WHERE descriptor IN ({','.join(['?'] * len(or_descriptors))})
//             )
//         """
//         filter_params.extend(or_descriptors)
//
//     releases: list[Release] = []
//     with connect(c) as conn:
//         cursor = conn.execute(
//             f"SELECT * FROM releases_view WHERE true {filter_sql} ORDER BY id",
//             filter_params,
//         )
//         for row in cursor:
//             releases.append(cached_release_from_view(c, row))
//     return releases
pub fn list_releases(c: &Config) -> Result<Vec<Release>> {
    // For now, implement without filters - just return all releases
    // TODO: Implement full filtering support with or_labels, or_genres, or_descriptors
    let conn = connect(c)?;
    let mut stmt = conn.prepare("SELECT * FROM releases_view ORDER BY id")?;

    let releases = stmt
        .query_map([], |row| {
            cached_release_from_view(c, row, true).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(releases)
}

// def get_track(c: Config, uuid: str) -> Track | None:
//     with connect(c) as conn:
//         cursor = conn.execute(
//             "SELECT * FROM tracks_view WHERE id = ?",
//             (uuid,),
//         )
//         row = cursor.fetchone()
//         if not row:
//             return None
//         release = get_release(c, row["release_id"])
//         assert release is not None
//         return cached_track_from_view(c, row, release)
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

// def list_tracks(c: Config, track_ids: list[str] | None = None) -> list[Track]:
//     """
//     Fetch all tracks. If track_ids is specified, only fetches tracks with an exact ID match.
//     Otherwise, returns all tracks in the library.
//     """
//     query = "SELECT * FROM tracks_view"
//     params: list[str] = []
//
//     if track_ids is not None:
//         if not track_ids:
//             return []
//         query += f" WHERE id IN ({','.join(['?'] * len(track_ids))})"
//         params.extend(track_ids)
//
//     query += " ORDER BY source_path"
//
//     tracks: list[Track] = []
//     releases: dict[str, Release] = {}
//     with connect(c) as conn:
//         cursor = conn.execute(query, params)
//         for row in cursor:
//             release_id = row["release_id"]
//             if release_id not in releases:
//                 release = get_release(c, release_id)
//                 assert release is not None
//                 releases[release_id] = release
//             tracks.append(cached_track_from_view(c, row, releases[release_id]))
//     return tracks
pub fn list_tracks(c: &Config) -> Result<Vec<Track>> {
    list_tracks_with_filter(c, None)
}

pub fn get_tracks_of_release(c: &Config, release: &Release) -> Result<Vec<Track>> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare("SELECT * FROM tracks_view WHERE release_id = ? ORDER BY tracknumber, id")?;
    let release_arc = Arc::new(release.clone());
    let tracks = stmt
        .query_map([&release.id], |row| {
            cached_track_from_view(c, row, release_arc.clone(), true)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(tracks)
}

pub fn list_tracks_with_filter(c: &Config, track_ids: Option<Vec<String>>) -> Result<Vec<Track>> {
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

// def list_collages(c: Config) -> list[str]:
//     with connect(c) as conn:
//         cursor = conn.execute("SELECT name FROM collages ORDER BY name")
//         return [row["name"] for row in cursor]
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

// def list_playlists(c: Config) -> list[str]:
//     with connect(c) as conn:
//         cursor = conn.execute("SELECT name FROM playlists ORDER BY name")
//         return [row["name"] for row in cursor]
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
    let mut genres: Vec<GenreEntry> = genre_map
        .into_iter()
        .map(|(name, only_new_releases)| GenreEntry { name, only_new_releases })
        .collect();
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
            let artists = r.releaseartists.main.iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(" & ");
            
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
            let artists = t.trackartists.main.iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(" & ");
            
            let date_part = if let Some(date) = t.release.releasedate {
                match date.year {
                    Some(year) => format!("[{}]", year),
                    None => "[Unknown]".to_string(),
                }
            } else {
                "[Unknown]".to_string()
            };
            
            let extension = t.source_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("unknown");
            
            Ok(format!("{} - {} {}.{}", artists, t.tracktitle, date_part, extension))
        }
        None => Ok("Unknown Track".to_string()),
    }
}

/// Get a collage by name
pub fn get_collage(c: &Config, collage_name: &str) -> Result<Option<Collage>> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare("SELECT name, source_mtime FROM collages WHERE name = ?")?;
    
    let collage = stmt.query_row([collage_name], |row| {
        Ok(Collage {
            name: row.get(0)?,
            source_mtime: row.get(1)?,
        })
    }).optional()?;
    
    Ok(collage)
}

/// Get all releases in a collage
pub fn get_collage_releases(c: &Config, collage_name: &str) -> Result<Vec<Release>> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare(
        "SELECT release_id FROM collages_releases 
         WHERE collage_name = ? AND NOT missing 
         ORDER BY position"
    )?;
    
    let release_ids: Vec<String> = stmt
        .query_map([collage_name], |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    
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
    
    let playlist = stmt.query_row([playlist_name], |row| {
        let cover_path: Option<String> = row.get(2)?;
        Ok(Playlist {
            name: row.get(0)?,
            source_mtime: row.get(1)?,
            cover_path: cover_path.map(PathBuf::from),
        })
    }).optional()?;
    
    Ok(playlist)
}

/// Get all tracks in a playlist
pub fn get_playlist_tracks(c: &Config, playlist_name: &str) -> Result<Vec<Track>> {
    let conn = connect(c)?;
    let mut stmt = conn.prepare(
        "SELECT track_id FROM playlists_tracks 
         WHERE playlist_name = ? AND NOT missing 
         ORDER BY position"
    )?;
    
    let track_ids: Vec<String> = stmt
        .query_map([playlist_name], |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    
    let mut tracks = Vec::new();
    for id in track_ids {
        if let Some(track) = get_track(c, &id)? {
            tracks.push(track);
        }
    }
    
    Ok(tracks)
}

// Additional types and functions from Python implementation:
//
// @dataclasses.dataclass(slots=True, frozen=True)
// class GenreEntry:
//     genre: str
//     only_new_releases: bool
//
// @dataclasses.dataclass(slots=True, frozen=True)
// class DescriptorEntry:
//     descriptor: str
//     only_new_releases: bool
//
// @dataclasses.dataclass(slots=True, frozen=True)
// class LabelEntry:
//     label: str
//     only_new_releases: bool
//
// def list_genres(c: Config) -> list[GenreEntry]:
//     # Implementation in cache_py.rs lines 2837-2854
//
// def genre_exists(c: Config, genre: str) -> bool:
//     # Implementation in cache_py.rs lines 2857-2865
//
// def list_descriptors(c: Config) -> list[DescriptorEntry]:
//     # Implementation in cache_py.rs lines 2874-2890
//
// def descriptor_exists(c: Config, descriptor: str) -> bool:
//     # Implementation in cache_py.rs lines 2893-2899
//
// def list_labels(c: Config) -> list[LabelEntry]:
//     # Implementation in cache_py.rs lines 2908-2920
//
// def label_exists(c: Config, label: str) -> bool:
//     # Implementation in cache_py.rs lines 2921-2927
//
// def list_artists(c: Config) -> list[str]:
//     # Implementation in cache_py.rs lines 2808-2812
//
// def artist_exists(c: Config, artist: str) -> bool:
//     # Implementation in cache_py.rs lines 2814-2827
//
// def get_collage(c: Config, collage_name: str) -> Collage | None:
//     # Implementation in cache_py.rs lines 2774-2787
//
// def get_collage_releases(c: Config, collage_name: str) -> list[Release]:
//     # Implementation in cache_py.rs lines 2789-2806
//
// def get_playlist(c: Config, playlist_name: str) -> Playlist | None:
//     # Implementation in cache_py.rs lines 2711-2732
//
// def get_playlist_tracks(c: Config, playlist_name: str) -> list[Track]:
//     # Implementation in cache_py.rs lines 2734-2766
//
// def filter_releases(...) -> list[Release]:
//     # Large function for filtering releases - Implementation in cache_py.rs lines 2252-2344
//
// def filter_tracks(...) -> list[Track]:
//     # Large function for filtering tracks - Implementation in cache_py.rs lines 2346-2457

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;
    use std::collections::HashMap;

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
        assert_eq!(default_true(), true);
        
        // Test default_added_at returns a valid ISO8601 timestamp
        let timestamp = default_added_at();
        assert!(timestamp.contains('T'));
        assert!(timestamp.ends_with('Z'));
        
        // Test deserializing with defaults
        let json = "{}";
        let data: StoredDataFile = serde_json::from_str(json).unwrap();
        assert_eq!(data.new, true);
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
    // TODO: Implement these tests once we have proper test utilities

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
        let result = stmt.query_row([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?
            ))
        }).unwrap();
        
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
            ).unwrap();
            conn.execute(
                "INSERT INTO _schema_hash (schema_hash, config_hash, version)
                 VALUES ('haha', 'lala', 'blabla')",
                [],
            ).unwrap();
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
        let result = stmt.query_row([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?
            ))
        }).unwrap();
        
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
    #[ignore = "Not yet implemented"]
    fn test_update_cache_all() {
        // Python source:
        // def test_update_cache_all(config: Config) -> None:
        //     """Test that the update all function works."""
        //     shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
        //     shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
        //
        //     # Test that we prune deleted releases too.
        //     with connect(config) as conn:
        //         conn.execute(
        //             """
        //             INSERT INTO releases (id, source_path, added_at, datafile_mtime, title, releasetype, disctotal, metahash)
        //             VALUES ('aaaaaa', '0000-01-01T00:00:00+00:00', '999', 'nonexistent', 'aa', 'unknown', false, '0')
        //             """
        //         )
        //
        //     update_cache(config)
        //
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT COUNT(*) FROM releases")
        //         assert cursor.fetchone()[0] == 2
        //         cursor = conn.execute("SELECT COUNT(*) FROM tracks")
        //         assert cursor.fetchone()[0] == 4

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_multiprocessing() {
        // Python source:
        // def test_update_cache_multiprocessing(config: Config) -> None:
        //     """Test that the update all function works."""
        //     shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
        //     shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
        //     update_cache_for_releases(config, force_multiprocessing=True)
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT COUNT(*) FROM releases")
        //         assert cursor.fetchone()[0] == 2
        //         cursor = conn.execute("SELECT COUNT(*) FROM tracks")
        //         assert cursor.fetchone()[0] == 4

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_releases() {
        // Python source:
        // def test_update_cache_releases(config: Config) -> None:
        //     release_dir = config.music_source_dir / TEST_RELEASE_1.name
        //     shutil.copytree(TEST_RELEASE_1, release_dir)
        //     update_cache_for_releases(config, [release_dir])
        //
        //     # Check that the release directory was given a UUID.
        //     release_id: str | None = None
        //     for f in release_dir.iterdir():
        //         if m := STORED_DATA_FILE_REGEX.match(f.name):
        //             release_id = m[1]
        //     assert release_id is not None
        //
        //     # Assert that the release metadata was read correctly.
        //     with connect(config) as conn:
        //         cursor = conn.execute(
        //             """
        //             SELECT id, source_path, title, releasetype, releasedate, compositiondate, catalognumber, new
        //             FROM releases WHERE id = ?
        //             """,
        //             (release_id,),
        //         )
        //         row = cursor.fetchone()
        //         assert row["source_path"] == str(release_dir)
        //         assert row["title"] == "I Love Blackpink"
        //         assert row["releasetype"] == "album"
        //         assert row["releasedate"] == "1990-02-05"
        //         assert row["compositiondate"] is None
        //         assert row["catalognumber"] is None
        //         assert row["new"]
        //
        //         cursor = conn.execute(
        //             "SELECT genre FROM releases_genres WHERE release_id = ?",
        //             (release_id,),
        //         )
        //         genres = {r["genre"] for r in cursor.fetchall()}
        //         assert genres == {"K-Pop", "Pop"}
        //
        //         cursor = conn.execute(
        //             "SELECT label FROM releases_labels WHERE release_id = ?",
        //             (release_id,),
        //         )
        //         labels = {r["label"] for r in cursor.fetchall()}
        //         assert labels == {"A Cool Label"}
        //
        //         cursor = conn.execute(
        //             "SELECT artist, role FROM releases_artists WHERE release_id = ?",
        //             (release_id,),
        //         )
        //         artists = {(r["artist"], r["role"]) for r in cursor.fetchall()}
        //         assert artists == {
        //             ("BLACKPINK", "main"),
        //         }
        //
        //         for f in release_dir.iterdir():
        //             if f.suffix != ".m4a":
        //                 continue
        //
        //             # Assert that the track metadata was read correctly.
        //             cursor = conn.execute(
        //                 """
        //                 SELECT
        //                     id, source_path, title, release_id, tracknumber, discnumber, duration_seconds
        //                 FROM tracks WHERE source_path = ?
        //                 """,
        //                 (str(f),),
        //             )
        //             row = cursor.fetchone()
        //             track_id = row["id"]
        //             assert row["title"].startswith("Track")
        //             assert row["release_id"] == release_id
        //             assert row["tracknumber"] != ""
        //             assert row["discnumber"] == "1"
        //             assert row["duration_seconds"] == 2
        //
        //             cursor = conn.execute(
        //                 "SELECT artist, role FROM tracks_artists WHERE track_id = ?",
        //                 (track_id,),
        //             )
        //             artists = {(r["artist"], r["role"]) for r in cursor.fetchall()}
        //             assert artists == {
        //                 ("BLACKPINK", "main"),
        //             }

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_releases_uncached_with_existing_id() {
        // Python source:
        // def test_update_cache_releases_uncached_with_existing_id(config: Config) -> None:
        //     """Test that IDs in filenames are read and preserved."""
        //     release_dir = config.music_source_dir / TEST_RELEASE_2.name
        //     shutil.copytree(TEST_RELEASE_2, release_dir)
        //     update_cache_for_releases(config, [release_dir])
        //
        //     # Check that the release directory was given a UUID.
        //     release_id: str | None = None
        //     for f in release_dir.iterdir():
        //         if m := STORED_DATA_FILE_REGEX.match(f.name):
        //             release_id = m[1]
        //     assert release_id == "ilovecarly"  # Hardcoded ID for testing.

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_releases_preserves_track_ids_across_rebuilds() {
        // Python source:
        // def test_update_cache_releases_preserves_track_ids_across_rebuilds(config: Config) -> None:
        //     """Test that track IDs are preserved across cache rebuilds."""
        //     release_dir = config.music_source_dir / TEST_RELEASE_3.name
        //     shutil.copytree(TEST_RELEASE_3, release_dir)
        //     update_cache_for_releases(config, [release_dir])
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT id FROM tracks")
        //         first_track_ids = {r["id"] for r in cursor}
        //
        //     # Nuke the database.
        //     config.cache_database_path.unlink()
        //     maybe_invalidate_cache_database(config)
        //
        //     # Repeat cache population.
        //     update_cache_for_releases(config, [release_dir])
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT id FROM tracks")
        //         second_track_ids = {r["id"] for r in cursor}
        //
        //     # Assert IDs are equivalent.
        //     assert first_track_ids == second_track_ids

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_releases_writes_ids_to_tags() {
        // Python source:
        // def test_update_cache_releases_writes_ids_to_tags(config: Config) -> None:
        //     """Test that track IDs and release IDs are written to files."""
        //     release_dir = config.music_source_dir / TEST_RELEASE_3.name
        //     shutil.copytree(TEST_RELEASE_3, release_dir)
        //
        //     af = AudioTags.from_file(release_dir / "01.m4a")
        //     assert af.id is None
        //     assert af.release_id is None
        //     af = AudioTags.from_file(release_dir / "02.m4a")
        //     assert af.id is None
        //     assert af.release_id is None
        //
        //     update_cache_for_releases(config, [release_dir])
        //
        //     af = AudioTags.from_file(release_dir / "01.m4a")
        //     assert af.id is not None
        //     assert af.release_id is not None
        //     af = AudioTags.from_file(release_dir / "02.m4a")
        //     assert af.id is not None
        //     assert af.release_id is not None

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_releases_already_fully_cached() {
        // Python source:
        // def test_update_cache_releases_already_fully_cached(config: Config) -> None:
        //     """Test that a fully cached release No Ops when updated again."""
        //     release_dir = config.music_source_dir / TEST_RELEASE_1.name
        //     shutil.copytree(TEST_RELEASE_1, release_dir)
        //     update_cache_for_releases(config, [release_dir])
        //     update_cache_for_releases(config, [release_dir])
        //
        //     # Assert that the release metadata was read correctly.
        //     with connect(config) as conn:
        //         cursor = conn.execute(
        //             "SELECT id, source_path, title, releasetype, releasedate, new FROM releases",
        //         )
        //         row = cursor.fetchone()
        //         assert row["source_path"] == str(release_dir)
        //         assert row["title"] == "I Love Blackpink"
        //         assert row["releasetype"] == "album"
        //         assert row["releasedate"] == "1990-02-05"
        //         assert row["new"]

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_releases_to_empty_multi_value_tag() {
        // Python source:
        // def test_update_cache_releases_to_empty_multi_value_tag(config: Config) -> None:
        //     """Test that 1:many relations are properly emptied when they are updated from something to nothing."""
        //     release_dir = config.music_source_dir / TEST_RELEASE_1.name
        //     shutil.copytree(TEST_RELEASE_1, release_dir)
        //
        //     update_cache(config)
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT EXISTS(SELECT * FROM releases_labels)")
        //         assert cursor.fetchone()[0]
        //
        //     for fn in ["01.m4a", "02.m4a"]:
        //         af = AudioTags.from_file(release_dir / fn)
        //         af.label = []
        //         af.flush(config)
        //
        //     update_cache(config)
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT EXISTS(SELECT * FROM releases_labels)")
        //         assert not cursor.fetchone()[0]

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_releases_disk_update_to_previously_cached() {
        // Python source:
        // def test_update_cache_releases_disk_update_to_previously_cached(config: Config) -> None:
        //     """Test that a cached release is updated after a track updates."""
        //     release_dir = config.music_source_dir / TEST_RELEASE_1.name
        //     shutil.copytree(TEST_RELEASE_1, release_dir)
        //     update_cache_for_releases(config, [release_dir])
        //     # I'm too lazy to mutagen update the files, so instead we're going to update the database. And
        //     # then touch a file to signify that "we modified it."
        //     with connect(config) as conn:
        //         conn.execute("UPDATE releases SET title = 'An Uncool Album'")
        //         (release_dir / "01.m4a").touch()
        //     update_cache_for_releases(config, [release_dir])
        //
        //     # Assert that the release metadata was re-read and updated correctly.
        //     with connect(config) as conn:
        //         cursor = conn.execute(
        //             "SELECT id, source_path, title, releasetype, releasedate, new FROM releases",
        //         )
        //         row = cursor.fetchone()
        //         assert row["source_path"] == str(release_dir)
        //         assert row["title"] == "I Love Blackpink"
        //         assert row["releasetype"] == "album"
        //         assert row["releasedate"] == "1990-02-05"
        //         assert row["new"]

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_releases_disk_update_to_datafile() {
        // Python source:
        // def test_update_cache_releases_disk_update_to_datafile(config: Config) -> None:
        //     """Test that a cached release is updated after a datafile updates."""
        //     release_dir = config.music_source_dir / TEST_RELEASE_1.name
        //     shutil.copytree(TEST_RELEASE_1, release_dir)
        //     update_cache_for_releases(config, [release_dir])
        //     with connect(config) as conn:
        //         conn.execute("UPDATE releases SET datafile_mtime = '0' AND new = false")
        //     update_cache_for_releases(config, [release_dir])
        //
        //     # Assert that the release metadata was re-read and updated correctly.
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT new, added_at FROM releases")
        //         row = cursor.fetchone()
        //         assert row["new"]
        //         assert row["added_at"]

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_releases_disk_upgrade_old_datafile() {
        // Python source:
        // def test_update_cache_releases_disk_upgrade_old_datafile(config: Config) -> None:
        //     """Test that a legacy invalid datafile is upgraded on index."""
        //     release_dir = config.music_source_dir / TEST_RELEASE_1.name
        //     shutil.copytree(TEST_RELEASE_1, release_dir)
        //     datafile = release_dir / ".rose.lalala.toml"
        //     datafile.touch()
        //     update_cache_for_releases(config, [release_dir])
        //
        //     # Assert that the release metadata was re-read and updated correctly.
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT id, new, added_at FROM releases")
        //         row = cursor.fetchone()
        //         assert row["id"] == "lalala"
        //         assert row["new"]
        //         assert row["added_at"]
        //     with datafile.open("r") as fp:
        //         data = fp.read()
        //         assert "new = true" in data
        //         assert "added_at = " in data

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_releases_source_path_renamed() {
        // Python source:
        // def test_update_cache_releases_source_path_renamed(config: Config) -> None:
        //     """Test that a cached release is updated after a directory rename."""
        //     release_dir = config.music_source_dir / TEST_RELEASE_1.name
        //     shutil.copytree(TEST_RELEASE_1, release_dir)
        //     update_cache_for_releases(config, [release_dir])
        //     moved_release_dir = config.music_source_dir / "moved lol"
        //     release_dir.rename(moved_release_dir)
        //     update_cache_for_releases(config, [moved_release_dir])
        //
        //     # Assert that the release metadata was re-read and updated correctly.
        //     with connect(config) as conn:
        //         cursor = conn.execute(
        //             "SELECT id, source_path, title, releasetype, releasedate, new FROM releases",
        //         )
        //         row = cursor.fetchone()
        //         assert row["source_path"] == str(moved_release_dir)
        //         assert row["title"] == "I Love Blackpink"
        //         assert row["releasetype"] == "album"
        //         assert row["releasedate"] == "1990-02-05"
        //         assert row["new"]

        // TODO: Implement test
    }

    #[test]
    fn test_update_cache_releases_delete_nonexistent() {
        let (config, _temp_dir) = testing::config();
        
        // Initialize database
        maybe_invalidate_cache_database(&config).unwrap();
        
        // Insert a release with nonexistent path
        let nonexistent_path = config.music_source_dir.join("nonexistent");
        let conn = connect(&config).unwrap();
        conn.execute(
            "INSERT INTO releases (id, source_path, added_at, datafile_mtime, title, releasetype, disctotal, new, metahash)
             VALUES ('aaaaaa', ?1, '0000-01-01T00:00:00+00:00', '999', 'aa', 'unknown', 1, 0, '0')",
            [nonexistent_path.to_str().unwrap()],
        ).unwrap();
        drop(conn);
        
        // Run eviction
        update_cache_evict_nonexistent_releases(&config).unwrap();
        
        // Check that the release was deleted
        let conn = connect(&config).unwrap();
        let count: i32 = conn.query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_releases_enforces_max_len() {
        // Python source:
        // def test_update_cache_releases_enforces_max_len(config: Config) -> None:
        //     """Test that an directory with no audio files is skipped."""
        //     config = dataclasses.replace(config, rename_source_files=True, max_filename_bytes=15)
        //     shutil.copytree(TEST_RELEASE_1, config.music_source_dir / "a")
        //     shutil.copytree(TEST_RELEASE_1, config.music_source_dir / "b")
        //     shutil.copy(TEST_RELEASE_1 / "01.m4a", config.music_source_dir / "b" / "03.m4a")
        //     update_cache_for_releases(config)
        //     assert set(config.music_source_dir.iterdir()) == {
        //         config.music_source_dir / "BLACKPINK - 199",
        //         config.music_source_dir / "BLACKPINK - [2]",
        //     }
        //     # Nondeterministic: Pick the one with the extra file.
        //     children_1 = set((config.music_source_dir / "BLACKPINK - 199").iterdir())
        //     children_2 = set((config.music_source_dir / "BLACKPINK - [2]").iterdir())
        //     files = children_1 if len(children_1) > len(children_2) else children_2
        //     release_dir = next(iter(files)).parent
        //     assert release_dir / "01. Track 1.m4a" in files
        //     assert release_dir / "01. Tra [2].m4a" in files

        // TODO: Implement test
    }

    #[test]
    fn test_update_cache_releases_skips_empty_directory() {
        let (config, _temp_dir) = testing::config();
        
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
    #[ignore = "Not yet implemented"]
    fn test_update_cache_releases_uncaches_empty_directory() {
        // Python source:
        // def test_update_cache_releases_uncaches_empty_directory(config: Config) -> None:
        //     """Test that a previously-cached directory with no audio files now is cleared from cache."""
        //     release_dir = config.music_source_dir / TEST_RELEASE_1.name
        //     shutil.copytree(TEST_RELEASE_1, release_dir)
        //     update_cache_for_releases(config, [release_dir])
        //     shutil.rmtree(release_dir)
        //     release_dir.mkdir()
        //     update_cache_for_releases(config, [release_dir])
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT COUNT(*) FROM releases")
        //         assert cursor.fetchone()[0] == 0

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_releases_evicts_relations() {
        // Python source:
        // def test_update_cache_releases_evicts_relations(config: Config) -> None:
        //     """
        //     Test that related entities (artist, genre, label) that have been removed from the tags are
        //     properly evicted from the cache on update.
        //     """
        //     release_dir = config.music_source_dir / TEST_RELEASE_2.name
        //     shutil.copytree(TEST_RELEASE_2, release_dir)
        //     # Initial cache population.
        //     update_cache_for_releases(config, [release_dir])
        //     # Pretend that we have more artists in the cache.
        //     with connect(config) as conn:
        //         conn.execute(
        //             """
        //             INSERT INTO releases_genres (release_id, genre, position)
        //             VALUES ('ilovecarly', 'lalala', 2)
        //             """,
        //         )
        //         conn.execute(
        //             """
        //             INSERT INTO releases_labels (release_id, label, position)
        //             VALUES ('ilovecarly', 'lalala', 1)
        //             """,
        //         )
        //         conn.execute(
        //             """
        //             INSERT INTO releases_artists (release_id, artist, role, position)
        //             VALUES ('ilovecarly', 'lalala', 'main', 1)
        //             """,
        //         )
        //         conn.execute(
        //             """
        //             INSERT INTO tracks_artists (track_id, artist, role, position)
        //             SELECT id, 'lalala', 'main', 1 FROM tracks
        //             """,
        //         )
        //     # Second cache refresh.
        //     update_cache_for_releases(config, [release_dir], force=True)
        //     # Assert that all of the above were evicted.
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT EXISTS (SELECT * FROM releases_genres WHERE genre = 'lalala')")
        //         assert not cursor.fetchone()[0]
        //         cursor = conn.execute("SELECT EXISTS (SELECT * FROM releases_labels WHERE label = 'lalala')")
        //         assert not cursor.fetchone()[0]
        //         cursor = conn.execute("SELECT EXISTS (SELECT * FROM releases_artists WHERE artist = 'lalala')")
        //         assert not cursor.fetchone()[0]
        //         cursor = conn.execute("SELECT EXISTS (SELECT * FROM tracks_artists WHERE artist = 'lalala')")
        //         assert not cursor.fetchone()[0]

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_releases_ignores_directories() {
        // Python source:
        // def test_update_cache_releases_ignores_directories(config: Config) -> None:
        //     """Test that the ignore_release_directories configuration value works."""
        //     config = dataclasses.replace(config, ignore_release_directories=["lalala"])
        //     release_dir = config.music_source_dir / "lalala"
        //     shutil.copytree(TEST_RELEASE_1, release_dir)
        //
        //     # Test that both arg+no-arg ignore the directory.
        //     update_cache_for_releases(config)
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT COUNT(*) FROM releases")
        //         assert cursor.fetchone()[0] == 0
        //
        //     update_cache_for_releases(config)
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT COUNT(*) FROM releases")
        //         assert cursor.fetchone()[0] == 0

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_releases_notices_deleted_track() {
        // Python source:
        // def test_update_cache_releases_notices_deleted_track(config: Config) -> None:
        //     """Test that we notice when a track is deleted."""
        //     release_dir = config.music_source_dir / TEST_RELEASE_1.name
        //     shutil.copytree(TEST_RELEASE_1, release_dir)
        //     update_cache(config)
        //
        //     (release_dir / "02.m4a").unlink()
        //     update_cache(config)
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT COUNT(*) FROM tracks")
        //         assert cursor.fetchone()[0] == 1

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_releases_ignores_partially_written_directory() {
        // Python source:
        // def test_update_cache_releases_ignores_partially_written_directory(config: Config) -> None:
        //     """Test that a partially-written cached release is ignored."""
        //     # 1. Write the directory and index it. This should give it IDs and shit.
        //     release_dir = config.music_source_dir / TEST_RELEASE_1.name
        //     shutil.copytree(TEST_RELEASE_1, release_dir)
        //     update_cache(config)
        //
        //     # 2. Move the directory and "remove" the ID file.
        //     renamed_release_dir = config.music_source_dir / "lalala"
        //     release_dir.rename(renamed_release_dir)
        //     datafile = next(f for f in renamed_release_dir.iterdir() if f.stem.startswith(".rose"))
        //     tmpfile = datafile.with_name("tmp")
        //     datafile.rename(tmpfile)
        //
        //     # 3. Re-update cache. We should see an empty cache now.
        //     update_cache(config)
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT COUNT(*) FROM releases")
        //         assert cursor.fetchone()[0] == 0
        //
        //     # 4. Put the datafile back. We should now see the release cache again properly.
        //     datafile.with_name("tmp").rename(datafile)
        //     update_cache(config)
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT COUNT(*) FROM releases")
        //         assert cursor.fetchone()[0] == 1
        //
        //     # 5. Rename and remove the ID file again. We should see an empty cache again.
        //     release_dir = renamed_release_dir
        //     renamed_release_dir = config.music_source_dir / "bahaha"
        //     release_dir.rename(renamed_release_dir)
        //     next(f for f in renamed_release_dir.iterdir() if f.stem.startswith(".rose")).unlink()
        //     update_cache(config)
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT COUNT(*) FROM releases")
        //         assert cursor.fetchone()[0] == 0
        //
        //     # 6. Run with force=True. This should index the directory and make a new .rose.toml file.
        //     update_cache(config, force=True)
        //     assert (renamed_release_dir / datafile.name).is_file()
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT COUNT(*) FROM releases")
        //         assert cursor.fetchone()[0] == 1

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_rename_source_files() {
        // Python source:
        // def test_update_cache_rename_source_files(config: Config) -> None:
        //     """Test that we properly rename the source directory on cache update."""
        //     config = dataclasses.replace(config, rename_source_files=True)
        //     shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
        //     (config.music_source_dir / TEST_RELEASE_1.name / "cover.jpg").touch()
        //     update_cache(config)
        //
        //     expected_dir = config.music_source_dir / "BLACKPINK - 1990. I Love Blackpink [NEW]"
        //     assert expected_dir in list(config.music_source_dir.iterdir())
        //
        //     files_in_dir = list(expected_dir.iterdir())
        //     assert expected_dir / "01. Track 1.m4a" in files_in_dir
        //     assert expected_dir / "02. Track 2.m4a" in files_in_dir
        //
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT source_path, cover_image_path FROM releases")
        //         row = cursor.fetchone()
        //         assert Path(row["source_path"]) == expected_dir
        //         assert Path(row["cover_image_path"]) == expected_dir / "cover.jpg"
        //         cursor = conn.execute("SELECT source_path FROM tracks")
        //         assert {Path(r[0]) for r in cursor} == {
        //             expected_dir / "01. Track 1.m4a",
        //             expected_dir / "02. Track 2.m4a",
        //         }

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_add_cover_art() {
        // Python source:
        // def test_update_cache_add_cover_art(config: Config) -> None:
        //     """
        //     Test that adding a cover art (i.e. modifying release w/out modifying tracks) does not affect
        //     the tracks.
        //     """
        //     config = dataclasses.replace(config, rename_source_files=True)
        //     shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
        //     update_cache(config)
        //     expected_dir = config.music_source_dir / "BLACKPINK - 1990. I Love Blackpink [NEW]"
        //
        //     (expected_dir / "cover.jpg").touch()
        //     update_cache(config)
        //
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT source_path, cover_image_path FROM releases")
        //         row = cursor.fetchone()
        //         assert Path(row["source_path"]) == expected_dir
        //         assert Path(row["cover_image_path"]) == expected_dir / "cover.jpg"
        //         cursor = conn.execute("SELECT source_path FROM tracks")
        //         assert {Path(r[0]) for r in cursor} == {
        //             expected_dir / "01. Track 1.m4a",
        //             expected_dir / "02. Track 2.m4a",
        //         }

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_rename_source_files_nested_file_directories() {
        // Python source:
        // def test_update_cache_rename_source_files_nested_file_directories(config: Config) -> None:
        //     """Test that we properly rename arbitrarily nested files and clean up the empty dirs."""
        //     config = dataclasses.replace(config, rename_source_files=True)
        //     shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
        //     (config.music_source_dir / TEST_RELEASE_1.name / "lala").mkdir()
        //     (config.music_source_dir / TEST_RELEASE_1.name / "01.m4a").rename(
        //         config.music_source_dir / TEST_RELEASE_1.name / "lala" / "1.m4a"
        //     )
        //     update_cache(config)
        //
        //     expected_dir = config.music_source_dir / "BLACKPINK - 1990. I Love Blackpink [NEW]"
        //     assert expected_dir in list(config.music_source_dir.iterdir())
        //
        //     files_in_dir = list(expected_dir.iterdir())
        //     assert expected_dir / "01. Track 1.m4a" in files_in_dir
        //     assert expected_dir / "02. Track 2.m4a" in files_in_dir
        //     assert expected_dir / "lala" not in files_in_dir
        //
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT source_path FROM releases")
        //         assert Path(cursor.fetchone()[0]) == expected_dir
        //         cursor = conn.execute("SELECT source_path FROM tracks")
        //         assert {Path(r[0]) for r in cursor} == {
        //             expected_dir / "01. Track 1.m4a",
        //             expected_dir / "02. Track 2.m4a",
        //         }

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_rename_source_files_collisions() {
        // Python source:
        // def test_update_cache_rename_source_files_collisions(config: Config) -> None:
        //     """Test that we properly rename arbitrarily nested files and clean up the empty dirs."""
        //     config = dataclasses.replace(config, rename_source_files=True)
        //     # Three copies of the same directory, and two instances of Track 1.
        //     shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
        //     shutil.copyfile(
        //         config.music_source_dir / TEST_RELEASE_1.name / "01.m4a",
        //         config.music_source_dir / TEST_RELEASE_1.name / "haha.m4a",
        //     )
        //     shutil.copytree(config.music_source_dir / TEST_RELEASE_1.name, config.music_source_dir / "Number 2")
        //     shutil.copytree(config.music_source_dir / TEST_RELEASE_1.name, config.music_source_dir / "Number 3")
        //     update_cache(config)
        //
        //     release_dirs = list(config.music_source_dir.iterdir())
        //     for expected_dir in [
        //         config.music_source_dir / "BLACKPINK - 1990. I Love Blackpink [NEW]",
        //         config.music_source_dir / "BLACKPINK - 1990. I Love Blackpink [NEW] [2]",
        //         config.music_source_dir / "BLACKPINK - 1990. I Love Blackpink [NEW] [3]",
        //     ]:
        //         assert expected_dir in release_dirs
        //
        //         files_in_dir = list(expected_dir.iterdir())
        //         assert expected_dir / "01. Track 1.m4a" in files_in_dir
        //         assert expected_dir / "01. Track 1 [2].m4a" in files_in_dir
        //         assert expected_dir / "02. Track 2.m4a" in files_in_dir
        //
        //         with connect(config) as conn:
        //             cursor = conn.execute("SELECT id FROM releases WHERE source_path = ?", (str(expected_dir),))
        //             release_id = cursor.fetchone()[0]
        //             assert release_id
        //             cursor = conn.execute("SELECT source_path FROM tracks WHERE release_id = ?", (release_id,))
        //             assert {Path(r[0]) for r in cursor} == {
        //                 expected_dir / "01. Track 1.m4a",
        //                 expected_dir / "01. Track 1 [2].m4a",
        //                 expected_dir / "02. Track 2.m4a",
        //             }

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_releases_updates_full_text_search() {
        // Python source:
        // def test_update_cache_releases_updates_full_text_search(config: Config) -> None:
        //     release_dir = config.music_source_dir / TEST_RELEASE_1.name
        //     shutil.copytree(TEST_RELEASE_1, release_dir)
        //
        //     update_cache_for_releases(config, [release_dir])
        //     with connect(config) as conn:
        //         cursor = conn.execute(
        //             """
        //             SELECT rowid, * FROM rules_engine_fts
        //             """
        //         )
        //         cursor = conn.execute(
        //             """
        //             SELECT rowid, * FROM tracks
        //             """
        //         )
        //     with connect(config) as conn:
        //         cursor = conn.execute(
        //             """
        //             SELECT t.source_path
        //             FROM rules_engine_fts s
        //             JOIN tracks t ON t.rowid = s.rowid
        //             WHERE s.tracktitle MATCH 'r a c k'
        //             """
        //         )
        //         fnames = {Path(r["source_path"]) for r in cursor}
        //         assert fnames == {
        //             release_dir / "01.m4a",
        //             release_dir / "02.m4a",
        //         }
        //
        //     # And then test the DELETE+INSERT behavior. And that the query still works.
        //     update_cache_for_releases(config, [release_dir], force=True)
        //     with connect(config) as conn:
        //         cursor = conn.execute(
        //             """
        //             SELECT t.source_path
        //             FROM rules_engine_fts s
        //             JOIN tracks t ON t.rowid = s.rowid
        //             WHERE s.tracktitle MATCH 'r a c k'
        //             """
        //         )
        //         fnames = {Path(r["source_path"]) for r in cursor}
        //         assert fnames == {
        //             release_dir / "01.m4a",
        //             release_dir / "02.m4a",
        //         }

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_releases_new_directory_same_path() {
        // Python source:
        // def test_update_cache_releases_new_directory_same_path(config: Config) -> None:
        //     """If a previous release is replaced by a new release with the same path, avoid a source_path unique conflict."""
        //     release_dir = config.music_source_dir / TEST_RELEASE_1.name
        //     shutil.copytree(TEST_RELEASE_1, release_dir)
        //     update_cache(config)
        //     shutil.rmtree(release_dir)
        //     shutil.copytree(TEST_RELEASE_2, release_dir)
        //     # Should not error.
        //     update_cache(config)

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_collages() {
        // Python source:
        // def test_update_cache_collages(config: Config) -> None:
        //     shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
        //     shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
        //     update_cache(config)
        //
        //     # Assert that the collage metadata was read correctly.
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT name, source_mtime FROM collages")
        //         rows = cursor.fetchall()
        //         assert len(rows) == 1
        //         row = rows[0]
        //         assert row["name"] == "Rose Gold"
        //         assert row["source_mtime"]
        //
        //         cursor = conn.execute("SELECT collage_name, release_id, position FROM collages_releases WHERE NOT missing")
        //         rows = cursor.fetchall()
        //         assert len(rows) == 1
        //         row = rows[0]
        //         assert row["collage_name"] == "Rose Gold"
        //         assert row["release_id"] == "ilovecarly"
        //         assert row["position"] == 1

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_collages_missing_release_id() {
        // Python source:
        // def test_update_cache_collages_missing_release_id(config: Config) -> None:
        //     shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
        //     update_cache(config)
        //
        //     # Assert that the releases in the collage were read as missing.
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT COUNT(*) FROM collages_releases WHERE missing")
        //         assert cursor.fetchone()[0] == 2
        //     # Assert that source file was updated to set the releases missing.
        //     with (config.music_source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
        //         data = tomllib.load(fp)
        //     assert len(data["releases"]) == 2
        //     assert len([r for r in data["releases"] if r["missing"]]) == 2
        //
        //     shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
        //     shutil.copytree(TEST_RELEASE_3, config.music_source_dir / TEST_RELEASE_3.name)
        //     update_cache(config)
        //
        //     # Assert that the releases in the collage were unflagged as missing.
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT COUNT(*) FROM collages_releases WHERE NOT missing")
        //         assert cursor.fetchone()[0] == 2
        //     # Assert that source file was updated to remove the missing flag.
        //     with (config.music_source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
        //         data = tomllib.load(fp)
        //     assert len([r for r in data["releases"] if "missing" not in r]) == 2

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_collages_missing_release_id_multiprocessing() {
        // Python source:
        // def test_update_cache_collages_missing_release_id_multiprocessing(config: Config) -> None:
        //     shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
        //     update_cache(config)
        //
        //     # Assert that the releases in the collage were read as missing.
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT COUNT(*) FROM collages_releases WHERE missing")
        //         assert cursor.fetchone()[0] == 2
        //     # Assert that source file was updated to set the releases missing.
        //     with (config.music_source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
        //         data = tomllib.load(fp)
        //     assert len(data["releases"]) == 2
        //     assert len([r for r in data["releases"] if r["missing"]]) == 2
        //
        //     shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
        //     shutil.copytree(TEST_RELEASE_3, config.music_source_dir / TEST_RELEASE_3.name)
        //     update_cache(config, force_multiprocessing=True)
        //
        //     # Assert that the releases in the collage were unflagged as missing.
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT COUNT(*) FROM collages_releases WHERE NOT missing")
        //         assert cursor.fetchone()[0] == 2
        //     # Assert that source file was updated to remove the missing flag.
        //     with (config.music_source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
        //         data = tomllib.load(fp)
        //     assert len([r for r in data["releases"] if "missing" not in r]) == 2

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_collages_on_release_rename() {
        // Python source:
        // def test_update_cache_collages_on_release_rename(config: Config) -> None:
        //     """
        //     Test that a renamed release source directory does not remove the release from any collages. This
        //     can occur because the rename operation is executed in SQL as release deletion followed by
        //     release creation.
        //     """
        //     shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
        //     shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
        //     shutil.copytree(TEST_RELEASE_3, config.music_source_dir / TEST_RELEASE_3.name)
        //     update_cache(config)
        //
        //     (config.music_source_dir / TEST_RELEASE_2.name).rename(config.music_source_dir / "lalala")
        //     update_cache(config)
        //
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT collage_name, release_id, position FROM collages_releases")
        //         rows = [dict(r) for r in cursor]
        //         assert rows == [
        //             {"collage_name": "Rose Gold", "release_id": "ilovecarly", "position": 1},
        //             {"collage_name": "Rose Gold", "release_id": "ilovenewjeans", "position": 2},
        //         ]
        //
        //     # Assert that source file was not updated to remove the release.
        //     with (config.music_source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
        //         data = tomllib.load(fp)
        //     assert not [r for r in data["releases"] if "missing" in r]
        //     assert len(data["releases"]) == 2

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_playlists() {
        // Python source:
        // def test_update_cache_playlists(config: Config) -> None:
        //     shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
        //     shutil.copytree(TEST_PLAYLIST_1, config.music_source_dir / "!playlists")
        //     update_cache(config)
        //
        //     # Assert that the playlist metadata was read correctly.
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT name, source_mtime, cover_path FROM playlists")
        //         rows = cursor.fetchall()
        //         assert len(rows) == 1
        //         row = rows[0]
        //         assert row["name"] == "Lala Lisa"
        //         assert row["source_mtime"] is not None
        //         assert row["cover_path"] == str(config.music_source_dir / "!playlists" / "Lala Lisa.jpg")
        //
        //         cursor = conn.execute("SELECT playlist_name, track_id, position FROM playlists_tracks ORDER BY position")
        //         assert [dict(r) for r in cursor] == [
        //             {"playlist_name": "Lala Lisa", "track_id": "iloveloona", "position": 1},
        //             {"playlist_name": "Lala Lisa", "track_id": "ilovetwice", "position": 2},
        //         ]

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_playlists_missing_track_id() {
        // Python source:
        // def test_update_cache_playlists_missing_track_id(config: Config) -> None:
        //     shutil.copytree(TEST_PLAYLIST_1, config.music_source_dir / "!playlists")
        //     update_cache(config)
        //
        //     # Assert that the tracks in the playlist were read as missing.
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT COUNT(*) FROM playlists_tracks WHERE missing")
        //         assert cursor.fetchone()[0] == 2
        //     # Assert that source file was updated to set the tracks missing.
        //     with (config.music_source_dir / "!playlists" / "Lala Lisa.toml").open("rb") as fp:
        //         data = tomllib.load(fp)
        //     assert len(data["tracks"]) == 2
        //     assert len([r for r in data["tracks"] if r["missing"]]) == 2
        //
        //     shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
        //     update_cache(config)
        //
        //     # Assert that the tracks in the playlist were unflagged as missing.
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT COUNT(*) FROM playlists_tracks WHERE NOT missing")
        //         assert cursor.fetchone()[0] == 2
        //     # Assert that source file was updated to remove the missing flag.
        //     with (config.music_source_dir / "!playlists" / "Lala Lisa.toml").open("rb") as fp:
        //         data = tomllib.load(fp)
        //     assert len([r for r in data["tracks"] if "missing" not in r]) == 2

        // TODO: Implement test
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_releases_updates_collages_description_meta() {
        // Python source:
        // @pytest.mark.parametrize("multiprocessing", [True, False])
        // def test_update_releases_updates_collages_description_meta(config: Config, multiprocessing: bool) -> None:
        //     shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
        //     shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
        //     shutil.copytree(TEST_RELEASE_3, config.music_source_dir / TEST_RELEASE_3.name)
        //     shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
        //     cpath = config.music_source_dir / "!collages" / "Rose Gold.toml"
        //
        //     # First cache update: releases are inserted, collage is new. This should update the collage
        //     # TOML.
        //     update_cache(config)
        //     with cpath.open("r") as fp:
        //         cfg = fp.read()
        //         assert (
        //             cfg
        //             == """\
        // releases = [
        //     { uuid = "ilovecarly", description_meta = "[1990-02-05] Carly Rae Jepsen - I Love Carly" },
        //     { uuid = "ilovenewjeans", description_meta = "[1990-02-05] NewJeans - I Love NewJeans" },
        // ]
        // """
        //         )
        //
        //     # Now prep for the second update. Reset the TOML to have garbage again, and update the database
        //     # such that the virtual dirnames are also incorrect.
        //     with cpath.open("w") as fp:
        //         fp.write(
        //             """\
        // [[releases]]
        // uuid = "ilovecarly"
        // description_meta = "lalala"
        // [[releases]]
        // uuid = "ilovenewjeans"
        // description_meta = "hahaha"
        // """
        //         )
        //
        //     # Second cache update: releases exist, collages exist, release is "updated." This should also
        //     # trigger a metadata update.
        //     update_cache_for_releases(config, force=True, force_multiprocessing=multiprocessing)
        //     with cpath.open("r") as fp:
        //         cfg = fp.read()
        //         assert (
        //             cfg
        //             == """\
        // releases = [
        //     { uuid = "ilovecarly", description_meta = "[1990-02-05] Carly Rae Jepsen - I Love Carly" },
        //     { uuid = "ilovenewjeans", description_meta = "[1990-02-05] NewJeans - I Love NewJeans" },
        // ]
        // """
        //         )

        // TODO: Implement test (with both multiprocessing=true and false)
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_tracks_updates_playlists_description_meta() {
        // Python source:
        // @pytest.mark.parametrize("multiprocessing", [True, False])
        // def test_update_tracks_updates_playlists_description_meta(config: Config, multiprocessing: bool) -> None:
        //     shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
        //     shutil.copytree(TEST_PLAYLIST_1, config.music_source_dir / "!playlists")
        //     ppath = config.music_source_dir / "!playlists" / "Lala Lisa.toml"
        //
        //     # First cache update: tracks are inserted, playlist is new. This should update the playlist
        //     # TOML.
        //     update_cache(config)
        //     with ppath.open("r") as fp:
        //         cfg = fp.read()
        //         assert (
        //             cfg
        //             == """\
        // tracks = [
        //     { uuid = "iloveloona", description_meta = "[1990-02-05] Carly Rae Jepsen - Track 1" },
        //     { uuid = "ilovetwice", description_meta = "[1990-02-05] Carly Rae Jepsen - Track 2" },
        // ]
        // """
        //         )
        //
        //     # Now prep for the second update. Reset the TOML to have garbage again, and update the database
        //     # such that the virtual filenames are also incorrect.
        //     with ppath.open("w") as fp:
        //         fp.write(
        //             """\
        // [[tracks]]
        // uuid = "iloveloona"
        // description_meta = "lalala"
        // [[tracks]]
        // uuid = "ilovetwice"
        // description_meta = "hahaha"
        // """
        //         )
        //
        //     # Second cache update: tracks exist, playlists exist, track is "updated." This should also
        //     # trigger a metadata update.
        //     update_cache_for_releases(config, force=True, force_multiprocessing=multiprocessing)
        //     with ppath.open("r") as fp:
        //         cfg = fp.read()
        //         assert (
        //             cfg
        //             == """\
        // tracks = [
        //     { uuid = "iloveloona", description_meta = "[1990-02-05] Carly Rae Jepsen - Track 1" },
        //     { uuid = "ilovetwice", description_meta = "[1990-02-05] Carly Rae Jepsen - Track 2" },
        // ]
        // """
        //         )

        // TODO: Implement test (with both multiprocessing=true and false)
    }

    #[test]
    #[ignore = "Not yet implemented"]
    fn test_update_cache_playlists_on_release_rename() {
        // Python source:
        // def test_update_cache_playlists_on_release_rename(config: Config) -> None:
        //     """
        //     Test that a renamed release source directory does not remove any of its tracks any playlists.
        //     This can occur because when a release is renamed, we remove all tracks from the database and
        //     then reinsert them.
        //     """
        //     shutil.copytree(TEST_PLAYLIST_1, config.music_source_dir / "!playlists")
        //     shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
        //     update_cache(config)
        //
        //     (config.music_source_dir / TEST_RELEASE_2.name).rename(config.music_source_dir / "lalala")
        //     update_cache(config)
        //
        //     with connect(config) as conn:
        //         cursor = conn.execute("SELECT playlist_name, track_id, position FROM playlists_tracks")
        //         rows = [dict(r) for r in cursor]
        //         assert rows == [
        //             {"playlist_name": "Lala Lisa", "track_id": "iloveloona", "position": 1},
        //             {"playlist_name": "Lala Lisa", "track_id": "ilovetwice", "position": 2},
        //         ]
        //
        //     # Assert that source file was not updated to remove the track.
        //     with (config.music_source_dir / "!playlists" / "Lala Lisa.toml").open("rb") as fp:
        //         data = tomllib.load(fp)
        //     assert not [t for t in data["tracks"] if "missing" in t]
        //     assert len(data["tracks"]) == 2

        // TODO: Implement test
    }

    #[test]
    fn test_list_releases() {
        let (config, _temp_dir) = testing::seeded_cache();

        let releases = list_releases(&config).unwrap();
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
        assert_eq!(release.new, false);
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
        assert_eq!(release.releaseartists.main[0].alias, false);
        assert_eq!(release.releaseartists.main[1].name, "Bass Man");
        assert_eq!(release.releaseartists.main[1].alias, false);
        assert_eq!(release.releaseartists.main[2].name, "Hype Boy");
        assert_eq!(release.releaseartists.main[2].alias, true);
        assert_eq!(release.releaseartists.main[3].name, "Bubble Gum");
        assert_eq!(release.releaseartists.main[3].alias, true);
        
        let tracks = get_tracks_of_release(&config, &release).unwrap();
        for track in tracks {
            assert_eq!(track.trackartists.main.len(), 4);
            assert_eq!(track.trackartists.main[0].name, "Techno Man");
            assert_eq!(track.trackartists.main[0].alias, false);
            assert_eq!(track.trackartists.main[1].name, "Bass Man");
            assert_eq!(track.trackartists.main[1].alias, false);
            assert_eq!(track.trackartists.main[2].name, "Hype Boy");
            assert_eq!(track.trackartists.main[2].alias, true);
            assert_eq!(track.trackartists.main[3].name, "Bubble Gum");
            assert_eq!(track.trackartists.main[3].alias, true);
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

        let tracks = list_tracks(&config).unwrap();
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
        let expected: HashSet<String> = vec![
            "Techno Man".to_string(),
            "Bass Man".to_string(),
            "Violin Woman".to_string(),
            "Conductor Woman".to_string(),
        ]
        .into_iter()
        .collect();

        assert_eq!(artist_set, expected);
    }

    #[test]
    fn test_list_genres() {
        let (config, _temp_dir) = testing::seeded_cache();

        // Test the accumulator too - add Classical Music to r3
        let conn = connect(&config).unwrap();
        conn.execute(
            "INSERT INTO releases_genres (release_id, genre, position) VALUES ('r3', 'Classical Music', 1)",
            [],
        )
        .unwrap();
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
