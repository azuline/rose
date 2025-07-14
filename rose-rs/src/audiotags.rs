use crate::common::{Artist, ArtistMapping, uniq};
use crate::config::Config;
use crate::error::{Result, RoseError, RoseExpectedError};
use crate::genre_hierarchy::get_transitive_parent_genres;
use lazy_static::lazy_static;
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::{Tag, TagType};
use lofty::config::WriteOptions;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

lazy_static! {
    static ref TAG_SPLITTER_REGEX: Regex = Regex::new(r" \\\\ | / |; ?| vs\. ").unwrap();
    static ref YEAR_REGEX: Regex = Regex::new(r"^\d{4}$").unwrap();
    static ref DATE_REGEX: Regex = Regex::new(r"^(\d{4})-(\d{2})-(\d{2})").unwrap();
}

pub const SUPPORTED_AUDIO_EXTENSIONS: &[&str] = &[".mp3", ".m4a", ".ogg", ".opus", ".flac"];

pub const SUPPORTED_RELEASE_TYPES: &[&str] = &[
    "album",
    "single",
    "ep",
    "compilation",
    "anthology",
    "soundtrack",
    "live",
    "remix",
    "djmix",
    "mixtape",
    "other",
    "bootleg",
    "loosetrack",
    "demo",
    "unknown",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedFiletypeError(pub String);

impl std::fmt::Display for UnsupportedFiletypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for UnsupportedFiletypeError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedTagValueTypeError(pub String);

impl std::fmt::Display for UnsupportedTagValueTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for UnsupportedTagValueTypeError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoseDate {
    pub year: i32,
    pub month: Option<u32>,
    pub day: Option<u32>,
}

impl RoseDate {
    pub fn parse(value: Option<&str>) -> Option<Self> {
        let value = value?;
        
        // Try parsing as just year
        if let Ok(year) = value.parse::<i32>() {
            if (1000..=9999).contains(&year) {
                return Some(RoseDate {
                    year,
                    month: None,
                    day: None,
                });
            }
        }
        
        // Try parsing as full date
        if let Some(captures) = DATE_REGEX.captures(value) {
            if let (Some(year), Some(month), Some(day)) = (
                captures.get(1).and_then(|m| m.as_str().parse::<i32>().ok()),
                captures.get(2).and_then(|m| m.as_str().parse::<u32>().ok()),
                captures.get(3).and_then(|m| m.as_str().parse::<u32>().ok()),
            ) {
                return Some(RoseDate {
                    year,
                    month: Some(month),
                    day: Some(day),
                });
            }
        }
        
        None
    }
}

impl std::fmt::Display for RoseDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.month, self.day) {
            (None, None) => write!(f, "{:04}", self.year),
            (Some(month), Some(day)) => write!(f, "{:04}-{:02}-{:02}", self.year, month, day),
            (Some(month), None) => write!(f, "{:04}-{:02}-01", self.year, month),
            (None, Some(_)) => write!(f, "{:04}-01-01", self.year),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AudioTags {
    pub id: Option<String>,
    pub release_id: Option<String>,
    
    pub tracktitle: Option<String>,
    pub tracknumber: Option<String>,
    pub tracktotal: Option<i32>,
    pub discnumber: Option<String>,
    pub disctotal: Option<i32>,
    pub trackartists: ArtistMapping,
    
    pub releasetitle: Option<String>,
    pub releasetype: String,
    pub releasedate: Option<RoseDate>,
    pub originaldate: Option<RoseDate>,
    pub compositiondate: Option<RoseDate>,
    pub genre: Vec<String>,
    pub secondarygenre: Vec<String>,
    pub descriptor: Vec<String>,
    pub edition: Option<String>,
    pub label: Vec<String>,
    pub catalognumber: Option<String>,
    pub releaseartists: ArtistMapping,
    
    pub duration_sec: i32,
    pub path: PathBuf,
}

impl AudioTags {
    pub fn from_file(path: &Path) -> Result<Self> {
        let extension = path.extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e.to_lowercase()));
        
        if !extension.as_ref().is_some_and(|ext| {
            SUPPORTED_AUDIO_EXTENSIONS.contains(&ext.as_str())
        }) {
            return Err(RoseError::Expected(RoseExpectedError::Generic(
                format!("{} not a supported filetype", extension.unwrap_or_else(|| "Unknown".to_string()))
            )));
        }
        
