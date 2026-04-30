//! Release-level CRUD operations: toggling new/favorite, setting ratings,
//! cover art management, the full interactive metadata editor (`edit_release`),
//! single release creation, and rule-based filtering/actions.
//!
//! This is the most complex CRUD module, containing the stateful `edit_release`
//! protocol with TOML null hacks, per-track dirty checking, and resume-on-failure.

use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::audiotags::{AudioTags, RoseDate};
use crate::cache::{
    artistsfmt, filter_releases, get_release, get_tracks_of_release, list_releases, lock,
    make_release_logtext, release_lock_name, update_cache_evict_nonexistent_releases,
    update_cache_for_collages, update_cache_for_playlists, update_cache_for_releases, Release,
    Track, STORED_DATA_FILE_REGEX,
};
use crate::common::{Artist, ArtistMapping, RoseError};
use crate::config::Config;
use crate::rule_parser::{Action, Matcher};
use crate::rules::{
    execute_metadata_actions, fast_search_for_matching_releases,
    filter_release_false_positives_using_read_cache,
};

// ---------------------------------------------------------------------------
// Error variants (mapped into RoseError)
// ---------------------------------------------------------------------------

fn release_not_found(release_id: &str) -> RoseError {
    RoseError::Internal(format!("Release {release_id} does not exist"))
}

fn invalid_cover_art(msg: String) -> RoseError {
    RoseError::Internal(msg)
}

fn release_edit_failed(msg: String) -> RoseError {
    RoseError::Internal(msg)
}

fn invalid_resume_file(msg: String) -> RoseError {
    RoseError::Internal(msg)
}

fn invalid_rating(msg: String) -> RoseError {
    RoseError::Internal(msg)
}

fn unknown_artist_role(msg: String) -> RoseError {
    RoseError::Internal(msg)
}

// ---------------------------------------------------------------------------
// Simple state operations
// ---------------------------------------------------------------------------

/// Delete a release by moving it to trash, then evict from cache.
pub fn delete_release(c: &Config, release_id: &str) -> Result<(), RoseError> {
    let release = get_release(c, release_id)?.ok_or_else(|| release_not_found(release_id))?;
    {
        let _lock = lock(c, &release_lock_name(release_id), 10.0)?;
        trash::delete(&release.source_path).map_err(|e| {
            RoseError::Internal(format!(
                "Failed to trash {}: {e}",
                release.source_path.display()
            ))
        })?;
    }
    let release_logtext = make_release_logtext(
        &release.releasetitle,
        release.releasedate.as_ref(),
        &release.releaseartists,
    );
    tracing::info!("Trashed release {release_logtext}");
    update_cache_evict_nonexistent_releases(c)?;
    update_cache_for_collages(c, None, true)?;
    update_cache_for_playlists(c, None, true)?;
    Ok(())
}

/// Toggle the `new` flag on a release's `.rose.{uuid}.toml` sidecar.
pub fn toggle_release_new(c: &Config, release_id: &str) -> Result<(), RoseError> {
    let release = get_release(c, release_id)?.ok_or_else(|| release_not_found(release_id))?;
    let release_logtext = make_release_logtext(
        &release.releasetitle,
        release.releasedate.as_ref(),
        &release.releaseartists,
    );

    for entry in std::fs::read_dir(&release.source_path).map_err(|e| {
        RoseError::Internal(format!(
            "Failed to read directory {}: {e}",
            release.source_path.display()
        ))
    })? {
        let entry =
            entry.map_err(|e| RoseError::Internal(format!("Failed to read dir entry: {e}")))?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !STORED_DATA_FILE_REGEX.is_match(&name_str) {
            continue;
        }
        let _lock = lock(c, &release_lock_name(release_id), 10.0)?;
        let content = std::fs::read_to_string(entry.path()).map_err(|e| {
            RoseError::Internal(format!("Failed to read {}: {e}", entry.path().display()))
        })?;
        let mut data: toml::Table = content.parse().map_err(|e| {
            RoseError::Internal(format!(
                "Failed to parse TOML {}: {e}",
                entry.path().display()
            ))
        })?;
        let current = data.get("new").and_then(|v| v.as_bool()).unwrap_or(true);
        data.insert("new".to_string(), toml::Value::Boolean(!current));
        let toml_str = toml::to_string_pretty(&data)
            .map_err(|e| RoseError::Internal(format!("Failed to serialize TOML: {e}")))?;
        std::fs::write(entry.path(), toml_str).map_err(|e| {
            RoseError::Internal(format!("Failed to write {}: {e}", entry.path().display()))
        })?;
        tracing::info!(
            "Toggled \"new\"-ness of release {release_logtext} to {}",
            !current
        );
        update_cache_for_releases(c, Some(vec![release.source_path.clone()]), true)?;
        return Ok(());
    }

    tracing::error!(
        "Failed to find .rose.toml in {}",
        release.source_path.display()
    );
    Ok(())
}

/// Toggle the `favorite` flag on a release's `.rose.{uuid}.toml` sidecar.
pub fn toggle_release_favorite(c: &Config, release_id: &str) -> Result<(), RoseError> {
    let release = get_release(c, release_id)?.ok_or_else(|| release_not_found(release_id))?;
    let release_logtext = make_release_logtext(
        &release.releasetitle,
        release.releasedate.as_ref(),
        &release.releaseartists,
    );

    for entry in std::fs::read_dir(&release.source_path).map_err(|e| {
        RoseError::Internal(format!(
            "Failed to read directory {}: {e}",
            release.source_path.display()
        ))
    })? {
        let entry =
            entry.map_err(|e| RoseError::Internal(format!("Failed to read dir entry: {e}")))?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !STORED_DATA_FILE_REGEX.is_match(&name_str) {
            continue;
        }
        let _lock = lock(c, &release_lock_name(release_id), 10.0)?;
        let content = std::fs::read_to_string(entry.path()).map_err(|e| {
            RoseError::Internal(format!("Failed to read {}: {e}", entry.path().display()))
        })?;
        let mut data: toml::Table = content.parse().map_err(|e| {
            RoseError::Internal(format!(
                "Failed to parse TOML {}: {e}",
                entry.path().display()
            ))
        })?;
        let current = data
            .get("favorite")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        data.insert("favorite".to_string(), toml::Value::Boolean(!current));
        let toml_str = toml::to_string_pretty(&data)
            .map_err(|e| RoseError::Internal(format!("Failed to serialize TOML: {e}")))?;
        std::fs::write(entry.path(), toml_str).map_err(|e| {
            RoseError::Internal(format!("Failed to write {}: {e}", entry.path().display()))
        })?;
        tracing::info!(
            "Toggled \"favorite\"-ness of release {release_logtext} to {}",
            !current
        );
        update_cache_for_releases(c, Some(vec![release.source_path.clone()]), true)?;
        return Ok(());
    }

    tracing::error!(
        "Failed to find .rose.toml in {}",
        release.source_path.display()
    );
    Ok(())
}

