/// The cache module encapsulates the read cache and exposes handles for working with the read cache. It
/// also exposes a locking mechanism that uses the read cache for synchronization.
///
/// The SQLite database is considered part of the cache, and so this module encapsulates the SQLite
/// database too. Though we cheap out a bit, so all the tests freely read from the SQLite database. No
/// budget!
///
/// The read cache is crucial to Rose. See `docs/CACHE_MAINTENANCE.md` for more information.
///
/// We consider a few problems in the cache update, whose solutions contribute to
/// the overall complexity of the cache update sequence:
///
/// 1. **Arbitrary renames:** Files and directories can be arbitrarily renamed in between cache scans.
///    We solve for these renames by writing [Stable Identifiers](#release-track-identifiers) to disk.
///    For performance, however, a track update ends up as a delete followed by an insert with the
///    just-deleted ID.
/// 2. **In-progress directory creation:** We may come across a directory while it is in the process of
///    being created. For example, due to `cp -r`. Unless --force is passed, we skip directories that
///    lack a `.rose.{uuid}.toml` file, yet have a `Release ID` written syncthing synchronization.
/// 3. **Performance:** We want to minimize file accesses, so we cache heavily and batch operations
///    together. This creates a lot of intermediate state that we accumulate throughout the cache
///    update.

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

use crate::audiotags::{AudioTags, SUPPORTED_AUDIO_EXTENSIONS};
use crate::common::{sanitize_dirname, sanitize_filename, uniq, Artist, ArtistMapping, Result, RoseDate, RoseError, RoseExpectedError, VERSION};
use crate::config::Config;
use crate::genre_hierarchy::{GenreHierarchy, TRANSITIVE_CHILD_GENRES, TRANSITIVE_PARENT_GENRES};
use crate::templates::{artistsfmt, evaluate_release_template, evaluate_track_template};
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, warn, info};
use uuid::Uuid;

// from __future__ import annotations
// 
// import contextlib
// import copy
// import dataclasses
// import hashlib
// import json
// import logging
// import math
// import multiprocessing
// import os
// import os.path
// import re
// import sqlite3
// import time
// import tomllib
// import unicodedata
// from collections import Counter, defaultdict
// from collections.abc import Iterator
// from datetime import datetime
// from hashlib import sha256
// from pathlib import Path
// from typing import Any, TypeVar
// 
// import tomli_w
// import uuid6
// 
// from rose.audiotags import SUPPORTED_AUDIO_EXTENSIONS, AudioTags, RoseDate
// from rose.common import (
//     VERSION,
//     Artist,
//     ArtistMapping,
//     flatten,
//     sanitize_dirname,
//     sanitize_filename,
//     sha256_dataclass,
//     uniq,
// )
// from rose.config import Config
// from rose.genre_hierarchy import TRANSITIVE_CHILD_GENRES, TRANSITIVE_PARENT_GENRES
// from rose.templates import artistsfmt, evaluate_release_template, evaluate_track_template
// 
// logger = logging.getLogger(__name__)
// 
// T = TypeVar("T")
// 
// CACHE_SCHEMA_PATH = Path(__file__).resolve().parent / "cache.sql"

static CACHE_SCHEMA: &str = include_str!("cache.sql");


/// Connect to the SQLite database with appropriate settings
pub fn connect(c: &Config) -> Result<Connection> {
    let conn = Connection::open(&c.cache_database_path)?;
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = WAL;
        PRAGMA busy_timeout = 15000;
        ",
    )?;
    Ok(conn)
}

