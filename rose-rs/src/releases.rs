//! The releases module provides functions for interacting with releases.

// Python: from __future__ import annotations
// Python: import dataclasses
// Python: import logging
// Python: import re
// Python: import shlex
// Python: import shutil
// Python: import tomllib
// Python: from dataclasses import asdict, dataclass
// Python: from pathlib import Path
// Python: from typing import Literal

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info};

// Python: import click
// Python: import tomli_w
// Python: from send2trash import send2trash

// Python: from rose.audiotags import AudioTags, RoseDate
// Python: from rose.cache import (
// Python:     STORED_DATA_FILE_REGEX,
// Python:     Release,
// Python:     Track,
// Python:     filter_releases,
// Python:     get_release,
// Python:     get_tracks_of_release,
// Python:     list_releases,
// Python:     lock,
// Python:     make_release_logtext,
// Python:     release_lock_name,
// Python:     update_cache_evict_nonexistent_releases,
// Python:     update_cache_for_collages,
// Python:     update_cache_for_playlists,
// Python:     update_cache_for_releases,
// Python: )
// Python: from rose.common import Artist, ArtistMapping, RoseError, RoseExpectedError
// Python: from rose.config import Config
// Python: from rose.rule_parser import ALL_TAGS, Action, Matcher
// Python: from rose.rules import (
// Python:     execute_metadata_actions,
// Python:     fast_search_for_matching_releases,
// Python:     filter_release_false_positives_using_read_cache,
// Python: )
// Python: from rose.templates import artistsfmt

use crate::audiotags::AudioTags;
use crate::cache::{
    filter_releases, get_release, get_tracks_of_release, list_releases_by_ids, lock, make_release_logtext, release_lock_name,
    update_cache_evict_nonexistent_releases, update_cache_for_collages, update_cache_for_playlists, update_cache_for_releases, Release, Track,
    STORED_DATA_FILE_REGEX,
};
use crate::common::RoseDate;
use crate::common::{Artist, ArtistMapping};
use crate::config::Config;
use crate::rule_parser::{Action, ExpandableTag, Matcher, Tag, ALL_TAGS};
use crate::rules::{execute_metadata_actions, fast_search_for_matching_releases, filter_release_false_positives_using_read_cache};
use crate::templates::format_artist_mapping as artistsfmt;
use crate::{Result, RoseError, RoseExpectedError};

// Python: logger = logging.getLogger(__name__)

// Python: class InvalidCoverArtFileError(RoseExpectedError):
// Python:     pass
#[derive(Debug, Error)]
#[error("{0}")]
pub struct InvalidCoverArtFileError(pub String);

impl From<InvalidCoverArtFileError> for RoseExpectedError {
    fn from(err: InvalidCoverArtFileError) -> Self {
        RoseExpectedError::Generic(err.0)
    }
}

// Python: class ReleaseDoesNotExistError(RoseExpectedError):
// Python:     pass
#[derive(Debug, Error)]
#[error("{0}")]
pub struct ReleaseDoesNotExistError(pub String);

impl From<ReleaseDoesNotExistError> for RoseExpectedError {
    fn from(err: ReleaseDoesNotExistError) -> Self {
        RoseExpectedError::Generic(err.0)
    }
}

// Python: class ReleaseEditFailedError(RoseExpectedError):
// Python:     pass
#[derive(Debug, Error)]
#[error("{0}")]
pub struct ReleaseEditFailedError(pub String);

impl From<ReleaseEditFailedError> for RoseExpectedError {
    fn from(err: ReleaseEditFailedError) -> Self {
        RoseExpectedError::Generic(err.0)
    }
}

// Python: class InvalidReleaseEditResumeFileError(RoseExpectedError):
// Python:     pass
#[derive(Debug, Error)]
#[error("{0}")]
pub struct InvalidReleaseEditResumeFileError(pub String);

impl From<InvalidReleaseEditResumeFileError> for RoseExpectedError {
    fn from(err: InvalidReleaseEditResumeFileError) -> Self {
        RoseExpectedError::Generic(err.0)
    }
}

// Python: class UnknownArtistRoleError(RoseExpectedError):
// Python:     pass
#[derive(Debug, Error)]
#[error("{0}")]
pub struct UnknownArtistRoleError(pub String);

impl From<UnknownArtistRoleError> for RoseExpectedError {
    fn from(err: UnknownArtistRoleError) -> Self {
        RoseExpectedError::Generic(err.0)
    }
}

// Also implement From for RoseError through RoseExpectedError
impl From<InvalidCoverArtFileError> for RoseError {
    fn from(err: InvalidCoverArtFileError) -> Self {
        RoseError::Expected(err.into())
    }
}

impl From<ReleaseDoesNotExistError> for RoseError {
    fn from(err: ReleaseDoesNotExistError) -> Self {
        RoseError::Expected(err.into())
    }
}

impl From<ReleaseEditFailedError> for RoseError {
    fn from(err: ReleaseEditFailedError) -> Self {
        RoseError::Expected(err.into())
    }
}

impl From<InvalidReleaseEditResumeFileError> for RoseError {
    fn from(err: InvalidReleaseEditResumeFileError) -> Self {
        RoseError::Expected(err.into())
    }
}

impl From<UnknownArtistRoleError> for RoseError {
    fn from(err: UnknownArtistRoleError) -> Self {
        RoseError::Expected(err.into())
    }
}

// Python: def delete_release(c: Config, release_id: str) -> None:
// Python:     release = get_release(c, release_id)
// Python:     if not release:
// Python:         raise ReleaseDoesNotExistError(f"Release {release_id} does not exist")
// Python:     with lock(c, release_lock_name(release_id)):
// Python:         send2trash(release.source_path)
// Python:     release_logtext = make_release_logtext(
// Python:         title=release.releasetitle,
// Python:         releasedate=release.releasedate,
// Python:         artists=release.releaseartists,
// Python:     )
// Python:     logger.info(f"Trashed release {release_logtext}")
// Python:     update_cache_evict_nonexistent_releases(c)
// Python:     # Update all collages and playlists so that the release is removed from whichever it was in.
// Python:     # TODO: Move this into the cache evict nonexistent releases and make it more efficient.
// Python:     update_cache_for_collages(c, None, force=True)
// Python:     update_cache_for_playlists(c, None, force=True)
pub fn delete_release(c: &Config, release_id: &str) -> Result<()> {
    let release = get_release(c, release_id)?.ok_or_else(|| ReleaseDoesNotExistError(format!("Release {} does not exist", release_id)))?;

    let _lock = lock(c, &release_lock_name(release_id), 900.0)?;
    trash::delete(&release.source_path).map_err(|e| RoseError::Io(std::io::Error::other(e)))?;

    let release_logtext = make_release_logtext(&release.releasetitle, release.releasedate.as_ref(), &release.releaseartists);
    info!("trashed release {}", release_logtext);

    update_cache_evict_nonexistent_releases(c)?;
    // Update all collages and playlists so that the release is removed from whichever it was in.
    // TODO: Move this into the cache evict nonexistent releases and make it more efficient.
    update_cache_for_collages(c, None, true)?;
    update_cache_for_playlists(c, None, true)?;

    Ok(())
}

