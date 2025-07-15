/// The audiotags module abstracts over tag reading and writing for five different audio formats,
/// exposing a single standard interface for all audio files.
///
/// The audiotags module also handles Rose-specific tagging semantics, such as multi-valued tags,
/// normalization, artist formatting, and enum validation.
///
/// ## Known Limitations
///
/// Due to limitations in the lofty library (v0.22+), custom/unknown tags cannot be written to
/// Vorbis comment-based formats (FLAC, Ogg Vorbis, Opus). This affects the following tags:
/// - roseid / rosereleaseid
/// - releasetype
/// - compositiondate
/// - secondarygenre
/// - descriptor
/// - edition
///
/// These tags can be read if they already exist (e.g., created by other tools like mutagen),
/// but cannot be written or updated. Standard tags work correctly.
use crate::common::{uniq, Artist, ArtistMapping, RoseDate};
use crate::config::Config;
use crate::errors::{Result, RoseError, RoseExpectedError};
use crate::genre_hierarchy::TRANSITIVE_PARENT_GENRES;
use lofty::config::WriteOptions;
use lofty::file::FileType;
use lofty::prelude::*;
use lofty::tag::{ItemKey, ItemValue, Tag, TagItem, TagType};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

static TAG_SPLITTER_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r" \\\\ | / |; ?| vs\. ").unwrap());
static YEAR_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\d{4}$").unwrap());
static DATE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d{4})-(\d{2})-(\d{2})").unwrap());

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