// @contextlib.contextmanager
// def connect(c: Config) -> Iterator[sqlite3.Connection]:
//     conn = sqlite3.connect(
//         c.cache_database_path,
//         detect_types=sqlite3.PARSE_DECLTYPES,
//         isolation_level=None,
//         timeout=15.0,
//     )
//     try:
//         conn.row_factory = sqlite3.Row
//         conn.execute("PRAGMA foreign_keys=ON")
//         conn.execute("PRAGMA journal_mode=WAL")
//         yield conn
//     finally:
//         if conn:
//             conn.close()


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
                .query_row(
                    "SELECT schema_hash, config_hash, version FROM _schema_hash",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
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
    if c.cache_database_path.exists() {
        fs::remove_file(&c.cache_database_path)?;
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

// def maybe_invalidate_cache_database(c: Config) -> None:
//     """
//     "Migrate" the database. If the schema in the database does not match that on disk, then nuke the
//     database and recreate it from scratch. Otherwise, no op.
// 
//     We can do this because the database is just a read cache. It is not source-of-truth for any of
//     its own data.
//     """
//     with CACHE_SCHEMA_PATH.open("rb") as fp:
//         schema_hash = hashlib.sha256(fp.read()).hexdigest()
// 
//     # Hash a subset of the config fields to use as the cache hash, which invalidates the cache on
//     # change. These are the fields that affect cache population. Invalidating the cache on config
//     # change ensures that the cache is consistent with the config.
//     config_hash_fields = {
//         "music_source_dir": str(c.music_source_dir),
//         "cache_dir": str(c.cache_dir),
//         "cover_art_stems": c.cover_art_stems,
//         "valid_art_exts": c.valid_art_exts,
//         "ignore_release_directories": c.ignore_release_directories,
//     }
//     config_hash = sha256(json.dumps(config_hash_fields).encode()).hexdigest()
// 
//     with connect(c) as conn:
//         cursor = conn.execute(
//             """
//             SELECT EXISTS(
//                 SELECT * FROM sqlite_master
//                 WHERE type = 'table' AND name = '_schema_hash'
//             )
//             """
//         )
//         if cursor.fetchone()[0]:
//             cursor = conn.execute("SELECT schema_hash, config_hash, version FROM _schema_hash")
//             row = cursor.fetchone()
//             if (
//                 row
//                 and row["schema_hash"] == schema_hash
//                 and row["config_hash"] == config_hash
//                 and row["version"] == VERSION
//             ):
//                 # Everything matches! Exit!
//                 return
// 
//     c.cache_database_path.unlink(missing_ok=True)
//     with connect(c) as conn:
//         with CACHE_SCHEMA_PATH.open("r") as fp:
//             conn.executescript(fp.read())
//         conn.execute(
//             """
//             CREATE TABLE _schema_hash (
//                 schema_hash TEXT
//               , config_hash TEXT
//               , version TEXT
//               , PRIMARY KEY (schema_hash, config_hash, version)
//             )
//             """
//         )
//         conn.execute(
//             "INSERT INTO _schema_hash (schema_hash, config_hash, version) VALUES (?, ?, ?)",
//             (schema_hash, config_hash, VERSION),
//         )


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
pub fn lock(c: &Config, name: &str, timeout: f64) -> Result<Lock> {
    loop {
        let conn = connect(c)?;
        let max_valid_until: Option<f64> = conn
            .query_row(
                "SELECT MAX(valid_until) FROM locks WHERE name = ?1",
                params![name],
                |row| row.get(0),
            )
            .optional()?;

        // If a lock exists, sleep until the lock is available. All locks should be very
        // short lived, so this shouldn't be a big performance penalty.
        if let Some(valid_until) = max_valid_until {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs_f64();
            if valid_until > now {
                let sleep_duration = Duration::from_secs_f64((valid_until - now).max(0.0));
                debug!("Failed to acquire lock for {}: sleeping for {:?}", name, sleep_duration);
                std::thread::sleep(sleep_duration);
                continue;
            }
        }

        debug!("Attempting to acquire lock for {} with timeout {}", name, timeout);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let valid_until = now + timeout;

        match conn.execute(
            "INSERT INTO locks (name, valid_until) VALUES (?1, ?2)",
            params![name, valid_until],
        ) {
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

// @contextlib.contextmanager
// def lock(c: Config, name: str, timeout: float = 1.0) -> Iterator[None]:
//     try:
//         while True:
//             with connect(c) as conn:
//                 cursor = conn.execute("SELECT MAX(valid_until) FROM locks WHERE name = ?", (name,))
//                 row = cursor.fetchone()
//                 # If a lock exists, sleep until the lock is available. All locks should be very
//                 # short lived, so this shouldn't be a big performance penalty.
//                 if row and row[0] and row[0] > time.time():
//                     sleep = max(0, row[0] - time.time())
//                     logger.debug(f"Failed to acquire lock for {name}: sleeping for {sleep}")
//                     time.sleep(sleep)
//                     continue
//                 logger.debug(f"Attempting to acquire lock for {name} with timeout {timeout}")
//                 valid_until = time.time() + timeout
//                 try:
//                     conn.execute("INSERT INTO locks (name, valid_until) VALUES (?, ?)", (name, valid_until))
//                 except sqlite3.IntegrityError as e:
//                     logger.debug(f"Failed to acquire lock for {name}, trying again: {e}")
//                     continue
//                 logger.debug(f"Successfully acquired lock for {name} with timeout {timeout} until {valid_until}")
//                 break
//         yield
//     finally:
//         logger.debug(f"Releasing lock {name}")
//         with connect(c) as conn:
//             conn.execute("DELETE FROM locks WHERE name = ?", (name,))


pub fn release_lock_name(release_id: &str) -> String {
    format!("release-{}", release_id)
}

pub fn collage_lock_name(collage_name: &str) -> String {
    format!("collage-{}", collage_name)
}

pub fn playlist_lock_name(playlist_name: &str) -> String {
    format!("playlist-{}", playlist_name)
}

// def release_lock_name(release_id: str) -> str:
//     return f"release-{release_id}"
// 
// 
// def collage_lock_name(collage_name: str) -> str:
//     return f"collage-{collage_name}"
// 
// 
// def playlist_lock_name(playlist_name: str) -> str:
//     return f"playlist-{playlist_name}"


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub id: String,
    pub source_path: PathBuf,
    pub cover_image_path: Option<PathBuf>,
    pub added_at: String,  // ISO8601 timestamp
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

// @dataclasses.dataclass(slots=True)
// class Release:
//     id: str
//     source_path: Path
//     cover_image_path: Path | None
//     added_at: str  # ISO8601 timestamp
//     datafile_mtime: str
//     releasetitle: str
//     releasetype: str
//     releasedate: RoseDate | None
//     originaldate: RoseDate | None
//     compositiondate: RoseDate | None
//     edition: str | None
//     catalognumber: str | None
//     new: bool
//     disctotal: int
//     genres: list[str]
//     parent_genres: list[str]
//     secondary_genres: list[str]
//     parent_secondary_genres: list[str]
//     descriptors: list[str]
//     labels: list[str]
//     releaseartists: ArtistMapping
//     metahash: str


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
fn _unpack(xxs: &[&str]) -> Vec<Vec<&str>> {
    let mut result = Vec::new();
    let split_lists: Vec<Vec<&str>> = xxs.iter()
        .map(|xs| {
            if xs.is_empty() {
                Vec::new()
            } else {
                xs.split(" ¬ ").collect()
            }
        })
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
fn _unpack_artists(
    c: &Config,
    names: &str,
    roles: &str,
    aliases: bool,
) -> ArtistMapping {
    let mut mapping = ArtistMapping::default();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    
    let unpacked = _unpack(&[names, roles]);
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
        cover_image_path: row.get::<_, Option<String>>("cover_image_path")?
            .map(PathBuf::from),
        added_at: row.get("added_at")?,
        datafile_mtime: row.get("datafile_mtime")?,
        releasetitle: row.get("releasetitle")?,
        releasetype: row.get("releasetype")?,
        releasedate: row.get::<_, Option<String>>("releasedate")?
            .and_then(|s| RoseDate::parse(Some(&s))),
        originaldate: row.get::<_, Option<String>>("originaldate")?
            .and_then(|s| RoseDate::parse(Some(&s))),
        compositiondate: row.get::<_, Option<String>>("compositiondate")?
            .and_then(|s| RoseDate::parse(Some(&s))),
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

// def cached_release_from_view(c: Config, row: dict[str, Any], aliases: bool = True) -> Release:
//     secondary_genres = _split(row["secondary_genres"]) if row["secondary_genres"] else []
//     genres = _split(row["genres"]) if row["genres"] else []
//     return Release(
//         id=row["id"],
//         source_path=Path(row["source_path"]),
//         cover_image_path=Path(row["cover_image_path"]) if row["cover_image_path"] else None,
//         added_at=row["added_at"],
//         datafile_mtime=row["datafile_mtime"],
//         releasetitle=row["releasetitle"],
//         releasetype=row["releasetype"],
//         releasedate=RoseDate.parse(row["releasedate"]),
//         originaldate=RoseDate.parse(row["originaldate"]),
//         compositiondate=RoseDate.parse(row["compositiondate"]),
//         catalognumber=row["catalognumber"],
//         edition=row["edition"],
//         disctotal=row["disctotal"],
//         new=bool(row["new"]),
//         genres=genres,
//         secondary_genres=secondary_genres,
//         parent_genres=_get_parent_genres(genres),
//         parent_secondary_genres=_get_parent_genres(secondary_genres),
//         descriptors=_split(row["descriptors"]) if row["descriptors"] else [],
//         labels=_split(row["labels"]) if row["labels"] else [],
//         releaseartists=_unpack_artists(c, row["releaseartist_names"], row["releaseartist_roles"], aliases=aliases),
//         metahash=row["metahash"],
//     )


#[derive(Debug, Clone, Serialize, Deserialize)]
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

// @dataclasses.dataclass(slots=True)
// class Track:
//     id: str
//     source_path: Path
//     source_mtime: str
//     tracktitle: str
//     tracknumber: str
//     tracktotal: int
//     discnumber: str
//     duration_seconds: int
//     trackartists: ArtistMapping
//     metahash: str
// 
//     release: Release


pub fn cached_track_from_view(
    c: &Config,
    row: &Row,
    release: Arc<Release>,
    aliases: bool,
) -> Result<Track> {
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

// def cached_track_from_view(
//     c: Config,
//     row: dict[str, Any],
//     release: Release,
//     aliases: bool = True,
// ) -> Track:
//     return Track(
//         id=row["id"],
//         source_path=Path(row["source_path"]),
//         source_mtime=row["source_mtime"],
//         tracktitle=row["tracktitle"],
//         tracknumber=row["tracknumber"],
//         tracktotal=row["tracktotal"],
//         discnumber=row["discnumber"],
//         duration_seconds=row["duration_seconds"],
//         trackartists=_unpack_artists(
//             c,
//             row["trackartist_names"],
//             row["trackartist_roles"],
//             aliases=aliases,
//         ),
//         metahash=row["metahash"],
//         release=release,
//     )


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

// @dataclasses.dataclass(slots=True)
// class Collage:
//     name: str
//     source_mtime: str
// 
// 
// @dataclasses.dataclass(slots=True)
// class Playlist:
//     name: str
//     source_mtime: str
//     cover_path: Path | None


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

static STORED_DATA_FILE_REGEX: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r"^\.rose\.([^.]+)\.toml$").unwrap()
});

// @dataclasses.dataclass(slots=True)
// class StoredDataFile:
//     new: bool = True
//     added_at: str = dataclasses.field(
//         default_factory=lambda: datetime.now().astimezone().replace(microsecond=0).isoformat()
//     )
// 
// 
// STORED_DATA_FILE_REGEX = re.compile(r"^\.rose\.([^.]+)\.toml$")


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

// def update_cache(
//     c: Config,
//     force: bool = False,
//     # For testing.
//     force_multiprocessing: bool = False,
// ) -> None:
//     """
//     Update the read cache to match the data for all releases in the music source directory. Delete
//     any cached releases that are no longer present on disk.
//     """
//     update_cache_for_releases(c, None, force, force_multiprocessing=force_multiprocessing)
//     update_cache_evict_nonexistent_releases(c)
//     update_cache_for_collages(c, None, force)
//     update_cache_evict_nonexistent_collages(c)
//     update_cache_for_playlists(c, None, force)
//     update_cache_evict_nonexistent_playlists(c)

// Placeholder functions - to be implemented
pub fn update_cache_for_releases(
    _c: &Config,
    _release_dirs: Option<Vec<PathBuf>>,
    _force: bool,
    _force_multiprocessing: bool,
) -> Result<()> {
    // TODO: Implement
    Ok(())
}

pub fn update_cache_evict_nonexistent_releases(_c: &Config) -> Result<()> {
    // TODO: Implement
    Ok(())
}

pub fn update_cache_for_collages(
    _c: &Config,
    _collage_names: Option<Vec<String>>,
    _force: bool,
) -> Result<()> {
    // TODO: Implement
    Ok(())
}

pub fn update_cache_evict_nonexistent_collages(_c: &Config) -> Result<()> {
    // TODO: Implement
    Ok(())
}

pub fn update_cache_for_playlists(
    _c: &Config,
    _playlist_names: Option<Vec<String>>,
    _force: bool,
) -> Result<()> {
    // TODO: Implement
    Ok(())
}

pub fn update_cache_evict_nonexistent_playlists(_c: &Config) -> Result<()> {
    // TODO: Implement
    Ok(())
}


def update_cache_evict_nonexistent_releases(c: Config) -> None:
    logger.debug("Evicting cached releases that are not on disk")
    dirs = [Path(d.path).resolve() for d in os.scandir(c.music_source_dir) if d.is_dir()]
    with connect(c) as conn:
        cursor = conn.execute(
            f"""
            DELETE FROM releases
            WHERE source_path NOT IN ({",".join(["?"] * len(dirs))})
            RETURNING source_path
            """,
            [str(d) for d in dirs],
        )
        for row in cursor:
            logger.info(f"Evicted missing release {row["source_path"]} from cache")


def update_cache_for_releases(
    c: Config,
    # Leave as None to update all releases.
    release_dirs: list[Path] | None = None,
    force: bool = False,
    # For testing.
    force_multiprocessing: bool = False,
) -> None:
    """
    Update the read cache to match the data for any passed-in releases. If a directory lacks a
    .rose.{uuid}.toml datafile, create the datafile for the release and set it to the initial state.

    This is a hot path and is thus performance-optimized. The bottleneck is disk accesses, so we
    structure this function in order to minimize them. We solely read files that have changed since
    last run and batch writes together. We trade higher memory for reduced disk accesses.
    Concretely, we:

    1. Execute one big SQL query at the start to fetch the relevant previous caches.
    2. Skip reading a file's data if the mtime has not changed since the previous cache update.
    3. Batch SQLite write operations to the end of this function, and only execute a SQLite upsert
       if the read data differs from the previous caches.

    We also shard the directories across multiple processes and execute them simultaneously.
    """
    release_dirs = release_dirs or [Path(d.path) for d in os.scandir(c.music_source_dir) if d.is_dir()]
    release_dirs = [
        d
        for d in release_dirs
        if d.name != "!collages" and d.name != "!playlists" and d.name not in c.ignore_release_directories
    ]
    if not release_dirs:
        logger.debug("No-Op: No whitelisted releases passed into update_cache_for_releases")
        return
    logger.debug(f"Refreshing the read cache for {len(release_dirs)} releases")
    if len(release_dirs) < 10:
        logger.debug(f"Refreshing cached data for {", ".join([r.name for r in release_dirs])}")

    # If the number of releases changed is less than 50; do not bother with all that multiprocessing
    # gunk: instead, directly call the executor.
    #
    # This has an added benefit of not spawning processes from the virtual filesystem and watchdog
    # processes, as those processes always update the cache for one release at a time and are
    # multithreaded. Starting other processes from threads is bad!
    if not force_multiprocessing and len(release_dirs) < 50:
        logger.debug(f"Running cache update executor in same process because {len(release_dirs)=} < 50")
        _update_cache_for_releases_executor(c, release_dirs, force)
        return

    # Batch size defaults to equal split across all processes. However, if the number of directories
    # is small, we shrink the # of processes to save on overhead.
    num_proc = c.max_proc
    if len(release_dirs) < c.max_proc * 50:
        num_proc = max(1, math.ceil(len(release_dirs) // 50))
    batch_size = len(release_dirs) // num_proc + 1

    manager = multiprocessing.Manager()
    # Have each process propagate the collages and playlists it wants to update back upwards. We
    # will dispatch the force updater only once in the main process, instead of many times in each
    # process.
    collages_to_force_update = manager.list()
    playlists_to_force_update = manager.list()

    errors: list[BaseException] = []

    logger.debug("Creating multiprocessing pool to parallelize cache executors.")
    with multiprocessing.Pool(processes=c.max_proc) as pool:
        # At 0, no batch. At 1, 1 batch. At 49, 1 batch. At 50, 1 batch. At 51, 2 batches.
        for i in range(0, len(release_dirs), batch_size):
            logger.debug(f"Spawning release cache update process for releases [{i}, {i + batch_size})")
            pool.apply_async(
                _update_cache_for_releases_executor,
                (
                    c,
                    release_dirs[i : i + batch_size],
                    force,
                    collages_to_force_update,
                    playlists_to_force_update,
                ),
                error_callback=lambda e: errors.append(e),
            )
        pool.close()
        pool.join()

    if errors:
        raise ExceptionGroup("Exception occurred in cache update subprocesses", errors)  # type: ignore

    if collages_to_force_update:
        update_cache_for_collages(c, uniq(list(collages_to_force_update)), force=True)
    if playlists_to_force_update:
        update_cache_for_playlists(c, uniq(list(playlists_to_force_update)), force=True)


def _update_cache_for_releases_executor(
    c: Config,
    release_dirs: list[Path],
    force: bool,
    # If these are not None, we will store the collages and playlists to update in here instead of
    # invoking the update functions directly. If these are None, we will not put anything in them
    # and instead invoke update_cache_for_{collages,playlists} directly. This is a Bad Pattern, but
    # good enough.
    collages_to_force_update_receiver: list[str] | None = None,
    playlists_to_force_update_receiver: list[str] | None = None,
) -> None:
    """The implementation logic, split out for multiprocessing."""
    # First, call readdir on every release directory. We store the results in a map of
    # Path Basename -> (Release ID if exists, filenames).
    dir_scan_start = time.time()
    dir_tree: list[tuple[Path, str | None, list[Path]]] = []
    release_uuids: list[str] = []
    for rd in release_dirs:
        release_id = None
        files: list[Path] = []
        if not rd.is_dir():
            logger.debug(f"Skipping scanning {rd} because it is not a directory")
            continue
        for root, _, subfiles in os.walk(str(rd)):
            for sf in subfiles:
                if m := STORED_DATA_FILE_REGEX.match(sf):
                    release_id = m[1]
                files.append(Path(root) / sf)
        # Force a deterministic file sort order.
        files.sort()
        dir_tree.append((rd.resolve(), release_id, files))
        if release_id is not None:
            release_uuids.append(release_id)
    logger.debug(f"Release update source dir scan time {time.time() - dir_scan_start=}")

    cache_read_start = time.time()
    # Then batch query for all metadata associated with the discovered IDs. This pulls all data into
    # memory for fast access throughout this function. We do this in two passes (and two queries!):
    # 1. Fetch all releases.
    # 2. Fetch all tracks in a single query, and then associates each track with a release.
    # The tracks are stored as a dict of source_path -> Track.
    cached_releases: dict[str, tuple[Release, dict[str, Track]]] = {}
    with connect(c) as conn:
        cursor = conn.execute(
            rf"""
            SELECT *
            FROM releases_view
            WHERE id IN ({",".join(["?"] * len(release_uuids))})
            """,
            release_uuids,
        )
        for row in cursor:
            cached_releases[row["id"]] = (cached_release_from_view(c, row, aliases=False), {})

        logger.debug(f"Found {len(cached_releases)}/{len(release_dirs)} releases in cache")

        cursor = conn.execute(
            rf"""
            SELECT *
            FROM tracks_view
            WHERE release_id IN ({",".join(["?"] * len(release_uuids))})
            """,
            release_uuids,
        )
        num_tracks_found = 0
        for row in cursor:
            cached_releases[row["release_id"]][1][row["source_path"]] = cached_track_from_view(
                c,
                row,
                cached_releases[row["release_id"]][0],
                aliases=False,
            )
            num_tracks_found += 1
        logger.debug(f"Found {num_tracks_found} tracks in cache")
    logger.debug(f"Release update cache read time {time.time() - cache_read_start=}")

    # Now iterate over all releases in the source directory. Leverage mtime from stat to determine
    # whether to even check the file tags or not. Compute the necessary database updates and store
    # them in the `upd_` variables. After this loop, we will execute the database updates based on
    # the `upd_` varaibles.
    loop_start = time.time()
    upd_delete_source_paths: list[str] = []
    upd_release_args: list[list[Any]] = []
    upd_release_ids: list[str] = []
    upd_release_artist_args: list[list[Any]] = []
    upd_release_genre_args: list[list[Any]] = []
    upd_release_secondary_genre_args: list[list[Any]] = []
    upd_release_descriptor_args: list[list[Any]] = []
    upd_release_label_args: list[list[Any]] = []
    upd_unknown_cached_tracks_args: list[tuple[str, list[str]]] = []
    upd_track_args: list[list[Any]] = []
    upd_track_ids: list[str] = []
    upd_track_artist_args: list[list[Any]] = []
    for source_path, preexisting_release_id, files in dir_tree:
        logger.debug(f"Scanning release {source_path.name}")
        # Check to see if we should even process the directory. If the directory does not have
        # any tracks, skip it. And if it does not have any tracks, but is in the cache, remove
        # it from the cache.
        first_audio_file: Path | None = None
        for f in files:
            if f.suffix.lower() in SUPPORTED_AUDIO_EXTENSIONS:
                first_audio_file = f
                break
        else:
            logger.debug(f"Did not find any audio files in release {source_path}, skipping")
            logger.debug(f"Scheduling cache deletion for empty directory release {source_path}")
            upd_delete_source_paths.append(str(source_path))
            continue
        assert first_audio_file is not None

        # This value is used to track whether to update the database for this release. If this
        # is False at the end of this loop body, we can save a database update call.
        release_dirty = False

        # Fetch the release from the cache. We will be updating this value on-the-fly, so
        # instantiate to zero values if we do not have a default value.
        try:
            release, cached_tracks = cached_releases[preexisting_release_id or ""]
        except KeyError:
            logger.debug(f"First-time unidentified release found at release {source_path}, writing UUID and new")
            release_dirty = True
            release = Release(
                id=preexisting_release_id or "",
                source_path=source_path,
                datafile_mtime="",
                cover_image_path=None,
                added_at="",
                releasetitle="",
                releasetype="",
                releasedate=None,
                originaldate=None,
                compositiondate=None,
                catalognumber=None,
                edition=None,
                new=True,
                disctotal=0,
                genres=[],
                parent_genres=[],
                secondary_genres=[],
                parent_secondary_genres=[],
                descriptors=[],
                labels=[],
                releaseartists=ArtistMapping(),
                metahash="",
            )
            cached_tracks = {}

        # Handle source path change; if it's changed, update the release.
        if source_path != release.source_path:
            logger.debug(f"Source path change detected for release {source_path}, updating")
            release.source_path = source_path
            release_dirty = True

        # The directory does not have a release ID, so create the stored data file. Also, in case
        # the directory changes mid-scan, wrap this in an error handler.
        try:
            if not preexisting_release_id:
                # However, skip this directory for a special case. Because directory copying/movement is
                # not atomic, we may read a directory in a in-progres creation state. If:
                #
                # 1. The directory lacks a `.rose.{uuid}.toml` file, but the files have Rose IDs,
                # 2. And the directory mtime is less than 3 seconds ago,
                #
                # We consider the directory to be in a in-progress creation state. And so we do not
                # process the directory at this time.
                release_id_from_first_file = None
                with contextlib.suppress(Exception):
                    release_id_from_first_file = AudioTags.from_file(first_audio_file).release_id
                if release_id_from_first_file and not force:
                    logger.warning(
                        f"No-Op: Skipping release at {source_path}: files in release already have "
                        f"release_id {release_id_from_first_file}, but .rose.{{uuid}}.toml is missing, "
                        "is another tool in the middle of writing the directory? Run with --force to "
                        "recreate .rose.{uuid}.toml"
                    )
                    continue

                logger.debug(f"Creating new stored data file for release {source_path}")
                stored_release_data = StoredDataFile(
                    new=True,
                    added_at=datetime.now().astimezone().replace(microsecond=0).isoformat(),
                )
                # Preserve the release ID already present the first file if we can.
                new_release_id = release_id_from_first_file or str(uuid6.uuid7())
                datafile_path = source_path / f".rose.{new_release_id}.toml"
                # No need to lock here, as since the release ID is new, there is no way there is a
                # concurrent writer.
                with datafile_path.open("wb") as fp:
                    tomli_w.dump(dataclasses.asdict(stored_release_data), fp)
                release.id = new_release_id
                release.new = stored_release_data.new
                release.added_at = stored_release_data.added_at
                release.datafile_mtime = str(os.stat(datafile_path).st_mtime)
                release_dirty = True
            else:
                # Otherwise, check to see if the mtime changed from what we know. If it has, read
                # from the datafile.
                datafile_path = source_path / f".rose.{preexisting_release_id}.toml"
                datafile_mtime = str(os.stat(datafile_path).st_mtime)
                if datafile_mtime != release.datafile_mtime or force:
                    logger.debug(f"Datafile changed for release {source_path}, updating")
                    release_dirty = True
                    release.datafile_mtime = datafile_mtime
                    # For performance reasons (!!), don't acquire a lock here. However, acquire a lock
                    # if we are to write to the file. We won't worry about lost writes here.
                    with datafile_path.open("rb") as fp:
                        diskdata = tomllib.load(fp)
                    datafile = StoredDataFile(
                        new=diskdata.get("new", True),
                        added_at=diskdata.get(
                            "added_at",
                            datetime.now().astimezone().replace(microsecond=0).isoformat(),
                        ),
                    )
                    release.new = datafile.new
                    release.added_at = datafile.added_at
                    new_resolved_data = dataclasses.asdict(datafile)
                    logger.debug(f"Updating values in stored data file for release {source_path}")
                    if new_resolved_data != diskdata:
                        # And then write the data back to disk if it changed. This allows us to update
                        # datafiles to contain newer default values.
                        lockname = release_lock_name(preexisting_release_id)
                        with lock(c, lockname), datafile_path.open("wb") as fp:
                            tomli_w.dump(new_resolved_data, fp)
        except FileNotFoundError:
            logger.warning(f"Skipping update on {source_path}: directory no longer exists")
            continue

        # Handle cover art change.
        cover = None
        for f in files:
            if f.name.lower() in c.valid_cover_arts:
                cover = f
                break
        if cover != release.cover_image_path:
            logger.debug(f"Cover art file for release {source_path} updated to path {cover}")
            release.cover_image_path = cover
            release_dirty = True

        # Now we'll switch over to processing some of the tracks. We need track metadata in
        # order to calculate some fields of the release, so we'll first compute the valid set of
        # Tracks, and then we will finalize the release and execute any required database
        # operations for the release and tracks.

        # We want to know which cached tracks are no longer on disk. By the end of the following
        # loop, this set should only contain the such tracks, which will be deleted in the
        # database execution handling step.
        unknown_cached_tracks: set[str] = set(cached_tracks.keys())
        # Next, we will construct the list of tracks that are on the release. We will also
        # leverage mtimes and such to avoid unnecessary recomputations. If a release has changed
        # and should be updated in the database, we add its ID to track_ids_to_insert, which
        # will be used in the database execution step.
        tracks: list[Track] = []
        track_ids_to_insert: set[str] = set()
        # This value is set to true if we read an AudioTags and used it to confirm the release
        # tags.
        pulled_release_tags = False
        totals_ctr: dict[str, int] = Counter()
        for f in files:
            if f.suffix.lower() not in SUPPORTED_AUDIO_EXTENSIONS:
                continue

            cached_track = cached_tracks.get(str(f), None)
            with contextlib.suppress(KeyError):
                unknown_cached_tracks.remove(str(f))

            try:
                track_mtime = str(os.stat(f).st_mtime)
                # Skip re-read if we can reuse a cached entry.
                if cached_track and track_mtime == cached_track.source_mtime and not force:
                    logger.debug(f"Track cache hit (mtime) for {os.path.basename(f)}, reusing cached data")
                    tracks.append(cached_track)
                    totals_ctr[cached_track.discnumber] += 1
                    continue

                # Otherwise, read tags from disk and construct a new cached_track.
                logger.debug(f"Track cache miss for {os.path.basename(f)}, reading tags from disk")
                tags = AudioTags.from_file(Path(f))
            except FileNotFoundError:
                logger.warning(f"Skipping track update for {os.path.basename(f)}: file no longer exists")
                continue

            # Now that we're here, pull the release tags. We also need them to compute the
            # formatted artist string.
            if not pulled_release_tags:
                pulled_release_tags = True
                release_title = tags.releasetitle or "Unknown Release"
                if release_title != release.releasetitle:
                    logger.debug(f"Release title change detected for {source_path}, updating")
                    release.releasetitle = release_title
                    release_dirty = True

                releasetype = tags.releasetype
                if releasetype != release.releasetype:
                    logger.debug(f"Release type change detected for {source_path}, updating")
                    release.releasetype = releasetype
                    release_dirty = True

                if tags.releasedate != release.releasedate:
                    logger.debug(f"Release year change detected for {source_path}, updating")
                    release.releasedate = tags.releasedate
                    release_dirty = True

                if tags.originaldate != release.originaldate:
                    logger.debug(f"Release original year change detected for {source_path}, updating")
                    release.originaldate = tags.originaldate
                    release_dirty = True

                if tags.compositiondate != release.compositiondate:
                    logger.debug(f"Release composition year change detected for {source_path}, updating")
                    release.compositiondate = tags.compositiondate
                    release_dirty = True

                if tags.edition != release.edition:
                    logger.debug(f"Release edition change detected for {source_path}, updating")
                    release.edition = tags.edition
                    release_dirty = True

                if tags.catalognumber != release.catalognumber:
                    logger.debug(f"Release catalog number change detected for {source_path}, updating")
                    release.catalognumber = tags.catalognumber
                    release_dirty = True

                if tags.genre != release.genres:
                    logger.debug(f"Release genre change detected for {source_path}, updating")
                    release.genres = uniq(tags.genre)
                    release.parent_genres = _get_parent_genres(release.genres)
                    release_dirty = True

                if tags.secondarygenre != release.secondary_genres:
                    logger.debug(f"Release secondary genre change detected for {source_path}, updating")
                    release.secondary_genres = uniq(tags.secondarygenre)
                    release.parent_secondary_genres = _get_parent_genres(release.secondary_genres)
                    release_dirty = True

                if tags.descriptor != release.descriptors:
                    logger.debug(f"Release descriptor change detected for {source_path}, updating")
                    release.descriptors = uniq(tags.descriptor)
                    release_dirty = True

                if tags.label != release.labels:
                    logger.debug(f"Release label change detected for {source_path}, updating")
                    release.labels = uniq(tags.label)
                    release_dirty = True

                if tags.releaseartists != release.releaseartists:
                    logger.debug(f"Release artists change detected for {source_path}, updating")
                    release.releaseartists = tags.releaseartists
                    release_dirty = True

            # Here we compute the track ID. We store the track ID on the audio file in order to
            # enable persistence. This does mutate the file!
            #
            # We don't attempt to optimize this write; however, there is not much purpose to doing
            # so, since this occurs once over the lifetime of the track's existence in Rose. We
            # optimize this function because it is called repeatedly upon every metadata edit, but
            # in this case, we skip this code path once an ID is generated.
            #
            # We also write the release ID to the tags. This is not needed in normal operations
            # (since we have .rose.{uuid}.toml!), but provides a layer of defense in situations like
            # a directory being written file-by-file and being processed in a half-written state.
            track_id = tags.id
            if not track_id or not tags.release_id or tags.release_id != release.id:
                # This is our first time reading this track in the system, so no cocurrent processes
                # should be reading/writing this file. We can avoid locking. And If we have two
                # concurrent first-time cache updates, other places will have issues too.
                tags.id = tags.id or str(uuid6.uuid7())
                tags.release_id = release.id
                try:
                    tags.flush(c)
                    # And refresh the mtime because we've just written to the file.
                    track_id = tags.id
                    track_mtime = str(os.stat(f).st_mtime)
                except FileNotFoundError:
                    logger.warning(f"Skipping track update for {os.path.basename(f)}: file no longer exists")
                    continue

            # And now create the cached track.
            track = Track(
                id=track_id,
                source_path=Path(f),
                source_mtime=track_mtime,
                tracktitle=tags.tracktitle or "Unknown Title",
                # Remove `.` here because we use `.` to parse out discno/trackno in the virtual
                # filesystem. It should almost never happen, but better to be safe. We set the
                # totals on all tracks the end of the loop.
                tracknumber=(tags.tracknumber or "1").replace(".", ""),
                tracktotal=tags.tracktotal or 1,
                discnumber=(tags.discnumber or "1").replace(".", ""),
                # This is calculated with the virtual filename.
                duration_seconds=tags.duration_sec,
                trackartists=tags.trackartists,
                metahash="",
                release=release,
            )
            tracks.append(track)
            track_ids_to_insert.add(track.id)
            totals_ctr[track.discnumber] += 1

        # Now set the tracktotals and disctotals.
        disctotal = len(totals_ctr)
        if release.disctotal != disctotal:
            logger.debug(f"Release disctotal change detected for {release.source_path}, updating")
            release_dirty = True
            release.disctotal = disctotal
        for track in tracks:
            tracktotal = totals_ctr[track.discnumber]
            assert tracktotal != 0, "This track isn't in the counter, impossible!"
            if tracktotal != track.tracktotal:
                logger.debug(f"Track tracktotal change detected for {track.source_path}, updating")
                track.tracktotal = tracktotal
                track_ids_to_insert.add(track.id)

        # And now perform directory/file renames if configured.
        if c.rename_source_files:
            if release_dirty:
                wanted_dirname = evaluate_release_template(c.path_templates.source.release, release)
                wanted_dirname = sanitize_dirname(c, wanted_dirname, True)
                # Iterate until we've either:
                # 1. Realized that the name of the source path matches the desired dirname (which we
                #    may not realize immediately if there are name conflicts).
                # 2. Or renamed the source directory to match our desired name.
                original_wanted_dirname = wanted_dirname
                collision_no = 2
                while not _compare_strs(wanted_dirname, release.source_path.name):
                    new_source_path = release.source_path.with_name(wanted_dirname)
                    # If there is a collision, bump the collision counter and retry.
                    if new_source_path.exists():
                        new_max_len = c.max_filename_bytes - (3 + len(str(collision_no)))
                        wanted_dirname = f"{original_wanted_dirname[:new_max_len]} [{collision_no}]"
                        collision_no += 1
                        continue
                    # If no collision, rename the directory.
                    old_source_path = release.source_path
                    old_source_path.rename(new_source_path)
                    logger.info(f"Renamed source release directory {old_source_path.name} to {new_source_path.name}")
                    release.source_path = new_source_path
                    # Update the cached cover image path.
                    if release.cover_image_path:
                        coverlocalpath = str(release.cover_image_path).removeprefix(f"{old_source_path}/")
                        release.cover_image_path = release.source_path / coverlocalpath
                    # Update the cached track paths and schedule them for database insertions.
                    for track in tracks:
                        tracklocalpath = str(track.source_path).removeprefix(f"{old_source_path}/")
                        track.source_path = release.source_path / tracklocalpath
                        track.source_mtime = str(os.stat(track.source_path).st_mtime)
                        track_ids_to_insert.add(track.id)
            for track in [t for t in tracks if t.id in track_ids_to_insert]:
                wanted_filename = evaluate_track_template(c.path_templates.source.track, track)
                wanted_filename = sanitize_filename(c, wanted_filename, True)
                # And repeat a similar process to the release rename handling. Except: we can have
                # arbitrarily nested files here, so we need to compare more than the name.
                original_wanted_stem = Path(wanted_filename).stem
                original_wanted_suffix = Path(wanted_filename).suffix
                collision_no = 2
                while (relpath := str(track.source_path).removeprefix(f"{release.source_path}/")) and not _compare_strs(
                    wanted_filename, relpath
                ):
                    new_source_path = release.source_path / wanted_filename
                    if new_source_path.exists():
                        new_max_len = c.max_filename_bytes - (3 + len(str(collision_no)) + len(original_wanted_suffix))
                        wanted_filename = (
                            f"{original_wanted_stem[:new_max_len]} [{collision_no}]{original_wanted_suffix}"
                        )
                        collision_no += 1
                        continue
                    old_source_path = track.source_path
                    old_source_path.rename(new_source_path)
                    track.source_path = new_source_path
                    track.source_mtime = str(os.stat(track.source_path).st_mtime)
                    logger.info(
                        f"Renamed source file {release.source_path.name}/{relpath} to {release.source_path.name}/{wanted_filename}"
                    )
                    # And clean out any empty directories post-rename.
                    while relpath := os.path.dirname(relpath):
                        relppp = release.source_path / relpath
                        if not relppp.is_dir() or list(relppp.iterdir()):
                            break
                        relppp.rmdir()

        # Schedule database executions.
        if unknown_cached_tracks or release_dirty or track_ids_to_insert:
            logger.info(f"Updating cache for release {release.source_path.name}")

        if unknown_cached_tracks:
            logger.debug(f"Deleting {len(unknown_cached_tracks)} unknown tracks from cache")
            upd_unknown_cached_tracks_args.append((release.id, list(unknown_cached_tracks)))

        if release_dirty:
            logger.debug(f"Scheduling upsert for dirty release in database: {release.source_path}")
            upd_release_args.append([
                release.id,
                str(release.source_path),
                str(release.cover_image_path) if release.cover_image_path else None,
                release.added_at,
                release.datafile_mtime,
                release.releasetitle,
                release.releasetype,
                str(release.releasedate) if release.releasedate else None,
                str(release.originaldate) if release.originaldate else None,
                str(release.compositiondate) if release.compositiondate else None,
                release.edition,
                release.catalognumber,
                release.disctotal,
                release.new,
                sha256_dataclass(release),
            ])
            upd_release_ids.append(release.id)
            for pos, genre in enumerate(release.genres):
                upd_release_genre_args.append([release.id, genre, pos])
            for pos, genre in enumerate(release.secondary_genres):
                upd_release_secondary_genre_args.append([release.id, genre, pos])
            for pos, desc in enumerate(release.descriptors):
                upd_release_descriptor_args.append([release.id, desc, pos])
            for pos, label in enumerate(release.labels):
                upd_release_label_args.append([release.id, label, pos])
            pos = 0
            for role, artists in release.releaseartists.items():
                for art in artists:
                    upd_release_artist_args.append([release.id, art.name, role, pos])
                    pos += 1

        if track_ids_to_insert:
            for track in tracks:
                if track.id not in track_ids_to_insert:
                    continue
                logger.debug(f"Scheduling upsert for dirty track in database: {track.source_path}")
                upd_track_args.append([
                    track.id,
                    str(track.source_path),
                    track.source_mtime,
                    track.tracktitle,
                    track.release.id,
                    track.tracknumber,
                    track.tracktotal,
                    track.discnumber,
                    track.duration_seconds,
                    sha256_dataclass(track),
                ])
                upd_track_ids.append(track.id)
                pos = 0
                for role, artists in track.trackartists.items():
                    for art in artists:
                        upd_track_artist_args.append([track.id, art.name, role, pos])
                        pos += 1
    logger.debug(f"Release update scheduling loop time {time.time() - loop_start=}")

    exec_start = time.time()
    # During execution, identify the collages and playlists to update afterwards. We will invoke an
    # update for those collages and playlists with force=True after updating the release tables.
    update_collages = None
    update_playlists = None
    with connect(c) as conn:
        if upd_delete_source_paths:
            conn.execute(
                f"DELETE FROM releases WHERE source_path IN ({",".join(["?"] * len(upd_delete_source_paths))})",
                upd_delete_source_paths,
            )
        if upd_unknown_cached_tracks_args:
            query = "DELETE FROM tracks WHERE false"
            args: list[Any] = []
            for release_id, utrks in upd_unknown_cached_tracks_args:
                query += f" OR (release_id = ? AND source_path IN ({",".join(["?"] * len(utrks))}))"
                args.extend([release_id, *utrks])
            conn.execute(query, args)
        if upd_release_args:
            # The OR REPLACE handles source_path conflicts. The ON CONFLICT handles normal updates.
            conn.execute(
                f"""
                INSERT OR REPLACE INTO releases (
                    id
                  , source_path
                  , cover_image_path
                  , added_at
                  , datafile_mtime
                  , title
                  , releasetype
                  , releasedate
                  , originaldate
                  , compositiondate
                  , edition
                  , catalognumber
                  , disctotal
                  , new
                  , metahash
                ) VALUES {",".join(["(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"] * len(upd_release_args))}
                ON CONFLICT (id) DO UPDATE SET
                    source_path      = excluded.source_path
                  , cover_image_path = excluded.cover_image_path
                  , added_at         = excluded.added_at
                  , datafile_mtime   = excluded.datafile_mtime
                  , title            = excluded.title
                  , releasetype      = excluded.releasetype
                  , releasedate      = excluded.releasedate
                  , originaldate     = excluded.originaldate
                  , compositiondate  = excluded.compositiondate
                  , edition          = excluded.edition
                  , catalognumber    = excluded.catalognumber
                  , disctotal        = excluded.disctotal
                  , new              = excluded.new
                  , metahash         = excluded.metahash
               """,
                flatten(upd_release_args),
            )
            conn.execute(
                f"""
                DELETE FROM releases_genres
                WHERE release_id IN ({",".join(["?"] * len(upd_release_args))})
                """,
                [a[0] for a in upd_release_args],
            )
            if upd_release_genre_args:
                conn.execute(
                    f"""
                    INSERT INTO releases_genres (release_id, genre, position)
                    VALUES {",".join(["(?,?,?)"] * len(upd_release_genre_args))}
                    """,
                    flatten(upd_release_genre_args),
                )
            conn.execute(
                f"""
                DELETE FROM releases_secondary_genres
                WHERE release_id IN ({",".join(["?"] * len(upd_release_args))})
                """,
                [a[0] for a in upd_release_args],
            )
            if upd_release_secondary_genre_args:
                conn.execute(
                    f"""
                    INSERT INTO releases_secondary_genres (release_id, genre, position)
                    VALUES {",".join(["(?,?,?)"] * len(upd_release_secondary_genre_args))}
                    """,
                    flatten(upd_release_secondary_genre_args),
                )
            conn.execute(
                f"""
                DELETE FROM releases_descriptors
                WHERE release_id IN ({",".join(["?"] * len(upd_release_args))})
                """,
                [a[0] for a in upd_release_args],
            )
            if upd_release_descriptor_args:
                conn.execute(
                    f"""
                    INSERT INTO releases_descriptors (release_id, descriptor, position)
                    VALUES {",".join(["(?,?,?)"] * len(upd_release_descriptor_args))}
                    """,
                    flatten(upd_release_descriptor_args),
                )
            conn.execute(
                f"""
                DELETE FROM releases_labels
                WHERE release_id IN ({",".join(["?"] * len(upd_release_args))})
                """,
                [a[0] for a in upd_release_args],
            )
            if upd_release_label_args:
                conn.execute(
                    f"""
                    INSERT INTO releases_labels (release_id, label, position)
                    VALUES {",".join(["(?,?,?)"] * len(upd_release_label_args))}
                    """,
                    flatten(upd_release_label_args),
                )
            conn.execute(
                f"""
                DELETE FROM releases_artists
                WHERE release_id IN ({",".join(["?"] * len(upd_release_args))})
                """,
                [a[0] for a in upd_release_args],
            )
            if upd_release_artist_args:
                conn.execute(
                    f"""
                    INSERT INTO releases_artists (release_id, artist, role, position)
                    VALUES {",".join(["(?,?,?,?)"] * len(upd_release_artist_args))}
                    """,
                    flatten(upd_release_artist_args),
                )
        if upd_track_args:
            # The OR REPLACE handles source_path conflicts. The ON CONFLICT handles normal updates.
            conn.execute(
                f"""
                INSERT OR REPLACE INTO tracks (
                    id
                  , source_path
                  , source_mtime
                  , title
                  , release_id
                  , tracknumber
                  , tracktotal
                  , discnumber
                  , duration_seconds
                  , metahash
                )
                VALUES {",".join(["(?,?,?,?,?,?,?,?,?,?)"] * len(upd_track_args))}
                ON CONFLICT (id) DO UPDATE SET
                    source_path                = excluded.source_path
                  , source_mtime               = excluded.source_mtime
                  , title                      = excluded.title
                  , release_id                 = excluded.release_id
                  , tracknumber                = excluded.tracknumber
                  , tracktotal                 = excluded.tracktotal
                  , discnumber                 = excluded.discnumber
                  , duration_seconds           = excluded.duration_seconds
                  , metahash                   = excluded.metahash
                """,
                flatten(upd_track_args),
            )
        if upd_track_artist_args:
            conn.execute(
                f"""
                DELETE FROM tracks_artists
                WHERE track_id IN ({",".join(["?"] * len(upd_track_artist_args))})
                """,
                [a[0] for a in upd_track_artist_args],
            )
            conn.execute(
                f"""
                INSERT INTO tracks_artists (track_id, artist, role, position)
                VALUES {",".join(["(?,?,?,?)"] * len(upd_track_artist_args))}
                """,
                flatten(upd_track_artist_args),
            )
        # And update the full text search engine here for any tracks and releases that have been
        # affected. Note that we do not worry about cleaning out deleted releases and tracks from
        # the full text search engine, since we join against tracks at the use site, which filters
        # out deleted tracks/releases from the full text search engine. Furthermore, the cache is
        # full-nuked often enough that there should not be much space waste.
        if upd_release_ids or upd_track_ids:
            conn.execute(
                f"""
                DELETE FROM rules_engine_fts WHERE rowid IN (
                    SELECT t.rowid
                    FROM tracks t
                    JOIN releases r ON r.id = t.release_id
                    WHERE t.id IN ({",".join(["?"] * len(upd_track_ids))})
                       OR r.id IN ({",".join(["?"] * len(upd_release_ids))})
               )
                """,
                [*upd_track_ids, *upd_release_ids],
            )
            # That cool section breaker shuriken character is our multi-value delimiter and how we
            # force-match strict prefix/suffix.
            conn.create_function("process_string_for_fts", 1, process_string_for_fts)
            conn.execute(
                f"""
                INSERT INTO rules_engine_fts (
                    rowid
                  , tracktitle
                  , tracknumber
                  , tracktotal
                  , discnumber
                  , disctotal
                  , releasetitle
                  , releasedate
                  , originaldate
                  , compositiondate
                  , edition
                  , catalognumber
                  , releasetype
                  , genre
                  , secondarygenre
                  , descriptor
                  , label
                  , releaseartist
                  , trackartist
                  , new
                )
                SELECT
                    t.rowid
                  , process_string_for_fts(t.title) AS tracktitle
                  , process_string_for_fts(t.tracknumber) AS tracknumber
                  , process_string_for_fts(t.tracktotal) AS tracknumber
                  , process_string_for_fts(t.discnumber) AS discnumber
                  , process_string_for_fts(r.disctotal) AS discnumber
                  , process_string_for_fts(r.title) AS releasetitle
                  , process_string_for_fts(r.releasedate) AS releasedate
                  , process_string_for_fts(r.originaldate) AS originaldate
                  , process_string_for_fts(r.compositiondate) AS compositiondate
                  , process_string_for_fts(r.edition) AS edition
                  , process_string_for_fts(r.catalognumber) AS catalognumber
                  , process_string_for_fts(r.releasetype) AS releasetype
                  , process_string_for_fts(COALESCE(GROUP_CONCAT(rg.genre, ' '), '')) AS genre
                  , process_string_for_fts(COALESCE(GROUP_CONCAT(rs.genre, ' '), '')) AS secondarygenre
                  , process_string_for_fts(COALESCE(GROUP_CONCAT(rd.descriptor, ' '), '')) AS descriptor
                  , process_string_for_fts(COALESCE(GROUP_CONCAT(rl.label, ' '), '')) AS label
                  , process_string_for_fts(COALESCE(GROUP_CONCAT(ra.artist, ' '), '')) AS releaseartist
                  , process_string_for_fts(COALESCE(GROUP_CONCAT(ta.artist, ' '), '')) AS trackartist
                  , process_string_for_fts(CASE WHEN r.new THEN 'true' ELSE 'false' END) AS new
                FROM tracks t
                JOIN releases r ON r.id = t.release_id
                LEFT JOIN releases_genres rg ON rg.release_id = r.id
                LEFT JOIN releases_secondary_genres rs ON rs.release_id = r.id
                LEFT JOIN releases_descriptors rd ON rd.release_id = r.id
                LEFT JOIN releases_labels rl ON rl.release_id = r.id
                LEFT JOIN releases_artists ra ON ra.release_id = r.id
                LEFT JOIN tracks_artists ta ON ta.track_id = t.id
                WHERE t.id IN ({",".join(["?"] * len(upd_track_ids))})
                   OR r.id IN ({",".join(["?"] * len(upd_release_ids))})
                GROUP BY t.id
                """,
                [*upd_track_ids, *upd_release_ids],
            )

        # Schedule collage/playlist updates in order to update description_meta. We simply update
        # collages and playlists if any of their members have changed--we do not try to be precise
        # here, as the update is very cheap. The point here is to avoid running the collage/playlist
        # update in the No Op case, not to optimize the invalidation case.
        if upd_release_ids:
            cursor = conn.execute(
                f"""
                SELECT DISTINCT cr.collage_name
                FROM collages_releases cr
                JOIN releases r ON r.id = cr.release_id
                WHERE cr.release_id IN ({",".join(["?"] * len(upd_release_ids))})
                ORDER BY cr.collage_name
                """,
                upd_release_ids,
            )
            update_collages = [row["collage_name"] for row in cursor]
        if upd_track_ids:
            cursor = conn.execute(
                f"""
                SELECT DISTINCT pt.playlist_name
                FROM playlists_tracks pt
                JOIN tracks t ON t.id = pt.track_id
                WHERE pt.track_id IN ({",".join(["?"] * len(upd_track_ids))})
                ORDER BY pt.playlist_name
                """,
                upd_track_ids,
            )
            update_playlists = [row["playlist_name"] for row in cursor]

    if update_collages:
        if collages_to_force_update_receiver is not None:
            collages_to_force_update_receiver.extend(update_collages)
        else:
            update_cache_for_collages(c, update_collages, force=True)
    if update_playlists:
        if playlists_to_force_update_receiver is not None:
            playlists_to_force_update_receiver.extend(update_playlists)
        else:
            update_cache_for_playlists(c, update_playlists, force=True)

    logger.debug(f"Database execution loop time {time.time() - exec_start=}")


def update_cache_for_collages(
    c: Config,
    # Leave as None to update all collages.
    collage_names: list[str] | None = None,
    force: bool = False,
) -> None:
    """
    Update the read cache to match the data for all stored collages.

    This is performance-optimized in a similar way to the update releases function. We:

    1. Execute one big SQL query at the start to fetch the relevant previous caches.
    2. Skip reading a file's data if the mtime has not changed since the previous cache update.
    3. Only execute a SQLite upsert if the read data differ from the previous caches.

    However, we do not batch writes to the end of the function, nor do we process the collages in
    parallel. This is because we should have far fewer collages than releases.
    """
    collage_dir = c.music_source_dir / "!collages"
    collage_dir.mkdir(exist_ok=True)

    files: list[tuple[Path, str, os.DirEntry[str]]] = []
    for f in os.scandir(str(collage_dir)):
        path = Path(f.path)
        if path.suffix != ".toml":
            continue
        if not path.is_file():
            logger.debug(f"Skipping processing collage {path.name} because it is not a file")
            continue
        if collage_names is None or path.stem in collage_names:
            files.append((path.resolve(), path.stem, f))
    logger.debug(f"Refreshing the read cache for {len(files)} collages")

    cached_collages: dict[str, tuple[Collage, list[str]]] = {}
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT
                c.name
              , c.source_mtime
              , COALESCE(GROUP_CONCAT(cr.release_id, ' ¬ '), '') AS release_ids
            FROM collages c
            LEFT JOIN collages_releases cr ON cr.collage_name = c.name
            GROUP BY c.name
            """,
        )
        for row in cursor:
            cached_collages[row["name"]] = (
                Collage(
                    name=row["name"],
                    source_mtime=row["source_mtime"],
                ),
                _split(row["release_ids"]) if row["release_ids"] else [],
            )

        # We want to validate that all release IDs exist before we write them. In order to do that,
        # we need to know which releases exist.
        cursor = conn.execute("SELECT id FROM releases")
        existing_release_ids = {row["id"] for row in cursor}

    loop_start = time.time()
    with connect(c) as conn:
        for source_path, name, f in files:
            try:
                cached_collage, release_ids = cached_collages[name]
            except KeyError:
                logger.debug(f"First-time unidentified collage found at {source_path}")
                cached_collage = Collage(
                    name=name,
                    source_mtime="",
                )
                release_ids = []

            try:
                source_mtime = str(f.stat().st_mtime)
            except FileNotFoundError:
                # Collage was deleted... continue without doing anything. It will be cleaned up by
                # the eviction function.
                continue
            if source_mtime == cached_collage.source_mtime and not force:
                logger.debug(f"Collage cache hit (mtime) for {source_path}, reusing cached data")
                continue

            logger.debug(f"Collage cache miss (mtime) for {source_path}, reading data from disk")
            cached_collage.source_mtime = source_mtime

            with lock(c, collage_lock_name(name)):
                with source_path.open("rb") as fp:
                    data = tomllib.load(fp)
                original_releases = data.get("releases", [])
                releases = copy.deepcopy(original_releases)

                # Update the markings for releases that no longer exist. We will flag releases as
                # missing/not-missing here, so that if they are re-added (maybe it was a temporary
                # disappearance)? they are recovered in the collage.
                for rls in releases:
                    if not rls.get("missing", False) and rls["uuid"] not in existing_release_ids:
                        logger.warning(
                            f"Marking missing release {rls["description_meta"]} as missing in collage {cached_collage.name}"
                        )
                        rls["missing"] = True
                    elif rls.get("missing", False) and rls["uuid"] in existing_release_ids:
                        logger.info(
                            f"Missing release {rls["description_meta"]} in collage {cached_collage.name} found: removing missing flag"
                        )
                        del rls["missing"]

                release_ids = [r["uuid"] for r in releases]
                logger.debug(f"Found {len(release_ids)} release(s) (including missing) in {source_path}")

                # Update the description_metas.
                desc_map: dict[str, str] = {}
                cursor = conn.execute(
                    f"""
                    SELECT id, releasetitle, originaldate, releasedate, releaseartist_names, releaseartist_roles FROM releases_view
                    WHERE id IN ({",".join(["?"] * len(releases))})
                    """,
                    release_ids,
                )
                for row in cursor:
                    meta = (
                        f"[{releasedate}]"
                        if (releasedate := RoseDate.parse(row["originaldate"] or row["releasedate"]))
                        else "[0000-00-00]"
                    )
                    artists = _unpack_artists(c, row["releaseartist_names"], row["releaseartist_roles"])
                    meta += f" {artistsfmt(artists)} - "
                    meta += row["releasetitle"]
                    desc_map[row["id"]] = meta
                for i, rls in enumerate(releases):
                    with contextlib.suppress(KeyError):
                        releases[i]["description_meta"] = desc_map[rls["uuid"]]
                    if rls.get("missing", False) and not releases[i]["description_meta"].endswith(" {MISSING}"):
                        releases[i]["description_meta"] += " {MISSING}"

                # Update the collage on disk if we have changed information.
                if releases != original_releases:
                    logger.debug(f"Updating release descriptions for {cached_collage.name}")
                    data["releases"] = releases
                    with source_path.open("wb") as fp:
                        tomli_w.dump(data, fp)
                    cached_collage.source_mtime = str(os.stat(source_path).st_mtime)

                logger.info(f"Updating cache for collage {cached_collage.name}")
                conn.execute(
                    """
                    INSERT INTO collages (name, source_mtime) VALUES (?, ?)
                    ON CONFLICT (name) DO UPDATE SET source_mtime = excluded.source_mtime
                    """,
                    (cached_collage.name, cached_collage.source_mtime),
                )
                conn.execute(
                    "DELETE FROM collages_releases WHERE collage_name = ?",
                    (cached_collage.name,),
                )
                args: list[Any] = []
                for position, rls in enumerate(releases):
                    args.extend([cached_collage.name, rls["uuid"], position + 1, rls.get("missing", False)])
                if args:
                    conn.execute(
                        f"""
                        INSERT INTO collages_releases (collage_name, release_id, position, missing)
                        VALUES {",".join(["(?, ?, ?, ?)"] * len(releases))}
                        """,
                        args,
                    )

    logger.debug(f"Collage update loop time {time.time() - loop_start=}")


def update_cache_evict_nonexistent_collages(c: Config) -> None:
    logger.debug("Evicting cached collages that are not on disk")
    collage_names: list[str] = []
    for f in os.scandir(c.music_source_dir / "!collages"):
        p = Path(f.path)
        if p.is_file() and p.suffix == ".toml":
            collage_names.append(p.stem)

    with connect(c) as conn:
        cursor = conn.execute(
            f"""
            DELETE FROM collages
            WHERE name NOT IN ({",".join(["?"] * len(collage_names))})
            RETURNING name
            """,
            collage_names,
        )
        for row in cursor:
            logger.info(f"Evicted missing collage {row["name"]} from cache")


def update_cache_for_playlists(
    c: Config,
    # Leave as None to update all playlists.
    playlist_names: list[str] | None = None,
    force: bool = False,
) -> None:
    """
    Update the read cache to match the data for all stored playlists.

    This is performance-optimized in a similar way to the update releases function. We:

    1. Execute one big SQL query at the start to fetch the relevant previous caches.
    2. Skip reading a file's data if the mtime has not changed since the previous cache update.
    3. Only execute a SQLite upsert if the read data differ from the previous caches.

    However, we do not batch writes to the end of the function, nor do we process the playlists in
    parallel. This is because we should have far fewer playlists than releases.
    """
    playlist_dir = c.music_source_dir / "!playlists"
    playlist_dir.mkdir(exist_ok=True)

    files: list[tuple[Path, str, os.DirEntry[str]]] = []
    all_files_in_dir: list[Path] = []
    for f in os.scandir(str(playlist_dir)):
        path = Path(f.path)
        all_files_in_dir.append(path)
        if path.suffix != ".toml":
            continue
        if not path.is_file():
            logger.debug(f"Skipping processing playlist {path.name} because it is not a file")
            continue
        if playlist_names is None or path.stem in playlist_names:
            files.append((path.resolve(), path.stem, f))
    logger.debug(f"Refreshing the read cache for {len(files)} playlists")

    cached_playlists: dict[str, tuple[Playlist, list[str]]] = {}
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT
                p.name
              , p.source_mtime
              , p.cover_path
              , COALESCE(GROUP_CONCAT(pt.track_id, ' ¬ '), '') AS track_ids
            FROM playlists p
            LEFT JOIN playlists_tracks pt ON pt.playlist_name = p.name
            GROUP BY p.name
            """,
        )
        for row in cursor:
            cached_playlists[row["name"]] = (
                Playlist(
                    name=row["name"],
                    source_mtime=row["source_mtime"],
                    cover_path=Path(row["cover_path"]) if row["cover_path"] else None,
                ),
                _split(row["track_ids"]) if row["track_ids"] else [],
            )

        # We want to validate that all track IDs exist before we write them. In order to do that,
        # we need to know which tracks exist.
        cursor = conn.execute("SELECT id FROM tracks")
        existing_track_ids = {row["id"] for row in cursor}

    loop_start = time.time()
    with connect(c) as conn:
        for source_path, name, f in files:
            try:
                cached_playlist, track_ids = cached_playlists[name]
            except KeyError:
                logger.debug(f"First-time unidentified playlist found at {source_path}")
                cached_playlist = Playlist(
                    name=name,
                    source_mtime="",
                    cover_path=None,
                )
                track_ids = []

            # We do a quick scan for the playlist's cover art here. We always do this check, as it
            # amounts to ~4 getattrs. If a change is detected, we ignore the mtime optimization and
            # always update the database.
            dirty = False
            if cached_playlist.cover_path and not cached_playlist.cover_path.is_file():
                cached_playlist.cover_path = None
                dirty = True
            if not cached_playlist.cover_path:
                for potential_art_file in all_files_in_dir:
                    if (
                        potential_art_file.stem == name
                        and potential_art_file.suffix.lower().lstrip(".") in c.valid_art_exts
                    ):
                        cached_playlist.cover_path = potential_art_file.resolve()
                        dirty = True
                        break

            try:
                source_mtime = str(f.stat().st_mtime)
            except FileNotFoundError:
                # Playlist was deleted... continue without doing anything. It will be cleaned up by
                # the eviction function.
                continue
            if source_mtime == cached_playlist.source_mtime and not force and not dirty:
                logger.debug(f"playlist cache hit (mtime) for {source_path}, reusing cached data")
                continue

            logger.debug(f"playlist cache miss (mtime/{dirty=}) for {source_path}, reading data from disk")
            cached_playlist.source_mtime = source_mtime

            with lock(c, playlist_lock_name(name)):
                with source_path.open("rb") as fp:
                    data = tomllib.load(fp)
                original_tracks = data.get("tracks", [])
                tracks = copy.deepcopy(original_tracks)

                # Update the markings for tracks that no longer exist. We will flag tracks as
                # missing/not-missing here, so that if they are re-added (maybe it was a temporary
                # disappearance)? they are recovered in the playlist.
                for trk in tracks:
                    if not trk.get("missing", False) and trk["uuid"] not in existing_track_ids:
                        logger.warning(
                            f"Marking missing track {trk["description_meta"]} as missing in playlist {cached_playlist.name}"
                        )
                        trk["missing"] = True
                    elif trk.get("missing", False) and trk["uuid"] in existing_track_ids:
                        logger.info(
                            f"Missing trk {trk["description_meta"]} in playlist {cached_playlist.name} found: removing missing flag"
                        )
                        del trk["missing"]

                track_ids = [t["uuid"] for t in tracks]
                logger.debug(f"Found {len(track_ids)} track(s) (including missing) in {source_path}")

                # Update the description_metas.
                desc_map: dict[str, str] = {}
                cursor = conn.execute(
                    f"""
                    SELECT
                        t.id
                      , t.tracktitle
                      , t.source_path
                      , t.trackartist_names
                      , t.trackartist_roles
                      , r.originaldate
                      , r.releasedate
                    FROM tracks_view t
                    JOIN releases_view r ON r.id = t.release_id
                    WHERE t.id IN ({",".join(["?"] * len(tracks))})
                    """,
                    track_ids,
                )
                for row in cursor:
                    meta = (
                        f"[{releasedate}]"
                        if (releasedate := RoseDate.parse(row["originaldate"] or row["releasedate"]))
                        else "[0000-00-00]"
                    )
                    artists = _unpack_artists(c, row["trackartist_names"], row["trackartist_roles"])
                    meta += f" {artistsfmt(artists)} - {row["tracktitle"]}"
                    desc_map[row["id"]] = meta
                for trk in tracks:
                    with contextlib.suppress(KeyError):
                        trk["description_meta"] = desc_map[trk["uuid"]]
                    if trk.get("missing", False) and not trk["description_meta"].endswith(" {MISSING}"):
                        trk["description_meta"] += " {MISSING}"

                # Update the playlist on disk if we have changed information.
                if tracks != original_tracks:
                    logger.debug(f"Updating track descriptions for {cached_playlist.name}")
                    data["tracks"] = tracks
                    with source_path.open("wb") as fp:
                        tomli_w.dump(data, fp)
                    cached_playlist.source_mtime = str(os.stat(source_path).st_mtime)

                logger.info(f"Updating cache for playlist {cached_playlist.name}")
                conn.execute(
                    """
                    INSERT INTO playlists (name, source_mtime, cover_path) VALUES (?, ?, ?)
                    ON CONFLICT (name) DO UPDATE SET
                        source_mtime = excluded.source_mtime
                      , cover_path = excluded.cover_path
                    """,
                    (
                        cached_playlist.name,
                        cached_playlist.source_mtime,
                        str(cached_playlist.cover_path) if cached_playlist.cover_path else None,
                    ),
                )
                conn.execute(
                    "DELETE FROM playlists_tracks WHERE playlist_name = ?",
                    (cached_playlist.name,),
                )
                args: list[Any] = []
                for position, trk in enumerate(tracks):
                    args.extend([cached_playlist.name, trk["uuid"], position + 1, trk.get("missing", False)])
                if args:
                    conn.execute(
                        f"""
                        INSERT INTO playlists_tracks (playlist_name, track_id, position, missing)
                        VALUES {",".join(["(?, ?, ?, ?)"] * len(tracks))}
                        """,
                        args,
                    )

    logger.debug(f"playlist update loop time {time.time() - loop_start=}")


def update_cache_evict_nonexistent_playlists(c: Config) -> None:
    logger.debug("Evicting cached playlists that are not on disk")
    playlist_names: list[str] = []
    for f in os.scandir(c.music_source_dir / "!playlists"):
        p = Path(f.path)
        if p.is_file() and p.suffix == ".toml":
            playlist_names.append(p.stem)

    with connect(c) as conn:
        cursor = conn.execute(
            f"""
            DELETE FROM playlists
            WHERE name NOT IN ({",".join(["?"] * len(playlist_names))})
            RETURNING name
            """,
            playlist_names,
        )
        for row in cursor:
            logger.info(f"Evicted missing playlist {row["name"]} from cache")


def filter_releases(
    c: Config,
    *,
    release_artist_filter: str | None = None,
    all_artist_filter: str | None = None,
    genre_filter: str | None = None,
    descriptor_filter: str | None = None,
    label_filter: str | None = None,
    release_type_filter: str | None = None,
    new: bool | None = None,
    include_loose_tracks: bool = True,
) -> list[Release]:
    with connect(c) as conn:
        query = "SELECT * FROM releases_view rv WHERE 1=1"
        args: list[str | bool] = []
        if not include_loose_tracks:
            query += " AND rv.releasetype <> 'loosetrack'"
        if release_artist_filter:
            artists: list[str] = [release_artist_filter]
            for alias in _get_all_artist_aliases(c, release_artist_filter):
                artists.append(alias)
            query += f"""
                AND EXISTS (
                    SELECT * FROM releases_artists ra
                    WHERE ra.release_id = rv.id AND ra.artist IN ({",".join(["?"] * len(artists))})
                )
            """
            args.extend(artists)
        if all_artist_filter:
            artists = [all_artist_filter]
            for alias in _get_all_artist_aliases(c, all_artist_filter):
                artists.append(alias)
            query += f"""
                AND (
                    EXISTS (
                        SELECT * FROM releases_artists
                        WHERE release_id = id AND artist IN ({",".join(["?"] * len(artists))})
                    )
                    OR EXISTS (
                        SELECT * FROM releases_artists
                        WHERE release_id = id AND artist IN ({",".join(["?"] * len(artists))})
                    )
                )
            """
            args.extend(artists)
            args.extend(artists)
        if genre_filter:
            genres = [genre_filter]
            genres.extend(TRANSITIVE_CHILD_GENRES.get(genre_filter, []))
            query += f"""
                AND (
                    EXISTS (
                        SELECT * FROM releases_genres
                        WHERE release_id = id AND genre IN ({",".join(["?"] * len(genres))})
                    )
                    OR EXISTS (
                        SELECT * FROM releases_secondary_genres
                        WHERE release_id = id AND genre IN ({",".join(["?"] * len(genres))})
                    )
                )
            """
            args.extend(genres)
            args.extend(genres)
        if descriptor_filter:
            query += """
                AND EXISTS (
                    SELECT * FROM releases_descriptors
                    WHERE release_id = id AND descriptor = ?
                )
            """
            args.append(descriptor_filter)
        if label_filter:
            query += """
                AND EXISTS (
                    SELECT * FROM releases_labels
                    WHERE release_id = id AND label = ?
                )
            """
            args.append(label_filter)
        if release_type_filter:
            query += " AND rv.releasetype = ?"
            args.append(release_type_filter)
        if new is not None:
            query += " AND new = ?"
            args.append(new)
        query += " ORDER BY source_path"

        cursor = conn.execute(query, args)
        releases: list[Release] = []
        for row in cursor:
            releases.append(cached_release_from_view(c, row))
        return releases


def filter_tracks(
    c: Config,
    track_artist_filter: str | None = None,
    release_artist_filter: str | None = None,
    all_artist_filter: str | None = None,
    genre_filter: str | None = None,
    descriptor_filter: str | None = None,
    label_filter: str | None = None,
    new: bool | None = None,
) -> list[Track]:
    with connect(c) as conn:
        query = "SELECT * FROM tracks_view tv WHERE 1=1"
        args: list[str | bool] = []
        if track_artist_filter:
            artists: list[str] = [track_artist_filter]
            for alias in _get_all_artist_aliases(c, track_artist_filter):
                artists.append(alias)
            query += f"""
                AND EXISTS (
                    SELECT * FROM tracks_artists ta
                    WHERE ta.track_id = tv.id AND ta.artist IN ({",".join(["?"] * len(artists))})
                )
            """
            args.extend(artists)
        if release_artist_filter:
            artists = [release_artist_filter]
            for alias in _get_all_artist_aliases(c, release_artist_filter):
                artists.append(alias)
            query += f"""
                AND EXISTS (
                    SELECT * FROM releases_artists ra
                    WHERE ra.release_id = tv.id AND ra.artist IN ({",".join(["?"] * len(artists))})
                )
            """
            args.extend(artists)
        if all_artist_filter:
            artists = [all_artist_filter]
            for alias in _get_all_artist_aliases(c, all_artist_filter):
                artists.append(alias)
            query += f"""
                AND (
                    EXISTS (
                        SELECT * FROM tracks_artists ta
                        WHERE ta.track_id = tv.id AND ta.artist IN ({",".join(["?"] * len(artists))})
                    )
                    OR EXISTS (
                        SELECT * FROM releases_artists ra
                        WHERE ra.release_id = tv.release_id AND ra.artist IN ({",".join(["?"] * len(artists))})
                    )
                )
            """
            args.extend(artists)
            args.extend(artists)
        if genre_filter:
            genres = [genre_filter]
            genres.extend(TRANSITIVE_CHILD_GENRES.get(genre_filter, []))
            query += f"""
                AND (
                    EXISTS (
                        SELECT * FROM releases_genres rg
                        WHERE rg.release_id = tv.release_id AND rg.genre IN ({",".join(["?"] * len(genres))})
                    )
                    OR EXISTS (
                        SELECT * FROM releases_secondary_genres rsg
                        WHERE rsg.release_id = tv.release_id AND rsg.genre IN ({",".join(["?"] * len(genres))})
                    )
                )
            """
            args.extend(genres)
            args.extend(genres)
        if descriptor_filter:
            query += """
                AND EXISTS (
                    SELECT * FROM releases_descriptors rd
                    WHERE rd.release_id = tv.release_id AND rd.descriptor = ?
                )
            """
            args.append(descriptor_filter)
        if label_filter:
            query += """
                AND EXISTS (
                    SELECT * FROM releases_labels rl
                    WHERE rl.release_id = tv.release_id AND rl.label = ?
                )
            """
            args.append(label_filter)
        if new is not None:
            query += " AND new = ?"
            args.append(new)
        query += " ORDER BY source_path"

        cursor = conn.execute(query, args)
        trackrows = cursor.fetchall()

        release_ids = [r["release_id"] for r in trackrows]
        cursor = conn.execute(
            f"""
            SELECT *
            FROM releases_view
            WHERE id IN ({",".join(["?"] * len(release_ids))})
            """,
            release_ids,
        )
        releases_map: dict[str, Release] = {}
        for row in cursor:
            releases_map[row["id"]] = cached_release_from_view(c, row)

        rval = []
        for row in trackrows:
            rval.append(cached_track_from_view(c, row, releases_map[row["release_id"]]))
        return rval


def list_releases(
    c: Config,
    release_ids: list[str] | None = None,
    *,
    include_loose_tracks: bool = True,
) -> list[Release]:
    """Fetch data associated with given release IDs. Pass None to fetch all."""
    query = "SELECT * FROM releases_view WHERE 1=1"
    args = []
    if release_ids is not None:
        query += f" AND id IN ({",".join(["?"] * len(release_ids))})"
        args = release_ids
    if not include_loose_tracks:
        query += " AND releasetype <> 'loosetrack'"
    query += " ORDER BY source_path"
    with connect(c) as conn:
        cursor = conn.execute(query, args)
        releases: list[Release] = []
        for row in cursor:
            releases.append(cached_release_from_view(c, row))
        return releases


def get_release(c: Config, release_id: str) -> Release | None:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT * FROM releases_view WHERE id = ?",
            (release_id,),
        )
        row = cursor.fetchone()
        if not row:
            return None
        return cached_release_from_view(c, row)


def get_release_logtext(c: Config, release_id: str) -> str | None:
    """Get a human-readable identifier for a release suitable for logging."""
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT releasetitle, releasedate, releaseartist_names, releaseartist_roles FROM releases_view WHERE id = ?",
            (release_id,),
        )
        row = cursor.fetchone()
        if not row:
            return None
        return make_release_logtext(
            title=row["releasetitle"],
            releasedate=RoseDate.parse(row["releasedate"]),
            artists=_unpack_artists(c, row["releaseartist_names"], row["releaseartist_roles"]),
        )


def make_release_logtext(
    title: str,
    releasedate: RoseDate | None,
    artists: ArtistMapping,
) -> str:
    logtext = f"{artistsfmt(artists)} - "
    if releasedate:
        logtext += f"{releasedate.year}. "
    logtext += title
    return logtext


def list_tracks(c: Config, track_ids: list[str] | None = None) -> list[Track]:
    """Fetch data associated with given track IDs. Pass None to fetch all."""
    query = "SELECT * FROM tracks_view"
    args = []
    if track_ids is not None:
        query += f" WHERE id IN ({",".join(["?"] * len(track_ids))})"
        args = track_ids
    query += " ORDER BY source_path"
    with connect(c) as conn:
        cursor = conn.execute(query, args)
        trackrows = cursor.fetchall()

        release_ids = [r["release_id"] for r in trackrows]
        cursor = conn.execute(
            f"""
            SELECT *
            FROM releases_view
            WHERE id IN ({",".join(["?"] * len(release_ids))})
            """,
            release_ids,
        )
        releases_map: dict[str, Release] = {}
        for row in cursor:
            releases_map[row["id"]] = cached_release_from_view(c, row)

        rval = []
        for row in trackrows:
            rval.append(cached_track_from_view(c, row, releases_map[row["release_id"]]))
        return rval


def get_track(c: Config, uuid: str) -> Track | None:
    with connect(c) as conn:
        cursor = conn.execute("SELECT * FROM tracks_view WHERE id = ?", (uuid,))
        trackrow = cursor.fetchone()
        if not trackrow:
            return None
        cursor = conn.execute("SELECT * FROM releases_view WHERE id = ?", (trackrow["release_id"],))
        release = cached_release_from_view(c, cursor.fetchone())
        return cached_track_from_view(c, trackrow, release)


def get_tracks_of_release(
    c: Config,
    release: Release,
) -> list[Track]:
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT *
            FROM tracks_view
            WHERE release_id = ?
            ORDER BY release_id, FORMAT('%4d.%4d', discnumber, tracknumber)
            """,
            (release.id,),
        )
        rval = []
        for row in cursor:
            rval.append(cached_track_from_view(c, row, release))
        return rval


def get_tracks_of_releases(
    c: Config,
    releases: list[Release],
) -> list[tuple[Release, list[Track]]]:
    releases_map = {r.id: r for r in releases}
    tracks_map: dict[str, list[Track]] = defaultdict(list)
    with connect(c) as conn:
        cursor = conn.execute(
            f"""
            SELECT *
            FROM tracks_view
            WHERE release_id IN ({",".join(["?"] * len(releases))})
            ORDER BY release_id, FORMAT('%4d.%4d', discnumber, tracknumber)
            """,
            [r.id for r in releases],
        )
        for row in cursor:
            tracks_map[row["release_id"]].append(cached_track_from_view(c, row, releases_map[row["release_id"]]))

    rval = []
    for release in releases:
        tracks = tracks_map[release.id]
        rval.append((release, tracks))
    return rval


def track_within_release(
    c: Config,
    track_id: str,
    release_id: str,
) -> bool | None:
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT 1
            FROM tracks
            WHERE id = ? AND release_id = ?
            """,
            (track_id, release_id),
        )
        return bool(cursor.fetchone())


def track_within_playlist(
    c: Config,
    track_id: str,
    playlist_name: str,
) -> bool:
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT 1
            FROM tracks t
            JOIN playlists_tracks pt ON pt.track_id = t.id AND pt.playlist_name = ?
            WHERE t.id = ?
            """,
            (playlist_name, track_id),
        )
        return bool(cursor.fetchone())


def release_within_collage(
    c: Config,
    release_id: str,
    collage_name: str,
) -> bool:
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT 1
            FROM releases t
            JOIN collages_releases pt ON pt.release_id = t.id AND pt.collage_name = ?
            WHERE t.id = ?
            """,
            (collage_name, release_id),
        )
        return bool(cursor.fetchone())


def get_track_logtext(c: Config, track_id: str) -> str | None:
    """Get a human-readable identifier for a track suitable for logging."""
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT
                t.tracktitle
              , t.source_path
              , t.trackartist_names
              , t.trackartist_roles
              , r.releasedate
            FROM tracks_view t
            JOIN releases_view r ON r.id = t.release_id
            WHERE t.id = ?
            """,
            (track_id,),
        )
        row = cursor.fetchone()
        if not row:
            return None
        return make_track_logtext(
            title=row["tracktitle"],
            artists=_unpack_artists(c, row["trackartist_names"], row["trackartist_roles"]),
            releasedate=RoseDate.parse(row["releasedate"]),
            suffix=Path(row["source_path"]).suffix,
        )


def make_track_logtext(
    title: str,
    artists: ArtistMapping,
    releasedate: RoseDate | None,
    suffix: str,
) -> str:
    rval = f"{artistsfmt(artists)} - {title or "Unknown Title"}"
    if releasedate:
        rval += f" [{releasedate.year}]"
    rval += suffix
    return rval


def list_playlists(c: Config) -> list[str]:
    with connect(c) as conn:
        cursor = conn.execute("SELECT DISTINCT name FROM playlists")
        return [r["name"] for r in cursor]


def get_playlist(c: Config, playlist_name: str) -> Playlist | None:
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT
                name
              , source_mtime
              , cover_path
            FROM playlists
            WHERE name = ?
            """,
            (playlist_name,),
        )
        row = cursor.fetchone()
        if not row:
            return None
        return Playlist(
            name=row["name"],
            source_mtime=row["source_mtime"],
            cover_path=Path(row["cover_path"]) if row["cover_path"] else None,
        )


def get_playlist_tracks(c: Config, playlist_name: str) -> list[Track]:
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT t.*
            FROM tracks_view t
            JOIN playlists_tracks pt ON pt.track_id = t.id
            WHERE pt.playlist_name = ? AND NOT pt.missing
            ORDER BY pt.position ASC
            """,
            (playlist_name,),
        )
        trackrows = cursor.fetchall()

        release_ids = [r["release_id"] for r in trackrows]
        cursor = conn.execute(
            f"""
            SELECT *
            FROM releases_view
            WHERE id IN ({",".join(["?"] * len(release_ids))})
            """,
            release_ids,
        )
        releases_map: dict[str, Release] = {}
        for row in cursor:
            releases_map[row["id"]] = cached_release_from_view(c, row)

        tracks: list[Track] = []
        for row in trackrows:
            tracks.append(cached_track_from_view(c, row, releases_map[row["release_id"]]))

    return tracks


def list_collages(c: Config) -> list[str]:
    with connect(c) as conn:
        cursor = conn.execute("SELECT DISTINCT name FROM collages")
        return [r["name"] for r in cursor]


def get_collage(c: Config, collage_name: str) -> Collage | None:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT name, source_mtime FROM collages WHERE name = ?",
            (collage_name,),
        )
        row = cursor.fetchone()
        if not row:
            return None
        return Collage(
            name=row["name"],
            source_mtime=row["source_mtime"],
        )


def get_collage_releases(c: Config, collage_name: str) -> list[Release]:
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT r.*
            FROM releases_view r
            JOIN collages_releases cr ON cr.release_id = r.id
            WHERE cr.collage_name = ? AND NOT cr.missing
            ORDER BY cr.position ASC
            """,
            (collage_name,),
        )
        releases: list[Release] = []
        for row in cursor:
            releases.append(cached_release_from_view(c, row))

    return releases