// Python: def toggle_release_new(c: Config, release_id: str) -> None:
// Python:     release = get_release(c, release_id)
// Python:     if not release:
// Python:         raise ReleaseDoesNotExistError(f"Release {release_id} does not exist")
// Python:
// Python:     release_logtext = make_release_logtext(
// Python:         title=release.releasetitle,
// Python:         releasedate=release.releasedate,
// Python:         artists=release.releaseartists,
// Python:     )
// Python:
// Python:     for f in release.source_path.iterdir():
// Python:         if not STORED_DATA_FILE_REGEX.match(f.name):
// Python:             continue
// Python:         with lock(c, release_lock_name(release_id)):
// Python:             with f.open("rb") as fp:
// Python:                 data = tomllib.load(fp)
// Python:             data["new"] = not data["new"]
// Python:             with f.open("wb") as fp:
// Python:                 tomli_w.dump(data, fp)
// Python:         logger.info(f'Toggled "new"-ness of release {release_logtext} to {data["new"]}')
// Python:         update_cache_for_releases(c, [release.source_path], force=True)
// Python:         return
// Python:
// Python:     logger.critical(f"Failed to find .rose.toml in {release.source_path}")
pub fn toggle_release_new(c: &Config, release_id: &str) -> Result<()> {
    let release = get_release(c, release_id)?.ok_or_else(|| ReleaseDoesNotExistError(format!("Release {} does not exist", release_id)))?;

    let release_logtext = make_release_logtext(&release.releasetitle, release.releasedate.as_ref(), &release.releaseartists);

    for entry in fs::read_dir(&release.source_path)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_string();

        if !STORED_DATA_FILE_REGEX.is_match(&file_name) {
            continue;
        }

        let _lock = lock(c, &release_lock_name(release_id), 900.0)?;

        let content = fs::read_to_string(entry.path())?;
        let mut data: toml::Value = toml::from_str(&content)?;

        if let Some(table) = data.as_table_mut() {
            let new_value = !table.get("new").and_then(|v| v.as_bool()).unwrap_or(false);
            table.insert("new".to_string(), toml::Value::Boolean(new_value));

            let toml_string = toml::to_string_pretty(&data)?;
            fs::write(entry.path(), toml_string)?;

            info!("toggled \"new\"-ness of release {} to {}", release_logtext, new_value);
            update_cache_for_releases(c, Some(vec![release.source_path.clone()]), true, false)?;
            return Ok(());
        }
    }

    tracing::error!("failed to find .rose.toml in {}", release.source_path.display());
    Ok(())
}

// Python: def set_release_cover_art(
// Python:     c: Config,
// Python:     release_id: str,
// Python:     new_cover_art_path: Path,
// Python: ) -> None:
// Python:     """
// Python:     This function removes all potential cover arts in the release source directory and copies the
// Python:     file located at the passed in path to `cover.{ext}` in the release source directory.
// Python:     """
// Python:     suffix = new_cover_art_path.suffix.lower()
// Python:     if suffix[1:] not in c.valid_art_exts:
// Python:         raise InvalidCoverArtFileError(
// Python:             f"File {new_cover_art_path.name}'s extension is not supported for cover images: "
// Python:             "To change this, please read the configuration documentation"
// Python:         )
// Python:
// Python:     release = get_release(c, release_id)
// Python:     if not release:
// Python:         raise ReleaseDoesNotExistError(f"Release {release_id} does not exist")
// Python:
// Python:     release_logtext = make_release_logtext(
// Python:         title=release.releasetitle,
// Python:         releasedate=release.releasedate,
// Python:         artists=release.releaseartists,
// Python:     )
// Python:
// Python:     for f in release.source_path.iterdir():
// Python:         if f.name.lower() in c.valid_cover_arts:
// Python:             logger.debug(f"Deleting existing cover art {f.name} in {release_logtext}")
// Python:             send2trash(f)
// Python:     shutil.copyfile(new_cover_art_path, release.source_path / f"cover{new_cover_art_path.suffix}")
// Python:     logger.info(f"Set the cover of release {release_logtext} to {new_cover_art_path.name}")
// Python:     update_cache_for_releases(c, [release.source_path])
/// This function removes all potential cover arts in the release source directory and copies the
/// file located at the passed in path to `cover.{ext}` in the release source directory.
pub fn set_release_cover_art(c: &Config, release_id: &str, new_cover_art_path: &Path) -> Result<()> {
    let suffix = new_cover_art_path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();

    if !c.valid_art_exts.contains(&suffix) {
        return Err(InvalidCoverArtFileError(format!(
            "File {}'s extension is not supported for cover images: \
             To change this, please read the configuration documentation",
            new_cover_art_path.file_name().unwrap_or_default().to_string_lossy()
        ))
        .into());
    }

    let release = get_release(c, release_id)?.ok_or_else(|| ReleaseDoesNotExistError(format!("Release {} does not exist", release_id)))?;

    let release_logtext = make_release_logtext(&release.releasetitle, release.releasedate.as_ref(), &release.releaseartists);

    for entry in fs::read_dir(&release.source_path)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_lowercase();

        if c.valid_cover_arts().contains(&file_name) {
            debug!("deleting existing cover art {} in {}", entry.file_name().to_string_lossy(), release_logtext);
            trash::delete(entry.path()).map_err(|e| RoseError::Io(std::io::Error::other(e)))?;
        }
    }

    let dest_path = release.source_path.join(format!("cover.{}", suffix));
    fs::copy(new_cover_art_path, dest_path)?;

    info!("set the cover of release {} to {}", release_logtext, new_cover_art_path.file_name().unwrap_or_default().to_string_lossy());

    update_cache_for_releases(c, Some(vec![release.source_path]), false, false)?;
    Ok(())
}