/// Set or clear the rating on a release's `.rose.{uuid}.toml` sidecar.
pub fn set_release_rating(
    c: &Config,
    release_id: &str,
    rating: Option<u8>,
) -> Result<(), RoseError> {
    if let Some(r) = rating {
        if !(1..=100).contains(&r) {
            return Err(invalid_rating(format!(
                "Rating must be between 1 and 100, got {r}"
            )));
        }
    }

    let release = get_release(c, release_id)?.ok_or_else(|| release_not_found(release_id))?;
    let release_logtext = make_release_logtext(
        &release.releasetitle,
        release.releasedate.as_ref(),
        &release.releaseartists,
    );

    for entry in std::fs::read_dir(&release.source_path).map_err(|e| {
        RoseError::Internal(format!(
            "Failed to read directory {}: {e}",
            release.source_path.display()
        ))
    })? {
        let entry =
            entry.map_err(|e| RoseError::Internal(format!("Failed to read dir entry: {e}")))?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !STORED_DATA_FILE_REGEX.is_match(&name_str) {
            continue;
        }
        let _lock = lock(c, &release_lock_name(release_id), 10.0)?;
        let content = std::fs::read_to_string(entry.path()).map_err(|e| {
            RoseError::Internal(format!("Failed to read {}: {e}", entry.path().display()))
        })?;
        let mut data: toml::Table = content.parse().map_err(|e| {
            RoseError::Internal(format!(
                "Failed to parse TOML {}: {e}",
                entry.path().display()
            ))
        })?;
        if let Some(r) = rating {
            data.insert("rating".to_string(), toml::Value::Integer(r as i64));
        } else {
            data.remove("rating");
        }
        let toml_str = toml::to_string_pretty(&data)
            .map_err(|e| RoseError::Internal(format!("Failed to serialize TOML: {e}")))?;
        std::fs::write(entry.path(), toml_str).map_err(|e| {
            RoseError::Internal(format!("Failed to write {}: {e}", entry.path().display()))
        })?;
        tracing::info!("Set rating of release {release_logtext} to {rating:?}");
        update_cache_for_releases(c, Some(vec![release.source_path.clone()]), true)?;
        return Ok(());
    }

    tracing::error!(
        "Failed to find .rose.toml in {}",
        release.source_path.display()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Cover art operations
// ---------------------------------------------------------------------------

/// Remove existing cover arts and copy a new cover image as `cover.{ext}`.
pub fn set_release_cover_art(
    c: &Config,
    release_id: &str,
    new_path: &Path,
) -> Result<(), RoseError> {
    let suffix = new_path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if !c.valid_art_exts.contains(&suffix) {
        return Err(invalid_cover_art(format!(
            "File {}'s extension is not supported for cover images: \
             To change this, please read the configuration documentation",
            new_path.file_name().unwrap_or_default().to_string_lossy()
        )));
    }

    let release = get_release(c, release_id)?.ok_or_else(|| release_not_found(release_id))?;
    let release_logtext = make_release_logtext(
        &release.releasetitle,
        release.releasedate.as_ref(),
        &release.releaseartists,
    );

    let valid_cover_arts = c.valid_cover_arts();
    for entry in std::fs::read_dir(&release.source_path).map_err(|e| {
        RoseError::Internal(format!(
            "Failed to read directory {}: {e}",
            release.source_path.display()
        ))
    })? {
        let entry =
            entry.map_err(|e| RoseError::Internal(format!("Failed to read dir entry: {e}")))?;
        let fname = entry.file_name().to_string_lossy().to_lowercase();
        if valid_cover_arts.contains(&fname) {
            tracing::debug!(
                "Deleting existing cover art {} in {release_logtext}",
                entry.file_name().to_string_lossy()
            );
            trash::delete(entry.path()).map_err(|e| {
                RoseError::Internal(format!("Failed to trash {}: {e}", entry.path().display()))
            })?;
        }
    }

    let dest = release.source_path.join(format!("cover.{suffix}"));
    std::fs::copy(new_path, &dest).map_err(|e| {
        RoseError::Internal(format!(
            "Failed to copy cover art to {}: {e}",
            dest.display()
        ))
    })?;
    tracing::info!(
        "Set the cover of release {release_logtext} to {}",
        new_path.file_name().unwrap_or_default().to_string_lossy()
    );
    update_cache_for_releases(c, Some(vec![release.source_path.clone()]), false)?;
    Ok(())
}

/// Delete all potential cover art files from a release directory.
pub fn delete_release_cover_art(c: &Config, release_id: &str) -> Result<(), RoseError> {
    let release = get_release(c, release_id)?.ok_or_else(|| release_not_found(release_id))?;
    let release_logtext = make_release_logtext(
        &release.releasetitle,
        release.releasedate.as_ref(),
        &release.releaseartists,
    );

    let valid_cover_arts = c.valid_cover_arts();
    let mut found = false;
    for entry in std::fs::read_dir(&release.source_path).map_err(|e| {
        RoseError::Internal(format!(
            "Failed to read directory {}: {e}",
            release.source_path.display()
        ))
    })? {
        let entry =
            entry.map_err(|e| RoseError::Internal(format!("Failed to read dir entry: {e}")))?;
        let fname = entry.file_name().to_string_lossy().to_lowercase();
        if valid_cover_arts.contains(&fname) {
            tracing::debug!(
                "Deleting existing cover art {} in {release_logtext}",
                entry.file_name().to_string_lossy()
            );
            trash::delete(entry.path()).map_err(|e| {
                RoseError::Internal(format!("Failed to trash {}: {e}", entry.path().display()))
            })?;
            found = true;
        }
    }
    if found {
        tracing::info!("Deleted cover arts of release {release_logtext}");
    } else {
        tracing::info!("No-Op: No cover arts found for release {release_logtext}");
    }
    update_cache_for_releases(c, Some(vec![release.source_path.clone()]), false)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Intermediary metadata types for edit_release
// ---------------------------------------------------------------------------

/// A single artist with an explicit role string, used for TOML serialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataArtist {
    pub name: String,
    pub role: String,
}

impl MetadataArtist {
    /// Convert an `ArtistMapping` to a flat list of `MetadataArtist`, excluding aliases.
    pub fn from_mapping(mapping: &ArtistMapping) -> Vec<MetadataArtist> {
        let mut result = Vec::new();
        for (role, artists) in mapping.items() {
            for art in artists {
                if !art.alias {
                    result.push(MetadataArtist {
                        name: art.name.clone(),
                        role: role.to_string(),
                    });
                }
            }
        }
        result
    }

    /// Convert a list of `MetadataArtist` back into an `ArtistMapping`.
    pub fn to_mapping(artists: &[MetadataArtist]) -> Result<ArtistMapping, RoseError> {
        let mut m = ArtistMapping::default();
        for a in artists {
            let role_vec = match a.role.to_lowercase().as_str() {
                "main" => &mut m.main,
                "guest" => &mut m.guest,
                "remixer" => &mut m.remixer,
                "producer" => &mut m.producer,
                "composer" => &mut m.composer,
                "conductor" => &mut m.conductor,
                "djmixer" => &mut m.djmixer,
                _ => {
                    return Err(unknown_artist_role(format!(
                        "Failed to write tags: Unknown role for artist {}: {}",
                        a.name, a.role
                    )));
                }
            };
            role_vec.push(Artist {
                name: a.name.clone(),
                alias: false,
            });
        }
        Ok(m)
    }
}

/// Track metadata in the editor intermediary format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataTrack {
    pub discnumber: String,
    pub tracknumber: String,
    pub title: String,
    pub artists: Vec<MetadataArtist>,
}

/// Full release metadata in the editor intermediary format.
#[derive(Debug, Clone)]
pub struct MetadataRelease {
    pub title: String,
    pub new: bool,
    pub favorite: bool,
    pub rating: Option<i32>,
    pub releasetype: String,
    pub releasedate: Option<RoseDate>,
    pub originaldate: Option<RoseDate>,
    pub compositiondate: Option<RoseDate>,
    pub artists: Vec<MetadataArtist>,
    pub edition: Option<String>,
    pub catalognumber: Option<String>,
    pub labels: Vec<String>,
    pub genres: Vec<String>,
    pub secondary_genres: Vec<String>,
    pub descriptors: Vec<String>,
    pub tracks: HashMap<String, MetadataTrack>,
}

impl MetadataRelease {
    /// Construct from cache types.
    pub fn from_cache(release: &Release, tracks: &[Track]) -> MetadataRelease {
        MetadataRelease {
            title: release.releasetitle.clone(),
            new: release.new,
            favorite: release.favorite,
            rating: release.rating,
            releasetype: release.releasetype.clone(),
            releasedate: release.releasedate.clone(),
            originaldate: release.originaldate.clone(),
            compositiondate: release.compositiondate.clone(),
            edition: release.edition.clone(),
            catalognumber: release.catalognumber.clone(),
            labels: release.labels.clone(),
            genres: release.genres.clone(),
            secondary_genres: release.secondary_genres.clone(),
            descriptors: release.descriptors.clone(),
            artists: MetadataArtist::from_mapping(&release.releaseartists),
            tracks: tracks
                .iter()
                .map(|t| {
                    (
                        t.id.clone(),
                        MetadataTrack {
                            discnumber: t.discnumber.clone(),
                            tracknumber: t.tracknumber.clone(),
                            title: t.tracktitle.clone(),
                            artists: MetadataArtist::from_mapping(&t.trackartists),
                        },
                    )
                })
                .collect(),
        }
    }

    /// Serialize to TOML string with null hacks:
    /// - rating `-1` means null
    /// - dates/edition/catalognumber `""` means null
    pub fn serialize(&self) -> String {
        let mut table = toml::map::Map::new();
        table.insert("title".to_string(), toml::Value::String(self.title.clone()));
        table.insert("new".to_string(), toml::Value::Boolean(self.new));
        table.insert("favorite".to_string(), toml::Value::Boolean(self.favorite));
        table.insert(
            "rating".to_string(),
            toml::Value::Integer(self.rating.map(|r| r as i64).unwrap_or(-1)),
        );
        table.insert(
            "releasetype".to_string(),
            toml::Value::String(self.releasetype.clone()),
        );
        table.insert(
            "releasedate".to_string(),
            toml::Value::String(
                self.releasedate
                    .as_ref()
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
            ),
        );
        table.insert(
            "originaldate".to_string(),
            toml::Value::String(
                self.originaldate
                    .as_ref()
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
            ),
        );
        table.insert(
            "compositiondate".to_string(),
            toml::Value::String(
                self.compositiondate
                    .as_ref()
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
            ),
        );

        // Artists as array of tables
        let artists_array: Vec<toml::Value> = self
            .artists
            .iter()
            .map(|a| {
                let mut m = toml::map::Map::new();
                m.insert("name".to_string(), toml::Value::String(a.name.clone()));
                m.insert("role".to_string(), toml::Value::String(a.role.clone()));
                toml::Value::Table(m)
            })
            .collect();
        table.insert("artists".to_string(), toml::Value::Array(artists_array));

        table.insert(
            "catalognumber".to_string(),
            toml::Value::String(self.catalognumber.clone().unwrap_or_default()),
        );
        table.insert(
            "edition".to_string(),
            toml::Value::String(self.edition.clone().unwrap_or_default()),
        );
        table.insert(
            "labels".to_string(),
            toml::Value::Array(
                self.labels
                    .iter()
                    .map(|s| toml::Value::String(s.clone()))
                    .collect(),
            ),
        );
        table.insert(
            "genres".to_string(),
            toml::Value::Array(
                self.genres
                    .iter()
                    .map(|s| toml::Value::String(s.clone()))
                    .collect(),
            ),
        );
        table.insert(
            "secondary_genres".to_string(),
            toml::Value::Array(
                self.secondary_genres
                    .iter()
                    .map(|s| toml::Value::String(s.clone()))
                    .collect(),
            ),
        );
        table.insert(
            "descriptors".to_string(),
            toml::Value::Array(
                self.descriptors
                    .iter()
                    .map(|s| toml::Value::String(s.clone()))
                    .collect(),
            ),
        );

        // Tracks as sub-tables
        let mut tracks_table = toml::map::Map::new();
        // Sort track IDs for stable output
        let mut track_ids: Vec<&String> = self.tracks.keys().collect();
        track_ids.sort();
        for tid in track_ids {
            let t = &self.tracks[tid];
            let mut track_map = toml::map::Map::new();
            track_map.insert(
                "discnumber".to_string(),
                toml::Value::String(t.discnumber.clone()),
            );
            track_map.insert(
                "tracknumber".to_string(),
                toml::Value::String(t.tracknumber.clone()),
            );
            track_map.insert("title".to_string(), toml::Value::String(t.title.clone()));
            let artists_arr: Vec<toml::Value> = t
                .artists
                .iter()
                .map(|a| {
                    let mut m = toml::map::Map::new();
                    m.insert("name".to_string(), toml::Value::String(a.name.clone()));
                    m.insert("role".to_string(), toml::Value::String(a.role.clone()));
                    toml::Value::Table(m)
                })
                .collect();
            track_map.insert("artists".to_string(), toml::Value::Array(artists_arr));
            tracks_table.insert(tid.clone(), toml::Value::Table(track_map));
        }
        table.insert("tracks".to_string(), toml::Value::Table(tracks_table));

        toml::to_string_pretty(&toml::Value::Table(table))
            .expect("MetadataRelease serialization should not fail")
    }

    /// Parse from TOML string, reversing the null hacks.
    pub fn from_toml(toml_str: &str) -> Result<MetadataRelease, RoseError> {
        let d: toml::Table = toml_str.parse().map_err(|e: toml::de::Error| {
            release_edit_failed(format!("Failed to decode TOML file: {e}"))
        })?;

        let title = d
            .get("title")
            .and_then(|v| v.as_str())
            .ok_or_else(|| release_edit_failed("Missing key 'title'".to_string()))?
            .to_string();
        let new = d
            .get("new")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| release_edit_failed("Missing key 'new'".to_string()))?;
        let favorite = d
            .get("favorite")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| release_edit_failed("Missing key 'favorite'".to_string()))?;
        let rating_val = d.get("rating").and_then(|v| v.as_integer()).unwrap_or(-1);
        let rating = if rating_val == -1 {
            None
        } else {
            Some(rating_val as i32)
        };

        let releasetype = d
            .get("releasetype")
            .and_then(|v| v.as_str())
            .ok_or_else(|| release_edit_failed("Missing key 'releasetype'".to_string()))?
            .to_string();

        let releasedate_str = d.get("releasedate").and_then(|v| v.as_str()).unwrap_or("");
        let originaldate_str = d.get("originaldate").and_then(|v| v.as_str()).unwrap_or("");
        let compositiondate_str = d
            .get("compositiondate")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let catalognumber_str = d
            .get("catalognumber")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let edition_str = d.get("edition").and_then(|v| v.as_str()).unwrap_or("");

        let genres = toml_string_array(&d, "genres")?;
        let secondary_genres = toml_string_array(&d, "secondary_genres")?;
        let descriptors = toml_string_array(&d, "descriptors")?;
        let labels = toml_string_array(&d, "labels")?;

        let artists = toml_artist_array(&d, "artists")?;

        let tracks_table = d
            .get("tracks")
            .and_then(|v| v.as_table())
            .ok_or_else(|| release_edit_failed("Missing key 'tracks'".to_string()))?;

        let mut tracks = HashMap::new();
        for (tid, tval) in tracks_table {
            let t = tval
                .as_table()
                .ok_or_else(|| release_edit_failed(format!("Track '{tid}' is not a table")))?;
            let tracknumber = t
                .get("tracknumber")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let discnumber = t
                .get("discnumber")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let track_title = t
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let track_artists = toml_artist_array_from_table(t)?;
            tracks.insert(
                tid.clone(),
                MetadataTrack {
                    tracknumber,
                    discnumber,
                    title: track_title,
                    artists: track_artists,
                },
            );
        }

        Ok(MetadataRelease {
            title,
            new,
            favorite,
            rating,
            releasetype,
            releasedate: RoseDate::parse(if releasedate_str.is_empty() {
                None
            } else {
                Some(releasedate_str)
            }),
            originaldate: RoseDate::parse(if originaldate_str.is_empty() {
                None
            } else {
                Some(originaldate_str)
            }),
            compositiondate: RoseDate::parse(if compositiondate_str.is_empty() {
                None
            } else {
                Some(compositiondate_str)
            }),
            catalognumber: if catalognumber_str.is_empty() {
                None
            } else {
                Some(catalognumber_str.to_string())
            },
            edition: if edition_str.is_empty() {
                None
            } else {
                Some(edition_str.to_string())
            },
            genres,
            secondary_genres,
            descriptors,
            labels,
            artists,
            tracks,
        })
    }
}