def list_artists(c: Config) -> list[str]:
    with connect(c) as conn:
        cursor = conn.execute("SELECT DISTINCT artist FROM releases_artists")
        return [row["artist"] for row in cursor]


def artist_exists(c: Config, artist: str) -> bool:
    args: list[str] = [artist]
    for alias in _get_all_artist_aliases(c, artist):
        args.append(alias)
    with connect(c) as conn:
        cursor = conn.execute(
            f"""
            SELECT EXISTS(
                SELECT * FROM releases_artists
                WHERE artist IN ({",".join(["?"] * len(args))})
            )
            """,
            args,
        )
        return bool(cursor.fetchone()[0])


@dataclasses.dataclass(slots=True, frozen=True)
class GenreEntry:
    genre: str
    only_new_releases: bool


def list_genres(c: Config) -> list[GenreEntry]:
    with connect(c) as conn:
        query = """
            SELECT rg.genre, MIN(r.id) AS has_non_new_release
            FROM releases_genres rg
            LEFT JOIN releases r ON r.id = rg.release_id AND NOT r.new
            GROUP BY rg.genre
        """
        cursor = conn.execute(query)
        rval: dict[str, bool] = {}
        for row in cursor:
            rval[row["genre"]] = row["has_non_new_release"] is None
            for g in TRANSITIVE_PARENT_GENRES.get(row["genre"], []):
                # We are accumulating here whether any release of this genre is not-new. Thus, if a
                # past iteration had a not-new release, make sure the accumulator stays false. And
                # if we have a not-new release this time, set it false. Otherwise, keep it true.
                rval[g] = not (rval.get(g) is False or row["has_non_new_release"] is not None)
        return [GenreEntry(genre=k, only_new_releases=v) for k, v in rval.items()]