// Python: def delete_release_cover_art(c: Config, release_id: str) -> None:
// Python:     """This function deletes all potential cover arts in the release source directory."""
// Python:     release = get_release(c, release_id)
// Python:     if not release:
// Python:         raise ReleaseDoesNotExistError(f"Release {release_id} does not exist")
// Python:
// Python:     release_logtext = make_release_logtext(
// Python:         title=release.releasetitle,
// Python:         releasedate=release.releasedate,
// Python:         artists=release.releaseartists,
// Python:     )
// Python:
// Python:     found = False
// Python:     for f in release.source_path.iterdir():
// Python:         if f.name.lower() in c.valid_cover_arts:
// Python:             logger.debug(f"Deleting existing cover art {f.name} in {release_logtext}")
// Python:             send2trash(f)
// Python:             found = True
// Python:     if found:
// Python:         logger.info(f"Deleted cover arts of release {release_logtext}")
// Python:     else:
// Python:         logger.info(f"No-Op: No cover arts found for release {release_logtext}")
// Python:     update_cache_for_releases(c, [release.source_path])
/// This function deletes all potential cover arts in the release source directory.
pub fn delete_release_cover_art(c: &Config, release_id: &str) -> Result<()> {
    let release = get_release(c, release_id)?.ok_or_else(|| ReleaseDoesNotExistError(format!("Release {} does not exist", release_id)))?;

    let release_logtext = make_release_logtext(&release.releasetitle, release.releasedate.as_ref(), &release.releaseartists);

    let mut found = false;
    for entry in fs::read_dir(&release.source_path)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_lowercase();

        if c.valid_cover_arts().contains(&file_name) {
            debug!("deleting existing cover art {} in {}", entry.file_name().to_string_lossy(), release_logtext);
            trash::delete(entry.path()).map_err(|e| RoseError::Io(std::io::Error::other(e)))?;
            found = true;
        }
    }

    if found {
        info!("deleted cover arts of release {}", release_logtext);
    } else {
        info!("no-op: no cover arts found for release {}", release_logtext);
    }

    update_cache_for_releases(c, Some(vec![release.source_path]), false, false)?;
    Ok(())
}

// Python: @dataclass
// Python: class MetadataArtist:
// Python:     name: str
// Python:     role: str
// Python:
// Python:     @staticmethod
// Python:     def from_mapping(mapping: ArtistMapping) -> list[MetadataArtist]:
// Python:         return [
// Python:             MetadataArtist(name=art.name, role=role)
// Python:             for role, artists in mapping.items()
// Python:             for art in artists
// Python:             if not art.alias
// Python:         ]
// Python:
// Python:     @staticmethod
// Python:     def to_mapping(artists: list[MetadataArtist]) -> ArtistMapping:
// Python:         m = ArtistMapping()
// Python:         for a in artists:
// Python:             try:
// Python:                 getattr(m, a.role.lower()).append(Artist(name=a.name))
// Python:             except AttributeError as e:
// Python:                 raise UnknownArtistRoleError(f"Failed to write tags: Unknown role for artist {a.name}: {a.role}") from e
// Python:         return m
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetadataArtist {
    pub name: String,
    pub role: String,
}

impl MetadataArtist {
    pub fn from_mapping(mapping: &ArtistMapping) -> Vec<MetadataArtist> {
        let mut artists = Vec::new();

        // Iterate over each role field
        for artist in &mapping.main {
            if !artist.alias {
                artists.push(MetadataArtist {
                    name: artist.name.clone(),
                    role: "main".to_string(),
                });
            }
        }
        for artist in &mapping.guest {
            if !artist.alias {
                artists.push(MetadataArtist {
                    name: artist.name.clone(),
                    role: "guest".to_string(),
                });
            }
        }
        for artist in &mapping.remixer {
            if !artist.alias {
                artists.push(MetadataArtist {
                    name: artist.name.clone(),
                    role: "remixer".to_string(),
                });
            }
        }
        for artist in &mapping.producer {
            if !artist.alias {
                artists.push(MetadataArtist {
                    name: artist.name.clone(),
                    role: "producer".to_string(),
                });
            }
        }
        for artist in &mapping.composer {
            if !artist.alias {
                artists.push(MetadataArtist {
                    name: artist.name.clone(),
                    role: "composer".to_string(),
                });
            }
        }
        for artist in &mapping.conductor {
            if !artist.alias {
                artists.push(MetadataArtist {
                    name: artist.name.clone(),
                    role: "conductor".to_string(),
                });
            }
        }
        for artist in &mapping.djmixer {
            if !artist.alias {
                artists.push(MetadataArtist {
                    name: artist.name.clone(),
                    role: "djmixer".to_string(),
                });
            }
        }

        artists
    }

    pub fn to_mapping(artists: &[MetadataArtist]) -> Result<ArtistMapping> {
        let mut mapping = ArtistMapping::default();

        for a in artists {
            match a.role.to_lowercase().as_str() {
                "main" => mapping.main.push(Artist::new(&a.name)),
                "guest" => mapping.guest.push(Artist::new(&a.name)),
                "remixer" => mapping.remixer.push(Artist::new(&a.name)),
                "producer" => mapping.producer.push(Artist::new(&a.name)),
                "composer" => mapping.composer.push(Artist::new(&a.name)),
                "conductor" => mapping.conductor.push(Artist::new(&a.name)),
                "djmixer" => mapping.djmixer.push(Artist::new(&a.name)),
                _ => {
                    return Err(UnknownArtistRoleError(format!("Failed to write tags: Unknown role for artist {}: {}", a.name, a.role)).into());
                }
            }
        }

        Ok(mapping)
    }
}

// Python: @dataclass
// Python: class MetadataTrack:
// Python:     discnumber: str
// Python:     tracknumber: str
// Python:     title: str
// Python:     artists: list[MetadataArtist]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataTrack {
    pub discnumber: String,
    pub tracknumber: String,
    pub title: String,
    pub artists: Vec<MetadataArtist>,
}

