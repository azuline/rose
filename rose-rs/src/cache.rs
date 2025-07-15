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
    
    if dirs.is_empty() {
        return Ok(());
    }
    
    let conn = connect(c)?;
    
    // Build the query with proper number of placeholders
    let placeholders = dirs.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query = format!(
        "DELETE FROM releases WHERE source_path NOT IN ({}) RETURNING source_path",
        placeholders
    );
    
    let mut stmt = conn.prepare(&query)?;
    let mut rows = stmt.query(rusqlite::params_from_iter(&dirs))?;
    
    while let Some(row) = rows.next()? {
        let source_path: String = row.get(0)?;
        info!("evicted missing release {} from cache", source_path);
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
        let deleted_names: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        
        for name in deleted_names {
            info!("Evicted missing collage {} from cache", name);
        }
    } else {
        // Delete collages not in the list
        let placeholders = vec!["?"; collage_names.len()].join(",");
        let query = format!(
            "DELETE FROM collages WHERE name NOT IN ({}) RETURNING name",
            placeholders
        );
        
        let mut stmt = conn.prepare(&query)?;
        let deleted_names: Vec<String> = stmt
            .query_map(
                rusqlite::params_from_iter(&collage_names),
                |row| row.get(0)
            )?
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
        let deleted_names: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        
        for name in deleted_names {
            info!("Evicted missing playlist {} from cache", name);
        }
    } else {
        // Delete playlists not in the list
        let placeholders = vec!["?"; playlist_names.len()].join(",");
        let query = format!(
            "DELETE FROM playlists WHERE name NOT IN ({}) RETURNING name",
            placeholders
        );
        
        let mut stmt = conn.prepare(&query)?;
        let deleted_names: Vec<String> = stmt
            .query_map(
                rusqlite::params_from_iter(&playlist_names),
                |row| row.get(0)
            )?
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
    
    let release = stmt.query_row(params![release_id], |row| {
        cached_release_from_view(c, row, true).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
    }).optional()?;
    
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
    
    let releases = stmt.query_map([], |row| {
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
    
    let track_result: Option<String> = track_stmt
        .query_row([id], |row| row.get("release_id"))
        .optional()?;
    
    if let Some(release_id) = track_result {
        // Get the release
        let release = get_release(c, &release_id)?;
        if let Some(release) = release {
            // Now get the full track with the release
            let track = track_stmt.query_row([id], |row| {
                cached_track_from_view(c, row, Arc::new(release.clone()), true)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
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

pub fn list_tracks_with_filter(c: &Config, track_ids: Option<Vec<String>>) -> Result<Vec<Track>> {
    let conn = connect(c)?;
    
    // Build query
    let query = if let Some(ref ids) = track_ids {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; ids.len()].join(",");
        format!("SELECT * FROM tracks_view WHERE id IN ({}) ORDER BY source_path", placeholders)
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
            let rows = stmt.query_map(rusqlite::params_from_iter(ids), |row| {
                row.get::<_, String>("release_id")
            })?;
            for release_id in rows {
                release_ids.insert(release_id?);
            }
        } else {
            let rows = stmt.query_map([], |row| {
                row.get::<_, String>("release_id")
            })?;
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
    let collages = stmt.query_map([], |row| {
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
    let playlists = stmt.query_map([], |row| {
        Ok(Playlist {
            name: row.get(0)?,
            source_mtime: row.get(1)?,
            cover_path: row.get::<_, Option<String>>(2)?.map(PathBuf::from),
        })
    })?
    .collect::<std::result::Result<Vec<_>, _>>()?;
    
    Ok(playlists)
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