def genre_exists(c: Config, genre: str) -> bool:
    with connect(c) as conn:
        args = [genre]
        args.extend(TRANSITIVE_CHILD_GENRES.get(genre, []))
        cursor = conn.execute(
            f"SELECT EXISTS(SELECT * FROM releases_genres WHERE genre IN ({",".join(["?"] * len(args))}))",
            args,
        )
        return bool(cursor.fetchone()[0])


@dataclasses.dataclass(slots=True, frozen=True)
class DescriptorEntry:
    descriptor: str
    only_new_releases: bool


def list_descriptors(c: Config) -> list[DescriptorEntry]:
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT rd.descriptor, MIN(r.id) AS has_non_new_release
            FROM releases_descriptors rd
            LEFT JOIN releases r ON r.id = rd.release_id AND NOT r.new
            GROUP BY rd.descriptor
            """
        )
        return [
            DescriptorEntry(
                descriptor=row["descriptor"],
                only_new_releases=row["has_non_new_release"] is None,
            )
            for row in cursor
        ]


def descriptor_exists(c: Config, descriptor: str) -> bool:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT EXISTS(SELECT * FROM releases_descriptors WHERE descriptor = ?)",
            (descriptor,),
        )
        return bool(cursor.fetchone()[0])


@dataclasses.dataclass(slots=True, frozen=True)
class LabelEntry:
    label: str
    only_new_releases: bool


def list_labels(c: Config) -> list[LabelEntry]:
    with connect(c) as conn:
        cursor = conn.execute(
            """
            SELECT rl.label, MIN(r.id) AS has_non_new_release
            FROM releases_labels rl
            LEFT JOIN releases r ON r.id = rl.release_id AND NOT r.new
            GROUP BY rl.label
            """
        )
        return [LabelEntry(label=row["label"], only_new_releases=row["has_non_new_release"] is None) for row in cursor]


def label_exists(c: Config, label: str) -> bool:
    with connect(c) as conn:
        cursor = conn.execute(
            "SELECT EXISTS(SELECT * FROM releases_labels WHERE label = ?)",
            (label,),
        )
        return bool(cursor.fetchone()[0])


def _split(xs: str) -> list[str]:
    """Split the stringly-encoded arrays from the database by the sentinel character."""
    if not xs:
        return []
    return xs.split(" ¬ ")


def _unpack_artists(
    c: Config,
    names: str,
    roles: str,
    *,
    aliases: bool = True,
) -> ArtistMapping:
    mapping = ArtistMapping()
    seen: set[tuple[str, str]] = set()
    for name, role in _unpack(names, roles):
        role_artists: list[Artist] = getattr(mapping, role)
        role_artists.append(Artist(name=name, alias=False))
        seen.add((name, role))
        if not aliases:
            continue

        # Get all immediate and transitive artist aliases.
        unvisited: set[str] = {name}
        while unvisited:
            cur = unvisited.pop()
            for alias in c.artist_aliases_parents_map.get(cur, []):
                if (alias, role) not in seen:
                    role_artists.append(Artist(name=alias, alias=True))
                    seen.add((alias, role))
                    unvisited.add(alias)
    return mapping


def _get_all_artist_aliases(c: Config, x: str) -> list[str]:
    """Includes transitive aliases."""
    aliases: set[str] = set()
    unvisited: set[str] = {x}
    while unvisited:
        cur = unvisited.pop()
        if cur in aliases:
            continue
        aliases.add(cur)
        unvisited.update(c.artist_aliases_map.get(cur, []))
    return list(aliases)


def _get_parent_genres(genres: list[str]) -> list[str]:
    rval: set[str] = set()
    for g in genres:
        rval.update(TRANSITIVE_PARENT_GENRES.get(g, []))
    return sorted(rval)


def _unpack(*xxs: str) -> Iterator[tuple[str, ...]]:
    """
    Unpack an arbitrary number of strings, each of which is a " ¬ "-delimited list in actuality,
    but encoded as a string. This " ¬ "-delimited list-as-a-string is the convention we use to
    return arrayed data from a SQL query without introducing additional disk accesses.

    As a concrete example:

        >>> _unpack("Rose ¬ Lisa ¬ Jisoo ¬ Jennie", "vocal ¬ dance ¬ visual ¬ vocal")
        [("Rose", "vocal"), ("Lisa", "dance"), ("Jisoo", "visual"), ("Jennie", "vocal")]
    """
    # If the strings are empty, then split will resolve to `[""]`. But we don't want to loop over an
    # empty string, so we specially exit if we hit that case.
    if all(not xs for xs in xxs):
        return
    yield from zip(*[_split(xs) for xs in xxs], strict=False)


def process_string_for_fts(x: str) -> str:
    # In order to have performant substring search, we use FTS and hack it such that every character
    # is a token. We use "¬" as our separator character, hoping that it is not used in any metadata.
    return "¬".join(str(x)) if x else x


def _compare_strs(a: str, b: str) -> bool:
    """
    Unicode normalize strings before comparison; there can be comparison failures when a
    library is ported across operating systems otherwise.

    Use for guarding significant mutations (cache updates are insignificant).
    """
    return unicodedata.normalize("NFC", a) == unicodedata.normalize("NFC", b)

# TESTS

import dataclasses
import hashlib
import shutil
import time
import tomllib
from pathlib import Path

import pytest

from conftest import TEST_COLLAGE_1, TEST_PLAYLIST_1, TEST_RELEASE_1, TEST_RELEASE_2, TEST_RELEASE_3
from rose.audiotags import AudioTags, RoseDate
from rose.cache import (
    CACHE_SCHEMA_PATH,
    STORED_DATA_FILE_REGEX,
    Collage,
    DescriptorEntry,
    GenreEntry,
    LabelEntry,
    Playlist,
    Release,
    Track,
    _unpack,
    artist_exists,
    connect,
    descriptor_exists,
    genre_exists,
    get_collage,
    get_collage_releases,
    get_playlist,
    get_playlist_tracks,
    get_release,
    get_release_logtext,
    get_track,
    get_track_logtext,
    get_tracks_of_release,
    get_tracks_of_releases,
    label_exists,
    list_artists,
    list_collages,
    list_descriptors,
    list_genres,
    list_labels,
    list_playlists,
    list_releases,
    list_tracks,
    lock,
    maybe_invalidate_cache_database,
    release_within_collage,
    track_within_playlist,
    track_within_release,
    update_cache,
    update_cache_evict_nonexistent_releases,
    update_cache_for_releases,
)
from rose.common import VERSION, Artist, ArtistMapping
from rose.config import Config


def test_schema(config: Config) -> None:
    """Test that the schema successfully bootstraps."""
    with CACHE_SCHEMA_PATH.open("rb") as fp:
        schema_hash = hashlib.sha256(fp.read()).hexdigest()
    maybe_invalidate_cache_database(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT schema_hash, config_hash, version FROM _schema_hash")
        row = cursor.fetchone()
        assert row["schema_hash"] == schema_hash
        assert row["config_hash"] is not None
        assert row["version"] == VERSION


def test_migration(config: Config) -> None:
    """Test that "migrating" the database correctly migrates it."""
    config.cache_database_path.unlink()
    with connect(config) as conn:
        conn.execute(
            """
            CREATE TABLE _schema_hash (
                schema_hash TEXT
              , config_hash TEXT
              , version TEXT
              , PRIMARY KEY (schema_hash, config_hash, version)
            )
            """
        )
        conn.execute(
            """
            INSERT INTO _schema_hash (schema_hash, config_hash, version)
            VALUES ('haha', 'lala', 'blabla')
            """,
        )

    with CACHE_SCHEMA_PATH.open("rb") as fp:
        latest_schema_hash = hashlib.sha256(fp.read()).hexdigest()
    maybe_invalidate_cache_database(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT schema_hash, config_hash, version FROM _schema_hash")
        row = cursor.fetchone()
        assert row["schema_hash"] == latest_schema_hash
        assert row["config_hash"] is not None
        assert row["version"] == VERSION
        cursor = conn.execute("SELECT COUNT(*) FROM _schema_hash")
        assert cursor.fetchone()[0] == 1


def test_locks(config: Config) -> None:
    """Test that taking locks works. The times are a bit loose b/c GH Actions is slow."""
    lock_name = "lol"

    # Test that the locking and timeout work.
    start = time.time()
    with lock(config, lock_name, timeout=0.2):
        lock1_acq = time.time()
        with lock(config, lock_name, timeout=0.2):
            lock2_acq = time.time()
    # Assert that we had to wait ~0.1sec to get the second lock.
    assert lock1_acq - start < 0.08
    assert lock2_acq - lock1_acq > 0.17

    # Test that releasing a lock actually works.
    start = time.time()
    with lock(config, lock_name, timeout=0.2):
        lock1_acq = time.time()
    with lock(config, lock_name, timeout=0.2):
        lock2_acq = time.time()
    # Assert that we had to wait negligible time to get the second lock.
    assert lock1_acq - start < 0.08
    assert lock2_acq - lock1_acq < 0.08


def test_update_cache_all(config: Config) -> None:
    """Test that the update all function works."""
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)

    # Test that we prune deleted releases too.
    with connect(config) as conn:
        conn.execute(
            """
            INSERT INTO releases (id, source_path, added_at, datafile_mtime, title, releasetype, disctotal, metahash)
            VALUES ('aaaaaa', '0000-01-01T00:00:00+00:00', '999', 'nonexistent', 'aa', 'unknown', false, '0')
            """
        )

    update_cache(config)

    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 2
        cursor = conn.execute("SELECT COUNT(*) FROM tracks")
        assert cursor.fetchone()[0] == 4


def test_update_cache_multiprocessing(config: Config) -> None:
    """Test that the update all function works."""
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    update_cache_for_releases(config, force_multiprocessing=True)
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 2
        cursor = conn.execute("SELECT COUNT(*) FROM tracks")
        assert cursor.fetchone()[0] == 4


def test_update_cache_releases(config: Config) -> None:
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    update_cache_for_releases(config, [release_dir])

    # Check that the release directory was given a UUID.
    release_id: str | None = None
    for f in release_dir.iterdir():
        if m := STORED_DATA_FILE_REGEX.match(f.name):
            release_id = m[1]
    assert release_id is not None

    # Assert that the release metadata was read correctly.
    with connect(config) as conn:
        cursor = conn.execute(
            """
            SELECT id, source_path, title, releasetype, releasedate, compositiondate, catalognumber, new
            FROM releases WHERE id = ?
            """,
            (release_id,),
        )
        row = cursor.fetchone()
        assert row["source_path"] == str(release_dir)
        assert row["title"] == "I Love Blackpink"
        assert row["releasetype"] == "album"
        assert row["releasedate"] == "1990-02-05"
        assert row["compositiondate"] is None
        assert row["catalognumber"] is None
        assert row["new"]

        cursor = conn.execute(
            "SELECT genre FROM releases_genres WHERE release_id = ?",
            (release_id,),
        )
        genres = {r["genre"] for r in cursor.fetchall()}
        assert genres == {"K-Pop", "Pop"}

        cursor = conn.execute(
            "SELECT label FROM releases_labels WHERE release_id = ?",
            (release_id,),
        )
        labels = {r["label"] for r in cursor.fetchall()}
        assert labels == {"A Cool Label"}

        cursor = conn.execute(
            "SELECT artist, role FROM releases_artists WHERE release_id = ?",
            (release_id,),
        )
        artists = {(r["artist"], r["role"]) for r in cursor.fetchall()}
        assert artists == {
            ("BLACKPINK", "main"),
        }

        for f in release_dir.iterdir():
            if f.suffix != ".m4a":
                continue

            # Assert that the track metadata was read correctly.
            cursor = conn.execute(
                """
                SELECT
                    id, source_path, title, release_id, tracknumber, discnumber, duration_seconds
                FROM tracks WHERE source_path = ?
                """,
                (str(f),),
            )
            row = cursor.fetchone()
            track_id = row["id"]
            assert row["title"].startswith("Track")
            assert row["release_id"] == release_id
            assert row["tracknumber"] != ""
            assert row["discnumber"] == "1"
            assert row["duration_seconds"] == 2

            cursor = conn.execute(
                "SELECT artist, role FROM tracks_artists WHERE track_id = ?",
                (track_id,),
            )
            artists = {(r["artist"], r["role"]) for r in cursor.fetchall()}
            assert artists == {
                ("BLACKPINK", "main"),
            }


def test_update_cache_releases_uncached_with_existing_id(config: Config) -> None:
    """Test that IDs in filenames are read and preserved."""
    release_dir = config.music_source_dir / TEST_RELEASE_2.name
    shutil.copytree(TEST_RELEASE_2, release_dir)
    update_cache_for_releases(config, [release_dir])

    # Check that the release directory was given a UUID.
    release_id: str | None = None
    for f in release_dir.iterdir():
        if m := STORED_DATA_FILE_REGEX.match(f.name):
            release_id = m[1]
    assert release_id == "ilovecarly"  # Hardcoded ID for testing.


def test_update_cache_releases_preserves_track_ids_across_rebuilds(config: Config) -> None:
    """Test that track IDs are preserved across cache rebuilds."""
    release_dir = config.music_source_dir / TEST_RELEASE_3.name
    shutil.copytree(TEST_RELEASE_3, release_dir)
    update_cache_for_releases(config, [release_dir])
    with connect(config) as conn:
        cursor = conn.execute("SELECT id FROM tracks")
        first_track_ids = {r["id"] for r in cursor}

    # Nuke the database.
    config.cache_database_path.unlink()
    maybe_invalidate_cache_database(config)

    # Repeat cache population.
    update_cache_for_releases(config, [release_dir])
    with connect(config) as conn:
        cursor = conn.execute("SELECT id FROM tracks")
        second_track_ids = {r["id"] for r in cursor}

    # Assert IDs are equivalent.
    assert first_track_ids == second_track_ids


def test_update_cache_releases_writes_ids_to_tags(config: Config) -> None:
    """Test that track IDs and release IDs are written to files."""
    release_dir = config.music_source_dir / TEST_RELEASE_3.name
    shutil.copytree(TEST_RELEASE_3, release_dir)

    af = AudioTags.from_file(release_dir / "01.m4a")
    assert af.id is None
    assert af.release_id is None
    af = AudioTags.from_file(release_dir / "02.m4a")
    assert af.id is None
    assert af.release_id is None

    update_cache_for_releases(config, [release_dir])

    af = AudioTags.from_file(release_dir / "01.m4a")
    assert af.id is not None
    assert af.release_id is not None
    af = AudioTags.from_file(release_dir / "02.m4a")
    assert af.id is not None
    assert af.release_id is not None


def test_update_cache_releases_already_fully_cached(config: Config) -> None:
    """Test that a fully cached release No Ops when updated again."""
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    update_cache_for_releases(config, [release_dir])
    update_cache_for_releases(config, [release_dir])

    # Assert that the release metadata was read correctly.
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT id, source_path, title, releasetype, releasedate, new FROM releases",
        )
        row = cursor.fetchone()
        assert row["source_path"] == str(release_dir)
        assert row["title"] == "I Love Blackpink"
        assert row["releasetype"] == "album"
        assert row["releasedate"] == "1990-02-05"
        assert row["new"]


def test_update_cache_releases_to_empty_multi_value_tag(config: Config) -> None:
    """Test that 1:many relations are properly emptied when they are updated from something to nothing."""
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)

    update_cache(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM releases_labels)")
        assert cursor.fetchone()[0]

    for fn in ["01.m4a", "02.m4a"]:
        af = AudioTags.from_file(release_dir / fn)
        af.label = []
        af.flush(config)

    update_cache(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT EXISTS(SELECT * FROM releases_labels)")
        assert not cursor.fetchone()[0]


def test_update_cache_releases_disk_update_to_previously_cached(config: Config) -> None:
    """Test that a cached release is updated after a track updates."""
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    update_cache_for_releases(config, [release_dir])
    # I'm too lazy to mutagen update the files, so instead we're going to update the database. And
    # then touch a file to signify that "we modified it."
    with connect(config) as conn:
        conn.execute("UPDATE releases SET title = 'An Uncool Album'")
        (release_dir / "01.m4a").touch()
    update_cache_for_releases(config, [release_dir])

    # Assert that the release metadata was re-read and updated correctly.
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT id, source_path, title, releasetype, releasedate, new FROM releases",
        )
        row = cursor.fetchone()
        assert row["source_path"] == str(release_dir)
        assert row["title"] == "I Love Blackpink"
        assert row["releasetype"] == "album"
        assert row["releasedate"] == "1990-02-05"
        assert row["new"]


def test_update_cache_releases_disk_update_to_datafile(config: Config) -> None:
    """Test that a cached release is updated after a datafile updates."""
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    update_cache_for_releases(config, [release_dir])
    with connect(config) as conn:
        conn.execute("UPDATE releases SET datafile_mtime = '0' AND new = false")
    update_cache_for_releases(config, [release_dir])

    # Assert that the release metadata was re-read and updated correctly.
    with connect(config) as conn:
        cursor = conn.execute("SELECT new, added_at FROM releases")
        row = cursor.fetchone()
        assert row["new"]
        assert row["added_at"]


def test_update_cache_releases_disk_upgrade_old_datafile(config: Config) -> None:
    """Test that a legacy invalid datafile is upgraded on index."""
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    datafile = release_dir / ".rose.lalala.toml"
    datafile.touch()
    update_cache_for_releases(config, [release_dir])

    # Assert that the release metadata was re-read and updated correctly.
    with connect(config) as conn:
        cursor = conn.execute("SELECT id, new, added_at FROM releases")
        row = cursor.fetchone()
        assert row["id"] == "lalala"
        assert row["new"]
        assert row["added_at"]
    with datafile.open("r") as fp:
        data = fp.read()
        assert "new = true" in data
        assert "added_at = " in data


def test_update_cache_releases_source_path_renamed(config: Config) -> None:
    """Test that a cached release is updated after a directory rename."""
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    update_cache_for_releases(config, [release_dir])
    moved_release_dir = config.music_source_dir / "moved lol"
    release_dir.rename(moved_release_dir)
    update_cache_for_releases(config, [moved_release_dir])

    # Assert that the release metadata was re-read and updated correctly.
    with connect(config) as conn:
        cursor = conn.execute(
            "SELECT id, source_path, title, releasetype, releasedate, new FROM releases",
        )
        row = cursor.fetchone()
        assert row["source_path"] == str(moved_release_dir)
        assert row["title"] == "I Love Blackpink"
        assert row["releasetype"] == "album"
        assert row["releasedate"] == "1990-02-05"
        assert row["new"]


def test_update_cache_releases_delete_nonexistent(config: Config) -> None:
    """Test that deleted releases that are no longer on disk are cleared from cache."""
    with connect(config) as conn:
        conn.execute(
            """
            INSERT INTO releases (id, source_path, added_at, datafile_mtime, title, releasetype, disctotal, metahash)
            VALUES ('aaaaaa', '0000-01-01T00:00:00+00:00', '999', 'nonexistent', 'aa', 'unknown', false, '0')
            """
        )
    update_cache_evict_nonexistent_releases(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 0


def test_update_cache_releases_enforces_max_len(config: Config) -> None:
    """Test that an directory with no audio files is skipped."""
    config = dataclasses.replace(config, rename_source_files=True, max_filename_bytes=15)
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / "a")
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / "b")
    shutil.copy(TEST_RELEASE_1 / "01.m4a", config.music_source_dir / "b" / "03.m4a")
    update_cache_for_releases(config)
    assert set(config.music_source_dir.iterdir()) == {
        config.music_source_dir / "BLACKPINK - 199",
        config.music_source_dir / "BLACKPINK - [2]",
    }
    # Nondeterministic: Pick the one with the extra file.
    children_1 = set((config.music_source_dir / "BLACKPINK - 199").iterdir())
    children_2 = set((config.music_source_dir / "BLACKPINK - [2]").iterdir())
    files = children_1 if len(children_1) > len(children_2) else children_2
    release_dir = next(iter(files)).parent
    assert release_dir / "01. Track 1.m4a" in files
    assert release_dir / "01. Tra [2].m4a" in files


def test_update_cache_releases_skips_empty_directory(config: Config) -> None:
    """Test that an directory with no audio files is skipped."""
    rd = config.music_source_dir / "lalala"
    rd.mkdir()
    (rd / "ignoreme.file").touch()
    update_cache_for_releases(config, [rd])
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 0


def test_update_cache_releases_uncaches_empty_directory(config: Config) -> None:
    """Test that a previously-cached directory with no audio files now is cleared from cache."""
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    update_cache_for_releases(config, [release_dir])
    shutil.rmtree(release_dir)
    release_dir.mkdir()
    update_cache_for_releases(config, [release_dir])
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 0


def test_update_cache_releases_evicts_relations(config: Config) -> None:
    """
    Test that related entities (artist, genre, label) that have been removed from the tags are
    properly evicted from the cache on update.
    """
    release_dir = config.music_source_dir / TEST_RELEASE_2.name
    shutil.copytree(TEST_RELEASE_2, release_dir)
    # Initial cache population.
    update_cache_for_releases(config, [release_dir])
    # Pretend that we have more artists in the cache.
    with connect(config) as conn:
        conn.execute(
            """
            INSERT INTO releases_genres (release_id, genre, position)
            VALUES ('ilovecarly', 'lalala', 2)
            """,
        )
        conn.execute(
            """
            INSERT INTO releases_labels (release_id, label, position)
            VALUES ('ilovecarly', 'lalala', 1)
            """,
        )
        conn.execute(
            """
            INSERT INTO releases_artists (release_id, artist, role, position)
            VALUES ('ilovecarly', 'lalala', 'main', 1)
            """,
        )
        conn.execute(
            """
            INSERT INTO tracks_artists (track_id, artist, role, position)
            SELECT id, 'lalala', 'main', 1 FROM tracks
            """,
        )
    # Second cache refresh.
    update_cache_for_releases(config, [release_dir], force=True)
    # Assert that all of the above were evicted.
    with connect(config) as conn:
        cursor = conn.execute("SELECT EXISTS (SELECT * FROM releases_genres WHERE genre = 'lalala')")
        assert not cursor.fetchone()[0]
        cursor = conn.execute("SELECT EXISTS (SELECT * FROM releases_labels WHERE label = 'lalala')")
        assert not cursor.fetchone()[0]
        cursor = conn.execute("SELECT EXISTS (SELECT * FROM releases_artists WHERE artist = 'lalala')")
        assert not cursor.fetchone()[0]
        cursor = conn.execute("SELECT EXISTS (SELECT * FROM tracks_artists WHERE artist = 'lalala')")
        assert not cursor.fetchone()[0]


def test_update_cache_releases_ignores_directories(config: Config) -> None:
    """Test that the ignore_release_directories configuration value works."""
    config = dataclasses.replace(config, ignore_release_directories=["lalala"])
    release_dir = config.music_source_dir / "lalala"
    shutil.copytree(TEST_RELEASE_1, release_dir)

    # Test that both arg+no-arg ignore the directory.
    update_cache_for_releases(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 0

    update_cache_for_releases(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 0


def test_update_cache_releases_notices_deleted_track(config: Config) -> None:
    """Test that we notice when a track is deleted."""
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    update_cache(config)

    (release_dir / "02.m4a").unlink()
    update_cache(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM tracks")
        assert cursor.fetchone()[0] == 1


def test_update_cache_releases_ignores_partially_written_directory(config: Config) -> None:
    """Test that a partially-written cached release is ignored."""
    # 1. Write the directory and index it. This should give it IDs and shit.
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    update_cache(config)

    # 2. Move the directory and "remove" the ID file.
    renamed_release_dir = config.music_source_dir / "lalala"
    release_dir.rename(renamed_release_dir)
    datafile = next(f for f in renamed_release_dir.iterdir() if f.stem.startswith(".rose"))
    tmpfile = datafile.with_name("tmp")
    datafile.rename(tmpfile)

    # 3. Re-update cache. We should see an empty cache now.
    update_cache(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 0

    # 4. Put the datafile back. We should now see the release cache again properly.
    datafile.with_name("tmp").rename(datafile)
    update_cache(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 1

    # 5. Rename and remove the ID file again. We should see an empty cache again.
    release_dir = renamed_release_dir
    renamed_release_dir = config.music_source_dir / "bahaha"
    release_dir.rename(renamed_release_dir)
    next(f for f in renamed_release_dir.iterdir() if f.stem.startswith(".rose")).unlink()
    update_cache(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 0

    # 6. Run with force=True. This should index the directory and make a new .rose.toml file.
    update_cache(config, force=True)
    assert (renamed_release_dir / datafile.name).is_file()
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 1


def test_update_cache_rename_source_files(config: Config) -> None:
    """Test that we properly rename the source directory on cache update."""
    config = dataclasses.replace(config, rename_source_files=True)
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    (config.music_source_dir / TEST_RELEASE_1.name / "cover.jpg").touch()
    update_cache(config)

    expected_dir = config.music_source_dir / "BLACKPINK - 1990. I Love Blackpink [NEW]"
    assert expected_dir in list(config.music_source_dir.iterdir())

    files_in_dir = list(expected_dir.iterdir())
    assert expected_dir / "01. Track 1.m4a" in files_in_dir
    assert expected_dir / "02. Track 2.m4a" in files_in_dir

    with connect(config) as conn:
        cursor = conn.execute("SELECT source_path, cover_image_path FROM releases")
        row = cursor.fetchone()
        assert Path(row["source_path"]) == expected_dir
        assert Path(row["cover_image_path"]) == expected_dir / "cover.jpg"
        cursor = conn.execute("SELECT source_path FROM tracks")
        assert {Path(r[0]) for r in cursor} == {
            expected_dir / "01. Track 1.m4a",
            expected_dir / "02. Track 2.m4a",
        }


def test_update_cache_add_cover_art(config: Config) -> None:
    """
    Test that adding a cover art (i.e. modifying release w/out modifying tracks) does not affect
    the tracks.
    """
    config = dataclasses.replace(config, rename_source_files=True)
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    update_cache(config)
    expected_dir = config.music_source_dir / "BLACKPINK - 1990. I Love Blackpink [NEW]"

    (expected_dir / "cover.jpg").touch()
    update_cache(config)

    with connect(config) as conn:
        cursor = conn.execute("SELECT source_path, cover_image_path FROM releases")
        row = cursor.fetchone()
        assert Path(row["source_path"]) == expected_dir
        assert Path(row["cover_image_path"]) == expected_dir / "cover.jpg"
        cursor = conn.execute("SELECT source_path FROM tracks")
        assert {Path(r[0]) for r in cursor} == {
            expected_dir / "01. Track 1.m4a",
            expected_dir / "02. Track 2.m4a",
        }


def test_update_cache_rename_source_files_nested_file_directories(config: Config) -> None:
    """Test that we properly rename arbitrarily nested files and clean up the empty dirs."""
    config = dataclasses.replace(config, rename_source_files=True)
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    (config.music_source_dir / TEST_RELEASE_1.name / "lala").mkdir()
    (config.music_source_dir / TEST_RELEASE_1.name / "01.m4a").rename(
        config.music_source_dir / TEST_RELEASE_1.name / "lala" / "1.m4a"
    )
    update_cache(config)

    expected_dir = config.music_source_dir / "BLACKPINK - 1990. I Love Blackpink [NEW]"
    assert expected_dir in list(config.music_source_dir.iterdir())

    files_in_dir = list(expected_dir.iterdir())
    assert expected_dir / "01. Track 1.m4a" in files_in_dir
    assert expected_dir / "02. Track 2.m4a" in files_in_dir
    assert expected_dir / "lala" not in files_in_dir

    with connect(config) as conn:
        cursor = conn.execute("SELECT source_path FROM releases")
        assert Path(cursor.fetchone()[0]) == expected_dir
        cursor = conn.execute("SELECT source_path FROM tracks")
        assert {Path(r[0]) for r in cursor} == {
            expected_dir / "01. Track 1.m4a",
            expected_dir / "02. Track 2.m4a",
        }


def test_update_cache_rename_source_files_collisions(config: Config) -> None:
    """Test that we properly rename arbitrarily nested files and clean up the empty dirs."""
    config = dataclasses.replace(config, rename_source_files=True)
    # Three copies of the same directory, and two instances of Track 1.
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    shutil.copyfile(
        config.music_source_dir / TEST_RELEASE_1.name / "01.m4a",
        config.music_source_dir / TEST_RELEASE_1.name / "haha.m4a",
    )
    shutil.copytree(config.music_source_dir / TEST_RELEASE_1.name, config.music_source_dir / "Number 2")
    shutil.copytree(config.music_source_dir / TEST_RELEASE_1.name, config.music_source_dir / "Number 3")
    update_cache(config)

    release_dirs = list(config.music_source_dir.iterdir())
    for expected_dir in [
        config.music_source_dir / "BLACKPINK - 1990. I Love Blackpink [NEW]",
        config.music_source_dir / "BLACKPINK - 1990. I Love Blackpink [NEW] [2]",
        config.music_source_dir / "BLACKPINK - 1990. I Love Blackpink [NEW] [3]",
    ]:
        assert expected_dir in release_dirs

        files_in_dir = list(expected_dir.iterdir())
        assert expected_dir / "01. Track 1.m4a" in files_in_dir
        assert expected_dir / "01. Track 1 [2].m4a" in files_in_dir
        assert expected_dir / "02. Track 2.m4a" in files_in_dir

        with connect(config) as conn:
            cursor = conn.execute("SELECT id FROM releases WHERE source_path = ?", (str(expected_dir),))
            release_id = cursor.fetchone()[0]
            assert release_id
            cursor = conn.execute("SELECT source_path FROM tracks WHERE release_id = ?", (release_id,))
            assert {Path(r[0]) for r in cursor} == {
                expected_dir / "01. Track 1.m4a",
                expected_dir / "01. Track 1 [2].m4a",
                expected_dir / "02. Track 2.m4a",
            }


def test_update_cache_releases_updates_full_text_search(config: Config) -> None:
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)

    update_cache_for_releases(config, [release_dir])
    with connect(config) as conn:
        cursor = conn.execute(
            """
            SELECT rowid, * FROM rules_engine_fts
            """
        )
        cursor = conn.execute(
            """
            SELECT rowid, * FROM tracks
            """
        )
    with connect(config) as conn:
        cursor = conn.execute(
            """
            SELECT t.source_path
            FROM rules_engine_fts s
            JOIN tracks t ON t.rowid = s.rowid
            WHERE s.tracktitle MATCH 'r a c k'
            """
        )
        fnames = {Path(r["source_path"]) for r in cursor}
        assert fnames == {
            release_dir / "01.m4a",
            release_dir / "02.m4a",
        }

    # And then test the DELETE+INSERT behavior. And that the query still works.
    update_cache_for_releases(config, [release_dir], force=True)
    with connect(config) as conn:
        cursor = conn.execute(
            """
            SELECT t.source_path
            FROM rules_engine_fts s
            JOIN tracks t ON t.rowid = s.rowid
            WHERE s.tracktitle MATCH 'r a c k'
            """
        )
        fnames = {Path(r["source_path"]) for r in cursor}
        assert fnames == {
            release_dir / "01.m4a",
            release_dir / "02.m4a",
        }


def test_update_cache_releases_new_directory_same_path(config: Config) -> None:
    """If a previous release is replaced by a new release with the same path, avoid a source_path unique conflict."""
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    update_cache(config)
    shutil.rmtree(release_dir)
    shutil.copytree(TEST_RELEASE_2, release_dir)
    # Should not error.
    update_cache(config)


def test_update_cache_collages(config: Config) -> None:
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
    update_cache(config)

    # Assert that the collage metadata was read correctly.
    with connect(config) as conn:
        cursor = conn.execute("SELECT name, source_mtime FROM collages")
        rows = cursor.fetchall()
        assert len(rows) == 1
        row = rows[0]
        assert row["name"] == "Rose Gold"
        assert row["source_mtime"]

        cursor = conn.execute("SELECT collage_name, release_id, position FROM collages_releases WHERE NOT missing")
        rows = cursor.fetchall()
        assert len(rows) == 1
        row = rows[0]
        assert row["collage_name"] == "Rose Gold"
        assert row["release_id"] == "ilovecarly"
        assert row["position"] == 1


def test_update_cache_collages_missing_release_id(config: Config) -> None:
    shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
    update_cache(config)

    # Assert that the releases in the collage were read as missing.
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM collages_releases WHERE missing")
        assert cursor.fetchone()[0] == 2
    # Assert that source file was updated to set the releases missing.
    with (config.music_source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
        data = tomllib.load(fp)
    assert len(data["releases"]) == 2
    assert len([r for r in data["releases"] if r["missing"]]) == 2

    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_RELEASE_3, config.music_source_dir / TEST_RELEASE_3.name)
    update_cache(config)

    # Assert that the releases in the collage were unflagged as missing.
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM collages_releases WHERE NOT missing")
        assert cursor.fetchone()[0] == 2
    # Assert that source file was updated to remove the missing flag.
    with (config.music_source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
        data = tomllib.load(fp)
    assert len([r for r in data["releases"] if "missing" not in r]) == 2


def test_update_cache_collages_missing_release_id_multiprocessing(config: Config) -> None:
    shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
    update_cache(config)

    # Assert that the releases in the collage were read as missing.
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM collages_releases WHERE missing")
        assert cursor.fetchone()[0] == 2
    # Assert that source file was updated to set the releases missing.
    with (config.music_source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
        data = tomllib.load(fp)
    assert len(data["releases"]) == 2
    assert len([r for r in data["releases"] if r["missing"]]) == 2

    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_RELEASE_3, config.music_source_dir / TEST_RELEASE_3.name)
    update_cache(config, force_multiprocessing=True)

    # Assert that the releases in the collage were unflagged as missing.
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM collages_releases WHERE NOT missing")
        assert cursor.fetchone()[0] == 2
    # Assert that source file was updated to remove the missing flag.
    with (config.music_source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
        data = tomllib.load(fp)
    assert len([r for r in data["releases"] if "missing" not in r]) == 2


def test_update_cache_collages_on_release_rename(config: Config) -> None:
    """
    Test that a renamed release source directory does not remove the release from any collages. This
    can occur because the rename operation is executed in SQL as release deletion followed by
    release creation.
    """
    shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_RELEASE_3, config.music_source_dir / TEST_RELEASE_3.name)
    update_cache(config)

    (config.music_source_dir / TEST_RELEASE_2.name).rename(config.music_source_dir / "lalala")
    update_cache(config)

    with connect(config) as conn:
        cursor = conn.execute("SELECT collage_name, release_id, position FROM collages_releases")
        rows = [dict(r) for r in cursor]
        assert rows == [
            {"collage_name": "Rose Gold", "release_id": "ilovecarly", "position": 1},
            {"collage_name": "Rose Gold", "release_id": "ilovenewjeans", "position": 2},
        ]

    # Assert that source file was not updated to remove the release.
    with (config.music_source_dir / "!collages" / "Rose Gold.toml").open("rb") as fp:
        data = tomllib.load(fp)
    assert not [r for r in data["releases"] if "missing" in r]
    assert len(data["releases"]) == 2


def test_update_cache_playlists(config: Config) -> None:
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_PLAYLIST_1, config.music_source_dir / "!playlists")
    update_cache(config)

    # Assert that the playlist metadata was read correctly.
    with connect(config) as conn:
        cursor = conn.execute("SELECT name, source_mtime, cover_path FROM playlists")
        rows = cursor.fetchall()
        assert len(rows) == 1
        row = rows[0]
        assert row["name"] == "Lala Lisa"
        assert row["source_mtime"] is not None
        assert row["cover_path"] == str(config.music_source_dir / "!playlists" / "Lala Lisa.jpg")

        cursor = conn.execute("SELECT playlist_name, track_id, position FROM playlists_tracks ORDER BY position")
        assert [dict(r) for r in cursor] == [
            {"playlist_name": "Lala Lisa", "track_id": "iloveloona", "position": 1},
            {"playlist_name": "Lala Lisa", "track_id": "ilovetwice", "position": 2},
        ]


def test_update_cache_playlists_missing_track_id(config: Config) -> None:
    shutil.copytree(TEST_PLAYLIST_1, config.music_source_dir / "!playlists")
    update_cache(config)

    # Assert that the tracks in the playlist were read as missing.
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM playlists_tracks WHERE missing")
        assert cursor.fetchone()[0] == 2
    # Assert that source file was updated to set the tracks missing.
    with (config.music_source_dir / "!playlists" / "Lala Lisa.toml").open("rb") as fp:
        data = tomllib.load(fp)
    assert len(data["tracks"]) == 2
    assert len([r for r in data["tracks"] if r["missing"]]) == 2

    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    update_cache(config)

    # Assert that the tracks in the playlist were unflagged as missing.
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM playlists_tracks WHERE NOT missing")
        assert cursor.fetchone()[0] == 2
    # Assert that source file was updated to remove the missing flag.
    with (config.music_source_dir / "!playlists" / "Lala Lisa.toml").open("rb") as fp:
        data = tomllib.load(fp)
    assert len([r for r in data["tracks"] if "missing" not in r]) == 2


@pytest.mark.parametrize("multiprocessing", [True, False])
def test_update_releases_updates_collages_description_meta(config: Config, multiprocessing: bool) -> None:
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_RELEASE_3, config.music_source_dir / TEST_RELEASE_3.name)
    shutil.copytree(TEST_COLLAGE_1, config.music_source_dir / "!collages")
    cpath = config.music_source_dir / "!collages" / "Rose Gold.toml"

    # First cache update: releases are inserted, collage is new. This should update the collage
    # TOML.
    update_cache(config)
    with cpath.open("r") as fp:
        cfg = fp.read()
        assert (
            cfg
            == """\