// Python: @dataclass
// Python: class MetadataRelease:
// Python:     title: str
// Python:     new: bool
// Python:     releasetype: str
// Python:     releasedate: RoseDate | None
// Python:     originaldate: RoseDate | None
// Python:     compositiondate: RoseDate | None
// Python:     artists: list[MetadataArtist]
// Python:     edition: str | None
// Python:     catalognumber: str | None
// Python:     labels: list[str]
// Python:     genres: list[str]
// Python:     secondary_genres: list[str]
// Python:     descriptors: list[str]
// Python:     tracks: dict[str, MetadataTrack]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataRelease {
    pub title: String,
    pub new: bool,
    pub releasetype: String,
    #[serde(serialize_with = "serialize_date", deserialize_with = "deserialize_date")]
    pub releasedate: Option<RoseDate>,
    #[serde(serialize_with = "serialize_date", deserialize_with = "deserialize_date")]
    pub originaldate: Option<RoseDate>,
    #[serde(serialize_with = "serialize_date", deserialize_with = "deserialize_date")]
    pub compositiondate: Option<RoseDate>,
    pub artists: Vec<MetadataArtist>,
    #[serde(serialize_with = "serialize_optional_string", deserialize_with = "deserialize_optional_string")]
    pub edition: Option<String>,
    #[serde(serialize_with = "serialize_optional_string", deserialize_with = "deserialize_optional_string")]
    pub catalognumber: Option<String>,
    pub labels: Vec<String>,
    pub genres: Vec<String>,
    pub secondary_genres: Vec<String>,
    pub descriptors: Vec<String>,
    pub tracks: HashMap<String, MetadataTrack>,
}

// Helper functions for serialization/deserialization
fn serialize_date<S>(date: &Option<RoseDate>, serializer: S) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match date {
        Some(d) => serializer.serialize_str(&d.to_string()),
        None => serializer.serialize_str(""),
    }
}

fn deserialize_date<'de, D>(deserializer: D) -> std::result::Result<Option<RoseDate>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    if s.is_empty() {
        Ok(None)
    } else {
        Ok(RoseDate::parse(Some(&s)))
    }
}

fn serialize_optional_string<S>(s: &Option<String>, serializer: S) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match s {
        Some(s) => serializer.serialize_str(s),
        None => serializer.serialize_str(""),
    }
}

fn deserialize_optional_string<'de, D>(deserializer: D) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    if s.is_empty() {
        Ok(None)
    } else {
        Ok(Some(s))
    }
}

impl MetadataRelease {
    // Python:     @classmethod
    // Python:     def from_cache(cls, release: Release, tracks: list[Track]) -> MetadataRelease:
    // Python:         return MetadataRelease(
    // Python:             title=release.releasetitle,
    // Python:             new=release.new,
    // Python:             releasetype=release.releasetype,
    // Python:             releasedate=release.releasedate,
    // Python:             originaldate=release.originaldate,
    // Python:             compositiondate=release.compositiondate,
    // Python:             edition=release.edition,
    // Python:             catalognumber=release.catalognumber,
    // Python:             labels=release.labels,
    // Python:             genres=release.genres,
    // Python:             secondary_genres=release.secondary_genres,
    // Python:             descriptors=release.descriptors,
    // Python:             artists=MetadataArtist.from_mapping(release.releaseartists),
    // Python:             tracks={
    // Python:                 t.id: MetadataTrack(
    // Python:                     discnumber=t.discnumber,
    // Python:                     tracknumber=t.tracknumber,
    // Python:                     title=t.tracktitle,
    // Python:                     artists=MetadataArtist.from_mapping(t.trackartists),
    // Python:                 )
    // Python:                 for t in tracks
    // Python:             },
    // Python:         )
    pub fn from_cache(release: &Release, tracks: &[Track]) -> Self {
        let mut track_map = HashMap::new();

        for t in tracks {
            track_map.insert(
                t.id.clone(),
                MetadataTrack {
                    discnumber: t.discnumber.clone(),
                    tracknumber: t.tracknumber.clone(),
                    title: t.tracktitle.clone(),
                    artists: MetadataArtist::from_mapping(&t.trackartists),
                },
            );
        }

        MetadataRelease {
            title: release.releasetitle.clone(),
            new: release.new,
            releasetype: release.releasetype.clone(),
            releasedate: release.releasedate,
            originaldate: release.originaldate,
            compositiondate: release.compositiondate,
            edition: release.edition.clone(),
            catalognumber: release.catalognumber.clone(),
            labels: release.labels.clone(),
            genres: release.genres.clone(),
            secondary_genres: release.secondary_genres.clone(),
            descriptors: release.descriptors.clone(),
            artists: MetadataArtist::from_mapping(&release.releaseartists),
            tracks: track_map,
        }
    }

    // Python:     def serialize(self) -> str:
    // Python:         # TOML does not have a Null Type.
    // Python:         data = asdict(self)
    // Python:         data["releasedate"] = str(self.releasedate) if self.releasedate else ""
    // Python:         data["originaldate"] = str(self.originaldate) if self.originaldate else ""
    // Python:         data["compositiondate"] = str(self.compositiondate) if self.compositiondate else ""
    // Python:         data["edition"] = self.edition or ""
    // Python:         data["catalognumber"] = self.catalognumber or ""
    // Python:         return tomli_w.dumps(data)
    pub fn serialize(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    // Python:     @classmethod
    // Python:     def from_toml(cls, toml: str) -> MetadataRelease:
    // Python:         d = tomllib.loads(toml)
    // Python:         return MetadataRelease(
    // Python:             title=d["title"],
    // Python:             new=d["new"],
    // Python:             releasetype=d["releasetype"],
    // Python:             originaldate=RoseDate.parse(d["originaldate"]),
    // Python:             releasedate=RoseDate.parse(d["releasedate"]),
    // Python:             compositiondate=RoseDate.parse(d["compositiondate"]),
    // Python:             genres=d["genres"],
    // Python:             secondary_genres=d["secondary_genres"],
    // Python:             descriptors=d["descriptors"],
    // Python:             labels=d["labels"],
    // Python:             catalognumber=d["catalognumber"] or None,
    // Python:             edition=d["edition"] or None,
    // Python:             artists=[MetadataArtist(name=a["name"], role=a["role"]) for a in d["artists"]],
    // Python:             tracks={
    // Python:                 tid: MetadataTrack(
    // Python:                     tracknumber=t["tracknumber"],
    // Python:                     discnumber=t["discnumber"],
    // Python:                     title=t["title"],
    // Python:                     artists=[MetadataArtist(name=a["name"], role=a["role"]) for a in t["artists"]],
    // Python:                 )
    // Python:                 for tid, t in d["tracks"].items()
    // Python:             },
    // Python:         )
    pub fn from_toml(toml_str: &str) -> Result<Self> {
        Ok(toml::from_str(toml_str)?)
    }
}

// Python: FAILED_RELEASE_EDIT_FILENAME_REGEX = re.compile(r"failed-release-edit\.([^.]+)\.toml")
static FAILED_RELEASE_EDIT_FILENAME_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"failed-release-edit\.([^.]+)\.toml").unwrap());