/// Helper: parse a TOML array of strings from a table.
fn toml_string_array(table: &toml::Table, key: &str) -> Result<Vec<String>, RoseError> {
    match table.get(key) {
        Some(toml::Value::Array(arr)) => {
            let mut result = Vec::new();
            for v in arr {
                result.push(
                    v.as_str()
                        .ok_or_else(|| {
                            release_edit_failed(format!("Non-string value in '{key}' array"))
                        })?
                        .to_string(),
                );
            }
            Ok(result)
        }
        Some(_) => Err(release_edit_failed(format!("'{key}' must be an array"))),
        None => Ok(Vec::new()),
    }
}

/// Helper: parse artists array from a TOML table at key "artists".
fn toml_artist_array(table: &toml::Table, key: &str) -> Result<Vec<MetadataArtist>, RoseError> {
    match table.get(key) {
        Some(toml::Value::Array(arr)) => {
            let mut result = Vec::new();
            for v in arr {
                let t = v.as_table().ok_or_else(|| {
                    release_edit_failed(format!("Non-table value in '{key}' array"))
                })?;
                let name = t
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let role = t
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                result.push(MetadataArtist { name, role });
            }
            Ok(result)
        }
        Some(_) => Err(release_edit_failed(format!("'{key}' must be an array"))),
        None => Ok(Vec::new()),
    }
}

/// Helper: parse artists array from a TOML table that has an "artists" key.
fn toml_artist_array_from_table(table: &toml::Table) -> Result<Vec<MetadataArtist>, RoseError> {
    toml_artist_array(table, "artists")
}

// ---------------------------------------------------------------------------
// Failed release edit filename
// ---------------------------------------------------------------------------

static FAILED_RELEASE_EDIT_FILENAME_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^failed-release-edit\.([^.]+)\.toml$").expect("invalid regex"));

// ---------------------------------------------------------------------------
// edit_release
// ---------------------------------------------------------------------------

/// Callback type for programmatic TOML editing (used in tests / headless mode).
pub type EditorFn<'a> = Option<&'a dyn Fn(&str) -> Result<String, RoseError>>;

/// Interactive metadata editor for a release.
///
/// Opens `$EDITOR` with a TOML file representing the release metadata.
/// On save, applies per-track per-field dirty checking to minimize disk writes.
/// On failure, saves the edited TOML to a resume file.
///
/// If `editor_fn` is `Some`, the callback is used instead of `$EDITOR`; it
/// receives the serialized TOML and must return the (possibly modified) TOML.
pub fn edit_release(
    c: &Config,
    release_id: &str,
    resume_file: Option<&Path>,
    editor_fn: EditorFn<'_>,
) -> Result<(), RoseError> {
    let release = get_release(c, release_id)?.ok_or_else(|| release_not_found(release_id))?;

    // Trigger a quick cache update to ensure we are reading the liveliest data.
    update_cache_for_releases(c, Some(vec![release.source_path.clone()]), false)?;
    // Reload release in case any source paths changed.
    let release = get_release(c, release_id)?.ok_or_else(|| release_not_found(release_id))?;

    let _lock = lock(c, &release_lock_name(release_id), 60.0)?;
    let tracks = get_tracks_of_release(c, &release)?;

    let original_toml = if let Some(resume_path) = resume_file {
        let m = FAILED_RELEASE_EDIT_FILENAME_REGEX
            .captures(
                resume_path
                    .file_name()
                    .unwrap_or_default()
                    .to_str()
                    .unwrap_or(""),
            )
            .ok_or_else(|| {
                invalid_resume_file(format!(
                    "{} is not a valid release edit resume file",
                    resume_path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                ))
            })?;
        let resume_uuid = m.get(1).unwrap().as_str();
        if resume_uuid != release_id {
            return Err(invalid_resume_file(format!(
                "{} is not associated with this release",
                resume_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            )));
        }
        std::fs::read_to_string(resume_path).map_err(|e| {
            RoseError::Internal(format!(
                "Failed to read resume file {}: {e}",
                resume_path.display()
            ))
        })?
    } else {
        let original_metadata = MetadataRelease::from_cache(&release, &tracks);
        original_metadata.serialize()
    };

    // Use the callback when provided, otherwise open $EDITOR.
    let toml = if let Some(f) = editor_fn {
        f(&original_toml)?
    } else {
        open_editor(&original_toml)?
    };

    if original_toml == toml && resume_file.is_none() {
        tracing::info!("Aborting manual release edit: no metadata change detected.");
        return Ok(());
    }

    let apply_result = apply_edited_toml(c, &release, &tracks, &toml);

    match apply_result {
        Ok(()) => {
            // On success, delete resume file if it was used.
            if let Some(resume_path) = resume_file {
                let _ = std::fs::remove_file(resume_path);
            }
            update_cache_for_releases(c, Some(vec![release.source_path.clone()]), true)?;
            Ok(())
        }
        Err(e) => {
            // On failure, save the edited TOML to a resume file.
            let new_resume_path = c
                .cache_dir
                .join(format!("failed-release-edit.{release_id}.toml"));
            let _ = std::fs::write(&new_resume_path, &toml);
            Err(release_edit_failed(format!(
                "Failed to apply release edit: {e}\n\n\
                 --------\n\n\
                 The submitted metadata TOML file has been written to {}.\n\n\
                 You can reattempt the release edit and fix the metadata file with the command:\n\n\
                     $ rose releases edit --resume {} {}\n",
                new_resume_path.display(),
                shell_escape::escape(new_resume_path.display().to_string().into()),
                shell_escape::escape(release_id.into()),
            )))
        }
    }
}

