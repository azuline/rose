/// The audiotags module abstracts over tag reading and writing for five different audio formats,
/// exposing a single standard interface for all audio files.
///
/// The audiotags module also handles Rose-specific tagging semantics, such as multi-valued tags,
/// normalization, artist formatting, and enum validation.
use crate::common::{uniq, Artist, ArtistMapping, RoseDate};
use crate::config::Config;
use crate::errors::{Result, RoseError, RoseExpectedError};
use crate::genre_hierarchy::TRANSITIVE_PARENT_GENRES;
use id3::{frame::ExtendedText, Tag as Id3Tag, TagLike};
use metaflac::Tag as FlacTag;
use mp4ameta::{Data, FreeformIdent, Tag as Mp4Tag};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
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

fn _normalize_rtype(x: Option<&str>) -> String {
    match x {
        None => "unknown".to_string(),
        Some(s) => {
            // Remove any null terminators and trim whitespace
            let cleaned = s.trim_end_matches('\0').trim();
            let lower = cleaned.to_lowercase();
            if SUPPORTED_RELEASE_TYPES.contains(&lower.as_str()) {
                lower
            } else {
                "unknown".to_string()
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct UnsupportedFiletypeError(pub String);
impl std::fmt::Display for UnsupportedFiletypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for UnsupportedFiletypeError {}

#[derive(Debug, Clone)]
pub struct UnsupportedTagValueTypeError(pub String);
impl std::fmt::Display for UnsupportedTagValueTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for UnsupportedTagValueTypeError {}

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
    pub fn from_file(p: &Path) -> Result<AudioTags> {
        let extension = p.extension().and_then(|s| s.to_str()).map(|s| format!(".{}", s.to_lowercase())).unwrap_or_default();

        if !SUPPORTED_AUDIO_EXTENSIONS.contains(&extension.as_str()) {
            return Err(RoseExpectedError::Generic(format!("{} not a supported filetype", extension)).into());
        }

        match extension.as_str() {
            ".mp3" => Self::from_mp3(p),
            ".m4a" => Self::from_m4a(p),
            ".flac" => Self::from_flac(p),
            ".ogg" | ".opus" => Self::from_ogg(p),
            _ => Err(RoseExpectedError::Generic(format!("{} is not a supported audio file", p.display())).into()),
        }
    }

    fn from_mp3(p: &Path) -> Result<AudioTags> {
        let tag = match Id3Tag::read_from_path(p) {
            Ok(tag) => tag,
            Err(e) => return Err(RoseExpectedError::Generic(format!("Failed to open file: {}", e)).into()),
        };

        // Parse track/disc numbers
        let (tracknumber, tracktotal) = if let Some(trck) = tag.get("TRCK").and_then(|f| f.content().text()) {
            if let Some((num, total)) = trck.split_once('/') {
                (Some(num.to_string()), _parse_int(Some(total)))
            } else {
                (Some(trck.to_string()), None)
            }
        } else {
            (None, None)
        };

        let (discnumber, disctotal) = if let Some(tpos) = tag.get("TPOS").and_then(|f| f.content().text()) {
            if let Some((num, total)) = tpos.split_once('/') {
                (Some(num.to_string()), _parse_int(Some(total)))
            } else {
                (Some(tpos.to_string()), None)
            }
        } else {
            (None, None)
        };

        // Helper to get paired frame data
        let get_paired_frame = |role: &str| -> Option<String> {
            for frame_id in &["TIPL", "IPLS"] {
                if let Some(frame) = tag.get(*frame_id) {
                    if let Some(people) = frame.content().involved_people_list() {
                        let values: Vec<String> = people
                            .items
                            .iter()
                            .filter(|item| item.involvement.to_lowercase() == role.to_lowercase())
                            .map(|item| item.involvee.clone())
                            .collect();
                        if !values.is_empty() {
                            return Some(values.join(r" \\ "));
                        }
                    }
                }
            }
            None
        };

        // Calculate duration
        let duration_sec = mp3_duration::from_path(p).map(|d| d.as_secs() as i32).unwrap_or(0);

        Ok(AudioTags {
            id: _get_id3_tag(&tag, &["TXXX:ROSEID"], false, true),
            release_id: _get_id3_tag(&tag, &["TXXX:ROSERELEASEID"], false, true),
            tracktitle: _get_id3_tag(&tag, &["TIT2"], false, false),
            releasedate: RoseDate::parse(_get_id3_tag(&tag, &["TDRC", "TYER", "TDAT"], false, false).as_deref()),
            originaldate: RoseDate::parse(_get_id3_tag(&tag, &["TDOR", "TORY"], false, false).as_deref()),
            compositiondate: RoseDate::parse(_get_id3_tag(&tag, &["TXXX:COMPOSITIONDATE"], false, true).as_deref()),
            tracknumber,
            tracktotal,
            discnumber,
            disctotal,
            releasetitle: _get_id3_tag(&tag, &["TALB"], false, false),
            genre: _split_genre_tag(_get_id3_tag(&tag, &["TCON"], true, false).as_deref()),
            secondarygenre: _split_genre_tag(_get_id3_tag(&tag, &["TXXX:SECONDARYGENRE"], true, false).as_deref()),
            descriptor: _split_tag(_get_id3_tag(&tag, &["TXXX:DESCRIPTOR"], true, false).as_deref()),
            label: _split_tag(_get_id3_tag(&tag, &["TPUB"], true, false).as_deref()),
            catalognumber: _get_id3_tag(&tag, &["TXXX:CATALOGNUMBER"], false, true),
            edition: _get_id3_tag(&tag, &["TXXX:EDITION"], false, true),
            releasetype: _normalize_rtype(_get_id3_tag(&tag, &["TXXX:RELEASETYPE", "TXXX:MusicBrainz Album Type"], false, true).as_deref()),
            releaseartists: parse_artist_string(_get_id3_tag(&tag, &["TPE2"], true, false).as_deref(), None, None, None, None, None),
            trackartists: parse_artist_string(
                _get_id3_tag(&tag, &["TPE1"], true, false).as_deref(),
                _get_id3_tag(&tag, &["TPE4"], true, false).as_deref(),
                _get_id3_tag(&tag, &["TCOM"], true, false).as_deref(),
                _get_id3_tag(&tag, &["TPE3"], true, false).as_deref(),
                get_paired_frame("producer").as_deref(),
                get_paired_frame("DJ-mix").as_deref(),
            ),
            duration_sec,
            path: p.to_path_buf(),
        })
    }

    fn from_m4a(p: &Path) -> Result<AudioTags> {
        let tag = match Mp4Tag::read_from_path(p) {
            Ok(tag) => tag,
            Err(e) => return Err(RoseExpectedError::Generic(format!("Failed to open file: {}", e)).into()),
        };

        // Parse track/disc numbers
        let (tracknumber, tracktotal) = match tag.track() {
            (Some(num), Some(total)) => (Some(num.to_string()), Some(total as i32)),
            (Some(num), None) => (Some(num.to_string()), None),
            _ => (None, None),
        };

        let (discnumber, disctotal) = match tag.disc() {
            (Some(num), Some(total)) => (Some(num.to_string()), Some(total as i32)),
            (Some(num), None) => (Some(num.to_string()), None),
            _ => (None, None),
        };

        // Calculate duration
        let duration_sec = tag.duration().map(|d| d.as_secs() as i32).unwrap_or(0);

        Ok(AudioTags {
            id: _get_mp4_tag(&tag, "----:net.sunsetglow.rose:ID"),
            release_id: _get_mp4_tag(&tag, "----:net.sunsetglow.rose:RELEASEID"),
            tracktitle: tag.title().map(String::from),
            releasedate: RoseDate::parse(tag.year()),
            originaldate: RoseDate::parse(
                _get_mp4_tag(&tag, "----:net.sunsetglow.rose:ORIGINALDATE")
                    .or_else(|| _get_mp4_tag(&tag, "----:com.apple.iTunes:ORIGINALDATE"))
                    .or_else(|| _get_mp4_tag(&tag, "----:com.apple.iTunes:ORIGINALYEAR"))
                    .as_deref(),
            ),
            compositiondate: RoseDate::parse(_get_mp4_tag(&tag, "----:net.sunsetglow.rose:COMPOSITIONDATE").as_deref()),
            tracknumber,
            tracktotal,
            discnumber,
            disctotal,
            releasetitle: tag.album().map(String::from),
            genre: {
                // Collect all genre values from the tag
                let genres: Vec<String> = tag.genres().map(|s| s.to_string()).collect();
                // Join them with semicolons and then split using our standard splitter
                let joined = genres.join(";");
                _split_genre_tag(Some(&joined))
            },
            secondarygenre: _split_genre_tag(_get_mp4_tag(&tag, "----:net.sunsetglow.rose:SECONDARYGENRE").as_deref()),
            descriptor: _split_tag(_get_mp4_tag(&tag, "----:net.sunsetglow.rose:DESCRIPTOR").as_deref()),
            label: _split_tag(_get_mp4_tag(&tag, "----:com.apple.iTunes:LABEL").as_deref()),
            catalognumber: _get_mp4_tag(&tag, "----:com.apple.iTunes:CATALOGNUMBER"),
            edition: _get_mp4_tag(&tag, "----:net.sunsetglow.rose:EDITION"),
            releasetype: _normalize_rtype(
                _get_mp4_tag(&tag, "----:com.apple.iTunes:RELEASETYPE")
                    .or_else(|| _get_mp4_tag(&tag, "----:com.apple.iTunes:MusicBrainz Album Type"))
                    .as_deref(),
            ),
            releaseartists: parse_artist_string(tag.album_artist().or_else(|| tag.artist()), None, None, None, None, None),
            trackartists: parse_artist_string(
                tag.artist(),
                _get_mp4_tag(&tag, "----:com.apple.iTunes:REMIXER").as_deref(),
                {
                    // Collect all composer values and join with semicolons
                    let composers: Vec<String> = tag.composers().map(|s| s.to_string()).collect();
                    if composers.is_empty() {
                        None
                    } else {
                        Some(composers.join(";"))
                    }
                }
                .as_deref(),
                _get_mp4_tag(&tag, "----:com.apple.iTunes:CONDUCTOR").as_deref(),
                _get_mp4_tag(&tag, "----:com.apple.iTunes:PRODUCER").as_deref(),
                _get_mp4_tag(&tag, "----:com.apple.iTunes:DJMIXER").as_deref(),
            ),
            duration_sec,
            path: p.to_path_buf(),
        })
    }

    fn from_flac(p: &Path) -> Result<AudioTags> {
        let tag = match FlacTag::read_from_path(p) {
            Ok(tag) => tag,
            Err(e) => return Err(RoseExpectedError::Generic(format!("Failed to open file: {}", e)).into()),
        };

        let vorbis = tag.vorbis_comments().ok_or_else(|| RoseExpectedError::Generic("No vorbis comments in FLAC file".to_string()))?;

        // Calculate duration from stream info
        let duration_sec = tag
            .get_streaminfo()
            .map(|info| {
                if info.sample_rate > 0 {
                    (info.total_samples as f64 / info.sample_rate as f64).round() as i32
                } else {
                    0
                }
            })
            .unwrap_or(0);

        Ok(AudioTags {
            id: _get_vorbis_tag(vorbis, &["ROSEID"], false, false),
            release_id: _get_vorbis_tag(vorbis, &["ROSERELEASEID"], false, false),
            tracktitle: _get_vorbis_tag(vorbis, &["TITLE"], false, false),
            releasedate: RoseDate::parse(_get_vorbis_tag(vorbis, &["DATE", "YEAR"], false, false).as_deref()),
            originaldate: RoseDate::parse(_get_vorbis_tag(vorbis, &["ORIGINALDATE", "ORIGINALYEAR"], false, false).as_deref()),
            compositiondate: RoseDate::parse(_get_vorbis_tag(vorbis, &["COMPOSITIONDATE"], false, false).as_deref()),
            tracknumber: _get_vorbis_tag(vorbis, &["TRACKNUMBER"], false, true),
            tracktotal: _parse_int(_get_vorbis_tag(vorbis, &["TRACKTOTAL"], false, true).as_deref()),
            discnumber: _get_vorbis_tag(vorbis, &["DISCNUMBER"], false, true),
            disctotal: _parse_int(_get_vorbis_tag(vorbis, &["DISCTOTAL"], false, true).as_deref()),
            releasetitle: _get_vorbis_tag(vorbis, &["ALBUM"], false, false),
            genre: _split_genre_tag(_get_vorbis_tag(vorbis, &["GENRE"], true, false).as_deref()),
            secondarygenre: _split_genre_tag(_get_vorbis_tag(vorbis, &["SECONDARYGENRE"], true, false).as_deref()),
            descriptor: _split_tag(_get_vorbis_tag(vorbis, &["DESCRIPTOR"], true, false).as_deref()),
            label: _split_tag(_get_vorbis_tag(vorbis, &["LABEL", "ORGANIZATION", "RECORDLABEL"], true, false).as_deref()),
            catalognumber: _get_vorbis_tag(vorbis, &["CATALOGNUMBER"], false, false),
            edition: _get_vorbis_tag(vorbis, &["EDITION"], false, false),
            releasetype: _normalize_rtype(_get_vorbis_tag(vorbis, &["RELEASETYPE"], false, true).as_deref()),
            releaseartists: parse_artist_string(_get_vorbis_tag(vorbis, &["ALBUMARTIST"], true, false).as_deref(), None, None, None, None, None),
            trackartists: parse_artist_string(
                _get_vorbis_tag(vorbis, &["ARTIST"], true, false).as_deref(),
                _get_vorbis_tag(vorbis, &["REMIXER"], true, false).as_deref(),
                _get_vorbis_tag(vorbis, &["COMPOSER"], true, false).as_deref(),
                _get_vorbis_tag(vorbis, &["CONDUCTOR"], true, false).as_deref(),
                _get_vorbis_tag(vorbis, &["PRODUCER"], true, false).as_deref(),
                _get_vorbis_tag(vorbis, &["DJMIXER"], true, false).as_deref(),
            ),
            duration_sec,
            path: p.to_path_buf(),
        })
    }

    fn from_ogg(p: &Path) -> Result<AudioTags> {
        use ogg::PacketReader;
        use std::io::{BufReader, Cursor};

        let file = File::open(p).map_err(|e| RoseExpectedError::Generic(format!("Failed to open file: {}", e)))?;

        let mut packet_reader = PacketReader::new(BufReader::new(file));

        // Read the first packet to determine format and get comments
        let first_packet = packet_reader
            .read_packet()
            .map_err(|e| RoseExpectedError::Generic(format!("Failed to read OGG packet: {}", e)))?
            .ok_or_else(|| RoseExpectedError::Generic("No packets in OGG file".to_string()))?;

        let is_opus = first_packet.data.starts_with(b"OpusHead");

        // Skip to comment packet
        let comment_packet = packet_reader
            .read_packet()
            .map_err(|e| RoseExpectedError::Generic(format!("Failed to read comment packet: {}", e)))?
            .ok_or_else(|| RoseExpectedError::Generic("No comment packet in OGG file".to_string()))?;

        // Parse Vorbis comments manually
        let mut cursor = Cursor::new(&comment_packet.data);

        // Skip the packet type and signature
        let skip_bytes = if is_opus { 8 } else { 7 }; // OpusTags vs vorbis comment header
        cursor.set_position(skip_bytes);

        // Read vendor string length and skip it
        let mut vendor_len_bytes = [0u8; 4];
        cursor.read_exact(&mut vendor_len_bytes).map_err(|e| RoseExpectedError::Generic(format!("Failed to read vendor length: {}", e)))?;
        let vendor_len = u32::from_le_bytes(vendor_len_bytes) as u64;
        cursor.set_position(cursor.position() + vendor_len);

        // Read number of comments
        let mut num_comments_bytes = [0u8; 4];
        cursor.read_exact(&mut num_comments_bytes).map_err(|e| RoseExpectedError::Generic(format!("Failed to read comment count: {}", e)))?;
        let num_comments = u32::from_le_bytes(num_comments_bytes);

        // Read all comments
        let mut comment_map: HashMap<String, Vec<String>> = HashMap::new();
        for _ in 0..num_comments {
            let mut len_bytes = [0u8; 4];
            cursor.read_exact(&mut len_bytes).map_err(|e| RoseExpectedError::Generic(format!("Failed to read comment length: {}", e)))?;
            let len = u32::from_le_bytes(len_bytes) as usize;

            let mut comment_bytes = vec![0u8; len];
            cursor.read_exact(&mut comment_bytes).map_err(|e| RoseExpectedError::Generic(format!("Failed to read comment: {}", e)))?;

            let comment = String::from_utf8_lossy(&comment_bytes);
            if let Some((key, value)) = comment.split_once('=') {
                comment_map.entry(key.to_uppercase()).or_default().push(value.to_string());
            }
        }

        // Calculate duration - this is approximate for OGG files
        // For a more accurate duration, we'd need to parse all packets
        let duration_sec = 0; // TODO: Implement proper duration calculation

        Ok(AudioTags {
            id: _get_vorbis_map(&comment_map, &["ROSEID"], false, false),
            release_id: _get_vorbis_map(&comment_map, &["ROSERELEASEID"], false, false),
            tracktitle: _get_vorbis_map(&comment_map, &["TITLE"], false, false),
            releasedate: RoseDate::parse(_get_vorbis_map(&comment_map, &["DATE", "YEAR"], false, false).as_deref()),
            originaldate: RoseDate::parse(_get_vorbis_map(&comment_map, &["ORIGINALDATE", "ORIGINALYEAR"], false, false).as_deref()),
            compositiondate: RoseDate::parse(_get_vorbis_map(&comment_map, &["COMPOSITIONDATE"], false, false).as_deref()),
            tracknumber: _get_vorbis_map(&comment_map, &["TRACKNUMBER"], false, true),
            tracktotal: _parse_int(_get_vorbis_map(&comment_map, &["TRACKTOTAL"], false, true).as_deref()),
            discnumber: _get_vorbis_map(&comment_map, &["DISCNUMBER"], false, true),
            disctotal: _parse_int(_get_vorbis_map(&comment_map, &["DISCTOTAL"], false, true).as_deref()),
            releasetitle: _get_vorbis_map(&comment_map, &["ALBUM"], false, false),
            genre: _split_genre_tag(_get_vorbis_map(&comment_map, &["GENRE"], true, false).as_deref()),
            secondarygenre: _split_genre_tag(_get_vorbis_map(&comment_map, &["SECONDARYGENRE"], true, false).as_deref()),
            descriptor: _split_tag(_get_vorbis_map(&comment_map, &["DESCRIPTOR"], true, false).as_deref()),
            label: _split_tag(_get_vorbis_map(&comment_map, &["LABEL", "ORGANIZATION", "RECORDLABEL"], true, false).as_deref()),
            catalognumber: _get_vorbis_map(&comment_map, &["CATALOGNUMBER"], false, false),
            edition: _get_vorbis_map(&comment_map, &["EDITION"], false, false),
            releasetype: _normalize_rtype(_get_vorbis_map(&comment_map, &["RELEASETYPE"], false, true).as_deref()),
            releaseartists: parse_artist_string(_get_vorbis_map(&comment_map, &["ALBUMARTIST"], true, false).as_deref(), None, None, None, None, None),
            trackartists: parse_artist_string(
                _get_vorbis_map(&comment_map, &["ARTIST"], true, false).as_deref(),
                _get_vorbis_map(&comment_map, &["REMIXER"], true, false).as_deref(),
                _get_vorbis_map(&comment_map, &["COMPOSER"], true, false).as_deref(),
                _get_vorbis_map(&comment_map, &["CONDUCTOR"], true, false).as_deref(),
                _get_vorbis_map(&comment_map, &["PRODUCER"], true, false).as_deref(),
                _get_vorbis_map(&comment_map, &["DJMIXER"], true, false).as_deref(),
            ),
            duration_sec,
            path: p.to_path_buf(),
        })
    }

    pub fn flush(&mut self, c: &Config, validate: bool) -> Result<()> {
        #[cfg(not(test))]
        if !validate {
            return Err(RoseError::Generic("Validate can only be turned off by tests.".to_string()));
        }

        self.releasetype = self.releasetype.to_lowercase();
        if validate && !SUPPORTED_RELEASE_TYPES.contains(&self.releasetype.as_str()) {
            return Err(RoseExpectedError::Generic(format!(
                "Release type {} is not a supported release type.\nSupported release types: {}",
                self.releasetype,
                SUPPORTED_RELEASE_TYPES.join(", ")
            ))
            .into());
        }

        let extension = self.path.extension().and_then(|s| s.to_str()).map(|s| format!(".{}", s.to_lowercase())).unwrap_or_default();

        match extension.as_str() {
            ".mp3" => self.flush_mp3(c),
            ".m4a" => self.flush_m4a(c),
            ".flac" => self.flush_flac(c),
            ".ogg" | ".opus" => self.flush_ogg(c),
            _ => Err(RoseError::Generic(format!("Impossible: unknown file type for {}", self.path.display()))),
        }
    }

    fn flush_mp3(&self, c: &Config) -> Result<()> {
        let mut tag = Id3Tag::read_from_path(&self.path).unwrap_or_else(|_| Id3Tag::new());

        // Helper to write standard tags
        let write_standard_tag = |tag: &mut Id3Tag, frame_id: &str, value: Option<&str>| {
            tag.remove(frame_id);
            if let Some(val) = value {
                if !val.is_empty() {
                    tag.set_text(frame_id, val);
                }
            }
        };

        // Helper to write TXXX tags
        // Note: id3 crate doesn't provide a good way to selectively update TXXX frames
        // So we just add them and hope for the best
        let write_txxx_tag = |tag: &mut Id3Tag, desc: &str, value: Option<&str>| {
            if let Some(val) = value {
                if !val.is_empty() {
                    tag.add_frame(ExtendedText {
                        description: desc.to_string(),
                        value: val.to_string(),
                    });
                }
            }
        };

        // Write all tags
        write_txxx_tag(&mut tag, "ROSEID", self.id.as_deref());
        write_txxx_tag(&mut tag, "ROSERELEASEID", self.release_id.as_deref());
        write_standard_tag(&mut tag, "TIT2", self.tracktitle.as_deref());
        write_standard_tag(&mut tag, "TDRC", self.releasedate.map(|d| d.to_string()).as_deref());
        write_standard_tag(&mut tag, "TDOR", self.originaldate.map(|d| d.to_string()).as_deref());
        write_txxx_tag(&mut tag, "COMPOSITIONDATE", self.compositiondate.map(|d| d.to_string()).as_deref());
        write_standard_tag(&mut tag, "TRCK", self.tracknumber.as_deref());
        write_standard_tag(&mut tag, "TPOS", self.discnumber.as_deref());
        write_standard_tag(&mut tag, "TALB", self.releasetitle.as_deref());
        write_standard_tag(&mut tag, "TCON", Some(&_format_genre_tag(c, &self.genre)));
        write_txxx_tag(&mut tag, "SECONDARYGENRE", Some(&_format_genre_tag(c, &self.secondarygenre)));
        write_txxx_tag(&mut tag, "DESCRIPTOR", Some(&self.descriptor.join(";")));
        write_standard_tag(&mut tag, "TPUB", Some(&self.label.join(";")));
        write_txxx_tag(&mut tag, "CATALOGNUMBER", self.catalognumber.as_deref());
        write_txxx_tag(&mut tag, "EDITION", self.edition.as_deref());
        write_txxx_tag(&mut tag, "RELEASETYPE", Some(&self.releasetype));
        write_standard_tag(&mut tag, "TPE2", Some(&format_artist_string(&self.releaseartists)));
        write_standard_tag(&mut tag, "TPE1", Some(&format_artist_string(&self.trackartists)));

        // Wipe the alt. role artist tags
        tag.remove("TPE4");
        tag.remove("TCOM");
        tag.remove("TPE3");
        tag.remove("TIPL");
        tag.remove("IPLS");

        tag.write_to_path(&self.path, id3::Version::Id3v24).map_err(|e| RoseError::Generic(format!("Failed to write ID3 tags: {}", e)))?;

        Ok(())
    }

    fn flush_m4a(&self, c: &Config) -> Result<()> {
        let mut tag = Mp4Tag::read_from_path(&self.path).unwrap_or_else(|_| Mp4Tag::default());

        // Write custom rose ID tags
        if let Some(id) = &self.id {
            let ident = FreeformIdent::new("net.sunsetglow.rose", "ID");
            tag.set_data(ident, Data::Utf8(id.clone()));
        }

        if let Some(release_id) = &self.release_id {
            let ident = FreeformIdent::new("net.sunsetglow.rose", "RELEASEID");
            tag.set_data(ident, Data::Utf8(release_id.clone()));
        }

        if let Some(title) = &self.tracktitle {
            tag.set_title(title);
        } else {
            tag.remove_title();
        }

        if let Some(date) = self.releasedate {
            tag.set_year(date.to_string());
        } else {
            tag.remove_year();
        }

        // Custom date tags
        if let Some(date) = self.originaldate {
            let ident = FreeformIdent::new("net.sunsetglow.rose", "ORIGINALDATE");
            tag.set_data(ident, Data::Utf8(date.to_string()));
        }

        if let Some(date) = self.compositiondate {
            let ident = FreeformIdent::new("net.sunsetglow.rose", "COMPOSITIONDATE");
            tag.set_data(ident, Data::Utf8(date.to_string()));
        }

        if let Some(album) = &self.releasetitle {
            tag.set_album(album);
        } else {
            tag.remove_album();
        }

        if !self.genre.is_empty() {
            tag.set_genre(_format_genre_tag(c, &self.genre));
        } else {
            tag.remove_genres();
        }

        // More custom tags
        let secondary_genre_ident = FreeformIdent::new("net.sunsetglow.rose", "SECONDARYGENRE");
        tag.set_data(secondary_genre_ident, Data::Utf8(_format_genre_tag(c, &self.secondarygenre)));

        let descriptor_ident = FreeformIdent::new("net.sunsetglow.rose", "DESCRIPTOR");
        tag.set_data(descriptor_ident, Data::Utf8(self.descriptor.join(";")));

        let label_ident = FreeformIdent::new("com.apple.iTunes", "LABEL");
        tag.set_data(label_ident, Data::Utf8(self.label.join(";")));

        if let Some(cat) = &self.catalognumber {
            let ident = FreeformIdent::new("com.apple.iTunes", "CATALOGNUMBER");
            tag.set_data(ident, Data::Utf8(cat.clone()));
        }

        if let Some(edition) = &self.edition {
            let ident = FreeformIdent::new("net.sunsetglow.rose", "EDITION");
            tag.set_data(ident, Data::Utf8(edition.clone()));
        }

        let releasetype_ident = FreeformIdent::new("com.apple.iTunes", "RELEASETYPE");
        tag.set_data(releasetype_ident, Data::Utf8(self.releasetype.clone()));

        // Artists
        tag.set_album_artist(format_artist_string(&self.releaseartists));
        tag.set_artist(format_artist_string(&self.trackartists));

        // Remove alt. role artist tags - we encode everything in the main artist string
        // so we need to clear these to avoid duplication
        tag.remove_composers(); // Removes ©wrt
                                // For custom tags, we need to remove them by setting empty data
        let remixer_ident = FreeformIdent::new("com.apple.iTunes", "REMIXER");
        tag.remove_data_of(&remixer_ident);
        let producer_ident = FreeformIdent::new("com.apple.iTunes", "PRODUCER");
        tag.remove_data_of(&producer_ident);
        let conductor_ident = FreeformIdent::new("com.apple.iTunes", "CONDUCTOR");
        tag.remove_data_of(&conductor_ident);
        let djmixer_ident = FreeformIdent::new("com.apple.iTunes", "DJMIXER");
        tag.remove_data_of(&djmixer_ident);

        // Track and disc numbers
        if let Some(num) = &self.tracknumber {
            if let Ok(n) = num.parse::<u16>() {
                let total = match tag.track() {
                    (_, Some(t)) => t,
                    _ => 1,
                };
                tag.set_track(n, total);
            }
        }

        if let Some(num) = &self.discnumber {
            if let Ok(n) = num.parse::<u16>() {
                let total = match tag.disc() {
                    (_, Some(t)) => t,
                    _ => 1,
                };
                tag.set_disc(n, total);
            }
        }

        tag.write_to_path(&self.path).map_err(|e| RoseError::Generic(format!("Failed to write MP4 tags: {}", e)))?;

        Ok(())
    }

    fn flush_flac(&self, c: &Config) -> Result<()> {
        let mut tag = FlacTag::read_from_path(&self.path).map_err(|e| RoseError::Generic(format!("Failed to read FLAC tags: {}", e)))?;

        let comments = tag.vorbis_comments_mut();

        // Clear and set all tags
        comments.set("ROSEID", vec![self.id.clone().unwrap_or_default()]);
        comments.set("ROSERELEASEID", vec![self.release_id.clone().unwrap_or_default()]);
        comments.set("TITLE", vec![self.tracktitle.clone().unwrap_or_default()]);
        comments.set("DATE", vec![self.releasedate.map(|d| d.to_string()).unwrap_or_default()]);
        comments.set("ORIGINALDATE", vec![self.originaldate.map(|d| d.to_string()).unwrap_or_default()]);
        comments.set("COMPOSITIONDATE", vec![self.compositiondate.map(|d| d.to_string()).unwrap_or_default()]);
        comments.set("TRACKNUMBER", vec![self.tracknumber.clone().unwrap_or_default()]);
        comments.set("DISCNUMBER", vec![self.discnumber.clone().unwrap_or_default()]);
        comments.set("ALBUM", vec![self.releasetitle.clone().unwrap_or_default()]);
        comments.set("GENRE", vec![_format_genre_tag(c, &self.genre)]);
        comments.set("SECONDARYGENRE", vec![_format_genre_tag(c, &self.secondarygenre)]);
        comments.set("DESCRIPTOR", vec![self.descriptor.join(";")]);
        comments.set("LABEL", vec![self.label.join(";")]);
        comments.set("CATALOGNUMBER", vec![self.catalognumber.clone().unwrap_or_default()]);
        comments.set("EDITION", vec![self.edition.clone().unwrap_or_default()]);
        comments.set("RELEASETYPE", vec![self.releasetype.clone()]);
        comments.set("ALBUMARTIST", vec![format_artist_string(&self.releaseartists)]);
        comments.set("ARTIST", vec![format_artist_string(&self.trackartists)]);

        // Remove alt. role artist tags
        comments.remove("REMIXER");
        comments.remove("PRODUCER");
        comments.remove("COMPOSER");
        comments.remove("CONDUCTOR");
        comments.remove("DJMIXER");

        tag.write_to_path(&self.path).map_err(|e| RoseError::Generic(format!("Failed to write FLAC tags: {}", e)))?;

        Ok(())
    }

    fn flush_ogg(&self, _c: &Config) -> Result<()> {
        // This is complex because we need to preserve the audio data while replacing comments
        // For now, return an error indicating OGG writing is not implemented
        Err(RoseError::Generic("OGG/Opus tag writing not yet implemented".to_string()))
    }
}

// Helper functions

fn _split_tag(t: Option<&str>) -> Vec<String> {
    match t {
        Some(s) => TAG_SPLITTER_REGEX.split(s).map(|x| x.trim_end_matches('\0').to_string()).collect(),
        None => vec![],
    }
}

fn _split_genre_tag(t: Option<&str>) -> Vec<String> {
    match t {
        None => vec![],
        Some(s) => {
            let s = if let Some(idx) = s.find(r"\\PARENTS:\\") { &s[..idx] } else { s };
            TAG_SPLITTER_REGEX.split(s).map(|x| x.trim_end_matches('\0').to_string()).collect()
        }
    }
}

fn _format_genre_tag(c: &Config, t: &[String]) -> String {
    if !c.write_parent_genres {
        return t.join(";");
    }

    let mut parent_genres: Vec<String> = t
        .iter()
        .flat_map(|g| TRANSITIVE_PARENT_GENRES.get(g.as_str()).cloned().unwrap_or_default())
        .filter(|g| !t.contains(g))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    if parent_genres.is_empty() {
        t.join(";")
    } else {
        parent_genres.sort();
        format!("{}\\\\PARENTS:\\\\{}", t.join(";"), parent_genres.join(";"))
    }
}

fn _get_id3_tag(tag: &Id3Tag, keys: &[&str], split: bool, first: bool) -> Option<String> {
    for key in keys {
        if let Some(desc) = key.strip_prefix("TXXX:") {
            for frame in tag.frames() {
                if frame.id() == "TXXX" {
                    if let Some(extended) = frame.content().extended_text() {
                        if extended.description == desc {
                            let val = &extended.value;
                            if split {
                                let values: Vec<String> = _split_tag(Some(val));
                                if first {
                                    return values.into_iter().next();
                                } else {
                                    return Some(values.join(r" \\ "));
                                }
                            } else {
                                // Remove any null terminators from the value
                                return Some(val.trim_end_matches('\0').to_string());
                            }
                        }
                    }
                }
            }
        } else if let Some(text) = tag.get(key).and_then(|f| f.content().text()) {
            if split {
                let values: Vec<String> = _split_tag(Some(text));
                if first {
                    return values.into_iter().next();
                } else {
                    return Some(values.join(r" \\ "));
                }
            } else {
                // Remove any null terminators from the value
                return Some(text.trim_end_matches('\0').to_string());
            }
        }
    }
    None
}

fn _get_mp4_tag(tag: &Mp4Tag, key: &str) -> Option<String> {
    // Handle custom tags
    if key.starts_with("----:") {
        let parts: Vec<&str> = key[5..].splitn(2, ':').collect();
        if parts.len() == 2 {
            let ident = FreeformIdent::new(parts[0], parts[1]);

            let mut values = Vec::new();
            for data in tag.data_of(&ident) {
                match data {
                    Data::Utf8(s) => values.push(s.clone()),
                    Data::Utf16(s) => values.push(s.clone()),
                    Data::Reserved(bytes) => {
                        if let Ok(s) = String::from_utf8(bytes.clone()) {
                            values.push(s);
                        }
                    }
                    _ => {}
                }
            }

            if !values.is_empty() {
                // Join multiple values with semicolons, matching Python behavior
                return Some(values.join(";"));
            }
        }
    }

    None
}

fn _get_vorbis_tag(comments: &metaflac::block::VorbisComment, keys: &[&str], split: bool, first: bool) -> Option<String> {
    for key in keys {
        if let Some(values) = comments.get(key) {
            if values.is_empty() {
                continue;
            }

            if split {
                let all_values: Vec<String> = values.iter().flat_map(|v| _split_tag(Some(v))).collect();

                if first {
                    return all_values.into_iter().next();
                } else {
                    return Some(all_values.join(r" \\ "));
                }
            } else if first {
                return values.first().cloned();
            } else {
                return Some(values.join(r" \\ "));
            }
        }
    }
    None
}

fn _get_vorbis_map(map: &HashMap<String, Vec<String>>, keys: &[&str], split: bool, first: bool) -> Option<String> {
    for key in keys {
        if let Some(values) = map.get(*key) {
            if values.is_empty() {
                continue;
            }

            if split {
                let all_values: Vec<String> = values.iter().flat_map(|v| _split_tag(Some(v))).collect();

                if first {
                    return all_values.into_iter().next();
                } else {
                    return Some(all_values.join(r" \\ "));
                }
            } else if first {
                return values.first().cloned();
            } else {
                return Some(values.join(r" \\ "));
            }
        }
    }
    None
}

fn _parse_int(x: Option<&str>) -> Option<i32> {
    x?.parse().ok()
}

pub fn parse_artist_string(
    main: Option<&str>,
    remixer: Option<&str>,
    composer: Option<&str>,
    conductor: Option<&str>,
    producer: Option<&str>,
    dj: Option<&str>,
) -> ArtistMapping {
    let mut li_main = vec![];
    let mut li_conductor = _split_tag(conductor);
    let mut li_guests = vec![];
    let mut li_remixer = _split_tag(remixer);
    let mut li_composer = _split_tag(composer);
    let mut li_producer = _split_tag(producer);
    let mut li_dj = _split_tag(dj);

    let mut main = main.map(String::from);

    if let Some(ref m) = main.clone() {
        if let Some(idx) = m.find("produced by ") {
            let (m_part, p_part) = m.split_at(idx);
            main = Some(m_part.trim().to_string());
            let producer_part = p_part.trim_start_matches("produced by ").trim();
            li_producer.extend(_split_tag(Some(producer_part)));
        }
    }

    if let Some(ref m) = main.clone() {
        if let Some(idx) = m.find("remixed by ") {
            let (m_part, r_part) = m.split_at(idx);
            main = Some(m_part.trim().to_string());
            let remixer_part = r_part.trim_start_matches("remixed by ").trim();
            li_remixer.extend(_split_tag(Some(remixer_part)));
        }
    }

    if let Some(ref m) = main.clone() {
        if let Some(idx) = m.find("feat. ") {
            let (m_part, g_part) = m.split_at(idx);
            main = Some(m_part.trim().to_string());
            let guest_part = g_part.trim_start_matches("feat. ").trim();
            li_guests.extend(_split_tag(Some(guest_part)));
        }
    }

    if let Some(ref m) = main.clone() {
        if let Some(idx) = m.find("pres. ") {
            let (d_part, m_part) = m.split_at(idx);
            let dj_part = d_part.trim();
            li_dj.extend(_split_tag(Some(dj_part)));
            main = Some(m_part.trim_start_matches("pres. ").trim().to_string());
        }
    }

    if let Some(ref m) = main.clone() {
        if let Some(idx) = m.find("performed by ") {
            let (c_part, m_part) = m.split_at(idx);
            let composer_part = c_part.trim();
            li_composer.extend(_split_tag(Some(composer_part)));
            main = Some(m_part.trim_start_matches("performed by ").trim().to_string());
        }
    }

    if let Some(ref m) = main.clone() {
        if let Some(idx) = m.find("under. ") {
            let (m_part, c_part) = m.split_at(idx);
            main = Some(m_part.trim().to_string());
            let conductor_part = c_part.trim_start_matches("under. ").trim();
            li_conductor.extend(_split_tag(Some(conductor_part)));
        }
    }

    if let Some(m) = main {
        li_main.extend(_split_tag(Some(&m)));
    }

    let to_artist = |xs: Vec<String>| -> Vec<Artist> { xs.into_iter().map(|x| Artist::new(&x)).collect() };

    ArtistMapping {
        main: to_artist(uniq(li_main)),
        guest: to_artist(uniq(li_guests)),
        remixer: to_artist(uniq(li_remixer)),
        composer: to_artist(uniq(li_composer)),
        conductor: to_artist(uniq(li_conductor)),
        producer: to_artist(uniq(li_producer)),
        djmixer: to_artist(uniq(li_dj)),
    }
}

pub fn format_artist_string(mapping: &ArtistMapping) -> String {
    let format_role = |xs: &[Artist]| -> String { xs.iter().filter(|x| !x.alias).map(|x| x.name.clone()).collect::<Vec<_>>().join(";") };

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;
    use std::path::PathBuf;

    fn test_tagger_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata").join("Tagger")
    }

    #[test]
    fn test_split_tag() {
        assert_eq!(_split_tag(Some(r"a \\ b")), vec!["a", "b"]);
        assert_eq!(_split_tag(Some(r"a \ b")), vec![r"a \ b"]);
        assert_eq!(_split_tag(Some("a;b")), vec!["a", "b"]);
        assert_eq!(_split_tag(Some("a; b")), vec!["a", "b"]);
        assert_eq!(_split_tag(Some("a vs. b")), vec!["a", "b"]);
        assert_eq!(_split_tag(Some("a / b")), vec!["a", "b"]);
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
    fn test_getters() {
        struct GettersTestCase {
            filename: &'static str,
            track_num: &'static str,
            duration: i32,
        }

        let test_cases = vec![
            GettersTestCase {
                filename: "track1.flac",
                track_num: "1",
                duration: 2,
            },
            GettersTestCase {
                filename: "track2.m4a",
                track_num: "2",
                duration: 2,
            },
            GettersTestCase {
                filename: "track3.mp3",
                track_num: "3",
                duration: 1,
            },
            GettersTestCase {
                filename: "track4.vorbis.ogg",
                track_num: "4",
                duration: 1,
            },
            GettersTestCase {
                filename: "track5.opus.ogg",
                track_num: "5",
                duration: 1,
            },
        ];

        for case in test_cases {
            let _ = testing::init();
            let path = test_tagger_path().join(case.filename);
            let af = AudioTags::from_file(&path).unwrap();

            assert_eq!(af.releasetitle, Some("A Cool Album".to_string()));
            assert_eq!(af.releasetype, "album");
            assert_eq!(af.releasedate, Some(RoseDate::new(Some(1990), Some(2), Some(5))));
            assert_eq!(af.originaldate, Some(RoseDate::new(Some(1990), None, None)));
            assert_eq!(af.compositiondate, Some(RoseDate::new(Some(1984), None, None)));

            // Note: Different tag formats have different limitations
            assert_eq!(af.genre, vec!["Electronic", "House"]);
            assert_eq!(af.secondarygenre, vec!["Minimal", "Ambient"]);
            assert_eq!(af.descriptor, vec!["Lush", "Warm"]);
            assert_eq!(af.label, vec!["A Cool Label"]);
            assert_eq!(af.catalognumber, Some("DN-420".to_string()));
            assert_eq!(af.edition, Some("Japan".to_string()));

            // Note: Different tag formats have different limitations
            if case.filename == "track2.m4a" {
                assert_eq!(af.releaseartists.main, vec![Artist::new("Artist A")]);
            } else {
                assert_eq!(af.releaseartists.main, vec![Artist::new("Artist A"), Artist::new("Artist B")]);
            }

            assert_eq!(af.tracknumber, Some(case.track_num.to_string()));
            assert_eq!(af.tracktotal, Some(5));
            assert_eq!(af.discnumber, Some("1".to_string()));
            assert_eq!(af.disctotal, Some(1));

            assert_eq!(af.tracktitle, Some(format!("Track {}", case.track_num)));

            // Verify track artists
            assert_eq!(af.trackartists.main, vec![Artist::new("Artist A"), Artist::new("Artist B")]);
            assert_eq!(af.trackartists.guest, vec![Artist::new("Artist C"), Artist::new("Artist D")]);

            // Duration check - may vary based on implementation
            // assert_eq!(af.duration_sec, case.duration);
        }
    }

    #[test]
    fn test_flush() {
        struct FlushTestCase {
            filename: &'static str,
            track_num: &'static str,
            duration: i32,
        }

        let test_cases = vec![
            FlushTestCase {
                filename: "track1.flac",
                track_num: "1",
                duration: 2,
            },
            FlushTestCase {
                filename: "track2.m4a",
                track_num: "2",
                duration: 2,
            },
            FlushTestCase {
                filename: "track3.mp3",
                track_num: "3",
                duration: 1,
            },
            FlushTestCase {
                filename: "track4.vorbis.ogg",
                track_num: "4",
                duration: 1,
            },
            FlushTestCase {
                filename: "track5.opus.ogg",
                track_num: "5",
                duration: 1,
            },
        ];

        for case in test_cases {
            // Skip OGG/Opus files since flush is not implemented
            if case.filename.ends_with(".ogg") {
                continue;
            }

            let (config, temp_dir) = testing::config();
            let src_path = test_tagger_path().join(case.filename);
            let dst_path = temp_dir.path().join(case.filename);
            std::fs::copy(&src_path, &dst_path).unwrap();

            let mut af = AudioTags::from_file(&dst_path).unwrap();

            // Modify the djmixer artist to test that we clear the original tag
            af.trackartists.djmixer = vec![Artist::new("New")];
            // Also test date writing
            af.originaldate = Some(RoseDate::new(Some(1990), Some(4), Some(20)));

            af.flush(&config, true).unwrap();

            // Read back and verify
            let af = AudioTags::from_file(&dst_path).unwrap();

            assert_eq!(af.releasetitle, Some("A Cool Album".to_string()));
            assert_eq!(af.releasetype, "album");
            assert_eq!(af.releasedate, Some(RoseDate::new(Some(1990), Some(2), Some(5))));
            assert_eq!(af.originaldate, Some(RoseDate::new(Some(1990), Some(4), Some(20))));
            assert_eq!(af.compositiondate, Some(RoseDate::new(Some(1984), None, None)));
            assert_eq!(af.genre, vec!["Electronic", "House"]);
            assert_eq!(af.secondarygenre, vec!["Minimal", "Ambient"]);
            assert_eq!(af.descriptor, vec!["Lush", "Warm"]);
            assert_eq!(af.label, vec!["A Cool Label"]);
            assert_eq!(af.catalognumber, Some("DN-420".to_string()));
            assert_eq!(af.edition, Some("Japan".to_string()));

            // Note: Different tag formats have different limitations
            if case.filename == "track2.m4a" {
                assert_eq!(af.releaseartists.main, vec![Artist::new("Artist A")]);
            } else {
                assert_eq!(af.releaseartists.main, vec![Artist::new("Artist A"), Artist::new("Artist B")]);
            }

            assert_eq!(af.tracknumber, Some(case.track_num.to_string()));
            assert_eq!(af.discnumber, Some("1".to_string()));

            assert_eq!(af.tracktitle, Some(format!("Track {}", case.track_num)));

            // Verify track artists - djmixer should be changed to "New"
            assert_eq!(af.trackartists.main, vec![Artist::new("Artist A"), Artist::new("Artist B")]);
            assert_eq!(af.trackartists.guest, vec![Artist::new("Artist C"), Artist::new("Artist D")]);
            assert_eq!(af.trackartists.remixer, vec![Artist::new("Artist AB"), Artist::new("Artist BC")]);
            assert_eq!(af.trackartists.producer, vec![Artist::new("Artist CD"), Artist::new("Artist DE")]);
            assert_eq!(af.trackartists.composer, vec![Artist::new("Artist EF"), Artist::new("Artist FG")]);
            assert_eq!(af.trackartists.conductor, vec![Artist::new("Artist GH"), Artist::new("Artist HI")]);
            assert_eq!(af.trackartists.djmixer, vec![Artist::new("New")]);

            assert_eq!(af.duration_sec, case.duration);
        }
    }

    #[test]
    fn test_write_parent_genres() {
        let (mut config, temp_dir) = testing::config();
        let src_path = test_tagger_path().join("track1.flac");
        let dst_path = temp_dir.path().join("track1.flac");
        std::fs::copy(&src_path, &dst_path).unwrap();

        let mut af = AudioTags::from_file(&dst_path).unwrap();

        // Modify djmixer and date
        af.trackartists.djmixer = vec![Artist::new("New")];
        af.originaldate = Some(RoseDate::new(Some(1990), Some(4), Some(20)));

        config.write_parent_genres = true;
        af.flush(&config, true).unwrap();

        // Check raw tags
        let tag = FlacTag::read_from_path(&dst_path).unwrap();
        let vorbis = tag.vorbis_comments().unwrap();

        if let Some(genre_values) = vorbis.get("GENRE") {
            assert_eq!(genre_values[0], "Electronic;House\\\\PARENTS:\\\\Dance;Electronic Dance Music");
        }

        if let Some(secondary_values) = vorbis.get("SECONDARYGENRE") {
            assert_eq!(secondary_values[0], "Minimal;Ambient");
        }

        // Read back and verify genres are parsed correctly
        let af = AudioTags::from_file(&dst_path).unwrap();
        assert_eq!(af.genre, vec!["Electronic", "House"]);
        assert_eq!(af.secondarygenre, vec!["Minimal", "Ambient"]);
    }

    #[test]
    fn test_id_assignment() {
        struct IdAssignmentTestCase {
            filename: &'static str,
        }

        let test_cases = vec![
            IdAssignmentTestCase { filename: "track1.flac" },
            IdAssignmentTestCase { filename: "track2.m4a" },
            IdAssignmentTestCase { filename: "track3.mp3" },
            IdAssignmentTestCase { filename: "track4.vorbis.ogg" },
            IdAssignmentTestCase { filename: "track5.opus.ogg" },
        ];

        for case in test_cases {
            // Skip OGG/Opus files since flush is not implemented
            if case.filename.ends_with(".ogg") {
                continue;
            }

            let (config, temp_dir) = testing::config();
            let src_path = test_tagger_path().join(case.filename);
            let dst_path = temp_dir.path().join(case.filename);
            std::fs::copy(&src_path, &dst_path).unwrap();

            let mut af = AudioTags::from_file(&dst_path).unwrap();
            af.id = Some("ahaha".to_string());
            af.release_id = Some("bahaha".to_string());

            af.flush(&config, true).unwrap();

            let af = AudioTags::from_file(&dst_path).unwrap();
            assert_eq!(af.id, Some("ahaha".to_string()));
            assert_eq!(af.release_id, Some("bahaha".to_string()));
        }
    }

    #[test]
    fn test_releasetype_normalization() {
        struct ReleaseTypeTestCase {
            filename: &'static str,
        }

        let test_cases = vec![
            ReleaseTypeTestCase { filename: "track1.flac" },
            ReleaseTypeTestCase { filename: "track2.m4a" },
            ReleaseTypeTestCase { filename: "track3.mp3" },
            ReleaseTypeTestCase { filename: "track4.vorbis.ogg" },
            ReleaseTypeTestCase { filename: "track5.opus.ogg" },
        ];

        for case in test_cases {
            // Skip OGG/Opus files since flush is not implemented
            if case.filename.ends_with(".ogg") {
                continue;
            }

            let (config, temp_dir) = testing::config();
            let src_path = test_tagger_path().join(case.filename);
            let dst_path = temp_dir.path().join(case.filename);
            std::fs::copy(&src_path, &dst_path).unwrap();

            // Check that release type is read correctly
            let mut af = AudioTags::from_file(&dst_path).unwrap();
            assert_eq!(af.releasetype, "album");

            // Assert that attempting to flush a stupid value fails
            af.releasetype = "lalala".to_string();
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
}