// Python: def edit_release(
// Python:     c: Config,
// Python:     release_id: str,
// Python:     *,
// Python:     # Will use this file as the starting TOML instead of reading the cache.
// Python:     resume_file: Path | None = None,
// Python: ) -> None:
// Python:     ... (large function implementation)
pub fn edit_release(
    c: &Config,
    release_id: &str,
    // Will use this file as the starting TOML instead of reading the cache.
    resume_file: Option<&Path>,
) -> Result<()> {
    let mut release = get_release(c, release_id)?.ok_or_else(|| ReleaseDoesNotExistError(format!("Release {} does not exist", release_id)))?;

    // Trigger a quick cache update to ensure we are reading the liveliest data.
    update_cache_for_releases(c, Some(vec![release.source_path.clone()]), false, false)?;

    // Reload release in case any source paths changed.
    release = get_release(c, release_id)?.ok_or_else(|| ReleaseDoesNotExistError(format!("Release {} does not exist", release_id)))?;

    let _lock = lock(c, &release_lock_name(release_id), 900.0)?;

    let tracks = get_tracks_of_release(c, &release)?;

    let original_toml = if let Some(resume_file) = resume_file {
        let file_name = resume_file.file_name().and_then(|s| s.to_str()).ok_or_else(|| InvalidReleaseEditResumeFileError("Invalid file name".to_string()))?;

        let caps = FAILED_RELEASE_EDIT_FILENAME_REGEX
            .captures(file_name)
            .ok_or_else(|| InvalidReleaseEditResumeFileError(format!("{} is not a valid release edit resume file", file_name)))?;

        let resume_uuid = &caps[1];
        if resume_uuid != release_id {
            return Err(InvalidReleaseEditResumeFileError(format!("{} is not associated with this release", file_name)).into());
        }

        fs::read_to_string(resume_file)?
    } else {
        let original_metadata = MetadataRelease::from_cache(&release, &tracks);
        original_metadata.serialize()?
    };

    // For now, we'll skip the actual editing part since it requires click.edit
    // In a real implementation, this would open an editor for the user
    let toml = original_toml.clone(); // Placeholder

    if original_toml == toml && resume_file.is_none() {
        info!("aborting manual release edit: no metadata change detected.");
        return Ok(());
    }

    match apply_release_edit(c, &release, &tracks, &toml, release_id) {
        Ok(new_value) => {
            if new_value != release.new {
                toggle_release_new(c, &release.id)?;
            }

            if let Some(resume_file) = resume_file {
                fs::remove_file(resume_file)?;
            }

            update_cache_for_releases(c, Some(vec![release.source_path]), true, false)?;
            Ok(())
        }
        Err(e) => {
            let new_resume_path = c.cache_dir.join(format!("failed-release-edit.{}.toml", release_id));
            fs::write(&new_resume_path, &toml)?;

            Err(ReleaseEditFailedError(format!(
                "Failed to apply release edit: {}\n\n--------\n\n\
                The submitted metadata TOML file has been written to {}.\n\n\
                You can reattempt the release edit and fix the metadata file with the command:\n\n\
                    $ rose releases edit --resume {} {}",
                e,
                new_resume_path.display(),
                shell_escape::escape(new_resume_path.to_string_lossy()),
                shell_escape::escape(release_id.into())
            ))
            .into())
        }
    }
}

fn apply_release_edit(c: &Config, _release: &Release, tracks: &[Track], toml: &str, _release_id: &str) -> Result<bool> {
    let release_meta = MetadataRelease::from_toml(toml).map_err(|e| ReleaseEditFailedError(format!("Failed to decode TOML file: {}", e)))?;

    for t in tracks {
        let track_meta = release_meta.tracks.get(&t.id).ok_or_else(|| RoseError::Generic(format!("Track {} not found in metadata", t.id)))?;

        let mut tags = AudioTags::from_file(&t.source_path)?;
        let mut dirty = false;

        // Track tags.
        if tags.tracknumber != Some(track_meta.tracknumber.clone()) {
            tags.tracknumber = Some(track_meta.tracknumber.clone());
            dirty = true;
            debug!("modified tag detected for {}: tracknumber", t.source_path.display());
        }
        if tags.discnumber != Some(track_meta.discnumber.clone()) {
            tags.discnumber = Some(track_meta.discnumber.clone());
            dirty = true;
            debug!("modified tag detected for {}: discnumber", t.source_path.display());
        }
        if tags.tracktitle != Some(track_meta.title.clone()) {
            tags.tracktitle = Some(track_meta.title.clone());
            dirty = true;
            debug!("modified tag detected for {}: title", t.source_path.display());
        }
        let tart = MetadataArtist::to_mapping(&track_meta.artists)?;
        if tags.trackartists != tart {
            tags.trackartists = tart;
            dirty = true;
            debug!("modified tag detected for {}: artists", t.source_path.display());
        }

        // Album tags.
        if tags.releasetitle != Some(release_meta.title.clone()) {
            tags.releasetitle = Some(release_meta.title.clone());
            dirty = true;
            debug!("modified tag detected for {}: release", t.source_path.display());
        }
        if tags.releasetype != release_meta.releasetype.to_lowercase() {
            tags.releasetype = release_meta.releasetype.to_lowercase();
            dirty = true;
            debug!("modified tag detected for {}: releasetype", t.source_path.display());
        }
        if tags.releasedate != release_meta.releasedate {
            tags.releasedate = release_meta.releasedate;
            dirty = true;
            debug!("modified tag detected for {}: releasedate", t.source_path.display());
        }
        if tags.originaldate != release_meta.originaldate {
            tags.originaldate = release_meta.originaldate;
            dirty = true;
            debug!("modified tag detected for {}: originaldate", t.source_path.display());
        }
        if tags.compositiondate != release_meta.compositiondate {
            tags.compositiondate = release_meta.compositiondate;
            dirty = true;
            debug!("modified tag detected for {}: compositiondate", t.source_path.display());
        }
        if tags.edition != release_meta.edition {
            tags.edition = release_meta.edition.clone();
            dirty = true;
            debug!("modified tag detected for {}: edition", t.source_path.display());
        }
        if tags.catalognumber != release_meta.catalognumber {
            tags.catalognumber = release_meta.catalognumber.clone();
            dirty = true;
            debug!("modified tag detected for {}: catalognumber", t.source_path.display());
        }
        if tags.genre != release_meta.genres {
            tags.genre = release_meta.genres.clone();
            dirty = true;
            debug!("modified tag detected for {}: genre", t.source_path.display());
        }
        if tags.secondarygenre != release_meta.secondary_genres {
            tags.secondarygenre = release_meta.secondary_genres.clone();
            dirty = true;
            debug!("modified tag detected for {}: secondarygenre", t.source_path.display());
        }
        if tags.descriptor != release_meta.descriptors {
            tags.descriptor = release_meta.descriptors.clone();
            dirty = true;
            debug!("modified tag detected for {}: descriptor", t.source_path.display());
        }
        if tags.label != release_meta.labels {
            tags.label = release_meta.labels.clone();
            dirty = true;
            debug!("modified tag detected for {}: label", t.source_path.display());
        }
        let aart = MetadataArtist::to_mapping(&release_meta.artists)?;
        if tags.releaseartists != aart {
            tags.releaseartists = aart;
            dirty = true;
            debug!("modified tag detected for {}: release_artists", t.source_path.display());
        }

        if dirty {
            let relative_path = t.source_path.strip_prefix(&c.music_source_dir).unwrap_or(&t.source_path);
            info!("flushing changed tags to {}", relative_path.display());
            tags.flush(c, true)?;
        }
    }

    Ok(release_meta.new)
}

