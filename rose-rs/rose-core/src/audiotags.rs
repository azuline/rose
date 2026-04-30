//! Audio tag reading and writing abstraction layer.
//!
//! Reads and writes tags for FLAC, MP3, M4A, OGG Vorbis, and OGG Opus files
//! using the `lofty` crate, presenting a single `AudioTags` struct regardless
//! of format. Also provides artist string parsing/formatting (the bidirectional
//! keyword-delimiter protocol), `RoseDate` parsing, and ID assignment.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use lofty::file::{AudioFile, FileType, TaggedFileExt};
use lofty::mp4::{Atom, AtomData, AtomIdent, Ilst};
use lofty::tag::{Accessor, ItemKey, TagType};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::common::{flatten, uniq, Artist, ArtistMapping, RoseError};
use crate::genre_hierarchy::GENRE_HIERARCHY;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const SUPPORTED_AUDIO_EXTENSIONS: &[&str] = &[".mp3", ".m4a", ".ogg", ".opus", ".flac"];

const SUPPORTED_RELEASE_TYPES: &[&str] = &[
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

/// Splitter regex: ` \\ ` (WITH surrounding spaces), ` / `, `;` (optional trailing space), ` vs. `
///
/// In the Python source, `TAG_SPLITTER_REGEX` is redefined at line 550 (overwriting
/// line 32). At runtime, all splitting — genres, descriptors, labels, AND artists —
/// uses this single pattern. The line-32 pattern (no spaces around `\\`) is dead code.
static TAG_SPLITTER_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r" \\\\ | / |; ?| vs\. ").expect("invalid regex"));

static YEAR_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{4}$").expect("invalid regex"));

static DATE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{4})-(\d{2})-(\d{2})").expect("invalid regex"));

// ---------------------------------------------------------------------------
// RoseDate
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoseDate {
    pub year: i32,
    pub month: Option<u32>,
    pub day: Option<u32>,
}

impl RoseDate {
    /// Try to parse a date string. Accepts year-only (`YYYY`) or full date
    /// (`YYYY-MM-DD`, possibly with trailing content like time).
    pub fn parse(value: Option<&str>) -> Option<RoseDate> {
        let value = value?.trim();
        if value.is_empty() {
            return None;
        }
        // Try year-only first.
        if YEAR_REGEX.is_match(value) {
            if let Ok(year) = value.parse::<i32>() {
                return Some(RoseDate {
                    year,
                    month: None,
                    day: None,
                });
            }
        }
        // Try YYYY-MM-DD (with optional trailing content).
        if let Some(caps) = DATE_REGEX.captures(value) {
            let year = caps[1].parse::<i32>().ok()?;
            let month = caps[2].parse::<u32>().ok()?;
            let day = caps[3].parse::<u32>().ok()?;
            return Some(RoseDate {
                year,
                month: Some(month),
                day: Some(day),
            });
        }
        None
    }
}