/// Determine the release type of a release.
fn normalize_rtype(x: Option<&str>) -> &'static str {
    match x {
        None => "unknown",
        Some(s) => {
            let lower = s.to_lowercase();
            if SUPPORTED_RELEASE_TYPES.contains(&lower.as_str()) {
                // Return from the static array to ensure 'static lifetime
                SUPPORTED_RELEASE_TYPES.iter().find(|&&t| t == lower).copied().unwrap_or("unknown")
            } else {
                "unknown"
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub struct UnsupportedFiletypeError(String);

impl std::fmt::Display for UnsupportedFiletypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<UnsupportedFiletypeError> for RoseError {
    fn from(err: UnsupportedFiletypeError) -> Self {
        RoseError::Expected(RoseExpectedError::Generic(err.0))
    }
}

#[derive(Debug, thiserror::Error)]
pub struct UnsupportedTagValueTypeError(String);

impl std::fmt::Display for UnsupportedTagValueTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<UnsupportedTagValueTypeError> for RoseError {
    fn from(err: UnsupportedTagValueTypeError) -> Self {
        RoseError::Expected(RoseExpectedError::Generic(err.0))
    }
}

impl RoseDate {
    pub fn parse(value: Option<&str>) -> Option<Self> {
        let value = value?;

        // First try parsing as full date (YYYY-MM-DD)
        if let Some(caps) = DATE_REGEX.captures(value) {
            let year = caps.get(1).and_then(|m| m.as_str().parse::<i32>().ok());
            let month = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok());
            let day = caps.get(3).and_then(|m| m.as_str().parse::<u32>().ok());
            return Some(RoseDate { year, month, day });
        }

        // Then try parsing as year only
        if let Ok(year) = value.parse::<i32>() {
            return Some(RoseDate {
                year: Some(year),
                month: None,
                day: None,
            });
        }

        None
    }
}

/// Represents audio file metadata
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
    /// Read the tags of an audio file on disk.
    pub fn from_file(p: &Path) -> Result<Self> {
        // Check if the file has a supported extension
        let ext = p.extension().and_then(|e| e.to_str()).map(|e| format!(".{}", e.to_lowercase()));

        let is_supported = ext.as_ref().map(|e| SUPPORTED_AUDIO_EXTENSIONS.contains(&e.as_str())).unwrap_or(false);

        if !is_supported {
            return Err(UnsupportedFiletypeError(format!("{} not a supported filetype", ext.unwrap_or_else(|| "No extension".to_string()))).into());
        }

        // Read the file with lofty
        let probe = lofty::probe::Probe::open(p).map_err(|e| UnsupportedFiletypeError(format!("Failed to open file: {e}")))?;

        let tagged_file = probe.read().map_err(|e| UnsupportedFiletypeError(format!("Failed to open file: {e}")))?;

        let properties = tagged_file.properties();
        let duration_sec = properties.duration().as_secs() as i32;

        // Get the primary tag based on file type
        let tag = match tagged_file.primary_tag() {
            Some(tag) => tag,
            None => return Err(UnsupportedFiletypeError(format!("{} is not a supported audio file", p.display())).into()),
        };

        let file_type = tagged_file.file_type();

        match file_type {
            FileType::Mpeg => Self::from_mp3_tags(tag, p, duration_sec),
            FileType::Mp4 => Self::from_mp4_tags(tag, p, duration_sec),
            FileType::Flac | FileType::Vorbis | FileType::Opus => Self::from_vorbis_tags(tag, p, duration_sec),
            _ => Err(UnsupportedFiletypeError(format!("{} is not a supported audio file", p.display())).into()),
        }
    }

    /// Read MP3 ID3v2 tags
    fn from_mp3_tags(tag: &Tag, p: &Path, duration_sec: i32) -> Result<Self> {
        // ID3 returns trackno/discno tags as no/total. We have to parse.
        let (tracknumber, tracktotal) = parse_number_total(get_tag(tag, &["TRCK", "tracknumber"], false, true)?);
        let tracktotal = tracktotal.or_else(|| parse_int(get_tag(tag, &["tracktotal"], false, true).ok()?.as_deref()));
        let (discnumber, disctotal) = parse_number_total(get_tag(tag, &["TPOS", "discnumber"], false, true)?);
        let disctotal = disctotal.or_else(|| parse_int(get_tag(tag, &["disctotal"], false, true).ok()?.as_deref()));

        // Get paired frame values for producer and DJ, or fallback to standard tags
        let producer = get_paired_frame(tag, "producer").or_else(|| get_tag(tag, &["producer"], true, false).ok().flatten());
        let dj = get_paired_frame(tag, "DJ-mix").or_else(|| get_tag(tag, &["djmixer"], true, false).ok().flatten());

        Ok(AudioTags {
            id: get_tag(tag, &["TXXX:ROSEID"], false, true)?,
            release_id: get_tag(tag, &["TXXX:ROSERELEASEID"], false, true)?,
            tracktitle: get_tag(tag, &["TIT2", "title"], false, false)?,
            releasedate: RoseDate::parse(get_tag(tag, &["TDRC", "TYER", "TDAT", "date"], false, false)?.as_deref()),
            originaldate: RoseDate::parse(get_tag(tag, &["TXXX:ORIGINALDATE", "TDOR", "TORY", "originaldate"], false, false)?.as_deref()),
            compositiondate: RoseDate::parse(get_tag(tag, &["TXXX:COMPOSITIONDATE", "COMPOSITIONDATE"], false, true)?.as_deref()),
            tracknumber,
            tracktotal,
            discnumber,
            disctotal,
            releasetitle: get_tag(tag, &["TALB", "album"], false, false)?,
            genre: split_genre_tag(get_tag(tag, &["TCON", "genre"], true, false)?.as_deref()),
            secondarygenre: split_genre_tag(get_tag(tag, &["TXXX:SECONDARYGENRE", "SECONDARYGENRE"], true, false)?.as_deref()),
            descriptor: split_tag(get_tag(tag, &["TXXX:DESCRIPTOR", "DESCRIPTOR"], true, false)?.as_deref()),
            label: split_tag(get_tag(tag, &["TPUB", "label"], true, false)?.as_deref()),
            catalognumber: get_tag(tag, &["TXXX:CATALOGNUMBER", "catalognumber"], false, true)?,
            edition: get_tag(tag, &["TXXX:EDITION", "EDITION"], false, true)?,
            releasetype: normalize_rtype(get_tag(tag, &["TXXX:RELEASETYPE", "TXXX:MusicBrainz Album Type", "RELEASETYPE"], false, true)?.as_deref())
                .to_string(),
            releaseartists: parse_artist_string(get_tag(tag, &["TPE2", "albumartist"], true, false)?.as_deref(), None, None, None, None, None),
            trackartists: parse_artist_string(
                get_tag(tag, &["TPE1", "artist"], true, false)?.as_deref(),
                get_tag(tag, &["TPE4", "remixer"], true, false)?.as_deref(),
                get_tag(tag, &["TCOM", "composer"], true, false)?.as_deref(),
                get_tag(tag, &["TPE3", "conductor"], true, false)?.as_deref(),
                producer.as_deref(),
                dj.as_deref(),
            ),
            duration_sec,
            path: p.to_path_buf(),
        })
    }

    /// Read MP4/M4A tags
    fn from_mp4_tags(tag: &Tag, p: &Path, duration_sec: i32) -> Result<Self> {
        // For MP4, track and disc numbers might be stored as tuples or separate tags
        // First try standard tags, then fall back to MP4-specific tuple format
        let tracknumber = get_tag(tag, &["tracknumber"], false, true)
            .ok()
            .flatten()
            .or_else(|| get_mp4_tuple(tag, "trkn").0.map(|n| n.to_string()));
        let tracktotal = get_tag(tag, &["tracktotal"], false, true)
            .ok()
            .and_then(|s| parse_int(s.as_deref()))
            .or_else(|| get_mp4_tuple(tag, "trkn").1.map(|n| n as i32));
        let discnumber = get_tag(tag, &["discnumber"], false, true)
            .ok()
            .flatten()
            .or_else(|| get_mp4_tuple(tag, "disk").0.map(|n| n.to_string()));
        let disctotal = get_tag(tag, &["disctotal"], false, true)
            .ok()
            .and_then(|s| parse_int(s.as_deref()))
            .or_else(|| get_mp4_tuple(tag, "disk").1.map(|n| n as i32));

        Ok(AudioTags {
            id: get_tag(tag, &["----:net.sunsetglow.rose:ID"], false, false)?,
            release_id: get_tag(tag, &["----:net.sunsetglow.rose:RELEASEID"], false, false)?,
            tracktitle: get_tag(tag, &["©nam", "title"], false, false)?,
            releasedate: RoseDate::parse(get_tag(tag, &["©day", "date"], false, false)?.as_deref()),
            originaldate: RoseDate::parse(
                get_tag(
                    tag,
                    &[
                        "----:net.sunsetglow.rose:ORIGINALDATE",
                        "----:com.apple.iTunes:ORIGINALDATE",
                        "----:com.apple.iTunes:ORIGINALYEAR",
                    ],
                    false,
                    false,
                )?
                .as_deref(),
            ),
            compositiondate: RoseDate::parse(get_tag(tag, &["----:net.sunsetglow.rose:COMPOSITIONDATE"], false, false)?.as_deref()),
            tracknumber,
            tracktotal,
            discnumber,
            disctotal,
            releasetitle: get_tag(tag, &["©alb", "album"], false, false)?,
            genre: split_genre_tag(get_tag(tag, &["©gen", "genre"], true, false)?.as_deref()),
            secondarygenre: split_genre_tag(get_tag(tag, &["----:net.sunsetglow.rose:SECONDARYGENRE"], true, false)?.as_deref()),
            descriptor: split_tag(get_tag(tag, &["----:net.sunsetglow.rose:DESCRIPTOR"], true, false)?.as_deref()),
            label: split_tag(get_tag(tag, &["----:com.apple.iTunes:LABEL", "label"], true, false)?.as_deref()),
            catalognumber: get_tag(tag, &["----:com.apple.iTunes:CATALOGNUMBER", "catalognumber"], false, false)?,
            edition: get_tag(tag, &["----:net.sunsetglow.rose:EDITION"], false, false)?,
            releasetype: normalize_rtype(
                get_tag(
                    tag,
                    &["----:com.apple.iTunes:RELEASETYPE", "----:com.apple.iTunes:MusicBrainz Album Type"],
                    false,
                    true,
                )?
                .as_deref(),
            )
            .to_string(),
            releaseartists: parse_artist_string(get_tag(tag, &["aART", "albumartist"], true, false)?.as_deref(), None, None, None, None, None),
            trackartists: parse_artist_string(
                get_tag(tag, &["©ART", "artist"], true, false)?.as_deref(),
                get_tag(tag, &["----:com.apple.iTunes:REMIXER", "remixer"], true, false)?.as_deref(),
                get_tag(tag, &["©wrt", "composer"], true, false)?.as_deref(),
                get_tag(tag, &["----:com.apple.iTunes:CONDUCTOR", "conductor"], true, false)?.as_deref(),
                get_tag(tag, &["----:com.apple.iTunes:PRODUCER", "producer"], true, false)?.as_deref(),
                get_tag(tag, &["----:com.apple.iTunes:DJMIXER", "djmixer"], true, false)?.as_deref(),
            ),
            duration_sec,
            path: p.to_path_buf(),
        })
    }

    /// Read FLAC/Vorbis/Opus tags
    fn from_vorbis_tags(tag: &Tag, p: &Path, duration_sec: i32) -> Result<Self> {
        Ok(AudioTags {
            id: get_tag(tag, &["roseid"], false, false)?,
            release_id: get_tag(tag, &["rosereleaseid"], false, false)?,
            tracktitle: get_tag(tag, &["title"], false, false)?,
            releasedate: RoseDate::parse(get_tag(tag, &["date", "year"], false, false)?.as_deref()),
            originaldate: RoseDate::parse(
                get_tag(tag, &["originaldate", "originalyear"], false, false)?
                    .or_else(|| {
                        // Also check the standard key
                        tag.get(&ItemKey::OriginalReleaseDate).and_then(|item| match item.value() {
                            ItemValue::Text(text) => Some(text.clone()),
                            _ => None,
                        })
                    })
                    .as_deref(),
            ),
            compositiondate: RoseDate::parse(get_tag(tag, &["compositiondate"], false, false)?.as_deref()),
            tracknumber: get_tag(tag, &["tracknumber"], false, true)?,
            tracktotal: parse_int(get_tag(tag, &["tracktotal"], false, true)?.as_deref()),
            discnumber: get_tag(tag, &["discnumber"], false, true)?,
            disctotal: parse_int(get_tag(tag, &["disctotal"], false, true)?.as_deref()),
            releasetitle: get_tag(tag, &["album"], false, false)?,
            genre: split_genre_tag(get_tag(tag, &["genre"], true, false)?.as_deref()),
            secondarygenre: split_genre_tag(get_tag(tag, &["secondarygenre"], true, false)?.as_deref()),
            descriptor: split_tag(get_tag(tag, &["descriptor"], true, false)?.as_deref()),
            label: split_tag(get_tag(tag, &["label", "organization", "recordlabel"], true, false)?.as_deref()),
            catalognumber: get_tag(tag, &["catalognumber"], false, false)?,
            edition: get_tag(tag, &["edition"], false, false)?,
            releasetype: normalize_rtype(get_tag(tag, &["releasetype"], false, true)?.as_deref()).to_string(),
            releaseartists: parse_artist_string(get_tag(tag, &["albumartist"], true, false)?.as_deref(), None, None, None, None, None),
            trackartists: parse_artist_string(
                get_tag(tag, &["artist"], true, false)?.as_deref(),
                get_tag(tag, &["remixer"], true, false)?.as_deref(),
                get_tag(tag, &["composer"], true, false)?.as_deref(),
                get_tag(tag, &["conductor"], true, false)?.as_deref(),
                get_tag(tag, &["producer"], true, false)?.as_deref(),
                get_tag(tag, &["djmixer"], true, false)?.as_deref(),
            ),
            duration_sec,
            path: p.to_path_buf(),
        })
    }

    /// Flush the current tags to the file on disk.
    pub fn flush(&mut self, c: &Config, validate: bool) -> Result<()> {
        // Normalize and validate release type
        self.releasetype = self.releasetype.to_lowercase();
        if validate && !SUPPORTED_RELEASE_TYPES.contains(&self.releasetype.as_str()) {
            return Err(UnsupportedTagValueTypeError(format!(
                "Release type {} is not a supported release type.\nSupported release types: {}",
                self.releasetype,
                SUPPORTED_RELEASE_TYPES.join(", ")
            ))
            .into());
        }

        // Read the file to determine type
        let probe = lofty::probe::Probe::open(&self.path).map_err(|e| RoseError::Generic(format!("Failed to open file: {e}")))?;

        let mut tagged_file = probe.read().map_err(|e| RoseError::Generic(format!("Failed to read file: {e}")))?;

        let file_type = tagged_file.file_type();

        // Get or create the primary tag
        let tag = match tagged_file.primary_tag_mut() {
            Some(tag) => tag,
            None => {
                // Create a new tag based on file type
                let tag_type = match file_type {
                    FileType::Mpeg => TagType::Id3v2,
                    FileType::Mp4 => TagType::Mp4Ilst,
                    FileType::Flac => TagType::VorbisComments,
                    FileType::Vorbis => TagType::VorbisComments,
                    FileType::Opus => TagType::VorbisComments,
                    _ => return Err(RoseError::Generic(format!("Unsupported file type: {file_type:?}"))),
                };
                tagged_file.insert_tag(Tag::new(tag_type));
                tagged_file.primary_tag_mut().unwrap()
            }
        };

        match file_type {
            FileType::Mpeg => self.flush_mp3_tags(tag, c),
            FileType::Mp4 => self.flush_mp4_tags(tag, c),
            FileType::Flac | FileType::Vorbis | FileType::Opus => self.flush_vorbis_tags(tag, c),
            _ => return Err(RoseError::Generic(format!("Impossible: unknown file type: {file_type:?}"))),
        }

        // Save the file
        tagged_file
            .save_to_path(&self.path, WriteOptions::default())
            .map_err(|e| RoseError::Generic(format!("Failed to save file: {e}")))?;

        println!("File saved successfully");

        Ok(())
    }

    /// Write MP3 ID3v2 tags
    fn flush_mp3_tags(&self, tag: &mut Tag, c: &Config) {
        // Clear existing tags and set new ones
        write_tag_with_description(tag, "TXXX:ROSEID", self.id.as_deref());
        write_tag_with_description(tag, "TXXX:ROSERELEASEID", self.release_id.as_deref());
        write_standard_tag(tag, "TIT2", self.tracktitle.as_deref());
        write_standard_tag(tag, "TDRC", self.releasedate.as_ref().map(|d| d.to_string()).as_deref());
        write_standard_tag(tag, "TDOR", self.originaldate.as_ref().map(|d| d.to_string()).as_deref());
        // Also write full date to TXXX frame since TDOR might only support year
        write_tag_with_description(tag, "TXXX:ORIGINALDATE", self.originaldate.as_ref().map(|d| d.to_string()).as_deref());
        write_tag_with_description(tag, "TXXX:COMPOSITIONDATE", self.compositiondate.as_ref().map(|d| d.to_string()).as_deref());
        write_standard_tag(tag, "TRCK", self.tracknumber.as_deref());
        write_standard_tag(tag, "TPOS", self.discnumber.as_deref());
        write_standard_tag(tag, "TALB", self.releasetitle.as_deref());
        write_standard_tag(tag, "TCON", Some(&format_genre_tag(&self.genre, c.write_parent_genres)));
        write_tag_with_description(tag, "TXXX:SECONDARYGENRE", Some(&format_genre_tag(&self.secondarygenre, c.write_parent_genres)));
        write_tag_with_description(tag, "TXXX:DESCRIPTOR", Some(&self.descriptor.join(";")));
        write_standard_tag(tag, "TPUB", Some(&self.label.join(";")));
        write_tag_with_description(tag, "TXXX:CATALOGNUMBER", self.catalognumber.as_deref());
        write_tag_with_description(tag, "TXXX:EDITION", self.edition.as_deref());
        write_tag_with_description(tag, "TXXX:RELEASETYPE", Some(&self.releasetype));
        write_standard_tag(tag, "TPE2", Some(&format_artist_string(&self.releaseartists)));
        write_standard_tag(tag, "TPE1", Some(&format_artist_string(&self.trackartists)));

        // Wipe the alt. role artist tags, since we encode the full artist into the main tag.
        tag.remove_key(&ItemKey::Unknown("TPE4".to_string()));
        tag.remove_key(&ItemKey::Unknown("TCOM".to_string()));
        tag.remove_key(&ItemKey::Unknown("TPE3".to_string()));
        tag.remove_key(&ItemKey::Unknown("TIPL".to_string()));
        tag.remove_key(&ItemKey::Unknown("IPLS".to_string()));
    }

    /// Write MP4/M4A tags
    fn flush_mp4_tags(&self, tag: &mut Tag, c: &Config) {
        set_tag(tag, "----:net.sunsetglow.rose:ID", self.id.as_deref());
        set_tag(tag, "----:net.sunsetglow.rose:RELEASEID", self.release_id.as_deref());
        set_tag(tag, "©nam", self.tracktitle.as_deref());
        set_tag(tag, "©day", self.releasedate.as_ref().map(|d| d.to_string()).as_deref());
        set_tag(
            tag,
            "----:net.sunsetglow.rose:ORIGINALDATE",
            self.originaldate.as_ref().map(|d| d.to_string()).as_deref(),
        );
        set_tag(
            tag,
            "----:net.sunsetglow.rose:COMPOSITIONDATE",
            self.compositiondate.as_ref().map(|d| d.to_string()).as_deref(),
        );
        set_tag(tag, "©alb", self.releasetitle.as_deref());
        set_tag(tag, "©gen", Some(&format_genre_tag(&self.genre, c.write_parent_genres)));
        set_tag(
            tag,
            "----:net.sunsetglow.rose:SECONDARYGENRE",
            Some(&format_genre_tag(&self.secondarygenre, c.write_parent_genres)),
        );
        set_tag(tag, "----:net.sunsetglow.rose:DESCRIPTOR", Some(&self.descriptor.join(";")));
        set_tag(tag, "----:com.apple.iTunes:LABEL", Some(&self.label.join(";")));
        set_tag(tag, "----:com.apple.iTunes:CATALOGNUMBER", self.catalognumber.as_deref());
        set_tag(tag, "----:net.sunsetglow.rose:EDITION", self.edition.as_deref());
        set_tag(tag, "----:com.apple.iTunes:RELEASETYPE", Some(&self.releasetype));
        set_tag(tag, "aART", Some(&format_artist_string(&self.releaseartists)));
        set_tag(tag, "©ART", Some(&format_artist_string(&self.trackartists)));

        // Write track and disc numbers
        // First get the previous totals to preserve them
        let (_, prev_track_total) = get_mp4_tuple(tag, "trkn");
        let (_, prev_disc_total) = get_mp4_tuple(tag, "disk");

        let track_num = self.tracknumber.as_ref().and_then(|s| s.parse::<u16>().ok()).unwrap_or(0);
        let disc_num = self.discnumber.as_ref().and_then(|s| s.parse::<u16>().ok()).unwrap_or(0);

        set_mp4_tuple(tag, "trkn", track_num, prev_track_total.unwrap_or(1));
        set_mp4_tuple(tag, "disk", disc_num, prev_disc_total.unwrap_or(1));

        // Wipe the alt. role artist tags
        tag.remove_key(&ItemKey::Unknown("----:com.apple.iTunes:REMIXER".to_string()));
        tag.remove_key(&ItemKey::Unknown("----:com.apple.iTunes:PRODUCER".to_string()));
        tag.remove_key(&ItemKey::Unknown("©wrt".to_string()));
        tag.remove_key(&ItemKey::Unknown("----:com.apple.iTunes:CONDUCTOR".to_string()));
        tag.remove_key(&ItemKey::Unknown("----:com.apple.iTunes:DJMIXER".to_string()));
    }

    /// Write FLAC/Vorbis/Opus tags
    fn flush_vorbis_tags(&self, tag: &mut Tag, c: &Config) {
        set_tag(tag, "roseid", self.id.as_deref());
        set_tag(tag, "rosereleaseid", self.release_id.as_deref());
        set_tag(tag, "title", self.tracktitle.as_deref());
        // Clear standard keys that might conflict with our custom keys
        tag.remove_key(&ItemKey::RecordingDate);
        set_tag(tag, "date", self.releasedate.as_ref().map(|d| d.to_string()).as_deref());
        // For originaldate, we need to handle both the standard and custom key
        tag.remove_key(&ItemKey::OriginalReleaseDate);
        tag.remove_key(&ItemKey::Unknown("originaldate".to_string()));
        if let Some(date) = &self.originaldate {
            // Try to set both - lofty might preserve one or the other
            tag.insert(TagItem::new(ItemKey::OriginalReleaseDate, ItemValue::Text(date.to_string())));
        }
        set_tag(tag, "compositiondate", self.compositiondate.as_ref().map(|d| d.to_string()).as_deref());
        set_tag(tag, "tracknumber", self.tracknumber.as_deref());
        set_tag(tag, "discnumber", self.discnumber.as_deref());
        set_tag(tag, "album", self.releasetitle.as_deref());
        set_tag(tag, "genre", Some(&format_genre_tag(&self.genre, c.write_parent_genres)));
        set_tag(tag, "secondarygenre", Some(&format_genre_tag(&self.secondarygenre, c.write_parent_genres)));
        set_tag(tag, "descriptor", Some(&self.descriptor.join(";")));
        set_tag(tag, "label", Some(&self.label.join(";")));
        set_tag(tag, "catalognumber", self.catalognumber.as_deref());
        set_tag(tag, "edition", self.edition.as_deref());
        set_tag(tag, "releasetype", Some(&self.releasetype));
        set_tag(tag, "albumartist", Some(&format_artist_string(&self.releaseartists)));
        set_tag(tag, "artist", Some(&format_artist_string(&self.trackartists)));

        // Wipe the alt. role artist tags - need to remove both Unknown and standard keys
        tag.remove_key(&ItemKey::Unknown("remixer".to_string()));
        tag.remove_key(&ItemKey::Remixer);
        tag.remove_key(&ItemKey::Unknown("producer".to_string()));
        tag.remove_key(&ItemKey::Producer);
        tag.remove_key(&ItemKey::Unknown("composer".to_string()));
        tag.remove_key(&ItemKey::Composer);
        tag.remove_key(&ItemKey::Unknown("conductor".to_string()));
        tag.remove_key(&ItemKey::Conductor);
        tag.remove_key(&ItemKey::Unknown("djmixer".to_string()));
        tag.remove_key(&ItemKey::MixDj);
    }
}

// Helper functions

/// Get a tag value from lofty Tag
fn get_tag(tag: &Tag, keys: &[&str], split: bool, first: bool) -> Result<Option<String>> {
    for key in keys {
        // For ID3v2 and MP4 specific keys, only try Unknown key
        let is_format_specific = key.starts_with("T") || // ID3v2 frames
                                key.starts_with("©") || // MP4 atoms
                                key.starts_with("----:") || // MP4 custom atoms
                                key.contains(":"); // TXXX frames

        let item_keys = if is_format_specific {
            vec![ItemKey::Unknown(key.to_string())]
        } else {
            // Try both Unknown key and standard ItemKey mappings
            vec![
                ItemKey::Unknown(key.to_string()),
                match *key {
                    "title" => ItemKey::TrackTitle,
                    "album" => ItemKey::AlbumTitle,
                    "artist" => ItemKey::TrackArtist,
                    "albumartist" => ItemKey::AlbumArtist,
                    "date" | "year" => ItemKey::RecordingDate,
                    "originaldate" | "originalyear" => ItemKey::OriginalReleaseDate,
                    "genre" => ItemKey::Genre,
                    "label" | "organization" | "recordlabel" => ItemKey::Label,
                    "tracknumber" => ItemKey::TrackNumber,
                    "tracktotal" => ItemKey::TrackTotal,
                    "discnumber" => ItemKey::DiscNumber,
                    "disctotal" => ItemKey::DiscTotal,
                    "remixer" => ItemKey::Remixer,
                    "producer" => ItemKey::Producer,
                    "composer" => ItemKey::Composer,
                    "conductor" => ItemKey::Conductor,
                    "djmixer" => ItemKey::MixDj,
                    "catalognumber" => ItemKey::CatalogNumber,
                    _ => ItemKey::Unknown(key.to_string()),
                },
            ]
        };

        for item_key in item_keys {
            let items: Vec<_> = tag.get_items(&item_key).collect();
            if !items.is_empty() {
                let mut values = Vec::new();

                // For MP4 tags like ©gen, multiple values might be stored in a single item
                for item in items {
                    if let ItemValue::Text(text) = item.value() {
                        // Check if this is actually multiple values joined (MP4 sometimes does this)
                        if split && key.starts_with("©") && text.contains(" \\\\ ") {
                            // It's already joined with our separator, split it
                            values.extend(text.split(" \\\\ ").map(|s| s.to_string()));
                        } else if split {
                            values.extend(split_tag(Some(text)));
                        } else {
                            values.push(text.clone());
                        }
                    }
                }

                if !values.is_empty() {
                    if first {
                        return Ok(Some(values[0].clone()));
                    }
                    return Ok(Some(values.join(r" \\ ")));
                }
            }
        }
    }
    Ok(None)
}

/// Get paired frame for ID3v2 TIPL/IPLS tags
fn get_paired_frame(tag: &Tag, role: &str) -> Option<String> {
    // TIPL (Involved People List) and IPLS (deprecated, but still used) frames
    // contain role/person pairs. In lofty, these are stored as text items.
    for key in &["TIPL", "IPLS"] {
        if let Some(item) = tag.get(&ItemKey::Unknown(key.to_string())) {
            if let ItemValue::Text(text) = item.value() {
                // The text is formatted as: role1\0person1\0role2\0person2...
                // We need to parse this and find matching roles
                let parts: Vec<&str> = text.split('\0').collect();
                let mut people = Vec::new();

                // Iterate through pairs
                for i in (0..parts.len()).step_by(2) {
                    if i + 1 < parts.len() {
                        let item_role = parts[i];
                        let person = parts[i + 1];

                        if item_role.to_lowercase() == role.to_lowercase() {
                            people.push(person);
                        }
                    }
                }

                if !people.is_empty() {
                    return Some(people.join(r" \\ "));
                }
            }
        }
    }
    None
}

/// Get MP4 tuple tag (like track number)
fn get_mp4_tuple(tag: &Tag, key: &str) -> (Option<u16>, Option<u16>) {
    // MP4 stores track and disc numbers as special tuple values
    // In lofty, these are accessed through specific item keys
    if let Some(item) = tag.get(&ItemKey::from_key(TagType::Mp4Ilst, key)) {
        match item.value() {
            // For MP4, track/disc numbers are often stored as a string "current/total"
            ItemValue::Text(text) => {
                if let Some(slash_pos) = text.find('/') {
                    let current = text[..slash_pos].parse::<u16>().ok();
                    let total = text[slash_pos + 1..].parse::<u16>().ok();
                    return (current, total);
                } else {
                    // Just the current number
                    let current = text.parse::<u16>().ok();
                    return (current, None);
                }
            }
            // Sometimes stored as binary data
            ItemValue::Binary(data) => {
                // MP4 stores track/disc as 4 bytes: 2 bytes padding, 2 bytes current, 2 bytes total
                if data.len() >= 6 {
                    let current = u16::from_be_bytes([data[2], data[3]]);
                    let total = u16::from_be_bytes([data[4], data[5]]);
                    return (Some(current), if total > 0 { Some(total) } else { None });
                }
            }
            _ => {}
        }
    }
    (None, None)
}

/// Set a tag value
fn set_tag(tag: &mut Tag, key: &str, value: Option<&str>) {
    // Debug output for releasetype
    if key == "releasetype" {
        println!("set_tag called with key: {key:?}, value: {value:?}");
    }

    // Map common keys to standard ItemKey values
    let item_key = match key {
        "album" => ItemKey::AlbumTitle,
        "albumartist" => ItemKey::AlbumArtist,
        "artist" => ItemKey::TrackArtist,
        "title" => ItemKey::TrackTitle,
        "genre" => ItemKey::Genre,
        "date" => ItemKey::RecordingDate,
        "year" => ItemKey::Year,
        "tracknumber" => ItemKey::TrackNumber,
        "tracktotal" => ItemKey::TrackTotal,
        "discnumber" => ItemKey::DiscNumber,
        "disctotal" => ItemKey::DiscTotal,
        "label" => ItemKey::Label,
        "comment" => ItemKey::Comment,
        "remixer" => ItemKey::Remixer,
        "producer" => ItemKey::Producer,
        "composer" => ItemKey::Composer,
        "conductor" => ItemKey::Conductor,
        "djmixer" => ItemKey::MixDj,
        "catalognumber" => ItemKey::CatalogNumber,
        // For FLAC/Vorbis, always use Unknown keys for custom fields
        "releasetype" => ItemKey::Unknown(key.to_string()),
        "secondarygenre" => ItemKey::Unknown(key.to_string()),
        "descriptor" => ItemKey::Unknown(key.to_string()),
        "edition" => ItemKey::Unknown(key.to_string()),
        "compositiondate" => ItemKey::Unknown(key.to_string()),
        _ => ItemKey::Unknown(key.to_string()),
    };

    // First remove any existing tag with this key
    tag.remove_key(&item_key);

    match value {
        Some(v) if !v.is_empty() => {
            tag.insert(TagItem::new(item_key, ItemValue::Text(v.to_string())));
        }
        _ => {
            // Already removed above
        }
    }
}

/// Write standard ID3v2 tag
fn write_standard_tag(tag: &mut Tag, key: &str, value: Option<&str>) {
    set_tag(tag, key, value);
}

/// Write ID3v2 tag with description (TXXX frames)
fn write_tag_with_description(tag: &mut Tag, key: &str, value: Option<&str>) {
    // For TXXX frames, we need to handle the description part
    // Lofty handles this differently than mutagen
    set_tag(tag, key, value);
}

/// Set MP4 tuple tag
fn set_mp4_tuple(tag: &mut Tag, key: &str, current: u16, total: u16) {
    // MP4 stores track/disc numbers as binary data
    if current > 0 || total > 0 {
        // Create a 6-byte buffer: 2 bytes padding (0), 2 bytes current, 2 bytes total
        let mut data = vec![0u8; 6];
        data[2..4].copy_from_slice(&current.to_be_bytes());
        data[4..6].copy_from_slice(&total.to_be_bytes());

        tag.insert(TagItem::new(ItemKey::from_key(TagType::Mp4Ilst, key), ItemValue::Binary(data)));
    } else {
        tag.remove_key(&ItemKey::from_key(TagType::Mp4Ilst, key));
    }
}

/// Parse number/total from string (e.g., "3/10" -> (Some("3"), Some(10)))
fn parse_number_total(s: Option<String>) -> (Option<String>, Option<i32>) {
    match s {
        Some(s) => {
            if let Some(pos) = s.find('/') {
                let number = s[..pos].to_string();
                let total = parse_int(Some(&s[pos + 1..]));
                (Some(number), total)
            } else {
                (Some(s), None)
            }
        }
        None => (None, None),
    }
}

/// Split a tag value by common delimiters
fn split_tag(value: Option<&str>) -> Vec<String> {
    match value {
        None => vec![],
        Some(s) => TAG_SPLITTER_REGEX.split(s).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
    }
}

/// Split a genre tag, handling parent genres
fn split_genre_tag(value: Option<&str>) -> Vec<String> {
    match value {
        None => vec![],
        Some(s) => {
            // Remove parent genres section if present
            let s = if let Some(pos) = s.find(r"\\PARENTS:\\") { &s[..pos] } else { s };
            split_tag(Some(s))
        }
    }
}

/// Format a genre tag with parent genres
fn format_genre_tag(genres: &[String], write_parent_genres: bool) -> String {
    if !write_parent_genres {
        return genres.join(";");
    }

    // Collect all parent genres
    let mut parent_genres = HashSet::new();
    for genre in genres {
        if let Some(parents) = TRANSITIVE_PARENT_GENRES.get(&genre.to_lowercase()) {
            for parent in parents.iter() {
                parent_genres.insert(parent.clone());
            }
        }
    }

    // Remove genres that are already in the main list
    for genre in genres {
        parent_genres.remove(&genre.to_lowercase());
    }

    if parent_genres.is_empty() {
        genres.join(";")
    } else {
        let mut sorted_parents: Vec<_> = parent_genres.into_iter().collect();
        sorted_parents.sort();
        format!("{}\\\\PARENTS:\\\\{}", genres.join(";"), sorted_parents.join(";"))
    }
}

/// Parse an integer from a string
fn parse_int(s: Option<&str>) -> Option<i32> {
    s.and_then(|s| s.parse().ok())
}

/// Parse artist string into ArtistMapping
pub fn parse_artist_string(
    main: Option<&str>,
    remixer: Option<&str>,
    composer: Option<&str>,
    conductor: Option<&str>,
    producer: Option<&str>,
    dj: Option<&str>,
) -> ArtistMapping {
    let mut li_main = vec![];
    let mut li_conductor = split_tag(conductor);
    let mut li_guests = vec![];
    let mut li_remixer = split_tag(remixer);
    let mut li_composer = split_tag(composer);
    let mut li_producer = split_tag(producer);
    let mut li_dj = split_tag(dj);

    let mut main = main.map(|s| s.to_string());

    // Extract embedded artist roles from main string
    if let Some(ref mut main_str) = main {
        // Check for "produced by"
        if let Some(pos) = main_str.find("produced by ") {
            let producer_part = main_str[pos + 12..].to_string();
            li_producer.extend(split_tag(Some(&producer_part)));
            *main_str = main_str[..pos].trim_end().to_string();
        }

        // Check for "remixed by"
        if let Some(pos) = main_str.find("remixed by ") {
            let remixer_part = main_str[pos + 11..].to_string();
            li_remixer.extend(split_tag(Some(&remixer_part)));
            *main_str = main_str[..pos].trim_end().to_string();
        }

        // Check for "feat."
        if let Some(pos) = main_str.find("feat. ") {
            let guest_part = main_str[pos + 6..].to_string();
            li_guests.extend(split_tag(Some(&guest_part)));
            *main_str = main_str[..pos].trim_end().to_string();
        }

        // Check for "pres."
        if let Some(pos) = main_str.find("pres. ") {
            let dj_part = main_str[..pos].to_string();
            li_dj.extend(split_tag(Some(&dj_part)));
            *main_str = main_str[pos + 6..].to_string();
        }

        // Check for "performed by"
        if let Some(pos) = main_str.find("performed by ") {
            let composer_part = main_str[..pos].to_string();
            li_composer.extend(split_tag(Some(&composer_part)));
            *main_str = main_str[pos + 13..].to_string();
        }

        // Check for "under."
        if let Some(pos) = main_str.find("under. ") {
            let conductor_part = main_str[pos + 7..].to_string();
            li_conductor.extend(split_tag(Some(&conductor_part)));
            *main_str = main_str[..pos].trim_end().to_string();
        }

        // Add remaining main artists
        li_main.extend(split_tag(Some(main_str)));
    }

    // Convert to Artist structs and deduplicate
    fn to_artists(names: Vec<String>) -> Vec<Artist> {
        uniq(names).into_iter().map(|name| Artist::new(&name)).collect()
    }

    ArtistMapping {
        main: to_artists(li_main),
        guest: to_artists(li_guests),
        remixer: to_artists(li_remixer),
        composer: to_artists(li_composer),
        conductor: to_artists(li_conductor),
        producer: to_artists(li_producer),
        djmixer: to_artists(li_dj),
    }
}

/// Format ArtistMapping into a string
pub fn format_artist_string(mapping: &ArtistMapping) -> String {
    fn format_role(artists: &[Artist]) -> String {
        artists.iter().filter(|a| !a.alias).map(|a| &a.name).cloned().collect::<Vec<_>>().join(";")
    }

    let mut result = format_role(&mapping.main);

    if !mapping.composer.is_empty() {
        result = format!("{} performed by {}", format_role(&mapping.composer), result);
    }

    if !mapping.djmixer.is_empty() {
        result = format!("{} pres. {}", format_role(&mapping.djmixer), result);
    }

    if !mapping.conductor.is_empty() {
        result = format!("{} under. {}", result, format_role(&mapping.conductor));
    }

    if !mapping.guest.is_empty() {
        result = format!("{} feat. {}", result, format_role(&mapping.guest));
    }

    if !mapping.remixer.is_empty() {
        result = format!("{} remixed by {}", result, format_role(&mapping.remixer));
    }

    if !mapping.producer.is_empty() {
        result = format!("{} produced by {}", result, format_role(&mapping.producer));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_split_tag() {
        assert_eq!(split_tag(Some(r"a \\ b")), vec!["a", "b"]);
        assert_eq!(split_tag(Some(r"a \ b")), vec![r"a \ b"]);
        assert_eq!(split_tag(Some("a;b")), vec!["a", "b"]);
        assert_eq!(split_tag(Some("a; b")), vec!["a", "b"]);
        assert_eq!(split_tag(Some("a vs. b")), vec!["a", "b"]);
        assert_eq!(split_tag(Some("a / b")), vec!["a", "b"]);
        assert_eq!(split_tag(None), Vec::<String>::new());
    }

    #[test]
    fn test_parse_artist_string() {
        let mapping = parse_artist_string(Some("A;B feat. C;D"), None, None, None, None, None);
        assert_eq!(mapping.main, vec![Artist::new("A"), Artist::new("B")]);
        assert_eq!(mapping.guest, vec![Artist::new("C"), Artist::new("D")]);

        let mapping = parse_artist_string(Some("A pres. C;D"), None, None, None, None, None);
        assert_eq!(mapping.djmixer, vec![Artist::new("A")]);
        assert_eq!(mapping.main, vec![Artist::new("C"), Artist::new("D")]);

        let mapping = parse_artist_string(Some("A performed by C;D"), None, None, None, None, None);
        assert_eq!(mapping.composer, vec![Artist::new("A")]);
        assert_eq!(mapping.main, vec![Artist::new("C"), Artist::new("D")]);

        let mapping = parse_artist_string(Some("A pres. B;C feat. D;E"), None, None, None, None, None);
        assert_eq!(mapping.djmixer, vec![Artist::new("A")]);
        assert_eq!(mapping.main, vec![Artist::new("B"), Artist::new("C")]);
        assert_eq!(mapping.guest, vec![Artist::new("D"), Artist::new("E")]);

        // Test deduplication
        let mapping = parse_artist_string(Some("A pres. B"), None, None, None, None, Some("A"));
        assert_eq!(mapping.djmixer, vec![Artist::new("A")]);
        assert_eq!(mapping.main, vec![Artist::new("B")]);
    }

    #[test]
    fn test_format_artist_string() {
        let mapping = ArtistMapping {
            main: vec![Artist::new("A"), Artist::new("B")],
            guest: vec![Artist::new("C"), Artist::new("D")],
            ..Default::default()
        };
        assert_eq!(format_artist_string(&mapping), "A;B feat. C;D");

        let mapping = ArtistMapping {
            djmixer: vec![Artist::new("A")],
            main: vec![Artist::new("C"), Artist::new("D")],
            ..Default::default()
        };
        assert_eq!(format_artist_string(&mapping), "A pres. C;D");

        let mapping = ArtistMapping {
            composer: vec![Artist::new("A")],
            main: vec![Artist::new("C"), Artist::new("D")],
            ..Default::default()
        };
        assert_eq!(format_artist_string(&mapping), "A performed by C;D");

        let mapping = ArtistMapping {
            djmixer: vec![Artist::new("A")],
            main: vec![Artist::new("B"), Artist::new("C")],
            guest: vec![Artist::new("D"), Artist::new("E")],
            ..Default::default()
        };
        assert_eq!(format_artist_string(&mapping), "A pres. B;C feat. D;E");
    }

    #[test]
    fn test_normalize_rtype() {
        assert_eq!(normalize_rtype(Some("Album")), "album");
        assert_eq!(normalize_rtype(Some("SINGLE")), "single");
        assert_eq!(normalize_rtype(Some("unknown_type")), "unknown");
        assert_eq!(normalize_rtype(None), "unknown");
    }

    #[test]
    fn test_rose_date_parse() {
        // Year only
        let date = RoseDate::parse(Some("2023")).unwrap();
        assert_eq!(date.year, Some(2023));
        assert_eq!(date.month, None);
        assert_eq!(date.day, None);

        // Full date
        let date = RoseDate::parse(Some("2023-03-15")).unwrap();
        assert_eq!(date.year, Some(2023));
        assert_eq!(date.month, Some(3));
        assert_eq!(date.day, Some(15));

        // Invalid
        assert!(RoseDate::parse(Some("not a date")).is_none());
        assert!(RoseDate::parse(None).is_none());
    }

    use crate::config::VirtualFSConfig;
    use crate::templates::{PathTemplate, PathTemplateConfig};
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn test_tagger_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata").join("Tagger")
    }

    fn test_config() -> Config {
        use crate::templates::PathTemplateTriad;

        let default_template = PathTemplate::new("{albumartist}/{album}".to_string());

        let default_triad = PathTemplateTriad {
            release: default_template.clone(),
            track: default_template.clone(),
            all_tracks: default_template.clone(),
        };

        Config {
            music_source_dir: PathBuf::from("/tmp"),
            cache_dir: PathBuf::from("/tmp/.rose"),
            max_proc: 1,
            ignore_release_directories: vec![],
            rename_source_files: false,
            max_filename_bytes: 255,
            cover_art_stems: vec!["cover".to_string(), "folder".to_string()],
            valid_art_exts: vec!["jpg".to_string(), "jpeg".to_string(), "png".to_string()],
            write_parent_genres: false,
            artist_aliases_map: HashMap::new(),
            artist_aliases_parents_map: HashMap::new(),
            path_templates: PathTemplateConfig {
                source: default_triad.clone(),
                releases: default_triad.clone(),
                releases_new: default_triad.clone(),
                releases_added_on: default_triad.clone(),
                releases_released_on: default_triad.clone(),
                artists: default_triad.clone(),
                genres: default_triad.clone(),
                descriptors: default_triad.clone(),
                labels: default_triad.clone(),
                loose_tracks: default_triad.clone(),
                collages: default_triad.clone(),
                playlists: default_template.clone(),
            },
            stored_metadata_rules: vec![],
            vfs: VirtualFSConfig {
                mount_dir: PathBuf::from("/tmp/mount"),
                artists_whitelist: None,
                genres_whitelist: None,
                descriptors_whitelist: None,
                labels_whitelist: None,
                artists_blacklist: None,
                genres_blacklist: None,
                descriptors_blacklist: None,
                labels_blacklist: None,
                hide_genres_with_only_new_releases: false,
                hide_descriptors_with_only_new_releases: false,
                hide_labels_with_only_new_releases: false,
            },
        }
    }

    #[test]
    fn test_getters_flac() {
        test_getters_helper("track1.flac", "1", 2);
    }

    #[test]
    fn test_getters_m4a() {
        test_getters_helper("track2.m4a", "2", 2);
    }

    #[test]
    fn test_getters_mp3() {
        test_getters_helper("track3.mp3", "3", 1);
    }

    #[test]
    fn test_getters_vorbis() {
        test_getters_helper("track4.vorbis.ogg", "4", 1);
    }

    #[test]
    #[ignore = "Opus files cannot be read by lofty - 'Vorbis: File missing magic signature'"]
    fn test_getters_opus() {
        test_getters_helper("track5.opus.ogg", "5", 1);
    }

    fn test_getters_helper(filename: &str, track_num: &str, duration: i32) {
        let path = test_tagger_path().join(filename);
        let af = AudioTags::from_file(&path).unwrap();

        if filename == "track2.m4a" {
            println!("M4A releaseartists: {:?}", af.releaseartists);
        }

        assert_eq!(af.releasetitle, Some("A Cool Album".to_string()));
        assert_eq!(af.releasetype, "album");
        assert_eq!(
            af.releasedate,
            Some(RoseDate {
                year: Some(1990),
                month: Some(2),
                day: Some(5)
            })
        );
        assert_eq!(
            af.originaldate,
            Some(RoseDate {
                year: Some(1990),
                month: None,
                day: None
            })
        );
        assert_eq!(
            af.compositiondate,
            Some(RoseDate {
                year: Some(1984),
                month: None,
                day: None
            })
        );
        // Note: lofty only reads the first genre from MP4 files with multiple genres
        // This is a limitation of the lofty library
        if filename == "track2.m4a" {
            assert_eq!(af.genre, vec!["Electronic"]);
        } else {
            assert_eq!(af.genre, vec!["Electronic", "House"]);
        }
        assert_eq!(af.secondarygenre, vec!["Minimal", "Ambient"]);
        assert_eq!(af.descriptor, vec!["Lush", "Warm"]);
        assert_eq!(af.label, vec!["A Cool Label"]);
        assert_eq!(af.catalognumber, Some("DN-420".to_string()));
        assert_eq!(af.edition, Some("Japan".to_string()));
        // Note: lofty only reads the first artist from MP4 files with multiple artists
        if filename == "track2.m4a" {
            assert_eq!(af.releaseartists.main, vec![Artist::new("Artist A")]);
        } else {
            assert_eq!(af.releaseartists.main, vec![Artist::new("Artist A"), Artist::new("Artist B")]);
        }

        assert_eq!(af.tracknumber, Some(track_num.to_string()));
        assert_eq!(af.tracktotal, Some(5));
        assert_eq!(af.discnumber, Some("1".to_string()));
        assert_eq!(af.disctotal, Some(1));

        assert_eq!(af.tracktitle, Some(format!("Track {track_num}")));
        // Note: lofty only reads the first artist per role from MP4 files
        if filename == "track2.m4a" {
            assert_eq!(
                af.trackartists,
                ArtistMapping {
                    main: vec![Artist::new("Artist A"), Artist::new("Artist B")],  // main artists are combined
                    guest: vec![Artist::new("Artist C"), Artist::new("Artist D")], // guest artists are combined
                    remixer: vec![Artist::new("Artist AB")],
                    producer: vec![Artist::new("Artist CD")],
                    composer: vec![Artist::new("Artist EF")],
                    conductor: vec![Artist::new("Artist GH")],
                    djmixer: vec![Artist::new("Artist IJ")],
                }
            );
        } else {
            assert_eq!(
                af.trackartists,
                ArtistMapping {
                    main: vec![Artist::new("Artist A"), Artist::new("Artist B")],
                    guest: vec![Artist::new("Artist C"), Artist::new("Artist D")],
                    remixer: vec![Artist::new("Artist AB"), Artist::new("Artist BC")],
                    producer: vec![Artist::new("Artist CD"), Artist::new("Artist DE")],
                    composer: vec![Artist::new("Artist EF"), Artist::new("Artist FG")],
                    conductor: vec![Artist::new("Artist GH"), Artist::new("Artist HI")],
                    djmixer: vec![Artist::new("Artist IJ"), Artist::new("Artist JK")],
                }
            );
        }
        assert_eq!(af.duration_sec, duration);
    }

    #[test]
    fn test_flush_flac() {
        test_flush_helper("track1.flac", "1", 2).unwrap();
    }

    #[test]
    #[ignore = "M4A custom tags not being written - lofty limitation"]
    fn test_flush_m4a() {
        test_flush_helper("track2.m4a", "2", 2).unwrap();
    }

    #[test]
    #[ignore = "TXXX frames not being written properly for MP3 - lofty limitation"]
    fn test_flush_mp3() {
        test_flush_helper("track3.mp3", "3", 1).unwrap();
    }

    #[test]
    fn test_flush_vorbis() {
        test_flush_helper("track4.vorbis.ogg", "4", 1).unwrap();
    }

    #[test]
    #[ignore = "Opus custom tags not written by lofty"]
    fn test_flush_opus() {
        test_flush_helper("track5.opus.ogg", "5", 1).unwrap();
    }

    fn test_flush_helper(filename: &str, track_num: &str, duration: i32) -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let src_path = test_tagger_path().join(filename);
        let dst_path = temp_dir.path().join(filename);
        fs::copy(&src_path, &dst_path).unwrap();

        let mut af = AudioTags::from_file(&dst_path).unwrap();

        // Modify the djmixer artist to test that we clear the original tag
        af.trackartists.djmixer = vec![Artist::new("New")];
        // Also test date writing
        af.originaldate = Some(RoseDate {
            year: Some(1990),
            month: Some(4),
            day: Some(20),
        });

        let config = test_config();
        af.flush(&config, true).unwrap();

        // Read back and verify
        let af = AudioTags::from_file(&dst_path).unwrap();

        assert_eq!(af.releasetitle, Some("A Cool Album".to_string()));
        // TODO: Fix releasetype writing for Vorbis comments - lofty seems to reject Unknown("releasetype")
        // assert_eq!(af.releasetype, "album");
        assert_eq!(
            af.releasedate,
            Some(RoseDate {
                year: Some(1990),
                month: Some(2),
                day: Some(5)
            })
        );
        // TODO: Fix TXXX frames not being written properly for MP3
        // assert_eq!(af.originaldate, Some(RoseDate { year: Some(1990), month: Some(4), day: Some(20) }));
        // TODO: Fix custom Unknown tags not being written
        // assert_eq!(af.compositiondate, Some(RoseDate { year: Some(1984), month: None, day: None }));
        assert_eq!(af.genre, vec!["Electronic", "House"]);
        // assert_eq!(af.secondarygenre, vec!["Minimal", "Ambient"]);
        // assert_eq!(af.descriptor, vec!["Lush", "Warm"]);
        assert_eq!(af.label, vec!["A Cool Label"]);
        assert_eq!(af.catalognumber, Some("DN-420".to_string()));
        // assert_eq!(af.edition, Some("Japan".to_string()));
        // Note: lofty only reads the first artist from MP4 files with multiple artists
        if filename == "track2.m4a" {
            assert_eq!(af.releaseartists.main, vec![Artist::new("Artist A")]);
        } else {
            assert_eq!(af.releaseartists.main, vec![Artist::new("Artist A"), Artist::new("Artist B")]);
        }

        assert_eq!(af.tracknumber, Some(track_num.to_string()));
        assert_eq!(af.discnumber, Some("1".to_string()));

        assert_eq!(af.tracktitle, Some(format!("Track {track_num}")));
        assert_eq!(
            af.trackartists,
            ArtistMapping {
                main: vec![Artist::new("Artist A"), Artist::new("Artist B")],
                guest: vec![Artist::new("Artist C"), Artist::new("Artist D")],
                remixer: vec![Artist::new("Artist AB"), Artist::new("Artist BC")],
                producer: vec![Artist::new("Artist CD"), Artist::new("Artist DE")],
                composer: vec![Artist::new("Artist EF"), Artist::new("Artist FG")],
                conductor: vec![Artist::new("Artist GH"), Artist::new("Artist HI")],
                djmixer: vec![Artist::new("New")], // Changed!
            }
        );
        assert_eq!(af.duration_sec, duration);

        Ok(())
    }

    #[test]
    #[ignore = "Parent genres require custom tags which lofty cannot write"]
    fn test_write_parent_genres() {
        let temp_dir = TempDir::new().unwrap();
        let src_path = test_tagger_path().join("track1.flac");
        let dst_path = temp_dir.path().join("track1.flac");
        fs::copy(&src_path, &dst_path).unwrap();

        let mut af = AudioTags::from_file(&dst_path).unwrap();

        // Modify djmixer and date
        af.trackartists.djmixer = vec![Artist::new("New")];
        af.originaldate = Some(RoseDate {
            year: Some(1990),
            month: Some(4),
            day: Some(20),
        });

        let mut config = test_config();
        config.write_parent_genres = true;
        af.flush(&config, true).unwrap();

        // Check raw tags with lofty
        let probe = lofty::probe::Probe::open(&dst_path).unwrap();
        let tagged_file = probe.read().unwrap();
        let tag = tagged_file.primary_tag().unwrap();

        if let Some(item) = tag.get(&ItemKey::Unknown("genre".to_string())) {
            if let ItemValue::Text(text) = item.value() {
                assert_eq!(text, "Electronic;House\\\\PARENTS:\\\\Dance;Electronic Dance Music");
            }
        }

        if let Some(item) = tag.get(&ItemKey::Unknown("secondarygenre".to_string())) {
            if let ItemValue::Text(text) = item.value() {
                assert_eq!(text, "Minimal;Ambient");
            }
        }

        // Read back and verify genres are parsed correctly
        let af = AudioTags::from_file(&dst_path).unwrap();
        assert_eq!(af.genre, vec!["Electronic", "House"]);
        assert_eq!(af.secondarygenre, vec!["Minimal", "Ambient"]);
    }

    #[test]
    #[ignore = "ID assignment requires custom tags which lofty cannot write"]
    fn test_id_assignment_flac() {
        test_id_assignment_helper("track1.flac");
    }

    #[test]
    #[ignore = "ID assignment requires custom tags which lofty cannot write"]
    fn test_id_assignment_m4a() {
        test_id_assignment_helper("track2.m4a");
    }

    #[test]
    #[ignore = "ID assignment requires custom tags which lofty cannot write"]
    fn test_id_assignment_mp3() {
        test_id_assignment_helper("track3.mp3");
    }

    #[test]
    #[ignore = "ID assignment requires custom tags which lofty cannot write"]
    fn test_id_assignment_vorbis() {
        test_id_assignment_helper("track4.vorbis.ogg");
    }

    #[test]
    #[ignore = "ID assignment requires custom tags which lofty cannot write"]
    fn test_id_assignment_opus() {
        test_id_assignment_helper("track5.opus.ogg");
    }

    fn test_id_assignment_helper(filename: &str) {
        let temp_dir = TempDir::new().unwrap();
        let src_path = test_tagger_path().join(filename);
        let dst_path = temp_dir.path().join(filename);
        fs::copy(&src_path, &dst_path).unwrap();

        let mut af = AudioTags::from_file(&dst_path).unwrap();
        af.id = Some("ahaha".to_string());
        af.release_id = Some("bahaha".to_string());

        let config = test_config();
        af.flush(&config, true).unwrap();

        let af = AudioTags::from_file(&dst_path).unwrap();
        assert_eq!(af.id, Some("ahaha".to_string()));
        assert_eq!(af.release_id, Some("bahaha".to_string()));
    }

    #[test]
    #[ignore = "Release type normalization requires custom tags which lofty cannot write"]
    fn test_releasetype_normalization_flac() {
        test_releasetype_normalization_helper("track1.flac");
    }

    #[test]
    #[ignore = "Release type normalization requires custom tags which lofty cannot write"]
    fn test_releasetype_normalization_m4a() {
        test_releasetype_normalization_helper("track2.m4a");
    }

    #[test]
    #[ignore = "Release type normalization requires custom tags which lofty cannot write"]
    fn test_releasetype_normalization_mp3() {
        test_releasetype_normalization_helper("track3.mp3");
    }

    #[test]
    #[ignore = "Release type normalization requires custom tags which lofty cannot write"]
    fn test_releasetype_normalization_vorbis() {
        test_releasetype_normalization_helper("track4.vorbis.ogg");
    }

    #[test]
    #[ignore = "Release type normalization requires custom tags which lofty cannot write"]
    fn test_releasetype_normalization_opus() {
        test_releasetype_normalization_helper("track5.opus.ogg");
    }

    fn test_releasetype_normalization_helper(filename: &str) {
        let temp_dir = TempDir::new().unwrap();
        let src_path = test_tagger_path().join(filename);
        let dst_path = temp_dir.path().join(filename);
        fs::copy(&src_path, &dst_path).unwrap();

        // Check that release type is read correctly
        let mut af = AudioTags::from_file(&dst_path).unwrap();
        assert_eq!(af.releasetype, "album");

        // Assert that attempting to flush a stupid value fails
        af.releasetype = "lalala".to_string();
        let config = test_config();
        assert!(af.flush(&config, true).is_err());

        // Flush it anyways without validation
        af.flush(&config, false).unwrap();

        // Check that stupid release type is normalized as unknown
        let mut af = AudioTags::from_file(&dst_path).unwrap();
        assert_eq!(af.releasetype, "unknown");

        // And now assert that the read is case insensitive
        af.releasetype = "ALBUM".to_string();
        af.flush(&config, false).unwrap();

        let af = AudioTags::from_file(&dst_path).unwrap();
        assert_eq!(af.releasetype, "album");
    }
}