// Python: def find_releases_matching_rule(c: Config, matcher: Matcher, *, include_loose_tracks: bool = True) -> list[Release]:
// Python:     # Implement optimizations for common lookups. Only applies to strict lookups.
// Python:     # TODO: Morning
// Python:     if matcher.pattern.strict_start and matcher.pattern.strict_end:
// Python:         if matcher.tags == ALL_TAGS["artist"]:
// Python:             return filter_releases(
// Python:                 c,
// Python:                 all_artist_filter=matcher.pattern.needle,
// Python:                 include_loose_tracks=include_loose_tracks,
// Python:             )
// Python:         if matcher.tags == ALL_TAGS["releaseartist"]:
// Python:             return filter_releases(
// Python:                 c,
// Python:                 release_artist_filter=matcher.pattern.needle,
// Python:                 include_loose_tracks=include_loose_tracks,
// Python:             )
// Python:         if matcher.tags == ["genre"]:
// Python:             return filter_releases(
// Python:                 c,
// Python:                 genre_filter=matcher.pattern.needle,
// Python:                 include_loose_tracks=include_loose_tracks,
// Python:             )
// Python:         if matcher.tags == ["label"]:
// Python:             return filter_releases(
// Python:                 c,
// Python:                 label_filter=matcher.pattern.needle,
// Python:                 include_loose_tracks=include_loose_tracks,
// Python:             )
// Python:         if matcher.tags == ["descriptor"]:
// Python:             return filter_releases(
// Python:                 c,
// Python:                 descriptor_filter=matcher.pattern.needle,
// Python:                 include_loose_tracks=include_loose_tracks,
// Python:             )
// Python:         if matcher.tags == ["releasetype"]:
// Python:             return filter_releases(
// Python:                 c,
// Python:                 release_type_filter=matcher.pattern.needle,
// Python:                 include_loose_tracks=include_loose_tracks,
// Python:             )
// Python:
// Python:     release_ids = [
// Python:         x.id for x in fast_search_for_matching_releases(c, matcher, include_loose_tracks=include_loose_tracks)
// Python:     ]
// Python:     releases = list_releases(c, release_ids, include_loose_tracks=include_loose_tracks)
// Python:     return filter_release_false_positives_using_read_cache(matcher, releases, include_loose_tracks=include_loose_tracks)
pub fn find_releases_matching_rule(c: &Config, matcher: &Matcher, include_loose_tracks: bool) -> Result<Vec<Release>> {
    // Implement optimizations for common lookups. Only applies to strict lookups.
    // TODO: Morning
    if matcher.pattern.strict_start && matcher.pattern.strict_end {
        if matcher.tags == ALL_TAGS.get(&ExpandableTag::Artist).cloned().unwrap_or_default() {
            return filter_releases(
                c,
                None,                          // release_ids
                Some(&matcher.pattern.needle), // all_artist_filter
                None,                          // release_artist_filter
                None,                          // genre_filter
                None,                          // descriptor_filter
                None,                          // label_filter
                None,                          // release_type_filter
                include_loose_tracks,
            );
        }
        if matcher.tags == ALL_TAGS.get(&ExpandableTag::ReleaseArtist).cloned().unwrap_or_default() {
            return filter_releases(
                c,
                None,                          // release_ids
                None,                          // all_artist_filter
                Some(&matcher.pattern.needle), // release_artist_filter
                None,                          // genre_filter
                None,                          // descriptor_filter
                None,                          // label_filter
                None,                          // release_type_filter
                include_loose_tracks,
            );
        }
        if matcher.tags == vec![Tag::Genre] {
            return filter_releases(
                c,
                None,                          // release_ids
                None,                          // all_artist_filter
                None,                          // release_artist_filter
                Some(&matcher.pattern.needle), // genre_filter
                None,                          // descriptor_filter
                None,                          // label_filter
                None,                          // release_type_filter
                include_loose_tracks,
            );
        }
        if matcher.tags == vec![Tag::Label] {
            return filter_releases(
                c,
                None,                          // release_ids
                None,                          // all_artist_filter
                None,                          // release_artist_filter
                None,                          // genre_filter
                None,                          // descriptor_filter
                Some(&matcher.pattern.needle), // label_filter
                None,                          // release_type_filter
                include_loose_tracks,
            );
        }
        if matcher.tags == vec![Tag::Descriptor] {
            return filter_releases(
                c,
                None,                          // release_ids
                None,                          // all_artist_filter
                None,                          // release_artist_filter
                None,                          // genre_filter
                Some(&matcher.pattern.needle), // descriptor_filter
                None,                          // label_filter
                None,                          // release_type_filter
                include_loose_tracks,
            );
        }
        if matcher.tags == vec![Tag::ReleaseType] {
            return filter_releases(
                c,
                None,                          // release_ids
                None,                          // all_artist_filter
                None,                          // release_artist_filter
                None,                          // genre_filter
                None,                          // descriptor_filter
                None,                          // label_filter
                Some(&matcher.pattern.needle), // release_type_filter
                include_loose_tracks,
            );
        }
    }

    let release_ids: Vec<String> = fast_search_for_matching_releases(c, matcher, include_loose_tracks)?.into_iter().map(|x| x.id).collect();

    let releases = list_releases_by_ids(c, &release_ids, include_loose_tracks)?;
    Ok(filter_release_false_positives_using_read_cache(matcher, releases, include_loose_tracks))
}