/// Open $EDITOR (or vi) with the given text in a temp file.
/// Returns the edited text.
fn open_editor(text: &str) -> Result<String, RoseError> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let tmp_dir = std::env::temp_dir();
    let tmp_file = tmp_dir.join(format!("rose-edit-{}.toml", uuid::Uuid::now_v7()));
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
        return Ok(text.to_string()); // treat non-zero exit as "unchanged"
    }

    let result = std::fs::read_to_string(&tmp_file).map_err(|e| {
        RoseError::Internal(format!(
            "Failed to read temp file {}: {e}",
            tmp_file.display()
        ))
    })?;
    let _ = std::fs::remove_file(&tmp_file);
    Ok(result)
}

/// Apply the edited TOML to the release tracks (per-field dirty checking).
fn apply_edited_toml(
    c: &Config,
    release: &Release,
    tracks: &[Track],
    toml_str: &str,
) -> Result<(), RoseError> {
    let release_meta = MetadataRelease::from_toml(toml_str)?;

    for t in tracks {
        let track_meta = release_meta.tracks.get(&t.id).ok_or_else(|| {
            RoseError::Internal(format!("Track {} not found in edited TOML", t.id))
        })?;
        let mut tags = AudioTags::from_file(&t.source_path)?;
        let mut dirty = false;

        // Track tags
        if tags.tracknumber.as_deref() != Some(&track_meta.tracknumber) {
            tags.tracknumber = Some(track_meta.tracknumber.clone());
            dirty = true;
            tracing::debug!(
                "Modified tag detected for {}: tracknumber",
                t.source_path.display()
            );
        }
        if tags.discnumber.as_deref() != Some(&track_meta.discnumber) {
            tags.discnumber = Some(track_meta.discnumber.clone());
            dirty = true;
            tracing::debug!(
                "Modified tag detected for {}: discnumber",
                t.source_path.display()
            );
        }
        if tags.tracktitle.as_deref() != Some(&track_meta.title) {
            tags.tracktitle = Some(track_meta.title.clone());
            dirty = true;
            tracing::debug!(
                "Modified tag detected for {}: title",
                t.source_path.display()
            );
        }
        let tart = MetadataArtist::to_mapping(&track_meta.artists)?;
        if tags.trackartists != tart {
            tags.trackartists = tart;
            dirty = true;
            tracing::debug!(
                "Modified tag detected for {}: artists",
                t.source_path.display()
            );
        }

        // Album tags
        if tags.releasetitle.as_deref() != Some(&release_meta.title) {
            tags.releasetitle = Some(release_meta.title.clone());
            dirty = true;
            tracing::debug!(
                "Modified tag detected for {}: release",
                t.source_path.display()
            );
        }
        if tags.releasetype != release_meta.releasetype.to_lowercase() {
            tags.releasetype = release_meta.releasetype.to_lowercase();
            dirty = true;
            tracing::debug!(
                "Modified tag detected for {}: releasetype",
                t.source_path.display()
            );
        }
        if tags.releasedate != release_meta.releasedate {
            tags.releasedate = release_meta.releasedate.clone();
            dirty = true;
            tracing::debug!(
                "Modified tag detected for {}: releasedate",
                t.source_path.display()
            );
        }
        if tags.originaldate != release_meta.originaldate {
            tags.originaldate = release_meta.originaldate.clone();
            dirty = true;
            tracing::debug!(
                "Modified tag detected for {}: originaldate",
                t.source_path.display()
            );
        }
        if tags.compositiondate != release_meta.compositiondate {
            tags.compositiondate = release_meta.compositiondate.clone();
            dirty = true;
            tracing::debug!(
                "Modified tag detected for {}: compositiondate",
                t.source_path.display()
            );
        }
        if tags.edition != release_meta.edition {
            tags.edition = release_meta.edition.clone();
            dirty = true;
            tracing::debug!(
                "Modified tag detected for {}: edition",
                t.source_path.display()
            );
        }
        if tags.catalognumber != release_meta.catalognumber {
            tags.catalognumber = release_meta.catalognumber.clone();
            dirty = true;
            tracing::debug!(
                "Modified tag detected for {}: catalognumber",
                t.source_path.display()
            );
        }
        if tags.genre != release_meta.genres {
            tags.genre = release_meta.genres.clone();
            dirty = true;
            tracing::debug!(
                "Modified tag detected for {}: genre",
                t.source_path.display()
            );
        }
        if tags.secondarygenre != release_meta.secondary_genres {
            tags.secondarygenre = release_meta.secondary_genres.clone();
            dirty = true;
            tracing::debug!(
                "Modified tag detected for {}: secondarygenre",
                t.source_path.display()
            );
        }
        if tags.descriptor != release_meta.descriptors {
            tags.descriptor = release_meta.descriptors.clone();
            dirty = true;
            tracing::debug!(
                "Modified tag detected for {}: descriptor",
                t.source_path.display()
            );
        }
        if tags.label != release_meta.labels {
            tags.label = release_meta.labels.clone();
            dirty = true;
            tracing::debug!(
                "Modified tag detected for {}: label",
                t.source_path.display()
            );
        }
        let aart = MetadataArtist::to_mapping(&release_meta.artists)?;
        if tags.releaseartists != aart {
            tags.releaseartists = aart;
            dirty = true;
            tracing::debug!(
                "Modified tag detected for {}: release_artists",
                t.source_path.display()
            );
        }

        if dirty {
            let relative = t
                .source_path
                .strip_prefix(&c.music_source_dir)
                .unwrap_or(&t.source_path);
            tracing::info!("Flushing changed tags to {}", relative.display());
            tags.flush(c.write_parent_genres)?;
        }
    }

    // After tag writes, apply new/favorite/rating changes
    if release_meta.new != release.new {
        toggle_release_new(c, &release.id)?;
    }
    if release_meta.favorite != release.favorite {
        toggle_release_favorite(c, &release.id)?;
    }
    if release_meta.rating != release.rating {
        let rating = release_meta.rating.map(|r| r as u8);
        set_release_rating(c, &release.id, rating)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// create_single_release
// ---------------------------------------------------------------------------

/// Takes a track and copies it into a brand new "single" (or "loosetrack") release.
pub fn create_single_release(
    c: &Config,
    track_path: &Path,
    releasetype: &str,
) -> Result<(), RoseError> {
    if !track_path.is_file() {
        return Err(RoseError::Internal(format!(
            "Failed to extract single: file {} not found",
            track_path.display()
        )));
    }

    // Step 1. Compute the new directory name for the single.
    let af = AudioTags::from_file(track_path)?;
    let title = af
        .tracktitle
        .as_deref()
        .unwrap_or("Unknown Title")
        .trim()
        .to_string();

    let mut dirname = format!("{} - ", artistsfmt(&af.trackartists));
    if let Some(ref date) = af.releasedate {
        dirname.push_str(&format!("{}. ", date.year));
    }
    dirname.push_str(&title);

    // Handle directory name collisions.
    let mut collision_no = 2u32;
    let original_dirname = dirname.clone();
    loop {
        if !c.music_source_dir.join(&dirname).exists() {
            break;
        }
        dirname = format!("{original_dirname} [{collision_no}]");
        collision_no += 1;
    }

    // Step 2. Make the new directory and copy the track.
    let source_path = c.music_source_dir.join(&dirname);
    std::fs::create_dir_all(&source_path).map_err(|e| {
        RoseError::Internal(format!(
            "Failed to create directory {}: {e}",
            source_path.display()
        ))
    })?;

    let ext = track_path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let new_track_path = source_path.join(format!("01. {title}{ext}"));
    std::fs::copy(track_path, &new_track_path).map_err(|e| {
        RoseError::Internal(format!(
            "Failed to copy track to {}: {e}",
            new_track_path.display()
        ))
    })?;

    // Copy cover art if present in the track's current directory.
    if let Some(parent) = track_path.parent() {
        let valid_cover_arts = c.valid_cover_arts();
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten() {
                let fname = entry.file_name().to_string_lossy().to_lowercase();
                if valid_cover_arts.contains(&fname) {
                    let _ = std::fs::copy(entry.path(), source_path.join(entry.file_name()));
                    break;
                }
            }
        }
    }

    // Step 3. Update the tags of the new track.
    let mut af = AudioTags::from_file(&new_track_path)?;
    af.releasetitle = Some(title);
    af.releasetype = releasetype.to_string();
    af.releaseartists = af.trackartists.clone();
    af.tracknumber = Some("1".to_string());
    af.discnumber = Some("1".to_string());
    af.release_id = None;
    af.id = None;
    af.flush(c.write_parent_genres)?;

    tracing::info!(
        "Created phony single release {}",
        source_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
    );

    // Step 4: Update the cache with rename_source_files=false.
    let mut c_tmp = c.clone();
    c_tmp.rename_source_files = false;
    update_cache_for_releases(&c_tmp, Some(vec![source_path.clone()]), false)?;

    // Step 5: Default extracted singles to not new.
    for entry in std::fs::read_dir(&source_path).map_err(|e| {
        RoseError::Internal(format!(
            "Failed to read directory {}: {e}",
            source_path.display()
        ))
    })? {
        let entry =
            entry.map_err(|e| RoseError::Internal(format!("Failed to read dir entry: {e}")))?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if let Some(m) = STORED_DATA_FILE_REGEX.captures(&name_str) {
            let new_release_id = m.get(1).unwrap().as_str();
            toggle_release_new(c, new_release_id)?;
            break;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Rule integration
// ---------------------------------------------------------------------------

/// Find releases matching a rule matcher.
///
/// Optimizes strict lookups via `filter_releases()`, falling back to FTS search.
pub fn find_releases_matching_rule(
    c: &Config,
    matcher: &Matcher,
    include_loose_tracks: bool,
) -> Result<Vec<Release>, RoseError> {
    use crate::rule_parser::resolve_tag;

    // Optimize strict lookups for common single-tag matchers.
    if matcher.pattern.strict_start && matcher.pattern.strict_end {
        let artist_tags = resolve_tag("artist");
        let releaseartist_tags = resolve_tag("releaseartist");

        if artist_tags.is_some() && matcher.tags == artist_tags.unwrap() {
            return filter_releases(
                c,
                None,                          // release_artist_filter
                Some(&matcher.pattern.needle), // all_artist_filter
                None,                          // genre_filter
                None,                          // descriptor_filter
                None,                          // label_filter
                None,                          // release_type_filter
                None,                          // new
                None,                          // favorite
                include_loose_tracks,
            );
        }
        if releaseartist_tags.is_some() && matcher.tags == releaseartist_tags.unwrap() {
            return filter_releases(
                c,
                Some(&matcher.pattern.needle), // release_artist_filter
                None,                          // all_artist_filter
                None,                          // genre_filter
                None,                          // descriptor_filter
                None,                          // label_filter
                None,                          // release_type_filter
                None,                          // new
                None,                          // favorite
                include_loose_tracks,
            );
        }
        if matcher.tags == ["genre"] {
            return filter_releases(
                c,
                None,
                None,
                Some(&matcher.pattern.needle),
                None,
                None,
                None,
                None,
                None,
                include_loose_tracks,
            );
        }
        if matcher.tags == ["label"] {
            return filter_releases(
                c,
                None,
                None,
                None,
                None,
                Some(&matcher.pattern.needle),
                None,
                None,
                None,
                include_loose_tracks,
            );
        }
        if matcher.tags == ["descriptor"] {
            return filter_releases(
                c,
                None,
                None,
                None,
                Some(&matcher.pattern.needle),
                None,
                None,
                None,
                None,
                include_loose_tracks,
            );
        }
        if matcher.tags == ["releasetype"] {
            return filter_releases(
                c,
                None,
                None,
                None,
                None,
                None,
                Some(&matcher.pattern.needle),
                None,
                None,
                include_loose_tracks,
            );
        }
    }

    // Fallback: FTS search + false positive filtering
    let release_ids: Vec<String> =
        fast_search_for_matching_releases(c, matcher, include_loose_tracks)?
            .iter()
            .map(|x| x.id.clone())
            .collect();
    let releases = list_releases(c, Some(&release_ids), include_loose_tracks)?;
    Ok(filter_release_false_positives_using_read_cache(
        matcher,
        releases,
        include_loose_tracks,
    ))
}

/// Run rule engine actions on a release.
pub fn run_actions_on_release(
    c: &Config,
    release_id: &str,
    actions: &[Action],
    dry_run: bool,
    confirm_yes: bool,
) -> Result<(), RoseError> {
    let release = get_release(c, release_id)?.ok_or_else(|| release_not_found(release_id))?;
    let tracks = get_tracks_of_release(c, &release)?;
    let audiotags: Vec<AudioTags> = tracks
        .iter()
        .map(|t| AudioTags::from_file(&t.source_path))
        .collect::<Result<Vec<_>, _>>()?;
    execute_metadata_actions(c, actions, audiotags, dry_run, confirm_yes)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // MetadataRelease serialize/from_toml roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn metadata_release_roundtrip() {
        let release = MetadataRelease {
            title: "Test Release".to_string(),
            new: true,
            favorite: false,
            rating: Some(85),
            releasetype: "album".to_string(),
            releasedate: RoseDate::parse(Some("2020")),
            originaldate: None,
            compositiondate: None,
            artists: vec![
                MetadataArtist {
                    name: "Artist A".to_string(),
                    role: "main".to_string(),
                },
                MetadataArtist {
                    name: "Artist B".to_string(),
                    role: "guest".to_string(),
                },
            ],
            edition: Some("Deluxe".to_string()),
            catalognumber: Some("CAT-001".to_string()),
            labels: vec!["Label1".to_string()],
            genres: vec!["Rock".to_string(), "Pop".to_string()],
            secondary_genres: vec!["Indie".to_string()],
            descriptors: vec!["Energetic".to_string()],
            tracks: {
                let mut m = HashMap::new();
                m.insert(
                    "track1".to_string(),
                    MetadataTrack {
                        discnumber: "1".to_string(),
                        tracknumber: "1".to_string(),
                        title: "Song One".to_string(),
                        artists: vec![MetadataArtist {
                            name: "Artist A".to_string(),
                            role: "main".to_string(),
                        }],
                    },
                );
                m
            },
        };

        let toml_str = release.serialize();
        let parsed = MetadataRelease::from_toml(&toml_str).unwrap();

        assert_eq!(parsed.title, "Test Release");
        assert!(parsed.new);
        assert!(!parsed.favorite);
        assert_eq!(parsed.rating, Some(85));
        assert_eq!(parsed.releasetype, "album");
        assert_eq!(parsed.releasedate, RoseDate::parse(Some("2020")));
        assert_eq!(parsed.originaldate, None);
        assert_eq!(parsed.compositiondate, None);
        assert_eq!(parsed.edition, Some("Deluxe".to_string()));
        assert_eq!(parsed.catalognumber, Some("CAT-001".to_string()));
        assert_eq!(parsed.labels, vec!["Label1"]);
        assert_eq!(parsed.genres, vec!["Rock", "Pop"]);
        assert_eq!(parsed.secondary_genres, vec!["Indie"]);
        assert_eq!(parsed.descriptors, vec!["Energetic"]);
        assert_eq!(parsed.artists.len(), 2);
        assert_eq!(parsed.artists[0].name, "Artist A");
        assert_eq!(parsed.artists[0].role, "main");
        assert_eq!(parsed.artists[1].name, "Artist B");
        assert_eq!(parsed.artists[1].role, "guest");
        assert_eq!(parsed.tracks.len(), 1);
        let track = &parsed.tracks["track1"];
        assert_eq!(track.title, "Song One");
        assert_eq!(track.tracknumber, "1");
        assert_eq!(track.discnumber, "1");
    }

    #[test]
    fn metadata_release_null_hacks() {
        let release = MetadataRelease {
            title: "Null Test".to_string(),
            new: false,
            favorite: true,
            rating: None,
            releasetype: "single".to_string(),
            releasedate: None,
            originaldate: None,
            compositiondate: None,
            artists: vec![],
            edition: None,
            catalognumber: None,
            labels: vec![],
            genres: vec![],
            secondary_genres: vec![],
            descriptors: vec![],
            tracks: HashMap::new(),
        };

        let toml_str = release.serialize();
        // Verify null hacks in the serialized string
        assert!(toml_str.contains("rating = -1"));
        assert!(toml_str.contains("releasedate = \"\""));
        assert!(toml_str.contains("originaldate = \"\""));
        assert!(toml_str.contains("compositiondate = \"\""));
        assert!(toml_str.contains("edition = \"\""));
        assert!(toml_str.contains("catalognumber = \"\""));

        let parsed = MetadataRelease::from_toml(&toml_str).unwrap();
        assert_eq!(parsed.rating, None);
        assert_eq!(parsed.releasedate, None);
        assert_eq!(parsed.originaldate, None);
        assert_eq!(parsed.compositiondate, None);
        assert_eq!(parsed.edition, None);
        assert_eq!(parsed.catalognumber, None);
    }

    #[test]
    fn metadata_artist_mapping_roundtrip() {
        let mapping = ArtistMapping {
            main: vec![Artist::new("A"), Artist::new("B")],
            guest: vec![Artist::new("C")],
            producer: vec![Artist::new("D")],
            ..Default::default()
        };
        let meta_artists = MetadataArtist::from_mapping(&mapping);
        assert_eq!(meta_artists.len(), 4);

        let roundtripped = MetadataArtist::to_mapping(&meta_artists).unwrap();
        assert_eq!(roundtripped.main.len(), 2);
        assert_eq!(roundtripped.main[0].name, "A");
        assert_eq!(roundtripped.main[1].name, "B");
        assert_eq!(roundtripped.guest.len(), 1);
        assert_eq!(roundtripped.guest[0].name, "C");
        assert_eq!(roundtripped.producer.len(), 1);
        assert_eq!(roundtripped.producer[0].name, "D");
    }

    #[test]
    fn metadata_artist_unknown_role() {
        let artists = vec![MetadataArtist {
            name: "X".to_string(),
            role: "bogus".to_string(),
        }];
        assert!(MetadataArtist::to_mapping(&artists).is_err());
    }

    #[test]
    fn rating_validation() {
        // Valid rating range is 1-100; None is allowed (clears rating).
        // Out of range should error.
        assert!(set_release_rating_validate(Some(0)).is_err());
        assert!(set_release_rating_validate(Some(101)).is_err());
        assert!(set_release_rating_validate(Some(1)).is_ok());
        assert!(set_release_rating_validate(Some(100)).is_ok());
        assert!(set_release_rating_validate(None).is_ok());
    }

    /// Helper to test rating validation without needing a real config.
    fn set_release_rating_validate(rating: Option<u8>) -> Result<(), RoseError> {
        if let Some(r) = rating {
            if !(1..=100).contains(&r) {
                return Err(invalid_rating(format!(
                    "Rating must be between 1 and 100, got {r}"
                )));
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // edit_release integration tests (editor callback refactor)
    // -----------------------------------------------------------------------

    use crate::audiotags::AudioTags;
    use crate::cache::{
        get_release, get_tracks_of_release, list_releases, maybe_invalidate_cache_database,
        update_cache_for_releases, STORED_DATA_FILE_REGEX,
    };
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    /// Locate the repo root (two parents up from CARGO_MANIFEST_DIR).
    fn repo_root() -> PathBuf {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    /// Copy the test audio file into `dest_dir` with the given filename.
    fn copy_audio_file(dest_dir: &Path, filename: &str) -> PathBuf {
        let src = repo_root().join("testdata/Test Release 1/01.m4a");
        let dst = dest_dir.join(filename);
        std::fs::copy(&src, &dst)
            .unwrap_or_else(|e| panic!("copy {} -> {}: {e}", src.display(), dst.display()));
        dst
    }

    /// Create a minimal Config with a temp directory, initialise the cache DB,
    /// seed one release with two tracks, and return the release ID.
    fn setup_release_env() -> (Config, TempDir, String) {
        let tmp = TempDir::new().unwrap();
        let music_dir = tmp.path().join("music");
        std::fs::create_dir_all(&music_dir).unwrap();
        let cache_dir = tmp.path().join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let vfs_dir = tmp.path().join("vfs");
        std::fs::create_dir_all(&vfs_dir).unwrap();

        let config_toml = format!(
            "music_source_dir = \"{}\"\ncache_dir = \"{}\"\nvfs.mount_dir = \"{}\"",
            music_dir.display(),
            cache_dir.display(),
            vfs_dir.display(),
        );
        let config_path = tmp.path().join("config.toml");
        std::fs::write(&config_path, &config_toml).unwrap();
        let c = Config::parse(Some(&config_path)).unwrap();

        maybe_invalidate_cache_database(&c).unwrap();

        // Seed a release with two tracks.
        let release_dir = music_dir.join("TestRelease");
        std::fs::create_dir_all(&release_dir).unwrap();
        copy_audio_file(&release_dir, "01.m4a");
        copy_audio_file(&release_dir, "02.m4a");

        update_cache_for_releases(&c, None, false).unwrap();

        let releases = list_releases(&c, None, true).unwrap();
        assert_eq!(releases.len(), 1);
        let release_id = releases[0].id.clone();

        (c, tmp, release_id)
    }

    /// Create a minimal Config backed by a temporary directory.
    fn test_config(dir: &TempDir) -> Config {
        let music_dir = dir.path().join("music");
        let cache_dir = dir.path().join("cache");
        std::fs::create_dir_all(&music_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();

        let cfg_path = dir.path().join("config.toml");
        let mut f = std::fs::File::create(&cfg_path).unwrap();
        write!(
            f,
            r#"
            music_source_dir = "{}"
            cache_dir = "{}"
            vfs.mount_dir = "{}"
            "#,
            music_dir.display(),
            cache_dir.display(),
            dir.path().join("vfs").display(),
        )
        .unwrap();
        Config::parse(Some(&cfg_path)).unwrap()
    }

    /// Set up a single-release environment with one audio file.
    /// Returns `(release_id, release_dir)`.
    fn setup_single_release(config: &Config) -> (String, PathBuf) {
        maybe_invalidate_cache_database(config).unwrap();

        let release_dir = config.music_source_dir.join("TestRelease");
        std::fs::create_dir_all(&release_dir).unwrap();
        copy_audio_file(&release_dir, "01.m4a");

        update_cache_for_releases(config, None, false).unwrap();

        let releases = list_releases(config, None, true).unwrap();
        assert_eq!(releases.len(), 1);
        let release_id = releases[0].id.clone();
        (release_id, release_dir)
    }

    /// Read and parse the `.rose.{uuid}.toml` sidecar from a release directory.
    fn read_sidecar(release_dir: &Path) -> toml::Table {
        for entry in std::fs::read_dir(release_dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if STORED_DATA_FILE_REGEX.is_match(&name_str) {
                let content = std::fs::read_to_string(entry.path()).unwrap();
                return content.parse().unwrap();
            }
        }
        panic!("No .rose.*.toml sidecar found in {}", release_dir.display());
    }

    // -----------------------------------------------------------------------
    // CRUD integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_delete_release() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let (release_id, release_dir) = setup_single_release(&config);

        // Pre-condition: release exists on disk and in DB.
        assert!(release_dir.exists());
        assert!(get_release(&config, &release_id).unwrap().is_some());

        // Delete the release (moves to trash).
        delete_release(&config, &release_id).unwrap();

        // Directory should be gone.
        assert!(!release_dir.exists());
        // DB should no longer contain the release.
        assert!(get_release(&config, &release_id).unwrap().is_none());
    }

    #[test]
    fn test_toggle_release_new() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let (release_id, release_dir) = setup_single_release(&config);

        // Default new=true.
        let r = get_release(&config, &release_id).unwrap().unwrap();
        assert!(r.new);
        let sidecar = read_sidecar(&release_dir);
        assert_eq!(sidecar["new"].as_bool().unwrap(), true);

        // Toggle → false.
        toggle_release_new(&config, &release_id).unwrap();
        let r = get_release(&config, &release_id).unwrap().unwrap();
        assert!(!r.new);
        let sidecar = read_sidecar(&release_dir);
        assert_eq!(sidecar["new"].as_bool().unwrap(), false);

        // Toggle → true again.
        toggle_release_new(&config, &release_id).unwrap();
        let r = get_release(&config, &release_id).unwrap().unwrap();
        assert!(r.new);
        let sidecar = read_sidecar(&release_dir);
        assert_eq!(sidecar["new"].as_bool().unwrap(), true);
    }

    #[test]
    fn test_toggle_release_favorite() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let (release_id, release_dir) = setup_single_release(&config);

        // Default favorite=false.
        let r = get_release(&config, &release_id).unwrap().unwrap();
        assert!(!r.favorite);
        let sidecar = read_sidecar(&release_dir);
        assert_eq!(sidecar["favorite"].as_bool().unwrap(), false);

        // Toggle → true.
        toggle_release_favorite(&config, &release_id).unwrap();
        let r = get_release(&config, &release_id).unwrap().unwrap();
        assert!(r.favorite);
        let sidecar = read_sidecar(&release_dir);
        assert_eq!(sidecar["favorite"].as_bool().unwrap(), true);

        // Toggle → false.
        toggle_release_favorite(&config, &release_id).unwrap();
        let r = get_release(&config, &release_id).unwrap().unwrap();
        assert!(!r.favorite);
        let sidecar = read_sidecar(&release_dir);
        assert_eq!(sidecar["favorite"].as_bool().unwrap(), false);
    }

    #[test]
    fn test_set_release_rating() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let (release_id, release_dir) = setup_single_release(&config);

        // Default rating is None.
        let r = get_release(&config, &release_id).unwrap().unwrap();
        assert_eq!(r.rating, None);

        // Set to 85.
        set_release_rating(&config, &release_id, Some(85)).unwrap();
        let r = get_release(&config, &release_id).unwrap().unwrap();
        assert_eq!(r.rating, Some(85));
        let sidecar = read_sidecar(&release_dir);
        assert_eq!(sidecar["rating"].as_integer().unwrap(), 85);

        // Clear (set to None).
        set_release_rating(&config, &release_id, None).unwrap();
        let r = get_release(&config, &release_id).unwrap().unwrap();
        assert_eq!(r.rating, None);
        // Sidecar uses -1 as the None sentinel after cache normalisation.
        let sidecar = read_sidecar(&release_dir);
        assert_eq!(sidecar["rating"].as_integer().unwrap(), -1);
    }

    #[test]
    fn test_set_release_cover_art() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let (release_id, release_dir) = setup_single_release(&config);

        // Place an existing cover art file and refresh the cache.
        let old_cover = release_dir.join("cover.jpg");
        std::fs::write(&old_cover, b"old-cover-bytes").unwrap();
        update_cache_for_releases(&config, Some(vec![release_dir.clone()]), false).unwrap();
        let r = get_release(&config, &release_id).unwrap().unwrap();
        assert!(r.cover_image_path.is_some());

        // Prepare a new image file.
        let new_cover = dir.path().join("new_cover.png");
        std::fs::write(&new_cover, b"new-cover-bytes").unwrap();

        // Replace cover art.
        set_release_cover_art(&config, &release_id, &new_cover).unwrap();

        // Old cover should be gone.
        assert!(!old_cover.exists());
        // New cover should exist as cover.png.
        let dest_cover = release_dir.join("cover.png");
        assert!(dest_cover.exists());
        assert_eq!(std::fs::read(&dest_cover).unwrap(), b"new-cover-bytes");

        // DB should reference the new cover.
        let r = get_release(&config, &release_id).unwrap().unwrap();
        let db_cover = r.cover_image_path.unwrap();
        assert!(
            db_cover.to_string_lossy().contains("cover.png"),
            "expected cover.png in path, got: {}",
            db_cover.display()
        );
    }

    #[test]
    fn test_delete_release_cover_art() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let (release_id, release_dir) = setup_single_release(&config);

        // Place a cover art file and refresh the cache.
        let cover = release_dir.join("cover.jpg");
        std::fs::write(&cover, b"cover-bytes").unwrap();
        update_cache_for_releases(&config, Some(vec![release_dir.clone()]), false).unwrap();
        let r = get_release(&config, &release_id).unwrap().unwrap();
        assert!(r.cover_image_path.is_some());

        // Delete cover art.
        delete_release_cover_art(&config, &release_id).unwrap();

        // File should be gone.
        assert!(!cover.exists());
        // DB should have NULL cover_image_path.
        let r = get_release(&config, &release_id).unwrap().unwrap();
        assert!(r.cover_image_path.is_none());
    }

    #[test]
    fn test_set_release_rating_invalid() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let (release_id, _) = setup_single_release(&config);

        // 0 is below the valid range (1–100).
        assert!(set_release_rating(&config, &release_id, Some(0)).is_err());
        // 101 is above the valid range.
        assert!(set_release_rating(&config, &release_id, Some(101)).is_err());
    }

    // -----------------------------------------------------------------------
    // edit_release integration tests (editor callback refactor)
    // -----------------------------------------------------------------------

    #[test]
    fn test_edit_release_full() {
        let (c, _tmp, release_id) = setup_release_env();

        // Edit callback: modify many fields via TOML manipulation.
        let editor_fn = |toml_str: &str| -> Result<String, RoseError> {
            let mut meta = MetadataRelease::from_toml(toml_str)?;

            meta.title = "Edited Title".to_string();
            meta.releasetype = "ep".to_string();
            meta.releasedate = RoseDate::parse(Some("2025"));
            meta.originaldate = RoseDate::parse(Some("2000"));
            meta.edition = Some("Deluxe".to_string());
            meta.catalognumber = Some("CAT-999".to_string());
            meta.labels = vec!["New Label".to_string()];
            meta.genres = vec!["Jazz".to_string(), "Funk".to_string()];
            meta.secondary_genres = vec!["Soul".to_string()];
            meta.descriptors = vec!["Groovy".to_string(), "Smooth".to_string()];
            meta.artists = vec![
                MetadataArtist {
                    name: "New Main".to_string(),
                    role: "main".to_string(),
                },
                MetadataArtist {
                    name: "New Guest".to_string(),
                    role: "guest".to_string(),
                },
            ];

            // Edit the first track we find.
            if let Some((_tid, track)) = meta.tracks.iter_mut().next() {
                track.title = "Edited Track Title".to_string();
                track.artists = vec![MetadataArtist {
                    name: "Track Artist X".to_string(),
                    role: "main".to_string(),
                }];
            }

            Ok(meta.serialize())
        };

        edit_release(&c, &release_id, None, Some(&editor_fn)).unwrap();

        // Reload from cache and assert release-level fields.
        let release = get_release(&c, &release_id).unwrap().unwrap();
        assert_eq!(release.releasetitle, "Edited Title");
        assert_eq!(release.releasetype, "ep");
        assert_eq!(release.releasedate.as_ref().unwrap().year, 2025);
        assert_eq!(release.originaldate.as_ref().unwrap().year, 2000);
        assert_eq!(release.edition, Some("Deluxe".to_string()));
        assert_eq!(release.catalognumber, Some("CAT-999".to_string()));
        assert_eq!(release.labels, vec!["New Label"]);
        assert_eq!(release.genres, vec!["Jazz", "Funk"]);
        assert_eq!(release.secondary_genres, vec!["Soul"]);
        assert_eq!(release.descriptors, vec!["Groovy", "Smooth"]);
        assert_eq!(release.releaseartists.main.len(), 1);
        assert_eq!(release.releaseartists.main[0].name, "New Main");
        assert_eq!(release.releaseartists.guest.len(), 1);
        assert_eq!(release.releaseartists.guest[0].name, "New Guest");

        // Check that at least one track has the edited title & artists.
        let tracks = get_tracks_of_release(&c, &release).unwrap();
        let edited_track = tracks.iter().find(|t| t.tracktitle == "Edited Track Title");
        assert!(
            edited_track.is_some(),
            "expected a track with the edited title"
        );
        let et = edited_track.unwrap();
        assert_eq!(et.trackartists.main.len(), 1);
        assert_eq!(et.trackartists.main[0].name, "Track Artist X");

        // Verify the audio file tags on disk match.
        let tags = AudioTags::from_file(&et.source_path).unwrap();
        assert_eq!(tags.releasetitle.as_deref(), Some("Edited Title"));
        assert_eq!(tags.releasetype, "ep");
        assert_eq!(tags.tracktitle.as_deref(), Some("Edited Track Title"));
        assert_eq!(tags.genre, vec!["Jazz", "Funk"]);
        assert_eq!(tags.secondarygenre, vec!["Soul"]);
        assert_eq!(tags.descriptor, vec!["Groovy", "Smooth"]);
        assert_eq!(tags.label, vec!["New Label"]);
        assert_eq!(tags.edition, Some("Deluxe".to_string()));
        assert_eq!(tags.catalognumber, Some("CAT-999".to_string()));
    }

    #[test]
    fn test_edit_release_no_changes() {
        let (c, _tmp, release_id) = setup_release_env();

        // Read the original tags from the first track to compare later.
        let release = get_release(&c, &release_id).unwrap().unwrap();
        let tracks = get_tracks_of_release(&c, &release).unwrap();
        let original_tags = AudioTags::from_file(&tracks[0].source_path).unwrap();
        let original_mtime = std::fs::metadata(&tracks[0].source_path)
            .unwrap()
            .modified()
            .unwrap();

        // Sleep briefly so any write would produce a different mtime.
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Editor callback returns TOML unchanged (identity).
        let editor_fn = |toml_str: &str| -> Result<String, RoseError> { Ok(toml_str.to_string()) };

        edit_release(&c, &release_id, None, Some(&editor_fn)).unwrap();

        // Verify the file was NOT re-written (mtime unchanged).
        let after_mtime = std::fs::metadata(&tracks[0].source_path)
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(
            original_mtime, after_mtime,
            "file should not have been flushed"
        );

        // Tags should be identical.
        let after_tags = AudioTags::from_file(&tracks[0].source_path).unwrap();
        assert_eq!(original_tags.releasetitle, after_tags.releasetitle);
        assert_eq!(original_tags.tracktitle, after_tags.tracktitle);
    }

    #[test]
    fn test_edit_release_failure_and_resume() {
        let (c, _tmp, release_id) = setup_release_env();

        // 1. Provide invalid TOML: an artist with an unknown role.
        let bad_editor = |toml_str: &str| -> Result<String, RoseError> {
            let mut meta = MetadataRelease::from_toml(toml_str)?;
            meta.artists = vec![MetadataArtist {
                name: "Bad".to_string(),
                role: "bogus_role".to_string(),
            }];
            Ok(meta.serialize())
        };

        let result = edit_release(&c, &release_id, None, Some(&bad_editor));
        assert!(result.is_err(), "should fail with unknown artist role");

        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("failed-release-edit"),
            "error should mention resume file"
        );

        // 2. Verify the resume file was written.
        let resume_path = c
            .cache_dir
            .join(format!("failed-release-edit.{release_id}.toml"));
        assert!(
            resume_path.exists(),
            "resume file should exist at {}",
            resume_path.display()
        );

        // 3. Re-invoke with resume file and a fixing callback.
        let fix_editor = |toml_str: &str| -> Result<String, RoseError> {
            let mut meta = MetadataRelease::from_toml(toml_str)?;
            // Fix the bad role to a valid one.
            for a in &mut meta.artists {
                if a.role == "bogus_role" {
                    a.role = "main".to_string();
                }
            }
            meta.title = "Fixed Title".to_string();
            Ok(meta.serialize())
        };

        edit_release(&c, &release_id, Some(&resume_path), Some(&fix_editor)).unwrap();

        // 4. Resume file should be deleted on success.
        assert!(
            !resume_path.exists(),
            "resume file should be deleted after successful edit"
        );

        // 5. Verify the fix was applied.
        let release = get_release(&c, &release_id).unwrap().unwrap();
        assert_eq!(release.releasetitle, "Fixed Title");
        assert_eq!(release.releaseartists.main.len(), 1);
        assert_eq!(release.releaseartists.main[0].name, "Bad");
    }

    // -----------------------------------------------------------------------
    // Integration tests: create_single_release (T-6.3)
    // -----------------------------------------------------------------------

    /// Copy a specific file from testdata into dest_dir.
    fn copy_testdata_file(relative: &str, dest_dir: &Path, filename: &str) -> PathBuf {
        let src = repo_root().join(relative);
        let dst = dest_dir.join(filename);
        std::fs::copy(&src, &dst)
            .unwrap_or_else(|e| panic!("copy {} -> {}: {e}", src.display(), dst.display()));
        dst
    }

    /// Create a minimal Config backed by a temporary directory.
    fn test_config_for_integration(dir: &TempDir) -> Config {
        let music_dir = dir.path().join("music");
        let cache_dir = dir.path().join("cache");
        std::fs::create_dir_all(&music_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();

        let cfg_path = dir.path().join("config.toml");
        std::fs::write(
            &cfg_path,
            format!(
                "music_source_dir = \"{}\"\ncache_dir = \"{}\"\nvfs.mount_dir = \"{}\"",
                music_dir.display(),
                cache_dir.display(),
                dir.path().join("vfs").display(),
            ),
        )
        .unwrap();
        Config::parse(Some(&cfg_path)).unwrap()
    }

    #[test]
    fn test_create_single_release() {
        let dir = TempDir::new().unwrap();
        let config = test_config_for_integration(&dir);
        maybe_invalidate_cache_database(&config).unwrap();

        // Seed a multi-track release with cover art.
        let release_dir = config.music_source_dir.join("OriginalRelease");
        std::fs::create_dir_all(&release_dir).unwrap();
        copy_testdata_file("testdata/Test Release 1/01.m4a", &release_dir, "01.m4a");
        copy_testdata_file("testdata/Test Release 1/02.m4a", &release_dir, "02.m4a");
        let cover_path = release_dir.join("cover.jpg");
        std::fs::write(&cover_path, b"fake-cover-data").unwrap();

        // Populate the cache.
        update_cache_for_releases(&config, None, false).unwrap();
        let releases_before = list_releases(&config, None, true).unwrap();
        assert_eq!(
            releases_before.len(),
            1,
            "should have 1 release before extraction"
        );

        // Extract track 02.m4a as a single.
        let track_path = release_dir.join("02.m4a");
        create_single_release(&config, &track_path, "single").unwrap();

        // Original release is untouched.
        assert!(
            release_dir.join("01.m4a").is_file(),
            "original 01.m4a still exists"
        );
        assert!(
            release_dir.join("02.m4a").is_file(),
            "original 02.m4a still exists"
        );
        assert!(cover_path.is_file(), "original cover.jpg still exists");

        // New single directory created (trackartists=BLACKPINK, year=1990, title=Track 2).
        let single_dir = config.music_source_dir.join("BLACKPINK - 1990. Track 2");
        assert!(
            single_dir.is_dir(),
            "single dir should exist: {}",
            single_dir.display()
        );

        // Track file copied as 01. {title}.{ext}.
        let single_track = single_dir.join("01. Track 2.m4a");
        assert!(single_track.is_file(), "single track should exist");

        // Cover art copied.
        assert!(
            single_dir.join("cover.jpg").is_file(),
            "cover art should be copied"
        );

        // Audio tags verified.
        let af = AudioTags::from_file(&single_track).unwrap();
        assert_eq!(af.tracknumber.as_deref(), Some("1"));
        assert_eq!(af.discnumber.as_deref(), Some("1"));
        assert_eq!(af.releasetype, "single");
        assert_eq!(af.releasetitle.as_deref(), Some("Track 2"));
        assert_eq!(af.releaseartists, af.trackartists);
        // Rose IDs are cleared and then new IDs assigned during cache update.
        // The original track's IDs should not appear in the new single.
        let orig_af = AudioTags::from_file(&track_path).unwrap();
        assert_ne!(
            af.release_id, orig_af.release_id,
            "new single should have a different release_id than original"
        );

        // Cache has 2 releases now.
        let releases_after = list_releases(&config, None, true).unwrap();
        assert_eq!(
            releases_after.len(),
            2,
            "should have 2 releases after extraction"
        );
    }

    #[test]
    fn test_create_single_release_trailing_space() {
        let dir = TempDir::new().unwrap();
        let config = test_config_for_integration(&dir);
        maybe_invalidate_cache_database(&config).unwrap();

        let release_dir = config.music_source_dir.join("OriginalRelease");
        std::fs::create_dir_all(&release_dir).unwrap();
        let track_path =
            copy_testdata_file("testdata/Test Release 1/02.m4a", &release_dir, "02.m4a");

        // Set trailing whitespace on title.
        let mut af = AudioTags::from_file(&track_path).unwrap();
        af.tracktitle = Some("Trailing Space ".to_string());
        af.flush(config.write_parent_genres).unwrap();

        update_cache_for_releases(&config, None, false).unwrap();
        create_single_release(&config, &track_path, "single").unwrap();

        // Directory and filename should have trailing space stripped.
        let single_dir = config
            .music_source_dir
            .join("BLACKPINK - 1990. Trailing Space");
        assert!(
            single_dir.is_dir(),
            "trailing space should be stripped from dir name"
        );
        assert!(
            single_dir.join("01. Trailing Space.m4a").is_file(),
            "trailing space should be stripped from filename"
        );
    }

    // -----------------------------------------------------------------------
    // Integration tests: find_releases_matching_rule (T-6.3)
    // -----------------------------------------------------------------------

    /// Seed a database with two releases and known metadata for rule matching.
    fn seeded_config_for_rules() -> (TempDir, Config) {
        use crate::cache::connect;

        let dir = TempDir::new().unwrap();
        let config = test_config_for_integration(&dir);
        maybe_invalidate_cache_database(&config).unwrap();

        let music_dir = &config.music_source_dir;

        let r1_dir = music_dir.join("Release1Dir");
        std::fs::create_dir_all(&r1_dir).unwrap();
        copy_audio_file(&r1_dir, "01.m4a");

        let r2_dir = music_dir.join("Release2Dir");
        std::fs::create_dir_all(&r2_dir).unwrap();
        copy_audio_file(&r2_dir, "01.m4a");

        update_cache_for_releases(&config, None, false).unwrap();

        let conn = connect(&config).unwrap();

        // Deterministic release IDs by source_path order.
        let mut stmt = conn
            .prepare("SELECT id FROM releases ORDER BY source_path")
            .unwrap();
        let release_ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(release_ids.len(), 2);
        let r1_id = &release_ids[0];
        let r2_id = &release_ids[1];

        conn.execute(
            "UPDATE releases SET title = 'Release 1' WHERE id = ?1",
            [r1_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE releases SET title = 'Release 2' WHERE id = ?1",
            [r2_id],
        )
        .unwrap();

        conn.execute_batch(
            "DELETE FROM releases_artists; DELETE FROM tracks_artists; \
             DELETE FROM releases_genres; DELETE FROM releases_secondary_genres; \
             DELETE FROM releases_descriptors; DELETE FROM releases_labels;",
        )
        .unwrap();

        // Artists
        conn.execute(
            "INSERT INTO releases_artists (release_id, artist, role, position) VALUES (?1, 'Techno Man', 'main', 1)",
            [r1_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO releases_artists (release_id, artist, role, position) VALUES (?1, 'Bass Man', 'main', 2)",
            [r1_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO releases_artists (release_id, artist, role, position) VALUES (?1, 'Violin Woman', 'main', 1)",
            [r2_id],
        ).unwrap();

        // Track artists
        let t1_ids: Vec<String> = conn
            .prepare("SELECT id FROM tracks WHERE release_id = ?1")
            .unwrap()
            .query_map([r1_id], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        let t2_ids: Vec<String> = conn
            .prepare("SELECT id FROM tracks WHERE release_id = ?1")
            .unwrap()
            .query_map([r2_id], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        for tid in &t1_ids {
            conn.execute(
                "INSERT INTO tracks_artists (track_id, artist, role, position) VALUES (?1, 'Techno Man', 'main', 1)",
                [tid],
            ).unwrap();
        }
        for tid in &t2_ids {
            conn.execute(
                "INSERT INTO tracks_artists (track_id, artist, role, position) VALUES (?1, 'Violin Woman', 'main', 1)",
                [tid],
            ).unwrap();
        }

        // Genres
        conn.execute(
            "INSERT INTO releases_genres (release_id, genre, position) VALUES (?1, 'Techno', 1)",
            [r1_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO releases_genres (release_id, genre, position) VALUES (?1, 'Deep House', 2)", [r1_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO releases_genres (release_id, genre, position) VALUES (?1, 'Modern Classical', 1)", [r2_id],
        ).unwrap();

        // Descriptors
        conn.execute(
            "INSERT INTO releases_descriptors (release_id, descriptor, position) VALUES (?1, 'Warm', 1)", [r1_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO releases_descriptors (release_id, descriptor, position) VALUES (?1, 'Wet', 1)", [r2_id],
        ).unwrap();

        // Labels
        conn.execute(
            "INSERT INTO releases_labels (release_id, label, position) VALUES (?1, 'Silk Music', 1)", [r1_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO releases_labels (release_id, label, position) VALUES (?1, 'Native State', 1)", [r2_id],
        ).unwrap();

        // Re-sync FTS index.
        let all_track_ids: Vec<String> = conn
            .prepare("SELECT id FROM tracks")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        crate::cache::sync_fts_index(&conn, &all_track_ids, &[r1_id.clone(), r2_id.clone()])
            .unwrap();

        (dir, config)
    }

    #[test]
    fn test_find_releases_matching_rule() {
        let (_dir, config) = seeded_config_for_rules();

        // 1. releasetitle:Release 2 (FTS fallback)
        let m = Matcher::parse("releasetitle:Release 2").unwrap();
        let r = find_releases_matching_rule(&config, &m, true).unwrap();
        assert_eq!(r.len(), 1, "releasetitle:Release 2 should match 1");
        assert_eq!(r[0].releasetitle, "Release 2");

        // 2. artist:^Techno Man$ (strict, optimized)
        let m = Matcher::parse("artist:^Techno Man$").unwrap();
        let r = find_releases_matching_rule(&config, &m, true).unwrap();
        assert_eq!(r.len(), 1, "artist:^Techno Man$ should match 1");
        assert_eq!(r[0].releasetitle, "Release 1");

        // 3. genre:^Deep House$ (strict, optimized)
        let m = Matcher::parse("genre:^Deep House$").unwrap();
        let r = find_releases_matching_rule(&config, &m, true).unwrap();
        assert_eq!(r.len(), 1, "genre:^Deep House$ should match 1");
        assert_eq!(r[0].releasetitle, "Release 1");

        // 4. label:^Native State$ (strict, optimized)
        let m = Matcher::parse("label:^Native State$").unwrap();
        let r = find_releases_matching_rule(&config, &m, true).unwrap();
        assert_eq!(r.len(), 1, "label:^Native State$ should match 1");
        assert_eq!(r[0].releasetitle, "Release 2");

        // 5. descriptor:^Wet$ (strict, optimized)
        let m = Matcher::parse("descriptor:^Wet$").unwrap();
        let r = find_releases_matching_rule(&config, &m, true).unwrap();
        assert_eq!(r.len(), 1, "descriptor:^Wet$ should match 1");
        assert_eq!(r[0].releasetitle, "Release 2");
    }

    // -----------------------------------------------------------------------
    // Integration tests: run_actions_on_release (T-6.3)
    // -----------------------------------------------------------------------

    #[test]
    fn test_run_actions_on_release() {
        let dir = TempDir::new().unwrap();
        let config = test_config_for_integration(&dir);
        maybe_invalidate_cache_database(&config).unwrap();

        let release_dir = config.music_source_dir.join("TestRelease2");
        std::fs::create_dir_all(&release_dir).unwrap();
        copy_testdata_file("testdata/Test Release 2/01.m4a", &release_dir, "01.m4a");
        copy_testdata_file("testdata/Test Release 2/02.m4a", &release_dir, "02.m4a");

        update_cache_for_releases(&config, None, false).unwrap();

        let releases = list_releases(&config, None, true).unwrap();
        assert_eq!(releases.len(), 1);
        let release_id = &releases[0].id;

        // Parse action: tracktitle/replace:Bop
        let action = Action::parse("tracktitle/replace:Bop", None, None).unwrap();

        // Run with confirm_yes=true to skip interactive prompt.
        run_actions_on_release(&config, release_id, &[action], false, true).unwrap();

        // Verify both tracks had their title replaced.
        let af1 = AudioTags::from_file(&release_dir.join("01.m4a")).unwrap();
        assert_eq!(
            af1.tracktitle.as_deref(),
            Some("Bop"),
            "track 1 title should be Bop"
        );
        let af2 = AudioTags::from_file(&release_dir.join("02.m4a")).unwrap();
        assert_eq!(
            af2.tracktitle.as_deref(),
            Some("Bop"),
            "track 2 title should be Bop"
        );
    }
}