        let tagged_file = Probe::open(path)
            .map_err(|e| RoseError::Expected(RoseExpectedError::Generic(
                format!("Failed to open file: {e}")
            )))?
            .read()
            .map_err(|e| RoseError::Expected(RoseExpectedError::Generic(
                format!("Failed to read file: {e}")
            )))?;
        
        let properties = tagged_file.properties();
        let duration_sec = properties.duration().as_secs() as i32;
        
        // Get the primary tag for the file type
        let tag = tagged_file.primary_tag()
            .or_else(|| tagged_file.first_tag())
            .ok_or_else(|| RoseError::Expected(RoseExpectedError::Generic(
                "No tags found in file".to_string()
            )))?;
        
        // Extract tags based on format
        let (id, release_id) = match tag.tag_type() {
            TagType::Id3v2 => (
                get_id3_txxx(tag, "ROSEID"),
                get_id3_txxx(tag, "ROSERELEASEID"),
            ),
            TagType::Mp4Ilst => (
                get_mp4_freeform(tag, "net.sunsetglow.rose:ID"),
                get_mp4_freeform(tag, "net.sunsetglow.rose:RELEASEID"),
            ),
            _ => (
                tag.get_string(&ItemKey::Unknown("roseid".to_string())).map(|s| s.to_string()),
                tag.get_string(&ItemKey::Unknown("rosereleaseid".to_string())).map(|s| s.to_string()),
            ),
        };
        
        let tracktitle = tag.title().map(|s| s.to_string());
        let releasetitle = tag.album().map(|s| s.to_string());
        
        // Parse track/disc numbers
        let (tracknumber, tracktotal) = parse_track_number(tag);
        let (discnumber, disctotal) = parse_disc_number(tag);
        
        // Parse dates
        let year_str = tag.year().map(|y| y.to_string());
        let releasedate = RoseDate::parse(
            tag.get_string(&ItemKey::RecordingDate)
                .or(year_str.as_deref())
        );
        
        let originaldate = match tag.tag_type() {
            TagType::Id3v2 => RoseDate::parse(tag.get_string(&ItemKey::OriginalReleaseDate)),
            TagType::Mp4Ilst => RoseDate::parse(get_mp4_freeform(tag, "net.sunsetglow.rose:ORIGINALDATE").as_deref()),
            _ => RoseDate::parse(tag.get_string(&ItemKey::Unknown("originaldate".to_string()))),
        };
        
        let compositiondate = match tag.tag_type() {
            TagType::Id3v2 => RoseDate::parse(get_id3_txxx(tag, "COMPOSITIONDATE").as_deref()),
            TagType::Mp4Ilst => RoseDate::parse(get_mp4_freeform(tag, "net.sunsetglow.rose:COMPOSITIONDATE").as_deref()),
            _ => RoseDate::parse(tag.get_string(&ItemKey::Unknown("compositiondate".to_string()))),
        };
        
        // Parse genres and other multi-value fields
        let genre = split_genre_tag(tag.genre().as_deref());
        let secondarygenre = match tag.tag_type() {
            TagType::Id3v2 => split_genre_tag(get_id3_txxx(tag, "SECONDARYGENRE").as_deref()),
            TagType::Mp4Ilst => split_genre_tag(get_mp4_freeform(tag, "net.sunsetglow.rose:SECONDARYGENRE").as_deref()),
            _ => split_genre_tag(tag.get_string(&ItemKey::Unknown("secondarygenre".to_string()))),
        };
        
        let descriptor = match tag.tag_type() {
            TagType::Id3v2 => split_tag(get_id3_txxx(tag, "DESCRIPTOR").as_deref()),
            TagType::Mp4Ilst => split_tag(get_mp4_freeform(tag, "net.sunsetglow.rose:DESCRIPTOR").as_deref()),
            _ => split_tag(tag.get_string(&ItemKey::Unknown("descriptor".to_string()))),
        };
        
        let label = split_tag(tag.get_string(&ItemKey::Label));
        
        let catalognumber = match tag.tag_type() {
            TagType::Id3v2 => get_id3_txxx(tag, "CATALOGNUMBER"),
            TagType::Mp4Ilst => get_mp4_freeform(tag, "com.apple.iTunes:CATALOGNUMBER"),
            _ => tag.get_string(&ItemKey::Unknown("catalognumber".to_string())).map(|s| s.to_string()),
        };
        