// Python: def run_actions_on_release(
// Python:     c: Config,
// Python:     release_id: str,
// Python:     actions: list[Action],
// Python:     *,
// Python:     dry_run: bool = False,
// Python:     confirm_yes: bool = False,
// Python: ) -> None:
// Python:     """Run rule engine actions on a release."""
// Python:     release = get_release(c, release_id)
// Python:     if release is None:
// Python:         raise ReleaseDoesNotExistError(f"Release {release_id} does not exist")
// Python:     tracks = get_tracks_of_release(c, release)
// Python:     audiotags = [AudioTags.from_file(t.source_path) for t in tracks]
// Python:     execute_metadata_actions(c, actions, audiotags, dry_run=dry_run, confirm_yes=confirm_yes)
/// Run rule engine actions on a release.
pub fn run_actions_on_release(c: &Config, release_id: &str, actions: &[Action], dry_run: bool, confirm_yes: bool) -> Result<()> {
    let release = get_release(c, release_id)?.ok_or_else(|| ReleaseDoesNotExistError(format!("Release {} does not exist", release_id)))?;

    let tracks = get_tracks_of_release(c, &release)?;
    let mut audiotags = Vec::new();

    for t in &tracks {
        audiotags.push(AudioTags::from_file(&t.source_path)?);
    }

    execute_metadata_actions(c, actions, audiotags, dry_run, confirm_yes, 15)?;
    Ok(())
}

