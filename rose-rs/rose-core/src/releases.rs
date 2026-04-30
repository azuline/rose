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

/// Interactive metadata editor for a release.
///
/// Opens `$EDITOR` with a TOML file representing the release metadata.
/// On save, applies per-track per-field dirty checking to minimize disk writes.
/// On failure, saves the edited TOML to a resume file.
pub fn edit_release(
    c: &Config,
    release_id: &str,
    resume_file: Option<&Path>,
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

    // Open $EDITOR via a temp file
    let toml = open_editor(&original_toml)?;

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
}