        let edition = match tag.tag_type() {
            TagType::Id3v2 => get_id3_txxx(tag, "EDITION"),
            TagType::Mp4Ilst => get_mp4_freeform(tag, "net.sunsetglow.rose:EDITION"),
            _ => tag.get_string(&ItemKey::Unknown("edition".to_string())).map(|s| s.to_string()),
        };
        
        let releasetype = normalize_releasetype(match tag.tag_type() {
            TagType::Id3v2 => get_id3_txxx(tag, "RELEASETYPE")
                .or_else(|| get_id3_txxx(tag, "MusicBrainz Album Type")),
            TagType::Mp4Ilst => get_mp4_freeform(tag, "com.apple.iTunes:RELEASETYPE")
                .or_else(|| get_mp4_freeform(tag, "com.apple.iTunes:MusicBrainz Album Type")),
            _ => tag.get_string(&ItemKey::Unknown("releasetype".to_string())).map(|s| s.to_string()),
        }.as_deref());
        
        // Parse artists
        let track_artists_main = split_tag(tag.artist().as_deref());
        let release_artists_main = split_tag(tag.get_string(&ItemKey::AlbumArtist));
        
        // Get additional artist roles based on tag type
        let (remixer, producer, composer, conductor, djmixer) = match tag.tag_type() {
            TagType::Id3v2 => {
                // TODO: Parse TIPL/IPLS frames for producer/djmixer
                (
                    split_tag(tag.get_string(&ItemKey::Remixer)),
                    vec![],
                    split_tag(tag.get_string(&ItemKey::Composer)),
                    split_tag(tag.get_string(&ItemKey::Conductor)),
                    vec![],
                )
            }
            TagType::Mp4Ilst => (
                split_tag(get_mp4_freeform(tag, "com.apple.iTunes:REMIXER").as_deref()),
                split_tag(get_mp4_freeform(tag, "com.apple.iTunes:PRODUCER").as_deref()),
                split_tag(tag.get_string(&ItemKey::Composer)),
                split_tag(get_mp4_freeform(tag, "com.apple.iTunes:CONDUCTOR").as_deref()),
                split_tag(get_mp4_freeform(tag, "com.apple.iTunes:DJMIXER").as_deref()),
            ),
            _ => (
                split_tag(tag.get_string(&ItemKey::Unknown("remixer".to_string()))),
                split_tag(tag.get_string(&ItemKey::Unknown("producer".to_string()))),
                split_tag(tag.get_string(&ItemKey::Composer)),
                split_tag(tag.get_string(&ItemKey::Unknown("conductor".to_string()))),
                split_tag(tag.get_string(&ItemKey::Unknown("djmixer".to_string()))),
            ),
        };
        
        let trackartists = parse_artist_string(
            track_artists_main.join(r" \\ ").as_str(),
            remixer.join(r" \\ ").as_str(),
            composer.join(r" \\ ").as_str(),
            conductor.join(r" \\ ").as_str(),
            producer.join(r" \\ ").as_str(),
            djmixer.join(r" \\ ").as_str(),
        );
        
        let releaseartists = parse_artist_string(
            release_artists_main.join(r" \\ ").as_str(),
            "",
            "",
            "",
            "",
            "",
        );
        
