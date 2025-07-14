// Python cache.py migrated to Rust
// Original docstring:
// """
// The cache module encapsulates the read cache and exposes handles for working with the read cache. It
// also exposes a locking mechanism that uses the read cache for synchronization.
//
// The SQLite database is considered part of the cache, and so this module encapsulates the SQLite
// database too. Though we cheap out a bit, so all the tests freely read from the SQLite database. No
// budget!
//
// The read cache is crucial to Rose. See `docs/CACHE_MAINTENANCE.md` for more information.
//
// We consider a few problems in the cache update, whose solutions contribute to
// the overall complexity of the cache update sequence:
//
// 1. **Arbitrary renames:** Files and directories can be arbitrarily renamed in between cache scans.
//    We solve for these renames by writing [Stable Identifiers](#release-track-identifiers) to disk.
//    For performance, however, a track update ends up as a delete followed by an insert with the
//    just-deleted ID.
// 2. **In-progress directory creation:** We may come across a directory while it is in the process of
//    being created. For example, due to `cp -r`. Unless --force is passed, we skip directories that
//    lack a `.rose.{uuid}.toml` file, yet have a `Release ID` written syncthing synchronization.
// 3. **Performance:** We want to minimize file accesses, so we cache heavily and batch operations
//    together. This creates a lot of intermediate state that we accumulate throughout the cache
//    update.
// """

use crate::audiotags::RoseDate;
use crate::common::{hash_dataclass, Artist, ArtistMapping};
use crate::config::Config;
use crate::error::{Result, RoseError, RoseExpectedError};
use crate::genre_hierarchy::get_transitive_parent_genres;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info};
use unicode_normalization::UnicodeNormalization;

// Constants
const CACHE_SCHEMA_PATH: &str = include_str!("cache.sql");
pub const SQL_ARRAY_DELIMITER: &str = " ¬ ";

// Python: @contextlib.contextmanager
// def connect(c: Config) -> Iterator[sqlite3.Connection]:
pub fn connect(config: &Config) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        config.cache_database_path(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
            | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
            | rusqlite::OpenFlags::SQLITE_OPEN_URI
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    // Python: conn = sqlite3.connect(
    //     c.cache_database_path,
    //     detect_types=sqlite3.PARSE_DECLTYPES,
    //     isolation_level=None,
    //     timeout=15.0,
    // )
    conn.busy_timeout(Duration::from_secs(15))?;

    // Python: conn.execute("PRAGMA foreign_keys=ON")
    conn.pragma_update(None, "foreign_keys", "ON")?;

    // Python: conn.execute("PRAGMA journal_mode=WAL")
    conn.pragma_update(None, "journal_mode", "WAL")?;

    Ok(conn)
}

// Python: def maybe_invalidate_cache_database(c: Config) -> None:
pub fn maybe_invalidate_cache_database(config: &Config) -> Result<()> {
    // """
    // "Migrate" the database. If the schema in the database does not match that on disk, then nuke the
    // database and recreate it from scratch. Otherwise, no op.
    //
    // We can do this because the database is just a read cache. It is not source-of-truth for any of
    // its own data.
    // """

    let db_path = config.cache_database_path();

    // Python: if not c.cache_database_path.exists():
    if !db_path.exists() {
        create_database(config)?;
        return Ok(());
    }

    // Python: with connect(c) as conn:
    let conn = connect(config)?;

    // Python: cursor = conn.execute(
    //     "SELECT EXISTS(SELECT * FROM sqlite_master WHERE type='table' AND name='_schema_hash')"
    // )
    // if not cursor.fetchone()[0]:
    let has_schema_table: bool = conn.query_row(
        "SELECT EXISTS(SELECT * FROM sqlite_master WHERE type='table' AND name='_schema_hash')",
        [],
        |row| row.get(0),
    )?;

    if !has_schema_table {
        drop(conn);
        fs::remove_file(&db_path)?;
        create_database(config)?;
        return Ok(());
    }

    // Python: schema_hash = hashlib.sha256(CACHE_SCHEMA_PATH.read_bytes()).hexdigest()
    let schema_hash = hash_dataclass(&CACHE_SCHEMA_PATH);

    // Python: config_hash = sha256_dataclass(c)[:16]
    let config_hash = hash_dataclass(&format!("{:?}", config))[..16].to_string();

    // Python: cursor = conn.execute(
    //     """
    //     SELECT EXISTS(
    //         SELECT * FROM _schema_hash
    //         WHERE schema_hash = ? AND config_hash = ? AND version = ?
    //     )
    //     """,
    //     (schema_hash, config_hash, VERSION),
    // )
    let matches: bool = conn.query_row(
        "SELECT EXISTS(
                SELECT * FROM _schema_hash
                WHERE schema_hash = ?1 AND config_hash = ?2 AND version = ?3
            )",
        params![schema_hash, config_hash, env!("CARGO_PKG_VERSION")],
        |row| row.get(0),
    )?;

    // Python: if not cursor.fetchone()[0]:
    if !matches {
        info!("Cache database schema/config changed, recreating database");
        drop(conn);
        fs::remove_file(&db_path)?;
        create_database(config)?;
    }

    Ok(())
}