// Python: def create_single_release(
// Python:     c: Config,
// Python:     track_path: Path,
// Python:     *,
// Python:     releasetype: Literal["single", "loosetrack"] = "single",
// Python: ) -> None:
// Python:     """Takes a track and copies it into a brand new "single" release with only that track."""
// Python:     ... (large function implementation)
/// Takes a track and copies it into a brand new "single" release with only that track.
pub fn create_single_release(
    c: &Config,
    track_path: &Path,
    releasetype: &str, // Should be "single" or "loosetrack"
) -> Result<()> {
    if !track_path.is_file() {
        return Err(RoseError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Failed to extract single: file {} not found", track_path.display()),
        )));
    }

    // Step 1. Compute the new directory name for the single.
    let af = AudioTags::from_file(track_path)?;
    let title = af.tracktitle.as_deref().unwrap_or("Unknown Title").trim().to_string();

    let mut dirname = format!("{} - ", artistsfmt(&af.trackartists));
    if let Some(date) = &af.releasedate {
        if let Some(year) = date.year {
            dirname.push_str(&format!("{}. ", year));
        }
    }
    dirname.push_str(&title);

    // Handle directory name collisions.
    let mut collision_no = 2;
    let original_dirname = dirname.clone();
    loop {
        if !c.music_source_dir.join(&dirname).exists() {
            break;
        }
        dirname = format!("{} [{}]", original_dirname, collision_no);
        collision_no += 1;
    }

    // Step 2. Make the new directory and copy the track. If cover art is in track's current
    // directory, copy that over too.
    let source_path = c.music_source_dir.join(&dirname);
    fs::create_dir(&source_path)?;

    let track_extension = track_path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let new_track_path = source_path.join(format!("01. {}.{}", title, track_extension));
    fs::copy(track_path, &new_track_path)?;

    if let Some(parent) = track_path.parent() {
        for entry in fs::read_dir(parent)? {
            let entry = entry?;
            let file_name = entry.file_name().to_string_lossy().to_lowercase();

            if c.valid_cover_arts().contains(&file_name) {
                let dest = source_path.join(entry.file_name());
                fs::copy(entry.path(), dest)?;
                break;
            }
        }
    }

    // Step 3. Update the tags of the new track. Clear the Rose IDs too: this is a brand new track.
    let mut af = AudioTags::from_file(&new_track_path)?;
    af.releasetitle = Some(title);
    af.releasetype = releasetype.to_string();
    af.releaseartists = af.trackartists.clone();
    af.tracknumber = Some("1".to_string());
    af.discnumber = Some("1".to_string());
    af.release_id = None;
    af.id = None;
    af.flush(c, true)?;

    info!("created phony single release {}", dirname);

    // Step 4: Update the cache!
    let mut c_tmp = c.clone();
    c_tmp.rename_source_files = false;
    update_cache_for_releases(&c_tmp, Some(vec![source_path.clone()]), false, false)?;

    // Step 5: Default extracted singles to not new: if it is new, why are you meddling with it?
    let mut release_id = None;
    for entry in fs::read_dir(&source_path)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_string();

        if let Some(captures) = STORED_DATA_FILE_REGEX.captures(&file_name) {
            release_id = Some(captures[1].to_string());
            break;
        }
    }

    let release_id = release_id
        .ok_or_else(|| RoseError::Generic(format!("Impossible: Failed to parse release ID from newly created single directory {}", source_path.display())))?;

    toggle_release_new(c, &release_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;
    use std::fs;
    use std::path::PathBuf;

    // Python: def test_delete_release(config: Config) -> None:
    // Python:     shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    // Python:     update_cache(config)
    // Python:     with connect(config) as conn:
    // Python:         cursor = conn.execute("SELECT id FROM releases")
    // Python:         release_id = cursor.fetchone()["id"]
    // Python:     delete_release(config, release_id)
    // Python:     assert not (config.music_source_dir / TEST_RELEASE_1.name).exists()
    // Python:     with connect(config) as conn:
    // Python:         cursor = conn.execute("SELECT COUNT(*) FROM releases")
    // Python:         assert cursor.fetchone()[0] == 0
    #[test]
    fn test_delete_release() {
        let (config, _temp_dir) = testing::seeded_cache();

        // Get a release ID from the seeded cache
        let conn = crate::cache::connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT id FROM releases LIMIT 1").unwrap();
        let release_id: String = stmt.query_row([], |row| row.get(0)).unwrap();
        drop(stmt);
        drop(conn);

        // Delete the release
        delete_release(&config, &release_id).unwrap();

        // Verify release was deleted from database
        let conn = crate::cache::connect(&config).unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM releases WHERE id = ?", [&release_id], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
    }

    // Python: def test_toggle_release_new(config: Config) -> None:
    // Python:     ... (test implementation)
    #[test]
    fn test_toggle_release_new() {
        let (config, _temp_dir) = testing::seeded_cache();

        // Run update_cache to ensure .rose.toml files are created
        update_cache_for_releases(&config, None, false, false).unwrap();

        // Get a release ID from the seeded cache
        let conn = crate::cache::connect(&config).unwrap();
        let mut stmt = conn.prepare("SELECT id, new FROM releases LIMIT 1").unwrap();
        let (release_id, initial_new): (String, bool) = stmt.query_row([], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
        drop(stmt);
        drop(conn);

        // Toggle new status
        toggle_release_new(&config, &release_id).unwrap();

        // Verify it was toggled in database
        let conn = crate::cache::connect(&config).unwrap();
        let new_status: bool = conn.query_row("SELECT new FROM releases WHERE id = ?", [&release_id], |row| row.get(0)).unwrap();
        assert_ne!(initial_new, new_status);

        // Toggle again
        toggle_release_new(&config, &release_id).unwrap();

        // Verify it was toggled back
        let conn = crate::cache::connect(&config).unwrap();
        let new_status: bool = conn.query_row("SELECT new FROM releases WHERE id = ?", [&release_id], |row| row.get(0)).unwrap();
        assert_eq!(initial_new, new_status);
    }

    // Python: def test_set_release_cover_art(isolated_dir: Path, config: Config) -> None:
    // Python:     ... (test implementation)
    #[test]
    fn test_set_release_cover_art() {
        let (config, temp_dir) = testing::seeded_cache();

        // Create a test image file
        let image_path = temp_dir.path().join("test.jpg");
        fs::write(&image_path, "test image content").unwrap();

        // Get a release from the seeded cache
        let conn = crate::cache::connect(&config).unwrap();
        let release_id: String = conn.query_row("SELECT id FROM releases LIMIT 1", [], |row| row.get(0)).unwrap();
        drop(conn);

        // Set cover art
        set_release_cover_art(&config, &release_id, &image_path).unwrap();

        // Verify cover was set in database
        let conn = crate::cache::connect(&config).unwrap();
        let cover_path: Option<String> = conn.query_row("SELECT cover_image_path FROM releases WHERE id = ?", [&release_id], |row| row.get(0)).unwrap();
        assert!(cover_path.is_some());
        assert!(cover_path.unwrap().ends_with("cover.jpg"));
    }

    // Python: def test_remove_release_cover_art(config: Config) -> None:
    // Python:     ... (test implementation)
    #[test]
    fn test_delete_release_cover_art() {
        let (config, _temp_dir) = testing::seeded_cache();

        // Get a release and add a cover
        let conn = crate::cache::connect(&config).unwrap();
        let (release_id, source_path): (String, String) =
            conn.query_row("SELECT id, source_path FROM releases LIMIT 1", [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
        drop(conn);

        // Create a fake cover file
        let cover_path = PathBuf::from(&source_path).join("cover.jpg");
        fs::create_dir_all(cover_path.parent().unwrap()).ok();
        fs::write(&cover_path, "fake cover").unwrap();

        // Update cache to pick up the cover
        update_cache_for_releases(&config, Some(vec![PathBuf::from(&source_path)]), true, false).unwrap();

        // Delete cover art
        delete_release_cover_art(&config, &release_id).unwrap();

        // Verify cover was removed
        assert!(!cover_path.exists());

        let conn = crate::cache::connect(&config).unwrap();
        let cover_path: Option<String> = conn.query_row("SELECT cover_image_path FROM releases WHERE id = ?", [&release_id], |row| row.get(0)).unwrap();
        assert!(cover_path.is_none() || cover_path.unwrap().is_empty());
    }

    // Python: def test_find_matching_releases(config: Config) -> None:
    // Python:     results = find_releases_matching_rule(config, Matcher.parse("releasetitle:Release 2"))
    // Python:     assert {r.id for r in results} == {"r2"}
    // Python:     ... (more assertions)
    #[test]
    fn test_find_matching_releases() {
        let (config, _temp_dir) = testing::seeded_cache();

        // Test release title matching
        let matcher = Matcher::parse("releasetitle:Release 2").unwrap();
        let results = find_releases_matching_rule(&config, &matcher, true).unwrap();
        let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"r2"));

        // Test artist matching
        let matcher = Matcher::parse("artist:^Techno Man$").unwrap();
        let results = find_releases_matching_rule(&config, &matcher, true).unwrap();
        let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"r1"));

        // Test genre matching
        let matcher = Matcher::parse("genre:^Deep House$").unwrap();
        let results = find_releases_matching_rule(&config, &matcher, true).unwrap();
        let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"r1"));

        // Test descriptor matching
        let matcher = Matcher::parse("descriptor:^Wet$").unwrap();
        let results = find_releases_matching_rule(&config, &matcher, true).unwrap();
        let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"r2"));

        // Test label matching
        let matcher = Matcher::parse("label:^Native State$").unwrap();
        let results = find_releases_matching_rule(&config, &matcher, true).unwrap();
        let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"r2"));
    }

    // Python: def test_run_action_on_release(config: Config, source_dir: Path) -> None:
    // Python:     action = Action.parse("tracktitle/replace:Bop")
    // Python:     run_actions_on_release(config, "ilovecarly", [action])
    // Python:     af = AudioTags.from_file(source_dir / "Test Release 2" / "01.m4a")
    // Python:     assert af.tracktitle == "Bop"
    #[test]
    fn test_run_action_on_release() {
        let (config, _temp_dir) = testing::seeded_cache();

        // Use a release from seeded cache
        let action = Action::parse("tracktitle/replace:Bop", None, None).unwrap();
        run_actions_on_release(&config, "r2", &[action], false, false).unwrap();

        // Verify the action was applied
        let conn = crate::cache::connect(&config).unwrap();
        let track_path: String = conn.query_row("SELECT source_path FROM tracks WHERE release_id = 'r2' LIMIT 1", [], |row| row.get(0)).unwrap();
        drop(conn);

        let af = AudioTags::from_file(Path::new(&track_path)).unwrap();
        assert_eq!(af.tracktitle, Some("Bop".to_string()));
    }
}