        Ok(AudioTags {
            id,
            release_id,
            tracktitle,
            tracknumber,
            tracktotal,
            discnumber,
            disctotal,
            trackartists,
            releasetitle,
            releasetype,
            releasedate,
            originaldate,
            compositiondate,
            genre,
            secondarygenre,
            descriptor,
            edition,
            label,
            catalognumber,
            releaseartists,
            duration_sec,
            path: path.to_path_buf(),
        })
    }
    
    pub fn flush(&mut self, config: &Config) -> Result<()> {
        // Normalize release type
        self.releasetype = normalize_releasetype(Some(&self.releasetype));
        
        // Open the file for writing
        let mut tagged_file = Probe::open(&self.path)
            .map_err(|e| RoseError::Expected(RoseExpectedError::Generic(
                format!("Failed to open file for writing: {e}")
            )))?
            .read()
            .map_err(|e| RoseError::Expected(RoseExpectedError::Generic(
                format!("Failed to read file for writing: {e}")
            )))?;
        
        let tag = if let Some(tag) = tagged_file.primary_tag_mut() {
            tag
        } else if let Some(tag) = tagged_file.first_tag_mut() {
            tag
        } else {
            return Err(RoseError::Expected(RoseExpectedError::Generic(
                "No tags found in file".to_string()
            )));
        };
        
        // Clear and set basic tags
        tag.set_title(self.tracktitle.clone().unwrap_or_default());
        tag.set_album(self.releasetitle.clone().unwrap_or_default());
        
        // Set track and disc numbers
        if let Some(num) = &self.tracknumber {
            tag.set_track(num.parse().unwrap_or(0));
        }
        if let Some(total) = self.tracktotal {
            tag.set_track_total(total as u32);
        }
        if let Some(num) = &self.discnumber {
            tag.set_disk(num.parse().unwrap_or(0));
        }
        if let Some(total) = self.disctotal {
            tag.set_disk_total(total as u32);
        }
        
        // Set dates
        if let Some(date) = &self.releasedate {
            tag.set_year(date.year as u32);
        }
        
        // Set genres
        tag.set_genre(format_genre_tag(config, &self.genre));
        
        // Set artists
        tag.set_artist(format_artist_string(&self.trackartists));
        tag.insert_text(ItemKey::AlbumArtist, format_artist_string(&self.releaseartists));
        
        // Format-specific tag writing
        match tag.tag_type() {
            TagType::Id3v2 => {
                set_id3_txxx(tag, "ROSEID", self.id.as_deref());
                set_id3_txxx(tag, "ROSERELEASEID", self.release_id.as_deref());
                set_id3_txxx(tag, "COMPOSITIONDATE", self.compositiondate.as_ref().map(|d| d.to_string()).as_deref());
                set_id3_txxx(tag, "SECONDARYGENRE", Some(&format_genre_tag(config, &self.secondarygenre)));
                set_id3_txxx(tag, "DESCRIPTOR", Some(&self.descriptor.join(";")));
                set_id3_txxx(tag, "CATALOGNUMBER", self.catalognumber.as_deref());
                set_id3_txxx(tag, "EDITION", self.edition.as_deref());
                set_id3_txxx(tag, "RELEASETYPE", Some(&self.releasetype));
                
                // Set label
                tag.insert_text(ItemKey::Label, self.label.join(";"));
                
                // Clear artist role tags since we encode everything in the main artist tag
                tag.remove_key(&ItemKey::Remixer);
                tag.remove_key(&ItemKey::Composer);
                tag.remove_key(&ItemKey::Conductor);
            }
            TagType::Mp4Ilst => {
                set_mp4_freeform(tag, "net.sunsetglow.rose:ID", self.id.as_deref());
                set_mp4_freeform(tag, "net.sunsetglow.rose:RELEASEID", self.release_id.as_deref());
                set_mp4_freeform(tag, "net.sunsetglow.rose:ORIGINALDATE", self.originaldate.as_ref().map(|d| d.to_string()).as_deref());
                set_mp4_freeform(tag, "net.sunsetglow.rose:COMPOSITIONDATE", self.compositiondate.as_ref().map(|d| d.to_string()).as_deref());
                set_mp4_freeform(tag, "net.sunsetglow.rose:SECONDARYGENRE", Some(&format_genre_tag(config, &self.secondarygenre)));
                set_mp4_freeform(tag, "net.sunsetglow.rose:DESCRIPTOR", Some(&self.descriptor.join(";")));
                set_mp4_freeform(tag, "com.apple.iTunes:LABEL", Some(&self.label.join(";")));
                set_mp4_freeform(tag, "com.apple.iTunes:CATALOGNUMBER", self.catalognumber.as_deref());
                set_mp4_freeform(tag, "net.sunsetglow.rose:EDITION", self.edition.as_deref());
                set_mp4_freeform(tag, "com.apple.iTunes:RELEASETYPE", Some(&self.releasetype));
                
                // Clear artist role tags
                remove_mp4_freeform(tag, "com.apple.iTunes:REMIXER");
                remove_mp4_freeform(tag, "com.apple.iTunes:PRODUCER");
                tag.remove_key(&ItemKey::Composer);
                remove_mp4_freeform(tag, "com.apple.iTunes:CONDUCTOR");
                remove_mp4_freeform(tag, "com.apple.iTunes:DJMIXER");
            }
            _ => {
                // FLAC/Vorbis comments
                tag.insert_text(ItemKey::Unknown("roseid".to_string()), self.id.clone().unwrap_or_default());
                tag.insert_text(ItemKey::Unknown("rosereleaseid".to_string()), self.release_id.clone().unwrap_or_default());
                tag.insert_text(ItemKey::Unknown("originaldate".to_string()), self.originaldate.as_ref().map(|d| d.to_string()).unwrap_or_default());
                tag.insert_text(ItemKey::Unknown("compositiondate".to_string()), self.compositiondate.as_ref().map(|d| d.to_string()).unwrap_or_default());
                tag.insert_text(ItemKey::Unknown("secondarygenre".to_string()), format_genre_tag(config, &self.secondarygenre));
                tag.insert_text(ItemKey::Unknown("descriptor".to_string()), self.descriptor.join(";"));
                tag.insert_text(ItemKey::Label, self.label.join(";"));
                tag.insert_text(ItemKey::Unknown("catalognumber".to_string()), self.catalognumber.clone().unwrap_or_default());
                tag.insert_text(ItemKey::Unknown("edition".to_string()), self.edition.clone().unwrap_or_default());
                tag.insert_text(ItemKey::Unknown("releasetype".to_string()), self.releasetype.clone());
                
                // Clear artist role tags
                tag.remove_key(&ItemKey::Unknown("remixer".to_string()));
                tag.remove_key(&ItemKey::Unknown("producer".to_string()));
                tag.remove_key(&ItemKey::Composer);
                tag.remove_key(&ItemKey::Unknown("conductor".to_string()));
                tag.remove_key(&ItemKey::Unknown("djmixer".to_string()));
            }
        }
        
        // Save the file
        tagged_file.save_to_path(&self.path, WriteOptions::default())
            .map_err(|e| RoseError::Expected(RoseExpectedError::Generic(
                format!("Failed to save tags: {e}")
            )))?;
        
        Ok(())
    }
}