// Helper function to create database
fn create_database(config: &Config) -> Result<()> {
    let conn = connect(config)?;

    // Create schema
    conn.execute_batch(CACHE_SCHEMA_PATH)?;

    // Create schema hash table
    conn.execute(
        "CREATE TABLE _schema_hash (
            schema_hash TEXT,
            config_hash TEXT,
            version TEXT,
            PRIMARY KEY (schema_hash, config_hash, version)
        )",
        [],
    )?;

    // Store current hashes
    let schema_hash = hash_dataclass(&CACHE_SCHEMA_PATH);
    let config_hash = hash_dataclass(&format!("{:?}", config))[..16].to_string();

    conn.execute(
        "INSERT INTO _schema_hash (schema_hash, config_hash, version) VALUES (?1, ?2, ?3)",
        params![schema_hash, config_hash, env!("CARGO_PKG_VERSION")],
    )?;

    // Python: conn.create_function("process_string_for_fts", 1, process_string_for_fts)
    conn.create_scalar_function(
        "process_string_for_fts",
        1,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8
            | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let s: String = ctx.get(0)?;
            Ok(process_string_for_fts(&s))
        },
    )?;

    Ok(())
}

// Python: def process_string_for_fts(x: str) -> str:
pub fn process_string_for_fts(x: &str) -> String {
    // """Transform strings into character tokens for the full text search index."""
    // Python: return "¬".join(x) if x else ""
    if x.is_empty() {
        String::new()
    } else {
        x.chars()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join("¬")
    }
}