releases = [
    { uuid = "ilovecarly", description_meta = "[1990-02-05] Carly Rae Jepsen - I Love Carly" },
    { uuid = "ilovenewjeans", description_meta = "[1990-02-05] NewJeans - I Love NewJeans" },
]
"""
        )

    # Now prep for the second update. Reset the TOML to have garbage again, and update the database
    # such that the virtual dirnames are also incorrect.
    with cpath.open("w") as fp:
        fp.write(
            """\
[[releases]]
uuid = "ilovecarly"
description_meta = "lalala"
[[releases]]
uuid = "ilovenewjeans"
description_meta = "hahaha"
"""
        )

    # Second cache update: releases exist, collages exist, release is "updated." This should also
    # trigger a metadata update.
    update_cache_for_releases(config, force=True, force_multiprocessing=multiprocessing)
    with cpath.open("r") as fp:
        cfg = fp.read()
        assert (
            cfg
            == """\
releases = [
    { uuid = "ilovecarly", description_meta = "[1990-02-05] Carly Rae Jepsen - I Love Carly" },
    { uuid = "ilovenewjeans", description_meta = "[1990-02-05] NewJeans - I Love NewJeans" },
]
"""
        )


@pytest.mark.parametrize("multiprocessing", [True, False])
def test_update_tracks_updates_playlists_description_meta(config: Config, multiprocessing: bool) -> None:
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    shutil.copytree(TEST_PLAYLIST_1, config.music_source_dir / "!playlists")
    ppath = config.music_source_dir / "!playlists" / "Lala Lisa.toml"

    # First cache update: tracks are inserted, playlist is new. This should update the playlist
    # TOML.
    update_cache(config)
    with ppath.open("r") as fp:
        cfg = fp.read()
        assert (
            cfg
            == """\