pub fn normalize_releasetype(rt: Option<&str>) -> String {
    match rt {
        Some(r) => {
            let lower = r.to_lowercase();
            if SUPPORTED_RELEASE_TYPES.contains(&lower.as_str()) {
                lower
            } else {
                "unknown".to_string()
            }
        }
        None => "unknown".to_string(),
    }
}

fn parse_track_number(tag: &Tag) -> (Option<String>, Option<i32>) {
    if let Some(track) = tag.track() {
        (Some(track.to_string()), tag.track_total().map(|t| t as i32))
    } else {
        (None, None)
    }
}

fn parse_disc_number(tag: &Tag) -> (Option<String>, Option<i32>) {
    if let Some(disc) = tag.disk() {
        (Some(disc.to_string()), tag.disk_total().map(|t| t as i32))
    } else {
        (None, None)
    }
}

pub fn split_tag(s: Option<&str>) -> Vec<String> {
    match s {
        Some(s) if !s.is_empty() => TAG_SPLITTER_REGEX.split(s).map(|s| s.to_string()).collect(),
        _ => vec![],
    }
}

pub fn split_genre_tag(s: Option<&str>) -> Vec<String> {
    match s {
        Some(s) if !s.is_empty() => {
            // Remove parent genres suffix if present
            let s = if let Some(idx) = s.find(r"\\PARENTS:\\") {
                &s[..idx]
            } else {
                s
            };
            TAG_SPLITTER_REGEX.split(s).map(|s| s.to_string()).collect()
        }
        _ => vec![],
    }
}

pub fn format_genre_tag(config: &Config, genres: &[String]) -> String {
    if !config.write_parent_genres || genres.is_empty() {
        return genres.join(";");
    }
    
    let mut parent_genres = Vec::new();
    for genre in genres {
        if let Some(parents) = get_transitive_parent_genres(genre) {
            for parent in parents {
                if !genres.contains(&parent) && !parent_genres.contains(&parent) {
                    parent_genres.push(parent);
                }
            }
        }
    }
    
    if parent_genres.is_empty() {
        genres.join(";")
    } else {
        parent_genres.sort();
        format!("{}\\\\PARENTS:\\\\{}", genres.join(";"), parent_genres.join(";"))
    }
}