// Python: @contextlib.contextmanager
// def lock(c: Config, name: str, timeout: float = 1.0) -> Iterator[None]:
pub fn lock(config: &Config, name: &str, timeout_secs: f64) -> Result<()> {
    let conn = connect(config)?;
    loop {
        // Python: cursor = conn.execute("SELECT MAX(valid_until) FROM locks WHERE name = ?", (name,))
        let valid_until: Option<f64> = conn
            .query_row(
                "SELECT MAX(valid_until) FROM locks WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .optional()?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();

        // Python: if row and row[0] and row[0] > time.time():
        if let Some(until) = valid_until {
            if until > now {
                let sleep_secs = (until - now).max(0.0);
                debug!(
                    "Failed to acquire lock for {}: sleeping for {}",
                    name, sleep_secs
                );
                std::thread::sleep(Duration::from_secs_f64(sleep_secs));
                continue;
            }
        }

        debug!(
            "Attempting to acquire lock for {} with timeout {}",
            name, timeout_secs
        );
        let new_valid_until = now + timeout_secs;

        // Python: try:
        //     conn.execute("INSERT INTO locks (name, valid_until) VALUES (?, ?)", (name, valid_until))
        match conn.execute(
            "INSERT INTO locks (name, valid_until) VALUES (?1, ?2)",
            params![name, new_valid_until],
        ) {
            Ok(_) => {
                debug!(
                    "Successfully acquired lock for {} with timeout {} until {}",
                    name, timeout_secs, new_valid_until
                );
                return Ok(());
            }
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                debug!("Failed to acquire lock for {}, trying again", name);
                continue;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

// Python: with connect(c) as conn:
//     conn.execute("DELETE FROM locks WHERE name = ?", (name,))
pub fn unlock(conn: &Connection, name: &str) -> Result<()> {
    debug!("Releasing lock {}", name);
    conn.execute("DELETE FROM locks WHERE name = ?1", params![name])?;
    Ok(())
}

// Python: def release_lock_name(release_id: str) -> str:
pub fn release_lock_name(release_id: &str) -> String {
    format!("release-{}", release_id)
}

// Python: def collage_lock_name(collage_name: str) -> str:
pub fn collage_lock_name(collage_name: &str) -> String {
    format!("collage-{}", collage_name)
}

// Python: def playlist_lock_name(playlist_name: str) -> str:
pub fn playlist_lock_name(playlist_name: &str) -> String {
    format!("playlist-{}", playlist_name)
}

// Python: @dataclasses.dataclass(slots=True)
// class Release:
#[derive(Debug, Clone)]
pub struct CachedRelease {
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

// Mutable Release struct for intermediate processing
#[derive(Debug, Clone)]
pub struct Release {
    pub id: String,
    pub source_path: PathBuf,
    pub cover_image_path: Option<PathBuf>,
    pub added_at: String,
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

// Python: def cached_release_from_view(c: Config, row: dict[str, Any], aliases: bool = True) -> Release:
pub fn cached_release_from_view(
    config: &Config,
    row: &rusqlite::Row,
    aliases: bool,
) -> Result<CachedRelease> {
    let secondary_genres =
        split_sql_string(row.get::<_, Option<String>>("secondary_genres")?.as_deref());
    let genres = split_sql_string(row.get::<_, Option<String>>("genres")?.as_deref());

    Ok(CachedRelease {
        id: row.get("id")?,
        source_path: PathBuf::from(row.get::<_, String>("source_path")?),
        cover_image_path: row
            .get::<_, Option<String>>("cover_image_path")?
            .map(PathBuf::from),
        added_at: row.get("added_at")?,
        datafile_mtime: row.get("datafile_mtime")?,
        releasetitle: row.get("releasetitle")?,
        releasetype: row.get("releasetype")?,
        releasedate: row
            .get::<_, Option<String>>("releasedate")?
            .and_then(|s| RoseDate::parse(Some(&s))),
        originaldate: row
            .get::<_, Option<String>>("originaldate")?
            .and_then(|s| RoseDate::parse(Some(&s))),
        compositiondate: row
            .get::<_, Option<String>>("compositiondate")?
            .and_then(|s| RoseDate::parse(Some(&s))),
        catalognumber: row.get("catalognumber")?,
        edition: row.get("edition")?,
        disctotal: row.get("disctotal")?,
        new: row.get::<_, i32>("new")? != 0,
        genres: genres.clone(),
        secondary_genres: secondary_genres.clone(),
        parent_genres: get_parent_genres(&genres),
        parent_secondary_genres: get_parent_genres(&secondary_genres),
        descriptors: split_sql_string(row.get::<_, Option<String>>("descriptors")?.as_deref()),
        labels: split_sql_string(row.get::<_, Option<String>>("labels")?.as_deref()),
        releaseartists: unpack_artists(
            config,
            row.get::<_, Option<String>>("releaseartist_names")?
                .as_deref(),
            row.get::<_, Option<String>>("releaseartist_roles")?
                .as_deref(),
            aliases,
        ),
        metahash: row.get("metahash")?,
    })
}

// Python: @dataclasses.dataclass(slots=True)
// class Track:
#[derive(Debug, Clone)]
pub struct CachedTrack {
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
    pub release: CachedRelease,
}

// Simplified Track struct without the release reference for intermediate processing
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
}

// Python: def cached_track_from_view(
pub fn cached_track_from_view(
    config: &Config,
    row: &rusqlite::Row,
    release: CachedRelease,
    aliases: bool,
) -> Result<CachedTrack> {
    Ok(CachedTrack {
        id: row.get("id")?,
        source_path: PathBuf::from(row.get::<_, String>("source_path")?),
        source_mtime: row.get("source_mtime")?,
        tracktitle: row.get("tracktitle")?,
        tracknumber: row.get("tracknumber")?,
        tracktotal: row.get("tracktotal")?,
        discnumber: row.get("discnumber")?,
        duration_seconds: row.get("duration_seconds")?,
        trackartists: unpack_artists(
            config,
            row.get::<_, Option<String>>("trackartist_names")?
                .as_deref(),
            row.get::<_, Option<String>>("trackartist_roles")?
                .as_deref(),
            aliases,
        ),
        metahash: row.get("metahash")?,
        release,
    })
}

// Python: @dataclasses.dataclass(slots=True)
// class Collage:
#[derive(Debug, Clone)]
pub struct CachedCollage {
    pub name: String,
    pub source_mtime: String,
}

// Python: @dataclasses.dataclass(slots=True)
// class Playlist:
#[derive(Debug, Clone)]
pub struct CachedPlaylist {
    pub name: String,
    pub source_mtime: String,
    pub cover_path: Option<PathBuf>,
}

// Python: @dataclasses.dataclass(slots=True)
// class StoredDataFile:
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

// Python: STORED_DATA_FILE_REGEX = re.compile(r"^\.rose\.([^.]+)\.toml$")
lazy_static::lazy_static! {
    pub static ref STORED_DATA_FILE_REGEX: regex::Regex = regex::Regex::new(r"^\.rose\.([^.]+)\.toml$").unwrap();
}

// Helper functions
fn split_sql_string(s: Option<&str>) -> Vec<String> {
    match s {
        Some(s) if !s.is_empty() => s.split(SQL_ARRAY_DELIMITER).map(String::from).collect(),
        _ => Vec::new(),
    }
}

fn get_parent_genres(genres: &[String]) -> Vec<String> {
    let mut parents = Vec::new();
    for genre in genres {
        if let Some(genre_parents) = get_transitive_parent_genres(genre) {
            for parent in genre_parents {
                if !parents.contains(&parent) {
                    parents.push(parent);
                }
            }
        }
    }
    parents
}

fn unpack_artists(
    config: &Config,
    names: Option<&str>,
    roles: Option<&str>,
    aliases: bool,
) -> ArtistMapping {
    let names_vec = split_sql_string(names);
    let roles_vec = split_sql_string(roles);

    let mut mapping = ArtistMapping::default();

    for (name, role) in names_vec.into_iter().zip(roles_vec.into_iter()) {
        let artist = if aliases {
            // Apply aliases
            let resolved = config
                .artist_aliases_parents_map
                .get(&name)
                .and_then(|parents| parents.first())
                .cloned()
                .unwrap_or(name.clone());
            let is_alias = name != resolved;
            Artist {
                name: resolved,
                alias: is_alias,
            }
        } else {
            Artist { name, alias: false }
        };

        match role.as_str() {
            "main" => mapping.main.push(artist),
            "guest" => mapping.guest.push(artist),
            "remixer" => mapping.remixer.push(artist),
            "producer" => mapping.producer.push(artist),
            "composer" => mapping.composer.push(artist),
            "conductor" => mapping.conductor.push(artist),
            "djmixer" => mapping.djmixer.push(artist),
            _ => {}
        }
    }

    mapping
}

// Helper function to pack artists for SQL storage
pub fn pack_artists(mapping: &ArtistMapping) -> (Vec<String>, Vec<String>) {
    let mut names = Vec::new();
    let mut roles = Vec::new();

    for artist in &mapping.main {
        names.push(artist.name.clone());
        roles.push("main".to_string());
    }
    for artist in &mapping.guest {
        names.push(artist.name.clone());
        roles.push("guest".to_string());
    }
    for artist in &mapping.remixer {
        names.push(artist.name.clone());
        roles.push("remixer".to_string());
    }
    for artist in &mapping.producer {
        names.push(artist.name.clone());
        roles.push("producer".to_string());
    }
    for artist in &mapping.composer {
        names.push(artist.name.clone());
        roles.push("composer".to_string());
    }
    for artist in &mapping.conductor {
        names.push(artist.name.clone());
        roles.push("conductor".to_string());
    }
    for artist in &mapping.djmixer {
        names.push(artist.name.clone());
        roles.push("djmixer".to_string());
    }

    (names, roles)
}

// Python: def _compare_strs(x: str, y: str) -> bool:
pub fn compare_strs(x: &str, y: &str) -> bool {
    // """Case-insensitive compare of dirname that strips some non-FS-safe punctuation."""
    // Python: return _normalize_dirname(x, False) == _normalize_dirname(y, False)
    normalize_dirname(x, false) == normalize_dirname(y, false)
}

// Python: def _normalize_dirname(dirname: str, wrap: bool = True) -> str:
pub fn normalize_dirname(dirname: &str, wrap: bool) -> String {
    // Python: dirname = unicodedata.normalize("NFD", dirname).strip()
    let mut dirname = dirname.nfd().collect::<String>().trim().to_string();

    // Python: Remove stuff like ' and …
    dirname = dirname.replace("'", "").replace("…", "...");

    // Python: for char in ':?"<>|/\\':
    //     dirname = dirname.replace(char, "_")
    for ch in &[':', '?', '"', '<', '>', '|', '/', '\\'] {
        dirname = dirname.replace(*ch, "_");
    }

    // Python: Case-insensitive
    dirname = dirname.to_lowercase();

    if wrap {
        // Python: return textwrap.fill(dirname, width=240, expand_tabs=False, break_long_words=False)
        // For simplicity, we'll just truncate at 240 chars
        if dirname.len() > 240 {
            dirname.truncate(240);
        }
    }

    dirname
}

// Python: def _get_parent_genres(genres: list[str]) -> list[str]:
fn _get_parent_genres(genres: &[String]) -> Vec<String> {
    // Python: return list({pg for g in genres for pg in TRANSITIVE_PARENT_GENRES.get(g, [])})
    let mut parent_genres = HashSet::new();
    for genre in genres {
        if let Some(parents) = get_transitive_parent_genres(genre) {
            for parent in parents {
                parent_genres.insert(parent);
            }
        }
    }
    parent_genres.into_iter().collect()
}

// Python: def get_release(c: Config, release_id: str) -> Release | None:
pub fn get_release(config: &Config, release_id: &str) -> Result<Option<CachedRelease>> {
    let conn = connect(config)?;
    let mut stmt = conn.prepare("SELECT * FROM releases_view WHERE id = ?1")?;

    let release = stmt
        .query_row([release_id], |row| {
            cached_release_from_view(config, row, true)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        })
        .optional()?;

    Ok(release)
}

// Python: def get_tracks_of_release(c: Config, release_id: str) -> list[Track]:
pub fn get_tracks_of_release(
    config: &Config,
    release_id: &str,
) -> Result<Vec<(CachedTrack, CachedRelease)>> {
    let conn = connect(config)?;

    // First get the release
    let release = get_release(config, release_id)?.ok_or_else(|| {
        RoseError::Expected(RoseExpectedError::Generic(format!(
            "Release {} not found",
            release_id
        )))
    })?;

    // Then get all tracks
    let mut stmt = conn.prepare(
        "SELECT * FROM tracks_view WHERE release_id = ?1 ORDER BY discnumber, tracknumber",
    )?;

    let tracks = stmt.query_map([release_id], |row| {
        cached_track_from_view(config, row, release.clone(), true)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
    })?;

    let mut result = Vec::new();
    for track in tracks {
        let track = track?;
        result.push((track, release.clone()));
    }

    Ok(result)
}

// Python: def list_releases(c: Config) -> list[Release]:
pub fn list_releases(config: &Config) -> Result<Vec<CachedRelease>> {
    let conn = connect(config)?;
    let mut stmt = conn.prepare("SELECT * FROM releases_view ORDER BY source_path")?;

    let releases = stmt.query_map([], |row| {
        cached_release_from_view(config, row, true)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
    })?;

    let mut result = Vec::new();
    for release in releases {
        result.push(release?);
    }

    Ok(result)
}