tracks = [
    { uuid = "iloveloona", description_meta = "[1990-02-05] Carly Rae Jepsen - Track 1" },
    { uuid = "ilovetwice", description_meta = "[1990-02-05] Carly Rae Jepsen - Track 2" },
]
"""
        )

    # Now prep for the second update. Reset the TOML to have garbage again, and update the database
    # such that the virtual filenames are also incorrect.
    with ppath.open("w") as fp:
        fp.write(
            """\
[[tracks]]
uuid = "iloveloona"
description_meta = "lalala"
[[tracks]]
uuid = "ilovetwice"
description_meta = "hahaha"
"""
        )

    # Second cache update: tracks exist, playlists exist, track is "updated." This should also
    # trigger a metadata update.
    update_cache_for_releases(config, force=True, force_multiprocessing=multiprocessing)
    with ppath.open("r") as fp:
        cfg = fp.read()
        assert (
            cfg
            == """\
tracks = [
    { uuid = "iloveloona", description_meta = "[1990-02-05] Carly Rae Jepsen - Track 1" },
    { uuid = "ilovetwice", description_meta = "[1990-02-05] Carly Rae Jepsen - Track 2" },
]
"""
        )


def test_update_cache_playlists_on_release_rename(config: Config) -> None:
    """
    Test that a renamed release source directory does not remove any of its tracks any playlists.
    This can occur because when a release is renamed, we remove all tracks from the database and
    then reinsert them.
    """
    shutil.copytree(TEST_PLAYLIST_1, config.music_source_dir / "!playlists")
    shutil.copytree(TEST_RELEASE_2, config.music_source_dir / TEST_RELEASE_2.name)
    update_cache(config)

    (config.music_source_dir / TEST_RELEASE_2.name).rename(config.music_source_dir / "lalala")
    update_cache(config)

    with connect(config) as conn:
        cursor = conn.execute("SELECT playlist_name, track_id, position FROM playlists_tracks")
        rows = [dict(r) for r in cursor]
        assert rows == [
            {"playlist_name": "Lala Lisa", "track_id": "iloveloona", "position": 1},
            {"playlist_name": "Lala Lisa", "track_id": "ilovetwice", "position": 2},
        ]

    # Assert that source file was not updated to remove the track.
    with (config.music_source_dir / "!playlists" / "Lala Lisa.toml").open("rb") as fp:
        data = tomllib.load(fp)
    assert not [t for t in data["tracks"] if "missing" in t]
    assert len(data["tracks"]) == 2


@pytest.mark.usefixtures("seeded_cache")
def test_list_releases(config: Config) -> None:
    expected = [
        Release(
            datafile_mtime="999",
            id="r1",
            source_path=Path(config.music_source_dir / "r1"),
            cover_image_path=None,
            added_at="0000-01-01T00:00:00+00:00",
            releasetitle="Release 1",
            releasetype="album",
            compositiondate=None,
            catalognumber=None,
            releasedate=RoseDate(2023),
            disctotal=1,
            new=False,
            genres=["Techno", "Deep House"],
            parent_genres=[
                "Dance",
                "Electronic",
                "Electronic Dance Music",
                "House",
            ],
            originaldate=None,
            edition=None,
            secondary_genres=["Rominimal", "Ambient"],
            parent_secondary_genres=[
                "Dance",
                "Electronic",
                "Electronic Dance Music",
                "House",
                "Tech House",
            ],
            descriptors=["Warm", "Hot"],
            labels=["Silk Music"],
            releaseartists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
            metahash="1",
        ),
        Release(
            datafile_mtime="999",
            id="r2",
            source_path=Path(config.music_source_dir / "r2"),
            cover_image_path=Path(config.music_source_dir / "r2" / "cover.jpg"),
            added_at="0000-01-01T00:00:00+00:00",
            releasetitle="Release 2",
            releasetype="album",
            releasedate=RoseDate(2021),
            compositiondate=None,
            catalognumber="DG-001",
            disctotal=1,
            new=True,
            genres=["Modern Classical"],
            parent_genres=["Classical Music", "Western Classical Music"],
            labels=["Native State"],
            originaldate=RoseDate(2019),
            edition="Deluxe",
            secondary_genres=["Orchestral Music"],
            parent_secondary_genres=[
                "Classical Music",
                "Western Classical Music",
            ],
            descriptors=["Wet"],
            releaseartists=ArtistMapping(main=[Artist("Violin Woman")], guest=[Artist("Conductor Woman")]),
            metahash="2",
        ),
        Release(
            datafile_mtime="999",
            id="r3",
            source_path=Path(config.music_source_dir / "r3"),
            cover_image_path=None,
            added_at="0000-01-01T00:00:00+00:00",
            releasetitle="Release 3",
            releasetype="album",
            releasedate=RoseDate(2021, 4, 20),
            compositiondate=RoseDate(1780),
            catalognumber="DG-002",
            disctotal=1,
            new=False,
            genres=[],
            parent_genres=[],
            labels=[],
            originaldate=None,
            edition=None,
            secondary_genres=[],
            parent_secondary_genres=[],
            descriptors=[],
            releaseartists=ArtistMapping(),
            metahash="3",
        ),
        Release(
            datafile_mtime="999",
            id="r4",
            source_path=Path(config.music_source_dir / "r4"),
            cover_image_path=None,
            added_at="0000-01-01T00:00:00+00:00",
            releasetitle="Release 4",
            releasetype="loosetrack",
            releasedate=RoseDate(2021, 4, 20),
            compositiondate=RoseDate(1780),
            catalognumber="DG-002",
            disctotal=1,
            new=False,
            genres=[],
            parent_genres=[],
            labels=[],
            originaldate=None,
            edition=None,
            secondary_genres=[],
            parent_secondary_genres=[],
            descriptors=[],
            releaseartists=ArtistMapping(),
            metahash="4",
        ),
    ]

    assert list_releases(config) == expected
    assert list_releases(config, ["r1"]) == expected[:1]


@pytest.mark.usefixtures("seeded_cache")
def test_get_release_and_associated_tracks(config: Config) -> None:
    release = get_release(config, "r1")
    assert release is not None
    assert release == Release(
        datafile_mtime="999",
        id="r1",
        source_path=Path(config.music_source_dir / "r1"),
        cover_image_path=None,
        added_at="0000-01-01T00:00:00+00:00",
        releasetitle="Release 1",
        releasetype="album",
        releasedate=RoseDate(2023),
        compositiondate=None,
        catalognumber=None,
        disctotal=1,
        new=False,
        genres=["Techno", "Deep House"],
        parent_genres=[
            "Dance",
            "Electronic",
            "Electronic Dance Music",
            "House",
        ],
        labels=["Silk Music"],
        originaldate=None,
        edition=None,
        secondary_genres=["Rominimal", "Ambient"],
        parent_secondary_genres=[
            "Dance",
            "Electronic",
            "Electronic Dance Music",
            "House",
            "Tech House",
        ],
        descriptors=["Warm", "Hot"],
        releaseartists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
        metahash="1",
    )

    expected_tracks = [
        Track(
            id="t1",
            source_path=config.music_source_dir / "r1" / "01.m4a",
            source_mtime="999",
            tracktitle="Track 1",
            tracknumber="01",
            tracktotal=2,
            discnumber="01",
            duration_seconds=120,
            trackartists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
            metahash="1",
            release=release,
        ),
        Track(
            id="t2",
            source_path=config.music_source_dir / "r1" / "02.m4a",
            source_mtime="999",
            tracktitle="Track 2",
            tracknumber="02",
            tracktotal=2,
            discnumber="01",
            duration_seconds=240,
            trackartists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
            metahash="2",
            release=release,
        ),
    ]

    assert get_tracks_of_release(config, release) == expected_tracks
    assert get_tracks_of_releases(config, [release]) == [(release, expected_tracks)]


@pytest.mark.usefixtures("seeded_cache")
def test_get_release_applies_artist_aliases(config: Config) -> None:
    config = dataclasses.replace(
        config,
        artist_aliases_map={"Hype Boy": ["Bass Man"], "Bubble Gum": ["Hype Boy"]},
        artist_aliases_parents_map={"Bass Man": ["Hype Boy"], "Hype Boy": ["Bubble Gum"]},
    )
    release = get_release(config, "r1")
    assert release is not None
    assert release.releaseartists == ArtistMapping(
        main=[
            Artist("Techno Man"),
            Artist("Bass Man"),
            Artist("Hype Boy", True),
            Artist("Bubble Gum", True),
        ],
    )
    tracks = get_tracks_of_release(config, release)
    for t in tracks:
        assert t.trackartists == ArtistMapping(
            main=[
                Artist("Techno Man"),
                Artist("Bass Man"),
                Artist("Hype Boy", True),
                Artist("Bubble Gum", True),
            ],
        )


@pytest.mark.usefixtures("seeded_cache")
def test_get_release_logtext(config: Config) -> None:
    assert get_release_logtext(config, "r1") == "Techno Man & Bass Man - 2023. Release 1"


@pytest.mark.usefixtures("seeded_cache")
def test_list_tracks(config: Config) -> None:
    expected = [
        Track(
            id="t1",
            source_path=config.music_source_dir / "r1" / "01.m4a",
            source_mtime="999",
            tracktitle="Track 1",
            tracknumber="01",
            tracktotal=2,
            discnumber="01",
            duration_seconds=120,
            trackartists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
            metahash="1",
            release=Release(
                datafile_mtime="999",
                id="r1",
                source_path=Path(config.music_source_dir / "r1"),
                cover_image_path=None,
                added_at="0000-01-01T00:00:00+00:00",
                releasetitle="Release 1",
                releasetype="album",
                releasedate=RoseDate(2023),
                compositiondate=None,
                catalognumber=None,
                disctotal=1,
                new=False,
                genres=["Techno", "Deep House"],
                parent_genres=[
                    "Dance",
                    "Electronic",
                    "Electronic Dance Music",
                    "House",
                ],
                labels=["Silk Music"],
                originaldate=None,
                edition=None,
                secondary_genres=["Rominimal", "Ambient"],
                parent_secondary_genres=[
                    "Dance",
                    "Electronic",
                    "Electronic Dance Music",
                    "House",
                    "Tech House",
                ],
                descriptors=["Warm", "Hot"],
                releaseartists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
                metahash="1",
            ),
        ),
        Track(
            id="t2",
            source_path=config.music_source_dir / "r1" / "02.m4a",
            source_mtime="999",
            tracktitle="Track 2",
            tracknumber="02",
            tracktotal=2,
            discnumber="01",
            duration_seconds=240,
            trackartists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
            metahash="2",
            release=Release(
                datafile_mtime="999",
                id="r1",
                source_path=Path(config.music_source_dir / "r1"),
                cover_image_path=None,
                added_at="0000-01-01T00:00:00+00:00",
                releasetitle="Release 1",
                releasetype="album",
                releasedate=RoseDate(2023),
                compositiondate=None,
                catalognumber=None,
                disctotal=1,
                new=False,
                genres=["Techno", "Deep House"],
                parent_genres=[
                    "Dance",
                    "Electronic",
                    "Electronic Dance Music",
                    "House",
                ],
                labels=["Silk Music"],
                originaldate=None,
                edition=None,
                secondary_genres=["Rominimal", "Ambient"],
                parent_secondary_genres=[
                    "Dance",
                    "Electronic",
                    "Electronic Dance Music",
                    "House",
                    "Tech House",
                ],
                descriptors=["Warm", "Hot"],
                releaseartists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
                metahash="1",
            ),
        ),
        Track(
            id="t3",
            source_path=config.music_source_dir / "r2" / "01.m4a",
            source_mtime="999",
            tracktitle="Track 1",
            tracknumber="01",
            tracktotal=1,
            discnumber="01",
            duration_seconds=120,
            trackartists=ArtistMapping(main=[Artist("Violin Woman")], guest=[Artist("Conductor Woman")]),
            metahash="3",
            release=Release(
                id="r2",
                source_path=config.music_source_dir / "r2",
                cover_image_path=config.music_source_dir / "r2" / "cover.jpg",
                added_at="0000-01-01T00:00:00+00:00",
                datafile_mtime="999",
                releasetitle="Release 2",
                releasetype="album",
                releasedate=RoseDate(2021),
                compositiondate=None,
                catalognumber="DG-001",
                new=True,
                disctotal=1,
                genres=["Modern Classical"],
                parent_genres=["Classical Music", "Western Classical Music"],
                labels=["Native State"],
                originaldate=RoseDate(2019),
                edition="Deluxe",
                secondary_genres=["Orchestral Music"],
                parent_secondary_genres=[
                    "Classical Music",
                    "Western Classical Music",
                ],
                descriptors=["Wet"],
                releaseartists=ArtistMapping(main=[Artist("Violin Woman")], guest=[Artist("Conductor Woman")]),
                metahash="2",
            ),
        ),
        Track(
            id="t4",
            source_path=config.music_source_dir / "r3" / "01.m4a",
            source_mtime="999",
            tracktitle="Track 1",
            tracknumber="01",
            tracktotal=1,
            discnumber="01",
            duration_seconds=120,
            trackartists=ArtistMapping(),
            metahash="4",
            release=Release(
                id="r3",
                source_path=config.music_source_dir / "r3",
                cover_image_path=None,
                added_at="0000-01-01T00:00:00+00:00",
                datafile_mtime="999",
                releasetitle="Release 3",
                releasetype="album",
                releasedate=RoseDate(2021, 4, 20),
                compositiondate=RoseDate(1780),
                catalognumber="DG-002",
                new=False,
                disctotal=1,
                genres=[],
                parent_genres=[],
                labels=[],
                originaldate=None,
                edition=None,
                secondary_genres=[],
                parent_secondary_genres=[],
                descriptors=[],
                releaseartists=ArtistMapping(),
                metahash="3",
            ),
        ),
        Track(
            id="t5",
            source_path=config.music_source_dir / "r4" / "01.m4a",
            source_mtime="999",
            tracktitle="Track 1",
            tracknumber="01",
            tracktotal=1,
            discnumber="01",
            duration_seconds=120,
            trackartists=ArtistMapping(),
            metahash="5",
            release=Release(
                id="r4",
                source_path=config.music_source_dir / "r4",
                cover_image_path=None,
                added_at="0000-01-01T00:00:00+00:00",
                datafile_mtime="999",
                releasetitle="Release 4",
                releasetype="loosetrack",
                releasedate=RoseDate(2021, 4, 20),
                compositiondate=RoseDate(1780),
                catalognumber="DG-002",
                new=False,
                disctotal=1,
                genres=[],
                parent_genres=[],
                labels=[],
                originaldate=None,
                edition=None,
                secondary_genres=[],
                parent_secondary_genres=[],
                descriptors=[],
                releaseartists=ArtistMapping(),
                metahash="4",
            ),
        ),
    ]

    assert list_tracks(config) == expected
    assert list_tracks(config, ["t1", "t2"]) == expected[:2]


@pytest.mark.usefixtures("seeded_cache")
def test_get_track(config: Config) -> None:
    assert get_track(config, "t1") == Track(
        id="t1",
        source_path=config.music_source_dir / "r1" / "01.m4a",
        source_mtime="999",
        tracktitle="Track 1",
        tracknumber="01",
        tracktotal=2,
        discnumber="01",
        duration_seconds=120,
        trackartists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
        metahash="1",
        release=Release(
            datafile_mtime="999",
            id="r1",
            source_path=Path(config.music_source_dir / "r1"),
            cover_image_path=None,
            added_at="0000-01-01T00:00:00+00:00",
            releasetitle="Release 1",
            releasetype="album",
            releasedate=RoseDate(2023),
            compositiondate=None,
            catalognumber=None,
            disctotal=1,
            new=False,
            genres=["Techno", "Deep House"],
            parent_genres=[
                "Dance",
                "Electronic",
                "Electronic Dance Music",
                "House",
            ],
            labels=["Silk Music"],
            originaldate=None,
            edition=None,
            secondary_genres=["Rominimal", "Ambient"],
            parent_secondary_genres=[
                "Dance",
                "Electronic",
                "Electronic Dance Music",
                "House",
                "Tech House",
            ],
            descriptors=["Warm", "Hot"],
            releaseartists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
            metahash="1",
        ),
    )


@pytest.mark.usefixtures("seeded_cache")
def test_track_within_release(config: Config) -> None:
    assert track_within_release(config, "t1", "r1")
    assert not track_within_release(config, "t3", "r1")
    assert not track_within_release(config, "lalala", "r1")
    assert not track_within_release(config, "t1", "lalala")


@pytest.mark.usefixtures("seeded_cache")
def test_track_within_playlist(config: Config) -> None:
    assert track_within_playlist(config, "t1", "Lala Lisa")
    assert not track_within_playlist(config, "t2", "Lala Lisa")
    assert not track_within_playlist(config, "lalala", "Lala Lisa")
    assert not track_within_playlist(config, "t1", "lalala")


@pytest.mark.usefixtures("seeded_cache")
def test_release_within_collage(config: Config) -> None:
    assert release_within_collage(config, "r1", "Rose Gold")
    assert not release_within_collage(config, "r1", "Ruby Red")
    assert not release_within_collage(config, "lalala", "Rose Gold")
    assert not release_within_collage(config, "r1", "lalala")


@pytest.mark.usefixtures("seeded_cache")
def test_get_track_logtext(config: Config) -> None:
    assert get_track_logtext(config, "t1") == "Techno Man & Bass Man - Track 1 [2023].m4a"


@pytest.mark.usefixtures("seeded_cache")
def test_list_artists(config: Config) -> None:
    artists = list_artists(config)
    assert set(artists) == {
        "Techno Man",
        "Bass Man",
        "Violin Woman",
        "Conductor Woman",
    }


@pytest.mark.usefixtures("seeded_cache")
def test_list_genres(config: Config) -> None:
    # Test the accumulator too.
    with connect(config) as conn:
        conn.execute("INSERT INTO releases_genres (release_id, genre, position) VALUES ('r3', 'Classical Music', 1)")
    genres = list_genres(config)
    assert set(genres) == {
        GenreEntry("Techno", False),
        GenreEntry("Deep House", False),
        GenreEntry("Dance", False),
        GenreEntry("Electronic", False),
        GenreEntry("Electronic Dance Music", False),
        GenreEntry("House", False),
        GenreEntry("Modern Classical", True),
        GenreEntry("Western Classical Music", True),
        GenreEntry("Classical Music", False),  # Final parent genre has not-new r3.
    }


@pytest.mark.usefixtures("seeded_cache")
def test_list_descriptors(config: Config) -> None:
    descriptors = list_descriptors(config)
    assert set(descriptors) == {
        DescriptorEntry("Warm", False),
        DescriptorEntry("Hot", False),
        DescriptorEntry("Wet", True),
    }


@pytest.mark.usefixtures("seeded_cache")
def test_list_labels(config: Config) -> None:
    labels = list_labels(config)
    assert set(labels) == {
        LabelEntry("Silk Music", False),
        LabelEntry("Native State", True),
    }


@pytest.mark.usefixtures("seeded_cache")
def test_list_collages(config: Config) -> None:
    collages = list_collages(config)
    assert set(collages) == {"Rose Gold", "Ruby Red"}


@pytest.mark.usefixtures("seeded_cache")
def test_get_collage(config: Config) -> None:
    assert get_collage(config, "Rose Gold") == Collage(
        name="Rose Gold",
        source_mtime="999",
    )
    assert get_collage_releases(config, "Rose Gold") == [
        Release(
            id="r1",
            source_path=config.music_source_dir / "r1",
            cover_image_path=None,
            added_at="0000-01-01T00:00:00+00:00",
            datafile_mtime="999",
            releasetitle="Release 1",
            releasetype="album",
            releasedate=RoseDate(2023),
            compositiondate=None,
            catalognumber=None,
            new=False,
            disctotal=1,
            genres=["Techno", "Deep House"],
            parent_genres=[
                "Dance",
                "Electronic",
                "Electronic Dance Music",
                "House",
            ],
            labels=["Silk Music"],
            originaldate=None,
            edition=None,
            secondary_genres=["Rominimal", "Ambient"],
            parent_secondary_genres=[
                "Dance",
                "Electronic",
                "Electronic Dance Music",
                "House",
                "Tech House",
            ],
            descriptors=["Warm", "Hot"],
            releaseartists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
            metahash="1",
        ),
        Release(
            id="r2",
            source_path=config.music_source_dir / "r2",
            cover_image_path=config.music_source_dir / "r2" / "cover.jpg",
            added_at="0000-01-01T00:00:00+00:00",
            datafile_mtime="999",
            releasetitle="Release 2",
            releasetype="album",
            releasedate=RoseDate(2021),
            compositiondate=None,
            catalognumber="DG-001",
            new=True,
            disctotal=1,
            genres=["Modern Classical"],
            parent_genres=["Classical Music", "Western Classical Music"],
            labels=["Native State"],
            originaldate=RoseDate(2019),
            edition="Deluxe",
            secondary_genres=["Orchestral Music"],
            parent_secondary_genres=[
                "Classical Music",
                "Western Classical Music",
            ],
            descriptors=["Wet"],
            releaseartists=ArtistMapping(main=[Artist("Violin Woman")], guest=[Artist("Conductor Woman")]),
            metahash="2",
        ),
    ]

    assert get_collage(config, "Ruby Red") == Collage(
        name="Ruby Red",
        source_mtime="999",
    )
    assert get_collage_releases(config, "Ruby Red") == []


@pytest.mark.usefixtures("seeded_cache")
def test_list_playlists(config: Config) -> None:
    playlists = list_playlists(config)
    assert set(playlists) == {"Lala Lisa", "Turtle Rabbit"}


@pytest.mark.usefixtures("seeded_cache")
def test_get_playlist(config: Config) -> None:
    assert get_playlist(config, "Lala Lisa") == Playlist(
        name="Lala Lisa",
        source_mtime="999",
        cover_path=config.music_source_dir / "!playlists" / "Lala Lisa.jpg",
    )
    assert get_playlist_tracks(config, "Lala Lisa") == [
        Track(
            id="t1",
            source_path=config.music_source_dir / "r1" / "01.m4a",
            source_mtime="999",
            tracktitle="Track 1",
            tracknumber="01",
            tracktotal=2,
            discnumber="01",
            duration_seconds=120,
            trackartists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
            metahash="1",
            release=Release(
                datafile_mtime="999",
                id="r1",
                source_path=Path(config.music_source_dir / "r1"),
                cover_image_path=None,
                added_at="0000-01-01T00:00:00+00:00",
                releasetitle="Release 1",
                releasetype="album",
                releasedate=RoseDate(2023),
                compositiondate=None,
                catalognumber=None,
                disctotal=1,
                new=False,
                genres=["Techno", "Deep House"],
                parent_genres=[
                    "Dance",
                    "Electronic",
                    "Electronic Dance Music",
                    "House",
                ],
                labels=["Silk Music"],
                originaldate=None,
                edition=None,
                secondary_genres=["Rominimal", "Ambient"],
                parent_secondary_genres=[
                    "Dance",
                    "Electronic",
                    "Electronic Dance Music",
                    "House",
                    "Tech House",
                ],
                descriptors=["Warm", "Hot"],
                releaseartists=ArtistMapping(main=[Artist("Techno Man"), Artist("Bass Man")]),
                metahash="1",
            ),
        ),
        Track(
            id="t3",
            source_path=config.music_source_dir / "r2" / "01.m4a",
            source_mtime="999",
            tracktitle="Track 1",
            tracknumber="01",
            tracktotal=1,
            discnumber="01",
            duration_seconds=120,
            trackartists=ArtistMapping(main=[Artist("Violin Woman")], guest=[Artist("Conductor Woman")]),
            metahash="3",
            release=Release(
                id="r2",
                source_path=config.music_source_dir / "r2",
                cover_image_path=config.music_source_dir / "r2" / "cover.jpg",
                added_at="0000-01-01T00:00:00+00:00",
                datafile_mtime="999",
                releasetitle="Release 2",
                releasetype="album",
                releasedate=RoseDate(2021),
                compositiondate=None,
                catalognumber="DG-001",
                new=True,
                disctotal=1,
                genres=["Modern Classical"],
                parent_genres=["Classical Music", "Western Classical Music"],
                labels=["Native State"],
                originaldate=RoseDate(2019),
                edition="Deluxe",
                secondary_genres=["Orchestral Music"],
                parent_secondary_genres=[
                    "Classical Music",
                    "Western Classical Music",
                ],
                descriptors=["Wet"],
                releaseartists=ArtistMapping(main=[Artist("Violin Woman")], guest=[Artist("Conductor Woman")]),
                metahash="2",
            ),
        ),
    ]


@pytest.mark.usefixtures("seeded_cache")
def test_artist_exists(config: Config) -> None:
    assert artist_exists(config, "Bass Man")
    assert not artist_exists(config, "lalala")


@pytest.mark.usefixtures("seeded_cache")
def test_artist_exists_with_alias(config: Config) -> None:
    config = dataclasses.replace(
        config,
        artist_aliases_map={"Hype Boy": ["Bass Man"]},
        artist_aliases_parents_map={"Bass Man": ["Hype Boy"]},
    )
    assert artist_exists(config, "Hype Boy")


@pytest.mark.usefixtures("seeded_cache")
def test_artist_exists_with_alias_transient(config: Config) -> None:
    config = dataclasses.replace(
        config,
        artist_aliases_map={"Hype Boy": ["Bass Man"], "Bubble Gum": ["Hype Boy"]},
        artist_aliases_parents_map={"Bass Man": ["Hype Boy"], "Hype Boy": ["Bubble Gum"]},
    )
    assert artist_exists(config, "Bubble Gum")


@pytest.mark.usefixtures("seeded_cache")
def test_genre_exists(config: Config) -> None:
    assert genre_exists(config, "Deep House")
    assert not genre_exists(config, "lalala")
    # Parent genre
    assert genre_exists(config, "Electronic")
    # Child genre
    assert not genre_exists(config, "Lo-Fi House")


@pytest.mark.usefixtures("seeded_cache")
def test_descriptor_exists(config: Config) -> None:
    assert descriptor_exists(config, "Warm")
    assert not descriptor_exists(config, "Icy")


@pytest.mark.usefixtures("seeded_cache")
def test_label_exists(config: Config) -> None:
    assert label_exists(config, "Silk Music")
    assert not label_exists(config, "Cotton Music")


def test_unpack() -> None:
    i = _unpack("Rose ¬ Lisa ¬ Jisoo ¬ Jennie", r"vocal ¬ dance ¬ visual ¬ vocal")
    assert list(i) == [
        ("Rose", "vocal"),
        ("Lisa", "dance"),
        ("Jisoo", "visual"),
        ("Jennie", "vocal"),
    ]
    assert list(_unpack("", "")) == []