pub fn parse_artist_string(
    main: &str,
    remixer: &str,
    composer: &str,
    conductor: &str,
    producer: &str,
    djmixer: &str,
) -> ArtistMapping {
    let mut main = main.to_string();
    let mut li_main = vec![];
    let mut li_conductor = split_tag(Some(conductor));
    let mut li_guests = vec![];
    let mut li_remixer = split_tag(Some(remixer));
    let mut li_composer = split_tag(Some(composer));
    let mut li_producer = split_tag(Some(producer));
    let mut li_dj = split_tag(Some(djmixer));
    
    // Parse special patterns in main artist string
    if let Some(idx) = main.find(" produced by ") {
        let producer_part = main[idx + 13..].to_string();
        main = main[..idx].to_string();
        li_producer.extend(split_tag(Some(&producer_part)));
    }
    
    if let Some(idx) = main.find(" remixed by ") {
        let remixer_part = main[idx + 12..].to_string();
        main = main[..idx].to_string();
        li_remixer.extend(split_tag(Some(&remixer_part)));
    }
    
    if let Some(idx) = main.find(" feat. ") {
        let guest_part = main[idx + 7..].to_string();
        main = main[..idx].to_string();
        li_guests.extend(split_tag(Some(&guest_part)));
    }
    
    if let Some(idx) = main.find(" pres. ") {
        let dj_part = main[..idx].to_string();
        li_dj.extend(split_tag(Some(&dj_part)));
        main = main[idx + 7..].to_string();
    }
    
    if let Some(idx) = main.find(" performed by ") {
        let composer_part = main[..idx].to_string();
        li_composer.extend(split_tag(Some(&composer_part)));
        main = main[idx + 14..].to_string();
    }
    
    if let Some(idx) = main.find(" under. ") {
        let conductor_part = main[idx + 8..].to_string();
        main = main[..idx].to_string();
        li_conductor.extend(split_tag(Some(&conductor_part)));
    }
    
    if !main.is_empty() {
        li_main.extend(split_tag(Some(&main)));
    }
    
    fn to_artist(xs: Vec<String>) -> Vec<Artist> {
        uniq(xs).into_iter()
            .map(|name| Artist { name, alias: false })
            .collect()
    }
    
    ArtistMapping {
        main: to_artist(li_main),
        guest: to_artist(li_guests),
        remixer: to_artist(li_remixer),
        composer: to_artist(li_composer),
        conductor: to_artist(li_conductor),
        producer: to_artist(li_producer),
        djmixer: to_artist(li_dj),
    }
}

pub fn format_artist_string(mapping: &ArtistMapping) -> String {
    fn format_role(artists: &[Artist]) -> String {
        artists.iter()
            .filter(|a| !a.alias)
            .map(|a| a.name.clone())
            .collect::<Vec<_>>()
            .join(";")
    }
    
    let mut r = format_role(&mapping.main);
    
    if !mapping.djmixer.is_empty() {
        r = format!("{} pres. {}", format_role(&mapping.djmixer), r);
    }
    
    if !mapping.composer.is_empty() {
        r = format!("{} performed by {}", format_role(&mapping.composer), r);
    }
    
    if !mapping.conductor.is_empty() {
        r = format!("{} under. {}", r, format_role(&mapping.conductor));
    }
    
    if !mapping.guest.is_empty() {
        r = format!("{} feat. {}", r, format_role(&mapping.guest));
    }
    
    if !mapping.remixer.is_empty() {
        r = format!("{} remixed by {}", r, format_role(&mapping.remixer));
    }
    
    if !mapping.producer.is_empty() {
        r = format!("{} produced by {}", r, format_role(&mapping.producer));
    }
    
    r
}

// Helper functions for ID3v2 TXXX frames
fn get_id3_txxx(tag: &Tag, desc: &str) -> Option<String> {
    tag.get_string(&ItemKey::Unknown(format!("TXXX:{desc}")))
        .map(|s| s.to_string())
}

fn set_id3_txxx(tag: &mut Tag, desc: &str, value: Option<&str>) {
    let key = ItemKey::Unknown(format!("TXXX:{desc}"));
    match value {
        Some(v) if !v.is_empty() => {
            tag.insert_text(key, v.to_string());
        }
        _ => {
            tag.remove_key(&key);
        }
    }
}

// Helper functions for MP4 freeform atoms
fn get_mp4_freeform(tag: &Tag, name: &str) -> Option<String> {
    tag.get_string(&ItemKey::Unknown(format!("----:{name}")))
        .map(|s| s.to_string())
}

fn set_mp4_freeform(tag: &mut Tag, name: &str, value: Option<&str>) {
    let key = ItemKey::Unknown(format!("----:{name}"));
    match value {
        Some(v) if !v.is_empty() => {
            tag.insert_text(key, v.to_string());
        }
        _ => {
            tag.remove_key(&key);
        }
    }
}

fn remove_mp4_freeform(tag: &mut Tag, name: &str) {
    tag.remove_key(&ItemKey::Unknown(format!("----:{name}")));
}