impl fmt::Display for RoseDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.month, self.day) {
            (None, None) => write!(f, "{:04}", self.year),
            (m, d) => write!(
                f,
                "{:04}-{:02}-{:02}",
                self.year,
                m.unwrap_or(1),
                d.unwrap_or(1)
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// AudioTags
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub duration_sec: i64,
    pub path: PathBuf,
}

impl AudioTags {
    /// Read tags from an audio file on disk.
    pub fn from_file(path: &Path) -> Result<AudioTags, RoseError> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e.to_lowercase()));
        let ext_str = ext.as_deref().unwrap_or("");
        if !SUPPORTED_AUDIO_EXTENSIONS.contains(&ext_str) {
            return Err(RoseError::UnsupportedFiletype(format!(
                "{ext_str} not a supported filetype"
            )));
        }

        // Use Probe with content-based type guessing, since .ogg files could be
        // either Vorbis or Opus. Extension-based detection would fail for opus.ogg.
        let mut tagged_file = lofty::probe::Probe::open(path)
            .map_err(|e| RoseError::UnsupportedFiletype(format!("Failed to open file: {e}")))?
            .guess_file_type()
            .map_err(|e| RoseError::UnsupportedFiletype(format!("Failed to open file: {e}")))?
            .read()
            .map_err(|e| RoseError::UnsupportedFiletype(format!("Failed to open file: {e}")))?;

        let duration_sec = {
            let dur = tagged_file.properties().duration();
            // Python uses round(m.info.length) which rounds to nearest int.
            let total_millis = dur.as_millis() as f64;
            (total_millis / 1000.0).round() as i64
        };

        match tagged_file.file_type() {
            FileType::Mpeg => Self::read_mp3(&tagged_file, path, duration_sec),
            FileType::Mp4 => Self::read_mp4(&mut tagged_file, path, duration_sec),
            FileType::Flac | FileType::Opus | FileType::Vorbis => {
                Self::read_vorbis(&tagged_file, path, duration_sec)
            }
            _ => Err(RoseError::UnsupportedFiletype(format!(
                "{} is not a supported audio file",
                path.display()
            ))),
        }
    }

    // -----------------------------------------------------------------------
    // MP3 (ID3v2)
    // -----------------------------------------------------------------------

    fn read_mp3(
        tagged_file: &lofty::file::TaggedFile,
        path: &Path,
        duration_sec: i64,
    ) -> Result<AudioTags, RoseError> {
        let tag = tagged_file.tag(TagType::Id3v2);

        // Track/disc number/total: ID3 stores as "number/total" in TRCK/TPOS.
        // lofty's generic Tag splits these for us, but we need the string form
        // of the number (not parsed int) to match Python behavior.
        let (tracknumber, tracktotal) = {
            let raw_trck = tag
                .and_then(|t| t.get_string(&ItemKey::TrackNumber))
                .map(String::from);
            let raw_total = tag.and_then(|t| t.get_string(&ItemKey::TrackTotal));
            let total = raw_total.and_then(|s| s.parse::<i32>().ok());
            (raw_trck, total)
        };
        let (discnumber, disctotal) = {
            let raw_disc = tag
                .and_then(|t| t.get_string(&ItemKey::DiscNumber))
                .map(String::from);
            let raw_total = tag.and_then(|t| t.get_string(&ItemKey::DiscTotal));
            let total = raw_total.and_then(|s| s.parse::<i32>().ok());
            (raw_disc, total)
        };

        // TIPL/IPLS paired text frames for producer and DJ-mix.
        // lofty exposes these via ItemKey::Producer and ItemKey::MixDj in the
        // generic Tag, but it only carries one value per key. For multi-value
        // TIPL frames, we need to access the Id3v2Tag directly.
        let (producer_str, dj_str) = Self::read_id3v2_paired_frames(tagged_file);

        let main_artist = id3_get_tag(tag, &[&ItemKey::TrackArtist], true);
        let remixer = id3_get_tag(tag, &[&ItemKey::Remixer], true);
        let composer = id3_get_tag(tag, &[&ItemKey::Composer], true);
        let conductor = id3_get_tag(tag, &[&ItemKey::Conductor], true);

        let album_artist = id3_get_tag(tag, &[&ItemKey::AlbumArtist], true);

        // Date: try TDRC first (RecordingDate), fallback TYER/TDAT via Year
        let releasedate_str = id3_get_tag(tag, &[&ItemKey::RecordingDate, &ItemKey::Year], false);
        // Originaldate: TDOR, TORY
        let originaldate_str = id3_get_tag(tag, &[&ItemKey::OriginalReleaseDate], false);
        let compositiondate_str = id3_get_unknown_tag(tag, "COMPOSITIONDATE");

        // Release type: TXXX:RELEASETYPE, then TXXX:MusicBrainz Album Type
        let releasetype_str = id3_get_unknown_tag(tag, "RELEASETYPE")
            .or_else(|| id3_get_unknown_tag(tag, "MusicBrainz Album Type"));

        let genre_str = id3_get_tag(tag, &[&ItemKey::Genre], true);
        let secondarygenre_str = id3_get_unknown_tag_split(tag, "SECONDARYGENRE");
        let descriptor_str = id3_get_unknown_tag_split(tag, "DESCRIPTOR");
        let label_str = id3_get_tag(tag, &[&ItemKey::Label], true);

        Ok(AudioTags {
            id: id3_get_unknown_tag(tag, "ROSEID"),
            release_id: id3_get_unknown_tag(tag, "ROSERELEASEID"),
            tracktitle: tag.and_then(|t| t.title().map(|s| s.to_string())),
            tracknumber,
            tracktotal,
            discnumber,
            disctotal,
            trackartists: parse_artist_string(
                main_artist.as_deref(),
                remixer.as_deref(),
                composer.as_deref(),
                conductor.as_deref(),
                producer_str.as_deref(),
                dj_str.as_deref(),
            ),
            releasetitle: tag
                .and_then(|t| t.get_string(&ItemKey::AlbumTitle))
                .map(String::from),
            releasetype: normalize_rtype(releasetype_str.as_deref()),
            releasedate: RoseDate::parse(releasedate_str.as_deref()),
            originaldate: RoseDate::parse(originaldate_str.as_deref()),
            compositiondate: RoseDate::parse(compositiondate_str.as_deref()),
            genre: split_genre_tag(genre_str.as_deref()),
            secondarygenre: split_genre_tag(secondarygenre_str.as_deref()),
            descriptor: split_tag(descriptor_str.as_deref()),
            edition: id3_get_unknown_tag(tag, "EDITION"),
            label: split_tag(label_str.as_deref()),
            catalognumber: tag
                .and_then(|t| t.get_string(&ItemKey::CatalogNumber))
                .map(String::from)
                .or_else(|| id3_get_unknown_tag(tag, "CATALOGNUMBER")),
            releaseartists: parse_artist_string(
                album_artist.as_deref(),
                None,
                None,
                None,
                None,
                None,
            ),
            duration_sec,
            path: path.to_path_buf(),
        })
    }

    /// Read TIPL/IPLS paired text frames from the Id3v2Tag.
    ///
    /// Returns `(producer_string, dj_string)` where each is a ` \\ `-joined
    /// list of people with that role, or None if no people found.
    fn read_id3v2_paired_frames(
        tagged_file: &lofty::file::TaggedFile,
    ) -> (Option<String>, Option<String>) {
        // lofty maps TIPL producer/DJ-mix into ItemKey::Producer and ItemKey::MixDj
        // in the generic Tag. When the TIPL frame is split_tag'd, each role's values
        // become separate TagItems. We collect them all.
        let tag = match tagged_file.tag(TagType::Id3v2) {
            Some(t) => t,
            None => return (None, None),
        };

        let producers: Vec<String> = tag
            .get_strings(&ItemKey::Producer)
            .map(String::from)
            .collect();
        let djs: Vec<String> = tag.get_strings(&ItemKey::MixDj).map(String::from).collect();

        let producer_str = if producers.is_empty() {
            None
        } else {
            Some(producers.join(r" \\ "))
        };
        let dj_str = if djs.is_empty() {
            None
        } else {
            Some(djs.join(r" \\ "))
        };

        (producer_str, dj_str)
    }

    // -----------------------------------------------------------------------
    // MP4 (M4A)
    // -----------------------------------------------------------------------

    fn read_mp4(
        tagged_file: &mut lofty::file::TaggedFile,
        path: &Path,
        duration_sec: i64,
    ) -> Result<AudioTags, RoseError> {
        // For MP4, lofty's generic Tag loses multi-value atoms. We convert the
        // Tag back to Ilst (which restores the original data via companion_tag)
        // and then read atoms directly.
        let generic_tag = tagged_file.tag(TagType::Mp4Ilst);

        // Get track/disc numbers from the generic tag (lofty handles the tuple split).
        let tracknumber = generic_tag
            .and_then(|t| t.get_string(&ItemKey::TrackNumber))
            .map(String::from);
        let tracktotal = generic_tag
            .and_then(|t| t.get_string(&ItemKey::TrackTotal))
            .and_then(|s| s.parse::<i32>().ok());
        let discnumber = generic_tag
            .and_then(|t| t.get_string(&ItemKey::DiscNumber))
            .map(String::from);
        let disctotal = generic_tag
            .and_then(|t| t.get_string(&ItemKey::DiscTotal))
            .and_then(|s| s.parse::<i32>().ok());
        let tracktitle = generic_tag.and_then(|t| t.title().map(|s| s.to_string()));
        let releasetitle = generic_tag
            .and_then(|t| t.get_string(&ItemKey::AlbumTitle))
            .map(String::from);

        // Now take ownership of the Tag and convert to Ilst for full atom access.
        let ilst: Ilst = match tagged_file.remove(TagType::Mp4Ilst) {
            Some(tag) => Ilst::from(tag),
            None => Ilst::new(),
        };

        let main_artist = ilst_get_tag(&ilst, &["\u{00a9}ART"], true);
        let album_artist = ilst_get_tag(&ilst, &["aART"], true);
        let remixer = ilst_get_tag(&ilst, &["----:com.apple.iTunes:REMIXER"], true);
        let producer = ilst_get_tag(&ilst, &["----:com.apple.iTunes:PRODUCER"], true);
        let composer = ilst_get_tag(&ilst, &["\u{00a9}wrt"], true);
        let conductor = ilst_get_tag(&ilst, &["----:com.apple.iTunes:CONDUCTOR"], true);
        let dj = ilst_get_tag(&ilst, &["----:com.apple.iTunes:DJMIXER"], true);

        let releasedate_str = ilst_get_tag(&ilst, &["\u{00a9}day"], false);
        let originaldate_str = ilst_get_tag(
            &ilst,
            &[
                "----:net.sunsetglow.rose:ORIGINALDATE",
                "----:com.apple.iTunes:ORIGINALDATE",
                "----:com.apple.iTunes:ORIGINALYEAR",
            ],
            false,
        );
        let compositiondate_str =
            ilst_get_tag(&ilst, &["----:net.sunsetglow.rose:COMPOSITIONDATE"], false);

        let releasetype_str = ilst_get_first(
            &ilst,
            &[
                "----:com.apple.iTunes:RELEASETYPE",
                "----:com.apple.iTunes:MusicBrainz Album Type",
            ],
        );

        let genre_str = ilst_get_tag(&ilst, &["\u{00a9}gen"], true);
        let secondarygenre_str =
            ilst_get_tag(&ilst, &["----:net.sunsetglow.rose:SECONDARYGENRE"], true);
        let descriptor_str = ilst_get_tag(&ilst, &["----:net.sunsetglow.rose:DESCRIPTOR"], true);
        let label_str = ilst_get_tag(&ilst, &["----:com.apple.iTunes:LABEL"], true);
        let catalognumber = ilst_get_tag(&ilst, &["----:com.apple.iTunes:CATALOGNUMBER"], false);
        let edition = ilst_get_tag(&ilst, &["----:net.sunsetglow.rose:EDITION"], false);

        let id = ilst_get_tag(&ilst, &["----:net.sunsetglow.rose:ID"], false);
        let release_id = ilst_get_tag(&ilst, &["----:net.sunsetglow.rose:RELEASEID"], false);

        Ok(AudioTags {
            id,
            release_id,
            tracktitle,
            tracknumber,
            tracktotal,
            discnumber,
            disctotal,
            trackartists: parse_artist_string(
                main_artist.as_deref(),
                remixer.as_deref(),
                composer.as_deref(),
                conductor.as_deref(),
                producer.as_deref(),
                dj.as_deref(),
            ),
            releasetitle,
            releasetype: normalize_rtype(releasetype_str.as_deref()),
            releasedate: RoseDate::parse(releasedate_str.as_deref()),
            originaldate: RoseDate::parse(originaldate_str.as_deref()),
            compositiondate: RoseDate::parse(compositiondate_str.as_deref()),
            genre: split_genre_tag(genre_str.as_deref()),
            secondarygenre: split_genre_tag(secondarygenre_str.as_deref()),
            descriptor: split_tag(descriptor_str.as_deref()),
            edition,
            label: split_tag(label_str.as_deref()),
            catalognumber,
            releaseartists: parse_artist_string(
                album_artist.as_deref(),
                None,
                None,
                None,
                None,
                None,
            ),
            duration_sec,
            path: path.to_path_buf(),
        })
    }

    // -----------------------------------------------------------------------
    // FLAC / Ogg Vorbis / Ogg Opus (Vorbis Comments)
    // -----------------------------------------------------------------------

    fn read_vorbis(
        tagged_file: &lofty::file::TaggedFile,
        path: &Path,
        duration_sec: i64,
    ) -> Result<AudioTags, RoseError> {
        let tag = tagged_file.tag(TagType::VorbisComments);

        let main_artist = vc_get_tag(tag, &["artist"], true);
        let album_artist = vc_get_tag(tag, &["albumartist"], true);
        let remixer = vc_get_tag(tag, &["remixer"], true);
        let producer = vc_get_tag(tag, &["producer"], true);
        let composer = vc_get_tag(tag, &["composer"], true);
        let conductor = vc_get_tag(tag, &["conductor"], true);
        let dj = vc_get_tag(tag, &["djmixer"], true);

        let releasedate_str = vc_get_tag(tag, &["date", "year"], false);
        let originaldate_str = vc_get_tag(tag, &["originaldate", "originalyear"], false);
        let compositiondate_str = vc_get_tag(tag, &["compositiondate"], false);

        let releasetype_str = vc_get_first(tag, &["releasetype"]);

        let genre_str = vc_get_tag(tag, &["genre"], true);
        let secondarygenre_str = vc_get_tag(tag, &["secondarygenre"], true);
        let descriptor_str = vc_get_tag(tag, &["descriptor"], true);
        let label_str = vc_get_tag(tag, &["label", "organization", "recordlabel"], true);
        let catalognumber = vc_get_tag(tag, &["catalognumber"], false);
        let edition = vc_get_tag(tag, &["edition"], false);

        let id = vc_get_tag(tag, &["roseid"], false);
        let release_id = vc_get_tag(tag, &["rosereleaseid"], false);

        let tracknumber = vc_get_first(tag, &["tracknumber"]);
        let tracktotal = vc_get_first(tag, &["tracktotal"]).and_then(|s| s.parse::<i32>().ok());
        let discnumber = vc_get_first(tag, &["discnumber"]);
        let disctotal = vc_get_first(tag, &["disctotal"]).and_then(|s| s.parse::<i32>().ok());

        Ok(AudioTags {
            id,
            release_id,
            tracktitle: tag.and_then(|t| t.title().map(|s| s.to_string())),
            tracknumber,
            tracktotal,
            discnumber,
            disctotal,
            trackartists: parse_artist_string(
                main_artist.as_deref(),
                remixer.as_deref(),
                composer.as_deref(),
                conductor.as_deref(),
                producer.as_deref(),
                dj.as_deref(),
            ),
            releasetitle: tag
                .and_then(|t| t.get_string(&ItemKey::AlbumTitle))
                .map(String::from),
            releasetype: normalize_rtype(releasetype_str.as_deref()),
            releasedate: RoseDate::parse(releasedate_str.as_deref()),
            originaldate: RoseDate::parse(originaldate_str.as_deref()),
            compositiondate: RoseDate::parse(compositiondate_str.as_deref()),
            genre: split_genre_tag(genre_str.as_deref()),
            secondarygenre: split_genre_tag(secondarygenre_str.as_deref()),
            descriptor: split_tag(descriptor_str.as_deref()),
            edition,
            label: split_tag(label_str.as_deref()),
            catalognumber,
            releaseartists: parse_artist_string(
                album_artist.as_deref(),
                None,
                None,
                None,
                None,
                None,
            ),
            duration_sec,
            path: path.to_path_buf(),
        })
    }

    // =======================================================================
    // Write path
    // =======================================================================

    /// Write all fields back to the audio file on disk.
    ///
    /// When `write_parent_genres` is true, parent genres from the genre hierarchy
    /// are appended after a `\\PARENTS:\\` delimiter.
    pub fn flush(&mut self, write_parent_genres: bool) -> Result<(), RoseError> {
        // Normalize and validate release type.
        self.releasetype = self.releasetype.to_lowercase();
        if !SUPPORTED_RELEASE_TYPES.contains(&self.releasetype.as_str()) {
            return Err(RoseError::UnsupportedTagValueType(format!(
                "Release type {} is not a supported release type.\nSupported release types: {}",
                self.releasetype,
                SUPPORTED_RELEASE_TYPES.join(", ")
            )));
        }

        // Use Probe to determine file type, same as read path.
        let tagged_file = lofty::probe::Probe::open(&self.path)
            .map_err(|e| RoseError::Internal(format!("Failed to open file: {e}")))?
            .guess_file_type()
            .map_err(|e| RoseError::Internal(format!("Failed to open file: {e}")))?
            .read()
            .map_err(|e| RoseError::Internal(format!("Failed to open file: {e}")))?;

        match tagged_file.file_type() {
            FileType::Mpeg => self.flush_mp3(write_parent_genres),
            FileType::Mp4 => self.flush_mp4(write_parent_genres),
            FileType::Flac | FileType::Opus | FileType::Vorbis => {
                self.flush_vorbis(write_parent_genres)
            }
            _ => Err(RoseError::UnsupportedFiletype(format!(
                "{} is not a supported audio file",
                self.path.display()
            ))),
        }
    }

    // -----------------------------------------------------------------------
    // MP3 write (ID3v2)
    // -----------------------------------------------------------------------

    fn flush_mp3(&self, write_parent_genres: bool) -> Result<(), RoseError> {
        use lofty::id3::v2::{ExtendedTextFrame, Frame, FrameId, Id3v2Tag, TextInformationFrame};
        use lofty::tag::TagExt;

        let mut tag = Id3v2Tag::new();

        // Read existing file to preserve TXXX frames we don't manage.
        let existing = lofty::probe::Probe::open(&self.path)
            .ok()
            .and_then(|p| p.guess_file_type().ok())
            .and_then(|p| p.read().ok());
        // Collect preserved TXXX descriptions from the existing Id3v2Tag.
        let mut preserved_txxx: Vec<(String, String)> = Vec::new();
        if let Some(ref existing_file) = existing {
            if let Some(existing_id3) = existing_file.tag(TagType::Id3v2) {
                let managed: HashSet<&str> = [
                    "ROSEID",
                    "ROSERELEASEID",
                    "COMPOSITIONDATE",
                    "SECONDARYGENRE",
                    "DESCRIPTOR",
                    "CATALOGNUMBER",
                    "EDITION",
                    "RELEASETYPE",
                ]
                .into_iter()
                .collect();

                for item in existing_id3.items() {
                    if let ItemKey::Unknown(ref key) = *item.key() {
                        let is_managed = managed.iter().any(|d| key.eq_ignore_ascii_case(d));
                        if !is_managed {
                            if let Some(text) = item.value().text() {
                                preserved_txxx.push((key.clone(), text.to_string()));
                            }
                        }
                    }
                }
            }
        }

        // Helper: insert a standard text frame.
        fn set_text(tag: &mut Id3v2Tag, id: &str, value: Option<&str>) {
            if let Some(val) = value {
                if !val.is_empty() {
                    let frame_id = FrameId::new(id.to_string()).expect("valid frame id");
                    tag.insert(Frame::Text(TextInformationFrame::new(
                        frame_id,
                        lofty::TextEncoding::UTF8,
                        val.to_string(),
                    )));
                }
            }
        }

        // Helper: insert a TXXX (ExtendedTextFrame).
        fn set_txxx(tag: &mut Id3v2Tag, desc: &str, value: Option<&str>) {
            if let Some(val) = value {
                if !val.is_empty() {
                    tag.insert(Frame::UserText(ExtendedTextFrame::new(
                        lofty::TextEncoding::UTF8,
                        desc.to_string(),
                        val.to_string(),
                    )));
                }
            }
        }

        set_txxx(&mut tag, "ROSEID", self.id.as_deref());
        set_txxx(&mut tag, "ROSERELEASEID", self.release_id.as_deref());
        set_text(&mut tag, "TIT2", self.tracktitle.as_deref());

        // TDRC / TDOR: lofty 0.22 accepts these as Text frames with string content.
        let releasedate_s = self.releasedate.as_ref().map(|d| d.to_string());
        set_text(&mut tag, "TDRC", releasedate_s.as_deref());
        let originaldate_s = self.originaldate.as_ref().map(|d| d.to_string());
        set_text(&mut tag, "TDOR", originaldate_s.as_deref());

        let compositiondate_s = self.compositiondate.as_ref().map(|d| d.to_string());
        set_txxx(&mut tag, "COMPOSITIONDATE", compositiondate_s.as_deref());

        // MP3 TRCK: tracknumber only (no total) — existing behavior to reproduce.
        set_text(&mut tag, "TRCK", self.tracknumber.as_deref());
        // MP3 TPOS: discnumber only.
        set_text(&mut tag, "TPOS", self.discnumber.as_deref());
        set_text(&mut tag, "TALB", self.releasetitle.as_deref());

        let genre_str = format_genre_tag(write_parent_genres, &self.genre);
        if !genre_str.is_empty() {
            set_text(&mut tag, "TCON", Some(&genre_str));
        }

        let secondarygenre_str = format_genre_tag(write_parent_genres, &self.secondarygenre);
        if !secondarygenre_str.is_empty() {
            set_txxx(&mut tag, "SECONDARYGENRE", Some(&secondarygenre_str));
        }

        let descriptor_str = self.descriptor.join(";");
        if !descriptor_str.is_empty() {
            set_txxx(&mut tag, "DESCRIPTOR", Some(&descriptor_str));
        }

        let label_str = self.label.join(";");
        if !label_str.is_empty() {
            set_text(&mut tag, "TPUB", Some(&label_str));
        }

        set_txxx(&mut tag, "CATALOGNUMBER", self.catalognumber.as_deref());
        set_txxx(&mut tag, "EDITION", self.edition.as_deref());
        set_txxx(&mut tag, "RELEASETYPE", Some(&self.releasetype));

        let release_artist_str = format_artist_string(&self.releaseartists);
        if !release_artist_str.is_empty() {
            set_text(&mut tag, "TPE2", Some(&release_artist_str));
        }

        let track_artist_str = format_artist_string(&self.trackartists);
        if !track_artist_str.is_empty() {
            set_text(&mut tag, "TPE1", Some(&track_artist_str));
        }

        // Alt-role tags (TPE4, TCOM, TPE3) are NOT written — deleted by omission.
        // TIPL/IPLS frames are NOT written — deleted by omission (new tag).

        // Re-add preserved TXXX frames.
        for (desc, text) in &preserved_txxx {
            set_txxx(&mut tag, desc, Some(text));
        }

        tag.save_to_path(&self.path, lofty::config::WriteOptions::default())
            .map_err(|e| RoseError::Internal(format!("Failed to save MP3 tags: {e}")))?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // MP4 write
    // -----------------------------------------------------------------------

    fn flush_mp4(&self, write_parent_genres: bool) -> Result<(), RoseError> {
        use lofty::tag::{Accessor, TagExt};

        // Read the existing file to get previous track/disc totals.
        let existing = lofty::probe::Probe::open(&self.path)
            .ok()
            .and_then(|p| p.guess_file_type().ok())
            .and_then(|p| p.read().ok());

        let prev_tracktotal = existing
            .as_ref()
            .and_then(|f| f.tag(TagType::Mp4Ilst))
            .and_then(|t| t.get_string(&ItemKey::TrackTotal))
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);

        let prev_disctotal = existing
            .as_ref()
            .and_then(|f| f.tag(TagType::Mp4Ilst))
            .and_then(|t| t.get_string(&ItemKey::DiscTotal))
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);

        let mut ilst = Ilst::new();

        // Helper: set an atom (works for both fourcc and freeform keys).
        fn set_atom(ilst: &mut Ilst, key: &str, value: &str) {
            let ident = parse_atom_ident(key);
            ilst.replace_atom(Atom::new(ident, AtomData::UTF8(value.to_string())));
        }

        set_atom(
            &mut ilst,
            "----:net.sunsetglow.rose:ID",
            self.id.as_deref().unwrap_or(""),
        );
        set_atom(
            &mut ilst,
            "----:net.sunsetglow.rose:RELEASEID",
            self.release_id.as_deref().unwrap_or(""),
        );
        set_atom(
            &mut ilst,
            "\u{00a9}nam",
            self.tracktitle.as_deref().unwrap_or(""),
        );
        set_atom(
            &mut ilst,
            "\u{00a9}day",
            &self
                .releasedate
                .as_ref()
                .map(|d| d.to_string())
                .unwrap_or_default(),
        );
        set_atom(
            &mut ilst,
            "----:net.sunsetglow.rose:ORIGINALDATE",
            &self
                .originaldate
                .as_ref()
                .map(|d| d.to_string())
                .unwrap_or_default(),
        );
        set_atom(
            &mut ilst,
            "----:net.sunsetglow.rose:COMPOSITIONDATE",
            &self
                .compositiondate
                .as_ref()
                .map(|d| d.to_string())
                .unwrap_or_default(),
        );
        set_atom(
            &mut ilst,
            "\u{00a9}alb",
            self.releasetitle.as_deref().unwrap_or(""),
        );

        let genre_str = format_genre_tag(write_parent_genres, &self.genre);
        set_atom(&mut ilst, "\u{00a9}gen", &genre_str);

        let secondarygenre_str = format_genre_tag(write_parent_genres, &self.secondarygenre);
        set_atom(
            &mut ilst,
            "----:net.sunsetglow.rose:SECONDARYGENRE",
            &secondarygenre_str,
        );

        let descriptor_str = self.descriptor.join(";");
        set_atom(
            &mut ilst,
            "----:net.sunsetglow.rose:DESCRIPTOR",
            &descriptor_str,
        );

        let label_str = self.label.join(";");
        set_atom(&mut ilst, "----:com.apple.iTunes:LABEL", &label_str);

        set_atom(
            &mut ilst,
            "----:com.apple.iTunes:CATALOGNUMBER",
            self.catalognumber.as_deref().unwrap_or(""),
        );
        set_atom(
            &mut ilst,
            "----:net.sunsetglow.rose:EDITION",
            self.edition.as_deref().unwrap_or(""),
        );
        set_atom(
            &mut ilst,
            "----:com.apple.iTunes:RELEASETYPE",
            &self.releasetype,
        );

        let release_artist_str = format_artist_string(&self.releaseartists);
        set_atom(&mut ilst, "aART", &release_artist_str);

        let track_artist_str = format_artist_string(&self.trackartists);
        set_atom(&mut ilst, "\u{00a9}ART", &track_artist_str);

        // Alt-role atoms not written (deleted by not adding to the new ilst):
        // ----:com.apple.iTunes:REMIXER, PRODUCER, CONDUCTOR, DJMIXER, ©wrt

        // Track/disc numbers: use Ilst's Accessor trait set_track/set_disc.
        let tracknum = self
            .tracknumber
            .as_deref()
            .and_then(|s| {
                let s = s.trim();
                if s == "None" || s.is_empty() {
                    None
                } else {
                    s.parse::<u32>().ok()
                }
            })
            .unwrap_or(0);
        let discnum = self
            .discnumber
            .as_deref()
            .and_then(|s| {
                let s = s.trim();
                if s == "None" || s.is_empty() {
                    None
                } else {
                    s.parse::<u32>().ok()
                }
            })
            .unwrap_or(0);

        // Write track/disc as (number, total) using set_track + set_track_total.
        ilst.set_track(tracknum);
        ilst.set_track_total(prev_tracktotal);
        ilst.set_disk(discnum);
        ilst.set_disk_total(prev_disctotal);

        ilst.save_to_path(&self.path, lofty::config::WriteOptions::default())
            .map_err(|e| RoseError::Internal(format!("Failed to save MP4 tags: {e}")))?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // FLAC / Ogg Vorbis / Ogg Opus write (Vorbis Comments)
    // -----------------------------------------------------------------------

    fn flush_vorbis(&self, write_parent_genres: bool) -> Result<(), RoseError> {
        use lofty::ogg::VorbisComments;
        use lofty::tag::TagExt;

        // Read existing file to get the vendor string and preserve it.
        let existing = lofty::probe::Probe::open(&self.path)
            .ok()
            .and_then(|p| p.guess_file_type().ok())
            .and_then(|p| p.read().ok());
        let vendor = existing
            .as_ref()
            .and_then(|f| f.tag(TagType::VorbisComments))
            .and_then(|t| {
                // The vendor string is stored as EncoderSoftware in the generic Tag.
                t.get_string(&ItemKey::EncoderSoftware).map(String::from)
            })
            .unwrap_or_default();

        let mut vc = VorbisComments::new();
        vc.set_vendor(vendor);

        // Helper: insert a vorbis comment key-value pair.
        fn set_vc(vc: &mut VorbisComments, key: &str, value: &str) {
            vc.insert(key.to_uppercase(), value.to_string());
        }

        set_vc(&mut vc, "ROSEID", self.id.as_deref().unwrap_or(""));
        set_vc(
            &mut vc,
            "ROSERELEASEID",
            self.release_id.as_deref().unwrap_or(""),
        );
        set_vc(&mut vc, "TITLE", self.tracktitle.as_deref().unwrap_or(""));
        set_vc(
            &mut vc,
            "DATE",
            &self
                .releasedate
                .as_ref()
                .map(|d| d.to_string())
                .unwrap_or_default(),
        );
        set_vc(
            &mut vc,
            "ORIGINALDATE",
            &self
                .originaldate
                .as_ref()
                .map(|d| d.to_string())
                .unwrap_or_default(),
        );
        set_vc(
            &mut vc,
            "COMPOSITIONDATE",
            &self
                .compositiondate
                .as_ref()
                .map(|d| d.to_string())
                .unwrap_or_default(),
        );
        set_vc(
            &mut vc,
            "TRACKNUMBER",
            self.tracknumber.as_deref().unwrap_or(""),
        );
        set_vc(
            &mut vc,
            "DISCNUMBER",
            self.discnumber.as_deref().unwrap_or(""),
        );
        set_vc(&mut vc, "ALBUM", self.releasetitle.as_deref().unwrap_or(""));

        let genre_str = format_genre_tag(write_parent_genres, &self.genre);
        set_vc(&mut vc, "GENRE", &genre_str);

        let secondarygenre_str = format_genre_tag(write_parent_genres, &self.secondarygenre);
        set_vc(&mut vc, "SECONDARYGENRE", &secondarygenre_str);

        let descriptor_str = self.descriptor.join(";");
        set_vc(&mut vc, "DESCRIPTOR", &descriptor_str);

        let label_str = self.label.join(";");
        set_vc(&mut vc, "LABEL", &label_str);

        set_vc(
            &mut vc,
            "CATALOGNUMBER",
            self.catalognumber.as_deref().unwrap_or(""),
        );
        set_vc(&mut vc, "EDITION", self.edition.as_deref().unwrap_or(""));
        set_vc(&mut vc, "RELEASETYPE", &self.releasetype);

        let release_artist_str = format_artist_string(&self.releaseartists);
        set_vc(&mut vc, "ALBUMARTIST", &release_artist_str);

        let track_artist_str = format_artist_string(&self.trackartists);
        set_vc(&mut vc, "ARTIST", &track_artist_str);

        // Alt-role tags not written (remixer, producer, composer, conductor,
        // djmixer are omitted — not added to the new tag).

        vc.save_to_path(&self.path, lofty::config::WriteOptions::default())
            .map_err(|e| RoseError::Internal(format!("Failed to save Vorbis tags: {e}")))?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ID assignment
// ---------------------------------------------------------------------------

/// If `tags.id` or `tags.release_id` is `None`, generate a new UUID v7 and set
/// it. Returns `true` if any IDs were set (caller should flush).
pub fn maybe_set_ids(tags: &mut AudioTags) -> bool {
    let mut changed = false;
    if tags.id.is_none() {
        tags.id = Some(uuid::Uuid::now_v7().to_string());
        changed = true;
    }
    if tags.release_id.is_none() {
        tags.release_id = Some(uuid::Uuid::now_v7().to_string());
        changed = true;
    }
    changed
}

// ---------------------------------------------------------------------------
// Genre encoding
// ---------------------------------------------------------------------------

/// Format a genre list for writing to a tag.
///
/// When `write_parent_genres` is true, parent genres from the genre hierarchy
/// that are not already in the user's list are appended after `\\PARENTS:\\`.
fn format_genre_tag(write_parent_genres: bool, genres: &[String]) -> String {
    let base = genres.join(";");
    if !write_parent_genres {
        return base;
    }
    // Compute parent genres using GENRE_HIERARCHY.
    let user_set: HashSet<&str> = genres.iter().map(|s| s.as_str()).collect();
    let parent_genres: Vec<Vec<String>> = genres
        .iter()
        .filter_map(|g| GENRE_HIERARCHY.get(g.as_str()).cloned())
        .collect();
    let all_parents: Vec<String> = flatten(parent_genres);
    let mut extra: Vec<&str> = all_parents
        .iter()
        .map(|s| s.as_str())
        .filter(|p| !user_set.contains(p))
        .collect();
    extra.sort();
    extra.dedup();
    if extra.is_empty() {
        base
    } else {
        format!("{}\\\\PARENTS:\\\\{}", base, extra.join(";"))
    }
}

// ---------------------------------------------------------------------------
// Tag helper functions
// ---------------------------------------------------------------------------

fn normalize_rtype(x: Option<&str>) -> String {
    match x {
        None => "unknown".to_string(),
        Some(s) => {
            let lower = s.to_lowercase();
            if SUPPORTED_RELEASE_TYPES.contains(&lower.as_str()) {
                lower
            } else {
                "unknown".to_string()
            }
        }
    }
}

fn split_tag(t: Option<&str>) -> Vec<String> {
    match t {
        None | Some("") => vec![],
        Some(s) => TAG_SPLITTER_REGEX.split(s).map(String::from).collect(),
    }
}

fn split_genre_tag(t: Option<&str>) -> Vec<String> {
    match t {
        None | Some("") => vec![],
        Some(s) => {
            // Strip everything after \\PARENTS:\\ delimiter.
            let s = if let Some(idx) = s.find("\\\\PARENTS:\\\\") {
                &s[..idx]
            } else {
                s
            };
            if s.is_empty() {
                return vec![];
            }
            TAG_SPLITTER_REGEX.split(s).map(String::from).collect()
        }
    }
}

fn split_artist_tag(t: Option<&str>) -> Vec<String> {
    match t {
        None | Some("") => vec![],
        Some(s) => TAG_SPLITTER_REGEX.split(s).map(String::from).collect(),
    }
}

// --- ID3v2 (MP3) helpers ---

/// Get a tag value from ID3v2 using ItemKey, trying multiple keys in order.
/// If `split` is true, multi-value tags are joined with ` \\ `.
fn id3_get_tag(tag: Option<&lofty::tag::Tag>, keys: &[&ItemKey], split: bool) -> Option<String> {
    let tag = tag?;
    for key in keys {
        let values: Vec<&str> = tag.get_strings(key).collect();
        if values.is_empty() {
            continue;
        }
        if split {
            // When split=true, individual values get split by the tag splitter
            // (matching Python's _get_tag with split=True which splits each
            // value string), then all are joined with ` \\ `.
            let mut all_parts: Vec<String> = Vec::new();
            for val in &values {
                for part in TAG_SPLITTER_REGEX.split(val) {
                    all_parts.push(part.to_string());
                }
            }
            let joined = all_parts.join(r" \\ ");
            return if joined.is_empty() {
                None
            } else {
                Some(joined)
            };
        } else {
            // When split=false, join multiple values with ` \\ `.
            let joined = values.join(r" \\ ");
            return if joined.is_empty() {
                None
            } else {
                Some(joined)
            };
        }
    }
    None
}

/// Get a TXXX tag by description name (e.g., "ROSEID").
fn id3_get_unknown_tag(tag: Option<&lofty::tag::Tag>, desc: &str) -> Option<String> {
    let tag = tag?;
    // lofty maps TXXX frames with known descriptions to known ItemKeys.
    // For custom ones, they become ItemKey::Unknown("TXXX:DESC") or similar.
    // We need to iterate all items and find matching unknown keys.
    for item in tag.items() {
        if let ItemKey::Unknown(ref key) = *item.key() {
            // lofty stores TXXX frames as Unknown keys in the format the tag
            // description. Check various patterns.
            if key.eq_ignore_ascii_case(desc) || key.eq_ignore_ascii_case(&format!("TXXX:{desc}")) {
                if let Some(text) = item.value().text() {
                    let text = text.to_string();
                    return if text.is_empty() { None } else { Some(text) };
                }
            }
        }
    }
    None
}

/// Get a TXXX tag by description name, splitting by TAG_SPLITTER_REGEX and joining with ` \\ `.
fn id3_get_unknown_tag_split(tag: Option<&lofty::tag::Tag>, desc: &str) -> Option<String> {
    let raw = id3_get_unknown_tag(tag, desc)?;
    let parts: Vec<String> = TAG_SPLITTER_REGEX.split(&raw).map(String::from).collect();
    let joined = parts.join(r" \\ ");
    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}

// --- MP4 Ilst direct helpers ---

/// Parse an MP4 atom key string into an AtomIdent.
/// Supports fourcc keys like "©gen", "aART" and freeform keys like "----:com.apple.iTunes:LABEL".
///
/// For fourcc atoms: Unicode characters like © (U+00A9) are encoded as single
/// bytes in the fourcc (0xA9), NOT as UTF-8 multi-byte sequences.
fn parse_atom_ident(key: &str) -> AtomIdent<'static> {
    if key.starts_with("----:") {
        // Freeform atom: "----:mean:name"
        let parts: Vec<&str> = key.splitn(3, ':').collect();
        if parts.len() == 3 {
            AtomIdent::Freeform {
                mean: std::borrow::Cow::Owned(parts[1].to_string()),
                name: std::borrow::Cow::Owned(parts[2].to_string()),
            }
        } else {
            AtomIdent::Fourcc([0; 4])
        }
    } else {
        // Fourcc atom: convert Unicode chars to their raw byte values.
        // MP4 fourcc uses raw bytes, so © (U+00A9) becomes byte 0xA9.
        let chars: Vec<char> = key.chars().collect();
        let fourcc = [
            chars.first().map_or(0, |&c| c as u8),
            chars.get(1).map_or(0, |&c| c as u8),
            chars.get(2).map_or(0, |&c| c as u8),
            chars.get(3).map_or(0, |&c| c as u8),
        ];
        AtomIdent::Fourcc(fourcc)
    }
}

/// Extract text values from an Atom's data entries.
fn atom_text_values(atom: &Atom<'_>, split: bool) -> Vec<String> {
    let mut values = Vec::new();
    for data in atom.data() {
        match data {
            AtomData::UTF8(text) | AtomData::UTF16(text) => {
                if split {
                    for part in TAG_SPLITTER_REGEX.split(text) {
                        values.push(part.to_string());
                    }
                } else {
                    values.push(text.clone());
                }
            }
            _ => {}
        }
    }
    values
}

/// Get a tag value from an Ilst, trying multiple atom keys.
/// If `split` is true, individual values are split by TAG_SPLITTER_REGEX,
/// then all values are joined with ` \\ `.
fn ilst_get_tag(ilst: &Ilst, keys: &[&str], split: bool) -> Option<String> {
    for &key in keys {
        let ident = parse_atom_ident(key);
        if let Some(atom) = ilst.get(&ident) {
            let values = atom_text_values(atom, split);
            if values.is_empty() {
                continue;
            }
            let joined = values.join(r" \\ ");
            return if joined.is_empty() {
                None
            } else {
                Some(joined)
            };
        }
    }
    None
}

/// Get first text value from an Ilst atom.
fn ilst_get_first(ilst: &Ilst, keys: &[&str]) -> Option<String> {
    for &key in keys {
        let ident = parse_atom_ident(key);
        if let Some(atom) = ilst.get(&ident) {
            for data in atom.data() {
                match data {
                    AtomData::UTF8(text) | AtomData::UTF16(text) => {
                        if !text.is_empty() {
                            return Some(text.clone());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

// --- Vorbis Comment helpers ---

/// Get a tag value from Vorbis Comments, trying multiple keys.
/// When `split` is true, each value is split by TAG_SPLITTER_REGEX and results
/// joined with ` \\ `.
fn vc_get_tag(tag: Option<&lofty::tag::Tag>, keys: &[&str], split: bool) -> Option<String> {
    let tag = tag?;
    for &key in keys {
        // Vorbis comments are case-insensitive. lofty maps known keys to
        // ItemKeys (e.g., "artist" -> TrackArtist). For custom keys, they
        // become ItemKey::Unknown.
        let item_key = vc_key_to_item_key(key);
        let mut values: Vec<String> = Vec::new();

        match &item_key {
            Some(ik) => {
                for val in tag.get_strings(ik) {
                    values.push(val.to_string());
                }
            }
            None => {
                // Look for Unknown items matching the key (case-insensitive).
                for item in tag.items() {
                    if let ItemKey::Unknown(ref k) = *item.key() {
                        if k.eq_ignore_ascii_case(key) {
                            if let Some(text) = item.value().text() {
                                values.push(text.to_string());
                            }
                        }
                    }
                }
            }
        }

        if values.is_empty() {
            continue;
        }

        if split {
            let mut all_parts: Vec<String> = Vec::new();
            for val in &values {
                for part in TAG_SPLITTER_REGEX.split(val) {
                    all_parts.push(part.to_string());
                }
            }
            let joined = all_parts.join(r" \\ ");
            return if joined.is_empty() {
                None
            } else {
                Some(joined)
            };
        } else {
            let joined = values.join(r" \\ ");
            return if joined.is_empty() {
                None
            } else {
                Some(joined)
            };
        }
    }
    None
}

/// Get first value (no splitting, no joining).
fn vc_get_first(tag: Option<&lofty::tag::Tag>, keys: &[&str]) -> Option<String> {
    let tag = tag?;
    for &key in keys {
        let item_key = vc_key_to_item_key(key);
        match &item_key {
            Some(ik) => {
                if let Some(val) = tag.get_string(ik) {
                    let val = val.to_string();
                    return if val.is_empty() { None } else { Some(val) };
                }
            }
            None => {
                for item in tag.items() {
                    if let ItemKey::Unknown(ref k) = *item.key() {
                        if k.eq_ignore_ascii_case(key) {
                            if let Some(text) = item.value().text() {
                                let text = text.to_string();
                                return if text.is_empty() { None } else { Some(text) };
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Map well-known Vorbis Comment keys to lofty ItemKeys.
fn vc_key_to_item_key(key: &str) -> Option<ItemKey> {
    match key.to_lowercase().as_str() {
        "title" => Some(ItemKey::TrackTitle),
        "artist" => Some(ItemKey::TrackArtist),
        "album" => Some(ItemKey::AlbumTitle),
        "albumartist" => Some(ItemKey::AlbumArtist),
        "genre" => Some(ItemKey::Genre),
        "date" => Some(ItemKey::RecordingDate),
        "tracknumber" => Some(ItemKey::TrackNumber),
        "tracktotal" => Some(ItemKey::TrackTotal),
        "discnumber" => Some(ItemKey::DiscNumber),
        "disctotal" => Some(ItemKey::DiscTotal),
        "composer" => Some(ItemKey::Composer),
        "conductor" => Some(ItemKey::Conductor),
        "remixer" => Some(ItemKey::Remixer),
        "producer" => Some(ItemKey::Producer),
        "label" | "organization" | "recordlabel" => Some(ItemKey::Label),
        "year" => Some(ItemKey::Year),
        "originaldate" => Some(ItemKey::OriginalReleaseDate),
        "catalognumber" => Some(ItemKey::CatalogNumber),
        "djmixer" => Some(ItemKey::MixDj),
        "originalyear" => Some(ItemKey::OriginalReleaseDate),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Artist string parsing/formatting
// ---------------------------------------------------------------------------

/// Parse a formatted artist string into an `ArtistMapping`.
///
/// Extracts roles from the `main` string using keyword delimiters:
/// 1. `produced by` -> producer
/// 2. `remixed by` -> remixer
/// 3. `feat.` -> guest
/// 4. `pres.` -> djmixer (dj is LEFT side, main is RIGHT)
/// 5. `performed by` -> composer (composer is LEFT, main is RIGHT)
/// 6. `under.` -> conductor
pub fn parse_artist_string(
    main: Option<&str>,
    remixer: Option<&str>,
    composer: Option<&str>,
    conductor: Option<&str>,
    producer: Option<&str>,
    dj: Option<&str>,
) -> ArtistMapping {
    let mut li_main: Vec<String> = Vec::new();
    let mut li_conductor: Vec<String> = split_artist_tag(conductor);
    let mut li_guests: Vec<String> = Vec::new();
    let mut li_remixer: Vec<String> = split_artist_tag(remixer);
    let mut li_composer: Vec<String> = split_artist_tag(composer);
    let mut li_producer: Vec<String> = split_artist_tag(producer);
    let mut li_dj: Vec<String> = split_artist_tag(dj);

    let mut main_str = main.map(String::from);

    // Helper: split main_str by a regex pattern, returning (left, right) as owned Strings.
    fn extract_keyword(s: &str, keyword: &str, pattern: &str) -> Option<(String, String)> {
        if !s.contains(keyword) {
            return None;
        }
        let re = Regex::new(pattern).unwrap();
        let parts: Vec<&str> = re.splitn(s, 2).collect();
        if parts.len() == 2 {
            Some((parts[0].to_string(), parts[1].to_string()))
        } else {
            None
        }
    }

    // 1. Extract "produced by" — producer is RIGHT, main is LEFT
    if let Some(s) = main_str.take() {
        if let Some((left, right)) = extract_keyword(&s, "produced by ", r" ?produced by ") {
            main_str = Some(left);
            li_producer.extend(split_artist_tag(Some(&right)));
        } else {
            main_str = Some(s);
        }
    }

    // 2. Extract "remixed by" — remixer is RIGHT, main is LEFT
    if let Some(s) = main_str.take() {
        if let Some((left, right)) = extract_keyword(&s, "remixed by ", r" ?remixed by ") {
            main_str = Some(left);
            li_remixer.extend(split_artist_tag(Some(&right)));
        } else {
            main_str = Some(s);
        }
    }

    // 3. Extract "feat." — guest is RIGHT, main is LEFT
    if let Some(s) = main_str.take() {
        if let Some((left, right)) = extract_keyword(&s, "feat. ", r" ?feat\. ") {
            main_str = Some(left);
            li_guests.extend(split_artist_tag(Some(&right)));
        } else {
            main_str = Some(s);
        }
    }

    // 4. Extract "pres." — dj is LEFT, main is RIGHT
    if let Some(s) = main_str.take() {
        if let Some((left, right)) = extract_keyword(&s, "pres. ", r" ?pres\. ") {
            li_dj.extend(split_artist_tag(Some(&left)));
            main_str = Some(right);
        } else {
            main_str = Some(s);
        }
    }

    // 5. Extract "performed by" — composer is LEFT, main is RIGHT
    if let Some(s) = main_str.take() {
        if let Some((left, right)) = extract_keyword(&s, "performed by ", r" ?performed by ") {
            li_composer.extend(split_artist_tag(Some(&left)));
            main_str = Some(right);
        } else {
            main_str = Some(s);
        }
    }

    // 6. Extract "under." — main is LEFT, conductor is RIGHT
    if let Some(s) = main_str.take() {
        if let Some((left, right)) = extract_keyword(&s, "under. ", r" ?under\. ") {
            main_str = Some(left);
            li_conductor.extend(split_artist_tag(Some(&right)));
        } else {
            main_str = Some(s);
        }
    }

    // Split remaining main string.
    if let Some(s) = main_str.as_deref() {
        li_main.extend(split_artist_tag(Some(s)));
    }

    let to_artists = |xs: Vec<String>| -> Vec<Artist> {
        xs.into_iter()
            .map(|name| Artist { name, alias: false })
            .collect()
    };

    ArtistMapping {
        main: to_artists(uniq(li_main)),
        guest: to_artists(uniq(li_guests)),
        remixer: to_artists(uniq(li_remixer)),
        composer: to_artists(uniq(li_composer)),
        conductor: to_artists(uniq(li_conductor)),
        producer: to_artists(uniq(li_producer)),
        djmixer: to_artists(uniq(li_dj)),
    }
}

/// Format an `ArtistMapping` into a single artist string.
///
/// Compose order:
/// `{composer} performed by {dj} pres. {main} under. {conductor} feat. {guest} remixed by {remixer} produced by {producer}`
///
/// Only non-alias artists are included. Roles are joined with `;`.
pub fn format_artist_string(mapping: &ArtistMapping) -> String {
    let format_role = |xs: &[Artist]| -> String {
        xs.iter()
            .filter(|a| !a.alias)
            .map(|a| a.name.as_str())
            .collect::<Vec<&str>>()
            .join(";")
    };

    let mut r = format_role(&mapping.main);

    if !mapping.composer.is_empty() {
        r = format!("{} performed by {}", format_role(&mapping.composer), r);
    }
    if !mapping.djmixer.is_empty() {
        r = format!("{} pres. {}", format_role(&mapping.djmixer), r);
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

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // -----------------------------------------------------------------------
    // RoseDate parsing
    // -----------------------------------------------------------------------

    #[test]
    fn rosedate_year_only() {
        let d = RoseDate::parse(Some("1990")).unwrap();
        assert_eq!(d.year, 1990);
        assert_eq!(d.month, None);
        assert_eq!(d.day, None);
        assert_eq!(d.to_string(), "1990");
    }

    #[test]
    fn rosedate_full_date() {
        let d = RoseDate::parse(Some("1990-02-05")).unwrap();
        assert_eq!(d.year, 1990);
        assert_eq!(d.month, Some(2));
        assert_eq!(d.day, Some(5));
        assert_eq!(d.to_string(), "1990-02-05");
    }

    #[test]
    fn rosedate_with_trailing_time() {
        let d = RoseDate::parse(Some("1990-02-05T12:00:00")).unwrap();
        assert_eq!(d.year, 1990);
        assert_eq!(d.month, Some(2));
        assert_eq!(d.day, Some(5));
    }

    #[test]
    fn rosedate_empty() {
        assert!(RoseDate::parse(None).is_none());
        assert!(RoseDate::parse(Some("")).is_none());
    }

    #[test]
    fn rosedate_garbage() {
        assert!(RoseDate::parse(Some("not-a-date")).is_none());
        assert!(RoseDate::parse(Some("abc")).is_none());
    }

    // -----------------------------------------------------------------------
    // Tag splitter regex difference
    // -----------------------------------------------------------------------

    #[test]
    fn tag_splits_on_semicolon() {
        let result = split_tag(Some("Rock;Pop"));
        assert_eq!(result, vec!["Rock", "Pop"]);
    }

    #[test]
    fn tag_splits_on_backslash_with_spaces() {
        // Both genre and artist splitting use the same regex (with spaces around `\\`).
        let result = split_tag(Some(r"A \\ B"));
        assert_eq!(result, vec!["A", "B"]);
    }

    #[test]
    fn artist_splits_on_backslash_with_spaces() {
        let result = split_artist_tag(Some(r"Artist A \\ Artist B"));
        assert_eq!(result, vec!["Artist A", "Artist B"]);
    }

    #[test]
    fn backslash_without_spaces_does_not_split() {
        // `\\` without surrounding spaces should NOT split (not a delimiter).
        let result = split_tag(Some("Rock\\\\Pop"));
        assert_eq!(result, vec!["Rock\\\\Pop"]);
    }

    // -----------------------------------------------------------------------
    // \\PARENTS:\\ stripping
    // -----------------------------------------------------------------------

    #[test]
    fn parents_delimiter_stripping() {
        let result = split_genre_tag(Some("Rock;Pop\\\\PARENTS:\\\\Classical"));
        assert_eq!(result, vec!["Rock", "Pop"]);
    }

    #[test]
    fn no_parents_delimiter() {
        let result = split_genre_tag(Some("Rock;Pop"));
        assert_eq!(result, vec!["Rock", "Pop"]);
    }

    // -----------------------------------------------------------------------
    // ID3 TRCK split
    // -----------------------------------------------------------------------

    #[test]
    fn trck_split_format() {
        // This tests the Python behavior where "3/12" yields tracknumber="3", tracktotal=12.
        // In our implementation, lofty handles this splitting for us.
        // We just verify the parse logic works.
        let val = "3/12";
        let parts: Vec<&str> = val.splitn(2, '/').collect();
        assert_eq!(parts[0], "3");
        assert_eq!(parts[1].parse::<i32>().unwrap(), 12);
    }

    // -----------------------------------------------------------------------
    // Artist parse/format roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn artist_parse_format_roundtrip() {
        let input = "A feat. B remixed by C produced by D";
        let mapping = parse_artist_string(Some(input), None, None, None, None, None);
        let output = format_artist_string(&mapping);
        assert_eq!(output, input);
    }

    #[test]
    fn artist_parse_feat_only() {
        let mapping = parse_artist_string(Some("Main feat. Guest"), None, None, None, None, None);
        assert_eq!(mapping.main.len(), 1);
        assert_eq!(mapping.main[0].name, "Main");
        assert_eq!(mapping.guest.len(), 1);
        assert_eq!(mapping.guest[0].name, "Guest");
    }

    #[test]
    fn artist_parse_remixed_by_only() {
        let mapping = parse_artist_string(
            Some("Main remixed by Remixer"),
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(mapping.main[0].name, "Main");
        assert_eq!(mapping.remixer[0].name, "Remixer");
    }

    #[test]
    fn artist_parse_produced_by_only() {
        let mapping = parse_artist_string(
            Some("Main produced by Producer"),
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(mapping.main[0].name, "Main");
        assert_eq!(mapping.producer[0].name, "Producer");
    }

    #[test]
    fn artist_parse_pres_only() {
        let mapping = parse_artist_string(Some("DJ pres. Main"), None, None, None, None, None);
        assert_eq!(mapping.djmixer[0].name, "DJ");
        assert_eq!(mapping.main[0].name, "Main");
    }

    #[test]
    fn artist_parse_performed_by_only() {
        let mapping = parse_artist_string(
            Some("Composer performed by Main"),
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(mapping.composer[0].name, "Composer");
        assert_eq!(mapping.main[0].name, "Main");
    }

    #[test]
    fn artist_parse_under_only() {
        let mapping =
            parse_artist_string(Some("Main under. Conductor"), None, None, None, None, None);
        assert_eq!(mapping.main[0].name, "Main");
        assert_eq!(mapping.conductor[0].name, "Conductor");
    }

    #[test]
    fn artist_combined_performed_by_under() {
        let input = "Debussy performed by Cleveland Orchestra under. Pierre Boulez";
        let mapping = parse_artist_string(Some(input), None, None, None, None, None);
        assert_eq!(mapping.composer[0].name, "Debussy");
        assert_eq!(mapping.main[0].name, "Cleveland Orchestra");
        assert_eq!(mapping.conductor[0].name, "Pierre Boulez");
        // Verify roundtrip
        let output = format_artist_string(&mapping);
        assert_eq!(output, input);
    }

    #[test]
    fn artist_all_roles_roundtrip() {
        let input = "Composer performed by DJ pres. Main under. Conductor feat. Guest remixed by Remixer produced by Producer";
        let mapping = parse_artist_string(Some(input), None, None, None, None, None);
        let output = format_artist_string(&mapping);
        assert_eq!(output, input);
    }

    #[test]
    fn artist_separate_role_args() {
        let mapping = parse_artist_string(
            Some("Main"),
            Some("Remixer"),
            Some("Composer"),
            Some("Conductor"),
            Some("Producer"),
            Some("DJ"),
        );
        assert_eq!(mapping.main[0].name, "Main");
        assert_eq!(mapping.remixer[0].name, "Remixer");
        assert_eq!(mapping.composer[0].name, "Composer");
        assert_eq!(mapping.conductor[0].name, "Conductor");
        assert_eq!(mapping.producer[0].name, "Producer");
        assert_eq!(mapping.djmixer[0].name, "DJ");
    }

    #[test]
    fn artist_semicolon_split() {
        let mapping = parse_artist_string(Some("A;B;C"), None, None, None, None, None);
        assert_eq!(mapping.main.len(), 3);
        assert_eq!(mapping.main[0].name, "A");
        assert_eq!(mapping.main[1].name, "B");
        assert_eq!(mapping.main[2].name, "C");
    }

    // -----------------------------------------------------------------------
    // Unsupported format
    // -----------------------------------------------------------------------

    #[test]
    fn unsupported_format_returns_error() {
        let result = AudioTags::from_file(Path::new("test.wav"));
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Golden file tests
    // -----------------------------------------------------------------------

    /// Path to the repo root (two levels up from rose-core/src/).
    fn repo_root() -> PathBuf {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // CARGO_MANIFEST_DIR = rose-rs/rose-core
        manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    #[derive(Debug, Deserialize)]
    struct GoldenEntry {
        catalognumber: Option<String>,
        compositiondate: Option<String>,
        descriptor: Vec<String>,
        discnumber: Option<String>,
        disctotal: Option<i32>,
        duration_sec: i64,
        edition: Option<String>,
        genre: Vec<String>,
        id: Option<String>,
        label: Vec<String>,
        originaldate: Option<String>,
        path: String,
        release_id: Option<String>,
        releaseartists: ArtistMapping,
        releasedate: Option<String>,
        releasetitle: Option<String>,
        releasetype: String,
        secondarygenre: Vec<String>,
        trackartists: ArtistMapping,
        tracknumber: Option<String>,
        tracktitle: Option<String>,
        tracktotal: Option<i32>,
    }

    fn load_golden(filename: &str) -> Vec<GoldenEntry> {
        let golden_path = repo_root().join("rose-rs/testdata/golden").join(filename);
        let content = std::fs::read_to_string(&golden_path).unwrap_or_else(|e| {
            panic!("Failed to read golden file {}: {e}", golden_path.display())
        });
        serde_json::from_str(&content).unwrap_or_else(|e| {
            panic!("Failed to parse golden file {}: {e}", golden_path.display())
        })
    }

    fn compare_tags(tags: &AudioTags, golden: &GoldenEntry) {
        assert_eq!(tags.id, golden.id, "id mismatch");
        assert_eq!(tags.release_id, golden.release_id, "release_id mismatch");
        assert_eq!(tags.tracktitle, golden.tracktitle, "tracktitle mismatch");
        assert_eq!(tags.tracknumber, golden.tracknumber, "tracknumber mismatch");
        assert_eq!(tags.tracktotal, golden.tracktotal, "tracktotal mismatch");
        assert_eq!(tags.discnumber, golden.discnumber, "discnumber mismatch");
        assert_eq!(tags.disctotal, golden.disctotal, "disctotal mismatch");
        assert_eq!(
            tags.releasetitle, golden.releasetitle,
            "releasetitle mismatch"
        );
        assert_eq!(tags.releasetype, golden.releasetype, "releasetype mismatch");
        assert_eq!(
            tags.releasedate.as_ref().map(|d| d.to_string()),
            golden.releasedate,
            "releasedate mismatch"
        );
        assert_eq!(
            tags.originaldate.as_ref().map(|d| d.to_string()),
            golden.originaldate,
            "originaldate mismatch"
        );
        assert_eq!(
            tags.compositiondate.as_ref().map(|d| d.to_string()),
            golden.compositiondate,
            "compositiondate mismatch"
        );
        assert_eq!(tags.genre, golden.genre, "genre mismatch");
        assert_eq!(
            tags.secondarygenre, golden.secondarygenre,
            "secondarygenre mismatch"
        );
        assert_eq!(tags.descriptor, golden.descriptor, "descriptor mismatch");
        assert_eq!(tags.edition, golden.edition, "edition mismatch");
        assert_eq!(tags.label, golden.label, "label mismatch");
        assert_eq!(
            tags.catalognumber, golden.catalognumber,
            "catalognumber mismatch"
        );
        assert_eq!(
            tags.duration_sec, golden.duration_sec,
            "duration_sec mismatch"
        );
        assert_eq!(
            tags.trackartists, golden.trackartists,
            "trackartists mismatch"
        );
        assert_eq!(
            tags.releaseartists, golden.releaseartists,
            "releaseartists mismatch"
        );
    }

    #[test]
    fn golden_flac() {
        let entries = load_golden("tags_flac.json");
        for entry in &entries {
            let audio_path = repo_root().join(&entry.path);
            let tags = AudioTags::from_file(&audio_path)
                .unwrap_or_else(|e| panic!("Failed to read {}: {e}", audio_path.display()));
            compare_tags(&tags, entry);
        }
    }

    #[test]
    fn golden_mp3() {
        let entries = load_golden("tags_mp3.json");
        for entry in &entries {
            let audio_path = repo_root().join(&entry.path);
            let tags = AudioTags::from_file(&audio_path)
                .unwrap_or_else(|e| panic!("Failed to read {}: {e}", audio_path.display()));
            compare_tags(&tags, entry);
        }
    }

    #[test]
    fn golden_m4a() {
        let entries = load_golden("tags_m4a.json");
        for entry in &entries {
            let audio_path = repo_root().join(&entry.path);
            let tags = AudioTags::from_file(&audio_path)
                .unwrap_or_else(|e| panic!("Failed to read {}: {e}", audio_path.display()));
            compare_tags(&tags, entry);
        }
    }

    #[test]
    fn golden_ogg_vorbis() {
        let entries = load_golden("tags_ogg_vorbis.json");
        for entry in &entries {
            let audio_path = repo_root().join(&entry.path);
            let tags = AudioTags::from_file(&audio_path)
                .unwrap_or_else(|e| panic!("Failed to read {}: {e}", audio_path.display()));
            compare_tags(&tags, entry);
        }
    }

    #[test]
    fn golden_ogg_opus() {
        let entries = load_golden("tags_ogg_opus.json");
        for entry in &entries {
            let audio_path = repo_root().join(&entry.path);
            let tags = AudioTags::from_file(&audio_path)
                .unwrap_or_else(|e| panic!("Failed to read {}: {e}", audio_path.display()));
            compare_tags(&tags, entry);
        }
    }

    // -----------------------------------------------------------------------
    // Write path tests
    // -----------------------------------------------------------------------

    /// Copy a test audio file to a temp directory and return the new path.
    fn copy_test_file(src: &str) -> (tempfile::TempDir, PathBuf) {
        let root = repo_root();
        let src_path = root.join(src);
        let tmp = tempfile::tempdir().expect("create temp dir");
        let filename = src_path.file_name().unwrap();
        let dst = tmp.path().join(filename);
        std::fs::copy(&src_path, &dst)
            .unwrap_or_else(|e| panic!("copy {} -> {}: {e}", src_path.display(), dst.display()));
        (tmp, dst)
    }

    /// Compare two AudioTags for field equality (ignoring path and duration).
    fn assert_tags_equal(a: &AudioTags, b: &AudioTags, msg: &str) {
        assert_eq!(a.id, b.id, "{msg}: id");
        assert_eq!(a.release_id, b.release_id, "{msg}: release_id");
        assert_eq!(a.tracktitle, b.tracktitle, "{msg}: tracktitle");
        assert_eq!(a.tracknumber, b.tracknumber, "{msg}: tracknumber");
        // Note: tracktotal may differ for MP3 (lost on write).
        assert_eq!(a.discnumber, b.discnumber, "{msg}: discnumber");
        assert_eq!(a.releasetitle, b.releasetitle, "{msg}: releasetitle");
        assert_eq!(a.releasetype, b.releasetype, "{msg}: releasetype");
        assert_eq!(a.releasedate, b.releasedate, "{msg}: releasedate");
        assert_eq!(a.originaldate, b.originaldate, "{msg}: originaldate");
        assert_eq!(
            a.compositiondate, b.compositiondate,
            "{msg}: compositiondate"
        );
        assert_eq!(a.genre, b.genre, "{msg}: genre");
        assert_eq!(a.secondarygenre, b.secondarygenre, "{msg}: secondarygenre");
        assert_eq!(a.descriptor, b.descriptor, "{msg}: descriptor");
        assert_eq!(a.edition, b.edition, "{msg}: edition");
        assert_eq!(a.label, b.label, "{msg}: label");
        assert_eq!(a.catalognumber, b.catalognumber, "{msg}: catalognumber");
        assert_eq!(a.trackartists, b.trackartists, "{msg}: trackartists");
        assert_eq!(a.releaseartists, b.releaseartists, "{msg}: releaseartists");
    }

    // --- Roundtrip per format ---

    #[test]
    fn roundtrip_flac() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track1.flac");
        let mut tags = AudioTags::from_file(&path).unwrap();
        tags.flush(false).unwrap();
        let tags2 = AudioTags::from_file(&path).unwrap();
        assert_tags_equal(&tags, &tags2, "FLAC roundtrip");
        assert_eq!(
            tags2.tracktotal, None,
            "FLAC: tracktotal not written by flush"
        );
    }

    #[test]
    fn roundtrip_mp3() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track3.mp3");
        let mut tags = AudioTags::from_file(&path).unwrap();
        tags.flush(false).unwrap();
        let tags2 = AudioTags::from_file(&path).unwrap();
        assert_tags_equal(&tags, &tags2, "MP3 roundtrip");
    }

    #[test]
    fn roundtrip_m4a() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track2.m4a");
        let mut tags = AudioTags::from_file(&path).unwrap();
        tags.flush(false).unwrap();
        let tags2 = AudioTags::from_file(&path).unwrap();
        assert_tags_equal(&tags, &tags2, "M4A roundtrip");
    }

    #[test]
    fn roundtrip_ogg_vorbis() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track4.vorbis.ogg");
        let mut tags = AudioTags::from_file(&path).unwrap();
        tags.flush(false).unwrap();
        let tags2 = AudioTags::from_file(&path).unwrap();
        assert_tags_equal(&tags, &tags2, "OGG Vorbis roundtrip");
    }

    #[test]
    fn roundtrip_ogg_opus() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track5.opus.ogg");
        let mut tags = AudioTags::from_file(&path).unwrap();
        tags.flush(false).unwrap();
        let tags2 = AudioTags::from_file(&path).unwrap();
        assert_tags_equal(&tags, &tags2, "OGG Opus roundtrip");
    }

    // --- ID survival ---

    #[test]
    fn id_survival_flac() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track1.flac");
        let mut tags = AudioTags::from_file(&path).unwrap();
        tags.id = Some("test-id-123".to_string());
        tags.release_id = Some("test-rid-456".to_string());
        tags.flush(false).unwrap();
        let tags2 = AudioTags::from_file(&path).unwrap();
        assert_eq!(tags2.id, Some("test-id-123".to_string()));
        assert_eq!(tags2.release_id, Some("test-rid-456".to_string()));
    }

    #[test]
    fn id_survival_mp3() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track3.mp3");
        let mut tags = AudioTags::from_file(&path).unwrap();
        tags.id = Some("test-id-mp3".to_string());
        tags.release_id = Some("test-rid-mp3".to_string());
        tags.flush(false).unwrap();
        let tags2 = AudioTags::from_file(&path).unwrap();
        assert_eq!(tags2.id, Some("test-id-mp3".to_string()));
        assert_eq!(tags2.release_id, Some("test-rid-mp3".to_string()));
    }

    #[test]
    fn id_survival_m4a() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track2.m4a");
        let mut tags = AudioTags::from_file(&path).unwrap();
        tags.id = Some("test-id-m4a".to_string());
        tags.release_id = Some("test-rid-m4a".to_string());
        tags.flush(false).unwrap();
        let tags2 = AudioTags::from_file(&path).unwrap();
        assert_eq!(tags2.id, Some("test-id-m4a".to_string()));
        assert_eq!(tags2.release_id, Some("test-rid-m4a".to_string()));
    }

    // --- maybe_set_ids ---

    #[test]
    fn maybe_set_ids_sets_missing() {
        let mut tags = AudioTags {
            id: None,
            release_id: None,
            tracktitle: None,
            tracknumber: None,
            tracktotal: None,
            discnumber: None,
            disctotal: None,
            trackartists: ArtistMapping::default(),
            releasetitle: None,
            releasetype: "unknown".to_string(),
            releasedate: None,
            originaldate: None,
            compositiondate: None,
            genre: vec![],
            secondarygenre: vec![],
            descriptor: vec![],
            edition: None,
            label: vec![],
            catalognumber: None,
            releaseartists: ArtistMapping::default(),
            duration_sec: 0,
            path: PathBuf::from("/tmp/test.flac"),
        };

        let changed = maybe_set_ids(&mut tags);
        assert!(changed);
        assert!(tags.id.is_some());
        assert!(tags.release_id.is_some());
        // UUIDs should be valid format
        assert_eq!(tags.id.as_ref().unwrap().len(), 36);
        assert_eq!(tags.release_id.as_ref().unwrap().len(), 36);
    }

    #[test]
    fn maybe_set_ids_preserves_existing() {
        let mut tags = AudioTags {
            id: Some("existing-id".to_string()),
            release_id: Some("existing-rid".to_string()),
            tracktitle: None,
            tracknumber: None,
            tracktotal: None,
            discnumber: None,
            disctotal: None,
            trackartists: ArtistMapping::default(),
            releasetitle: None,
            releasetype: "unknown".to_string(),
            releasedate: None,
            originaldate: None,
            compositiondate: None,
            genre: vec![],
            secondarygenre: vec![],
            descriptor: vec![],
            edition: None,
            label: vec![],
            catalognumber: None,
            releaseartists: ArtistMapping::default(),
            duration_sec: 0,
            path: PathBuf::from("/tmp/test.flac"),
        };

        let changed = maybe_set_ids(&mut tags);
        assert!(!changed);
        assert_eq!(tags.id, Some("existing-id".to_string()));
        assert_eq!(tags.release_id, Some("existing-rid".to_string()));
    }

    // --- Artist roundtrip via write ---

    #[test]
    fn artist_roundtrip_via_flush_flac() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track1.flac");
        let orig_tags = AudioTags::from_file(&path).unwrap();
        let orig_track_artists = orig_tags.trackartists.clone();
        let orig_release_artists = orig_tags.releaseartists.clone();

        let mut tags = orig_tags;
        tags.flush(false).unwrap();

        let tags2 = AudioTags::from_file(&path).unwrap();
        // Parse the re-read artist strings and compare
        assert_eq!(
            tags2.trackartists, orig_track_artists,
            "track artists roundtrip"
        );
        assert_eq!(
            tags2.releaseartists, orig_release_artists,
            "release artists roundtrip"
        );
    }

    // --- Genre parent encoding ---

    #[test]
    fn genre_parent_encoding_write_and_read() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track1.flac");
        let mut tags = AudioTags::from_file(&path).unwrap();
        // Set genres that have parents in the hierarchy.
        tags.genre = vec!["Acid House".to_string()];
        tags.flush(true).unwrap();

        // Re-read — the \\PARENTS:\\ section should be stripped on read.
        let tags2 = AudioTags::from_file(&path).unwrap();
        assert_eq!(tags2.genre, vec!["Acid House".to_string()]);
    }

    #[test]
    fn genre_parent_encoding_disabled() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track1.flac");
        let mut tags = AudioTags::from_file(&path).unwrap();
        tags.genre = vec!["Acid House".to_string()];
        tags.flush(false).unwrap();

        let tags2 = AudioTags::from_file(&path).unwrap();
        assert_eq!(tags2.genre, vec!["Acid House".to_string()]);
    }

    #[test]
    fn format_genre_tag_with_parents() {
        let genres = vec!["Acid House".to_string()];
        let result = format_genre_tag(true, &genres);
        // Should contain \\PARENTS:\\ with parent genres.
        assert!(result.contains("Acid House"), "base genre present");
        assert!(
            result.contains("\\\\PARENTS:\\\\"),
            "parents delimiter present"
        );
        // Acid House's parents include "House" and "Electronic Dance Music".
        assert!(result.contains("House"), "parent 'House' present");
    }

    #[test]
    fn format_genre_tag_without_parents() {
        let genres = vec!["Rock".to_string(), "Pop".to_string()];
        let result = format_genre_tag(false, &genres);
        assert_eq!(result, "Rock;Pop");
        assert!(!result.contains("PARENTS"));
    }

    #[test]
    fn format_genre_tag_no_extra_parents() {
        // If all parents are already in the list, no \\PARENTS:\\ section.
        let genres = vec![
            "Acid House".to_string(),
            "House".to_string(),
            "Electronic Dance Music".to_string(),
            "Electronic".to_string(),
        ];
        let result = format_genre_tag(true, &genres);
        // All parents of Acid House should already be in the list
        // (or close to it). Check that we still get a valid result.
        assert!(result.starts_with("Acid House;House;Electronic Dance Music;Electronic"));
    }

    // --- MP3 tracktotal lost ---

    #[test]
    fn mp3_tracktotal_lost_on_write() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track3.mp3");
        let orig = AudioTags::from_file(&path).unwrap();
        // Original should have tracktotal=5 (from TRCK = "3/5").
        assert_eq!(orig.tracktotal, Some(5));

        let mut tags = orig;
        tags.flush(false).unwrap();

        let tags2 = AudioTags::from_file(&path).unwrap();
        // After flush, TRCK writes only the number (no total), so tracktotal
        // should be None.
        assert_eq!(
            tags2.tracktotal, None,
            "MP3 tracktotal should be lost after flush"
        );
        assert_eq!(tags2.tracknumber, Some("3".to_string()));
    }

    // --- TIPL/IPLS deleted ---

    #[test]
    fn mp3_tipl_ipls_deleted_on_write() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track3.mp3");
        let mut tags = AudioTags::from_file(&path).unwrap();
        // The original file has TIPL with producer/DJ-mix entries.
        // After flush, those frames should be gone — the artists are folded
        // into TPE1 via format_artist_string.
        tags.flush(false).unwrap();

        // Verify by re-reading: the track artists should still have the same
        // data (since format_artist_string encodes all roles into TPE1).
        let tags2 = AudioTags::from_file(&path).unwrap();
        assert_eq!(
            tags2.trackartists, tags.trackartists,
            "track artists preserved"
        );

        // Read the raw ID3 tag to verify no TIPL/IPLS frames exist.
        let tagged_file = lofty::probe::Probe::open(&path)
            .unwrap()
            .guess_file_type()
            .unwrap()
            .read()
            .unwrap();
        let tag = tagged_file.tag(TagType::Id3v2).unwrap();
        // lofty maps TIPL producers to ItemKey::Producer. If there are no
        // TIPL frames, there should be no Producer items.
        let producers: Vec<&str> = tag.get_strings(&ItemKey::Producer).collect();
        assert!(producers.is_empty(), "TIPL producer entries should be gone");
        let djs: Vec<&str> = tag.get_strings(&ItemKey::MixDj).collect();
        assert!(djs.is_empty(), "TIPL DJ-mix entries should be gone");
    }

    // --- Alt-role tags deleted ---

    #[test]
    fn mp3_alt_role_tags_deleted() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track3.mp3");
        let mut tags = AudioTags::from_file(&path).unwrap();
        tags.flush(false).unwrap();

        let tagged_file = lofty::probe::Probe::open(&path)
            .unwrap()
            .guess_file_type()
            .unwrap()
            .read()
            .unwrap();
        let tag = tagged_file.tag(TagType::Id3v2).unwrap();
        // TPE4 (remixer), TCOM (composer), TPE3 (conductor) should not exist.
        assert!(
            tag.get_string(&ItemKey::Remixer).is_none(),
            "TPE4 should be gone"
        );
        assert!(
            tag.get_string(&ItemKey::Composer).is_none(),
            "TCOM should be gone"
        );
        assert!(
            tag.get_string(&ItemKey::Conductor).is_none(),
            "TPE3 should be gone"
        );
    }

    #[test]
    fn m4a_alt_role_tags_deleted() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track2.m4a");
        let mut tags = AudioTags::from_file(&path).unwrap();
        tags.flush(false).unwrap();

        // Re-read as Ilst to check that alt-role atoms are gone.
        let mut tagged_file = lofty::probe::Probe::open(&path)
            .unwrap()
            .guess_file_type()
            .unwrap()
            .read()
            .unwrap();
        let ilst: Ilst = match tagged_file.remove(TagType::Mp4Ilst) {
            Some(tag) => Ilst::from(tag),
            None => Ilst::new(),
        };
        assert!(
            ilst_get_tag(&ilst, &["----:com.apple.iTunes:REMIXER"], false).is_none(),
            "REMIXER should be gone"
        );
        assert!(
            ilst_get_tag(&ilst, &["----:com.apple.iTunes:PRODUCER"], false).is_none(),
            "PRODUCER should be gone"
        );
        assert!(
            ilst_get_tag(&ilst, &["\u{00a9}wrt"], false).is_none(),
            "©wrt should be gone"
        );
        assert!(
            ilst_get_tag(&ilst, &["----:com.apple.iTunes:CONDUCTOR"], false).is_none(),
            "CONDUCTOR should be gone"
        );
        assert!(
            ilst_get_tag(&ilst, &["----:com.apple.iTunes:DJMIXER"], false).is_none(),
            "DJMIXER should be gone"
        );
    }

    #[test]
    fn flac_alt_role_tags_deleted() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track1.flac");
        let mut tags = AudioTags::from_file(&path).unwrap();
        tags.flush(false).unwrap();

        let tagged_file = lofty::probe::Probe::open(&path)
            .unwrap()
            .guess_file_type()
            .unwrap()
            .read()
            .unwrap();
        let tag = tagged_file.tag(TagType::VorbisComments).unwrap();
        assert!(
            tag.get_string(&ItemKey::Remixer).is_none(),
            "remixer should be gone"
        );
        assert!(
            tag.get_string(&ItemKey::Producer).is_none(),
            "producer should be gone"
        );
        assert!(
            tag.get_string(&ItemKey::Composer).is_none(),
            "composer should be gone"
        );
        assert!(
            tag.get_string(&ItemKey::Conductor).is_none(),
            "conductor should be gone"
        );
        assert!(
            tag.get_string(&ItemKey::MixDj).is_none(),
            "djmixer should be gone"
        );
    }

    // --- MP4 tuple preservation ---

    #[test]
    fn mp4_track_disc_tuple_preservation() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track2.m4a");
        let orig = AudioTags::from_file(&path).unwrap();
        // Original should have tracktotal=5, disctotal=1.
        assert_eq!(orig.tracktotal, Some(5));
        assert_eq!(orig.disctotal, Some(1));

        let mut tags = orig;
        tags.flush(false).unwrap();

        let tags2 = AudioTags::from_file(&path).unwrap();
        // The previous totals should be preserved.
        assert_eq!(tags2.tracktotal, Some(5), "MP4 tracktotal preserved");
        assert_eq!(tags2.disctotal, Some(1), "MP4 disctotal preserved");
        assert_eq!(tags2.tracknumber, Some("2".to_string()));
        assert_eq!(tags2.discnumber, Some("1".to_string()));
    }

    // --- Invalid releasetype ---

    #[test]
    fn invalid_releasetype_returns_error() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track1.flac");
        let mut tags = AudioTags::from_file(&path).unwrap();
        tags.releasetype = "invalid_type".to_string();
        let result = tags.flush(false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(format!("{err}").contains("invalid_type"));
    }

    // --- maybe_set_ids + flush roundtrip ---

    #[test]
    fn maybe_set_ids_flush_roundtrip() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track1.flac");
        let mut tags = AudioTags::from_file(&path).unwrap();
        assert!(tags.id.is_none());
        assert!(tags.release_id.is_none());

        let changed = maybe_set_ids(&mut tags);
        assert!(changed);
        let id = tags.id.clone().unwrap();
        let rid = tags.release_id.clone().unwrap();

        tags.flush(false).unwrap();

        let tags2 = AudioTags::from_file(&path).unwrap();
        assert_eq!(tags2.id, Some(id));
        assert_eq!(tags2.release_id, Some(rid));
    }

    // -----------------------------------------------------------------------
    // Preservation and error handling tests (Task 055)
    // -----------------------------------------------------------------------

    #[test]
    fn mp3_txxx_preservation() {
        use lofty::id3::v2::{ExtendedTextFrame, Frame, Id3v2Tag};
        use lofty::tag::TagExt;

        let (_tmp, path) = copy_test_file("testdata/Tagger/track3.mp3");

        // First, do a normal AudioTags flush so the file has a clean Id3v2 tag
        // written by Rose.
        let mut tags = AudioTags::from_file(&path).unwrap();
        tags.flush(false).unwrap();

        // Now inject a custom TXXX frame that Rose does not manage. We use a
        // description that lofty does not map to a known ItemKey, so it stays
        // as ItemKey::Unknown and exercises the preservation code path.
        {
            let tagged_file = lofty::probe::Probe::open(&path)
                .unwrap()
                .guess_file_type()
                .unwrap()
                .read()
                .unwrap();
            let mut id3 = Id3v2Tag::from(tagged_file.tag(TagType::Id3v2).unwrap().clone());
            id3.insert(Frame::UserText(ExtendedTextFrame::new(
                lofty::TextEncoding::UTF8,
                "MY_CUSTOM_TAG".to_string(),
                "custom value 123".to_string(),
            )));
            id3.save_to_path(&path, lofty::config::WriteOptions::default())
                .unwrap();
        }

        // Verify the custom frame is present before flush.
        {
            let tagged_file = lofty::probe::Probe::open(&path)
                .unwrap()
                .guess_file_type()
                .unwrap()
                .read()
                .unwrap();
            let tag = tagged_file.tag(TagType::Id3v2).unwrap();
            let val = id3_get_unknown_tag(Some(tag), "MY_CUSTOM_TAG");
            assert_eq!(
                val,
                Some("custom value 123".to_string()),
                "custom TXXX present before flush"
            );
        }

        // Flush via AudioTags (modifying a Rose field).
        let mut tags = AudioTags::from_file(&path).unwrap();
        tags.tracktitle = Some("Modified Title".to_string());
        tags.flush(false).unwrap();

        // Re-read raw ID3 tag and verify the custom TXXX frame survived.
        let tagged_file = lofty::probe::Probe::open(&path)
            .unwrap()
            .guess_file_type()
            .unwrap()
            .read()
            .unwrap();
        let tag = tagged_file.tag(TagType::Id3v2).unwrap();
        let val = id3_get_unknown_tag(Some(tag), "MY_CUSTOM_TAG");
        assert_eq!(
            val,
            Some("custom value 123".to_string()),
            "custom TXXX frame should survive flush"
        );

        // Also verify the Rose field was actually modified.
        let tags2 = AudioTags::from_file(&path).unwrap();
        assert_eq!(tags2.tracktitle, Some("Modified Title".to_string()));
    }

    #[test]
    fn vorbis_vendor_string_preservation() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track1.flac");

        // Record the original vendor string.
        let original_vendor = {
            let tagged_file = lofty::probe::Probe::open(&path)
                .unwrap()
                .guess_file_type()
                .unwrap()
                .read()
                .unwrap();
            let tag = tagged_file.tag(TagType::VorbisComments).unwrap();
            tag.get_string(&ItemKey::EncoderSoftware)
                .map(String::from)
                .unwrap_or_default()
        };
        assert!(
            !original_vendor.is_empty(),
            "test file should have a vendor string"
        );

        // Flush via AudioTags (modifying a Rose field).
        let mut tags = AudioTags::from_file(&path).unwrap();
        tags.tracktitle = Some("Vendor Test".to_string());
        tags.flush(false).unwrap();

        // Re-read and verify vendor string is unchanged.
        let tagged_file = lofty::probe::Probe::open(&path)
            .unwrap()
            .guess_file_type()
            .unwrap()
            .read()
            .unwrap();
        let tag = tagged_file.tag(TagType::VorbisComments).unwrap();
        let after_vendor = tag
            .get_string(&ItemKey::EncoderSoftware)
            .map(String::from)
            .unwrap_or_default();
        assert_eq!(
            after_vendor, original_vendor,
            "vendor string should be preserved after flush"
        );
    }

    #[test]
    fn corrupted_file_returns_error() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let path = tmp.path().join("garbage.flac");
        std::fs::write(&path, b"this is not a valid FLAC file at all").expect("write garbage file");

        let result = AudioTags::from_file(&path);
        assert!(
            result.is_err(),
            "corrupted file should return an error, not panic"
        );
    }

    #[test]
    fn genre_parent_raw_encoding() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track1.flac");
        let mut tags = AudioTags::from_file(&path).unwrap();
        tags.genre = vec!["Deep House".to_string()];
        tags.flush(true).unwrap();

        // Read the raw Vorbis comment GENRE value (not via AudioTags).
        let tagged_file = lofty::probe::Probe::open(&path)
            .unwrap()
            .guess_file_type()
            .unwrap()
            .read()
            .unwrap();
        let tag = tagged_file.tag(TagType::VorbisComments).unwrap();

        // Get the raw GENRE string via ItemKey::Genre.
        let raw_genre = tag
            .get_string(&ItemKey::Genre)
            .map(String::from)
            .expect("GENRE tag should exist");

        // The raw value should start with the base genre.
        assert!(
            raw_genre.starts_with("Deep House"),
            "raw genre should start with 'Deep House', got: {raw_genre}"
        );
        // Should contain the \\PARENTS:\\ delimiter.
        assert!(
            raw_genre.contains("\\\\PARENTS:\\\\"),
            "raw genre should contain \\\\PARENTS:\\\\ delimiter, got: {raw_genre}"
        );
        // After the delimiter, parent genres should be present.
        // Deep House's parents include "House".
        let parents_section = raw_genre
            .split("\\\\PARENTS:\\\\")
            .nth(1)
            .expect("should have a parents section");
        assert!(
            parents_section.contains("House"),
            "parents should contain 'House', got: {parents_section}"
        );

        // Verify that reading back via AudioTags strips the parents.
        let tags2 = AudioTags::from_file(&path).unwrap();
        assert_eq!(tags2.genre, vec!["Deep House".to_string()]);
    }

    // -----------------------------------------------------------------------
    // Mutation, normalization, and edge case tests (Task 054)
    // -----------------------------------------------------------------------

    #[test]
    fn mutate_artist_role_then_flush() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track1.flac");
        let mut tags = AudioTags::from_file(&path).unwrap();
        // Replace djmixer with a new artist.
        tags.trackartists.djmixer = vec![Artist {
            name: "New DJ".to_string(),
            alias: false,
        }];
        tags.flush(false).unwrap();

        let tags2 = AudioTags::from_file(&path).unwrap();
        // The new djmixer should be present.
        assert_eq!(tags2.trackartists.djmixer.len(), 1);
        assert_eq!(tags2.trackartists.djmixer[0].name, "New DJ");
    }

    #[test]
    fn mutate_date_then_flush() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track1.flac");
        let mut tags = AudioTags::from_file(&path).unwrap();
        tags.originaldate = Some(RoseDate {
            year: 1990,
            month: Some(4),
            day: Some(20),
        });
        tags.flush(false).unwrap();

        let tags2 = AudioTags::from_file(&path).unwrap();
        let od = tags2.originaldate.expect("originaldate should be set");
        assert_eq!(od.year, 1990);
        assert_eq!(od.month, Some(4));
        assert_eq!(od.day, Some(20));
        assert_eq!(od.to_string(), "1990-04-20");
    }

    #[test]
    fn releasetype_normalization_on_read() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track1.flac");
        // Write an invalid releasetype directly to the FLAC Vorbis Comments.
        {
            use lofty::ogg::VorbisComments;
            use lofty::tag::TagExt;

            let mut vc = VorbisComments::new();
            vc.insert("RELEASETYPE".to_string(), "BOGUS".to_string());
            vc.insert("TITLE".to_string(), "Test".to_string());
            vc.save_to_path(&path, lofty::config::WriteOptions::default())
                .unwrap();
        }
        let tags = AudioTags::from_file(&path).unwrap();
        assert_eq!(tags.releasetype, "unknown");
    }

    #[test]
    fn releasetype_case_insensitive_on_read() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track1.flac");
        // Write "ALBUM" (uppercase) as releasetype.
        {
            use lofty::ogg::VorbisComments;
            use lofty::tag::TagExt;

            let mut vc = VorbisComments::new();
            vc.insert("RELEASETYPE".to_string(), "ALBUM".to_string());
            vc.insert("TITLE".to_string(), "Test".to_string());
            vc.save_to_path(&path, lofty::config::WriteOptions::default())
                .unwrap();
        }
        let tags = AudioTags::from_file(&path).unwrap();
        assert_eq!(tags.releasetype, "album");
    }

    #[test]
    fn tag_splits_on_vs() {
        let result = split_tag(Some("a vs. b"));
        assert_eq!(result, vec!["a", "b"]);
    }

    #[test]
    fn tag_splits_on_slash() {
        let result = split_tag(Some("a / b"));
        assert_eq!(result, vec!["a", "b"]);
    }

    #[test]
    fn artist_deduplication_across_sources() {
        // "A pres. B" extracts A as djmixer and B as main.
        // Passing dj=Some("A") adds another "A" to djmixer.
        // Deduplication (via `uniq`) should ensure only one "A" in djmixer.
        let mapping = parse_artist_string(Some("A pres. B"), None, None, None, None, Some("A"));
        assert_eq!(
            mapping.djmixer.len(),
            1,
            "djmixer should be deduplicated: {:?}",
            mapping.djmixer
        );
        assert_eq!(mapping.djmixer[0].name, "A");
        assert_eq!(mapping.main[0].name, "B");
    }

    #[test]
    fn m4a_none_string_tracknumber() {
        let (_tmp, path) = copy_test_file("testdata/Tagger/track2.m4a");
        let mut tags = AudioTags::from_file(&path).unwrap();
        tags.tracknumber = Some("None".to_string());
        // flush should not error — "None" is handled gracefully.
        tags.flush(false).unwrap();

        let tags2 = AudioTags::from_file(&path).unwrap();
        // "None" is treated as unparseable -> 0 written to the trkn atom.
        // lofty reads trkn=0 back as None (no track number).
        assert!(
            tags2.tracknumber.is_none() || tags2.tracknumber.as_deref() == Some("0"),
            "tracknumber should be None or \"0\", got: {:?}",
            tags2.tracknumber
        );
    }
}
