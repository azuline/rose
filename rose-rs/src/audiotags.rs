/// The audiotags module abstracts over tag reading and writing for five different audio formats,
/// exposing a single standard interface for all audio files.
///
/// The audiotags module also handles Rose-specific tagging semantics, such as multi-valued tags,
/// normalization, artist formatting, and enum validation.
use crate::common::{uniq, Artist, ArtistMapping, Result, RoseDate, RoseError, RoseExpectedError};
use crate::genre_hierarchy::GenreHierarchy;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;
use std::path::Path;
use tracing::warn;

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

        // Try parsing as year only
        if let Ok(year) = value.parse::<i32>() {
            return Some(RoseDate {
                year: Some(year),
                month: None,
                day: None,
            });
        }

        // Try parsing as date with regex
        if let Some(caps) = DATE_REGEX.captures(value) {
            let year = caps.get(1).and_then(|m| m.as_str().parse::<i32>().ok());
            let month = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok());
            let day = caps.get(3).and_then(|m| m.as_str().parse::<u32>().ok());
            return Some(RoseDate { year, month, day });
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
    pub path: std::path::PathBuf,
}

impl AudioTags {
    /// Read the tags of an audio file on disk.
    ///
    /// NOTE: This is a placeholder implementation. The full implementation would use
    /// the lofty crate to read actual audio file metadata. For now, this returns
    /// a default structure to allow compilation.
    pub fn from_file(p: &Path) -> Result<Self> {
        // Check if the file has a supported extension
        let ext = p.extension().and_then(|e| e.to_str()).map(|e| format!(".{}", e.to_lowercase()));

        let is_supported = ext.as_ref().map(|e| SUPPORTED_AUDIO_EXTENSIONS.contains(&e.as_str())).unwrap_or(false);

        if !is_supported {
            return Err(UnsupportedFiletypeError(format!("{} not a supported filetype", ext.unwrap_or_else(|| "No extension".to_string()))).into());
        }

        // NOTE: This is a placeholder. Real implementation would use lofty to read tags
        warn!("AudioTags::from_file is not fully implemented - returning default values");

        Ok(AudioTags {
            id: None,
            release_id: None,
            tracktitle: Some("Unknown Track".to_string()),
            tracknumber: Some("1".to_string()),
            tracktotal: Some(1),
            discnumber: Some("1".to_string()),
            disctotal: Some(1),
            trackartists: ArtistMapping::default(),
            releasetitle: Some("Unknown Album".to_string()),
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
            path: p.to_path_buf(),
        })
    }

    /// Flush the current tags to the file on disk.
    ///
    /// NOTE: This is a placeholder implementation.
    pub fn flush(&self, _validate: bool) -> Result<()> {
        // Validate release type
        if _validate && !SUPPORTED_RELEASE_TYPES.contains(&self.releasetype.as_str()) {
            return Err(UnsupportedTagValueTypeError(format!(
                "Release type {} is not a supported release type.\nSupported release types: {}",
                self.releasetype,
                SUPPORTED_RELEASE_TYPES.join(", ")
            ))
            .into());
        }

        warn!("AudioTags::flush is not fully implemented");
        Ok(())
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
        if let Some(parents) = GenreHierarchy::transitive_parents(&genre.to_lowercase()) {
            for parent in parents {
                parent_genres.insert(parent.to_string());
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
}

// PYTHON CODE TO BE TRANSLATED:

// @classmethod
// def from_file(cls, p: Path) -> AudioTags:
//     """Read the tags of an audio file on disk."""
//     import mutagen
//     import mutagen.flac
//     import mutagen.id3
//     import mutagen.mp3
//     import mutagen.mp4
//     import mutagen.oggopus
//     import mutagen.oggvorbis
//
//     if not any(p.suffix.lower() == ext for ext in SUPPORTED_AUDIO_EXTENSIONS):
//         raise UnsupportedFiletypeError(f"{p.suffix} not a supported filetype")
//     try:
//         m = mutagen.File(p)  # type: ignore
//     except mutagen.MutagenError as e:  # type: ignore
//         raise UnsupportedFiletypeError(f"Failed to open file: {e}") from e
//     if isinstance(m, mutagen.mp3.MP3):
//         # ID3 returns trackno/discno tags as no/total. We have to parse.
//         tracknumber = discnumber = tracktotal = disctotal = None
//         if tracknos := _get_tag(m.tags, ["TRCK"]):
//             try:
//                 tracknumber, tracktotalstr = tracknos.split("/", 1)
//                 tracktotal = _parse_int(tracktotalstr)
//             except ValueError:
//                 tracknumber = tracknos
//         if discnos := _get_tag(m.tags, ["TPOS"]):
//             try:
//                 discnumber, disctotalstr = discnos.split("/", 1)
//                 disctotal = _parse_int(disctotalstr)
//             except ValueError:
//                 discnumber = discnos
//
//         def _get_paired_frame(x: str) -> str | None:
//             if not m.tags:
//                 return None
//             for tag in ["TIPL", "IPLS"]:
//                 try:
//                     frame = m.tags[tag]
//                 except KeyError:
//                     continue
//                 return r" \\ ".join([p[1] for p in frame.people if p[0].lower() == x.lower()])
//             return None
//
//         return AudioTags(
//             id=_get_tag(m.tags, ["TXXX:ROSEID"], first=True),
//             release_id=_get_tag(m.tags, ["TXXX:ROSERELEASEID"], first=True),
//             tracktitle=_get_tag(m.tags, ["TIT2"]),
//             releasedate=RoseDate.parse(_get_tag(m.tags, ["TDRC", "TYER", "TDAT"])),
//             originaldate=RoseDate.parse(_get_tag(m.tags, ["TDOR", "TORY"])),
//             compositiondate=RoseDate.parse(_get_tag(m.tags, ["TXXX:COMPOSITIONDATE"], first=True)),
//             tracknumber=tracknumber,
//             tracktotal=tracktotal,
//             discnumber=discnumber,
//             disctotal=disctotal,
//             releasetitle=_get_tag(m.tags, ["TALB"]),
//             genre=_split_genre_tag(_get_tag(m.tags, ["TCON"], split=True)),
//             secondarygenre=_split_genre_tag(_get_tag(m.tags, ["TXXX:SECONDARYGENRE"], split=True)),
//             descriptor=_split_tag(_get_tag(m.tags, ["TXXX:DESCRIPTOR"], split=True)),
//             label=_split_tag(_get_tag(m.tags, ["TPUB"], split=True)),
//             catalognumber=_get_tag(m.tags, ["TXXX:CATALOGNUMBER"], first=True),
//             edition=_get_tag(m.tags, ["TXXX:EDITION"], first=True),
//             releasetype=_normalize_rtype(
//                 _get_tag(m.tags, ["TXXX:RELEASETYPE", "TXXX:MusicBrainz Album Type"], first=True)
//             ),
//             releaseartists=parse_artist_string(main=_get_tag(m.tags, ["TPE2"], split=True)),
//             trackartists=parse_artist_string(
//                 main=_get_tag(m.tags, ["TPE1"], split=True),
//                 remixer=_get_tag(m.tags, ["TPE4"], split=True),
//                 composer=_get_tag(m.tags, ["TCOM"], split=True),
//                 conductor=_get_tag(m.tags, ["TPE3"], split=True),
//                 producer=_get_paired_frame("producer"),
//                 dj=_get_paired_frame("DJ-mix"),
//             ),
//             duration_sec=round(m.info.length),
//             path=p,
//         )
//     if isinstance(m, mutagen.mp4.MP4):
//         tracknumber = discnumber = tracktotal = disctotal = None
//         with contextlib.suppress(ValueError):
//             tracknumber, tracktotalstr = _get_tuple_tag(m.tags, ["trkn"])  # type: ignore
//             tracktotal = _parse_int(tracktotalstr)
//         with contextlib.suppress(ValueError):
//             discnumber, disctotalstr = _get_tuple_tag(m.tags, ["disk"])  # type: ignore
//             disctotal = _parse_int(disctotalstr)
//
//         return AudioTags(
//             id=_get_tag(m.tags, ["----:net.sunsetglow.rose:ID"]),
//             release_id=_get_tag(m.tags, ["----:net.sunsetglow.rose:RELEASEID"]),
//             tracktitle=_get_tag(m.tags, ["\xa9nam"]),
//             releasedate=RoseDate.parse(_get_tag(m.tags, ["\xa9day"])),
//             originaldate=RoseDate.parse(
//                 _get_tag(
//                     m.tags,
//                     [
//                         "----:net.sunsetglow.rose:ORIGINALDATE",
//                         "----:com.apple.iTunes:ORIGINALDATE",
//                         "----:com.apple.iTunes:ORIGINALYEAR",
//                     ],
//                 )
//             ),
//             compositiondate=RoseDate.parse(_get_tag(m.tags, ["----:net.sunsetglow.rose:COMPOSITIONDATE"])),
//             tracknumber=str(tracknumber),
//             tracktotal=tracktotal,
//             discnumber=str(discnumber),
//             disctotal=disctotal,
//             releasetitle=_get_tag(m.tags, ["\xa9alb"]),
//             genre=_split_genre_tag(_get_tag(m.tags, ["\xa9gen"], split=True)),
//             secondarygenre=_split_genre_tag(
//                 _get_tag(m.tags, ["----:net.sunsetglow.rose:SECONDARYGENRE"], split=True)
//             ),
//             descriptor=_split_tag(_get_tag(m.tags, ["----:net.sunsetglow.rose:DESCRIPTOR"], split=True)),
//             label=_split_tag(_get_tag(m.tags, ["----:com.apple.iTunes:LABEL"], split=True)),
//             catalognumber=_get_tag(m.tags, ["----:com.apple.iTunes:CATALOGNUMBER"]),
//             edition=_get_tag(m.tags, ["----:net.sunsetglow.rose:EDITION"]),
//             releasetype=_normalize_rtype(
//                 _get_tag(
//                     m.tags,
//                     [
//                         "----:com.apple.iTunes:RELEASETYPE",
//                         "----:com.apple.iTunes:MusicBrainz Album Type",
//                     ],
//                     first=True,
//                 )
//             ),
//             releaseartists=parse_artist_string(main=_get_tag(m.tags, ["aART"], split=True)),
//             trackartists=parse_artist_string(
//                 main=_get_tag(m.tags, ["\xa9ART"], split=True),
//                 remixer=_get_tag(m.tags, ["----:com.apple.iTunes:REMIXER"], split=True),
//                 producer=_get_tag(m.tags, ["----:com.apple.iTunes:PRODUCER"], split=True),
//                 composer=_get_tag(m.tags, ["\xa9wrt"], split=True),
//                 conductor=_get_tag(m.tags, ["----:com.apple.iTunes:CONDUCTOR"], split=True),
//                 dj=_get_tag(m.tags, ["----:com.apple.iTunes:DJMIXER"], split=True),
//             ),
//             duration_sec=round(m.info.length),  # type: ignore
//             path=p,
//         )
//     if isinstance(m, mutagen.flac.FLAC | mutagen.oggvorbis.OggVorbis | mutagen.oggopus.OggOpus):
//         return AudioTags(
//             id=_get_tag(m.tags, ["roseid"]),
//             release_id=_get_tag(m.tags, ["rosereleaseid"]),
//             tracktitle=_get_tag(m.tags, ["title"]),
//             releasedate=RoseDate.parse(_get_tag(m.tags, ["date", "year"])),
//             originaldate=RoseDate.parse(_get_tag(m.tags, ["originaldate", "originalyear"])),
//             compositiondate=RoseDate.parse(_get_tag(m.tags, ["compositiondate"])),
//             tracknumber=_get_tag(m.tags, ["tracknumber"], first=True),
//             tracktotal=_parse_int(_get_tag(m.tags, ["tracktotal"], first=True)),
//             discnumber=_get_tag(m.tags, ["discnumber"], first=True),
//             disctotal=_parse_int(_get_tag(m.tags, ["disctotal"], first=True)),
//             releasetitle=_get_tag(m.tags, ["album"]),
//             genre=_split_genre_tag(_get_tag(m.tags, ["genre"], split=True)),
//             secondarygenre=_split_genre_tag(_get_tag(m.tags, ["secondarygenre"], split=True)),
//             descriptor=_split_tag(_get_tag(m.tags, ["descriptor"], split=True)),
//             label=_split_tag(_get_tag(m.tags, ["label", "organization", "recordlabel"], split=True)),
//             catalognumber=_get_tag(m.tags, ["catalognumber"]),
//             edition=_get_tag(m.tags, ["edition"]),
//             releasetype=_normalize_rtype(_get_tag(m.tags, ["releasetype"], first=True)),
//             releaseartists=parse_artist_string(main=_get_tag(m.tags, ["albumartist"], split=True)),
//             trackartists=parse_artist_string(
//                 main=_get_tag(m.tags, ["artist"], split=True),
//                 remixer=_get_tag(m.tags, ["remixer"], split=True),
//                 producer=_get_tag(m.tags, ["producer"], split=True),
//                 composer=_get_tag(m.tags, ["composer"], split=True),
//                 conductor=_get_tag(m.tags, ["conductor"], split=True),
//                 dj=_get_tag(m.tags, ["djmixer"], split=True),
//             ),
//             duration_sec=round(m.info.length),  # type: ignore
//             path=p,
//         )
//     raise UnsupportedFiletypeError(f"{p} is not a supported audio file")
//
// @no_type_check
// def flush(self, c: Config, *, validate: bool = True) -> None:
//     """Flush the current tags to the file on disk."""
//     import mutagen
//     import mutagen.flac
//     import mutagen.id3
//     import mutagen.mp3
//     import mutagen.mp4
//     import mutagen.oggopus
//     import mutagen.oggvorbis
//
//     m = mutagen.File(self.path)
//     if not validate and "pytest" not in sys.modules:
//         raise Exception("Validate can only be turned off by tests.")
//
//     self.releasetype = (self.releasetype or "unknown").lower()
//     if validate and self.releasetype not in SUPPORTED_RELEASE_TYPES:
//         raise UnsupportedTagValueTypeError(
//             f"Release type {self.releasetype} is not a supported release type.\n"
//             f"Supported release types: {", ".join(SUPPORTED_RELEASE_TYPES)}"
//         )
//
//     if isinstance(m, mutagen.mp3.MP3):
//         if m.tags is None:
//             m.tags = mutagen.id3.ID3()
//
//         def _write_standard_tag(key: str, value: str | None) -> None:
//             m.tags.delall(key)
//             if value:
//                 frame = getattr(mutagen.id3, key)(text=value)
//                 m.tags.add(frame)
//
//         def _write_tag_with_description(name: str, value: str | None) -> None:
//             key, desc = name.split(":", 1)
//             # Since the ID3 tags work with the shared prefix key before `:`, manually preserve
//             # the other tags with the shared prefix key.
//             keep_fields = [f for f in m.tags.getall(key) if getattr(f, "desc", None) != desc]
//             m.tags.delall(key)
//             if value:
//                 frame = getattr(mutagen.id3, key)(desc=desc, text=[value])
//                 m.tags.add(frame)
//             for f in keep_fields:
//                 m.tags.add(f)
//
//         _write_tag_with_description("TXXX:ROSEID", self.id)
//         _write_tag_with_description("TXXX:ROSERELEASEID", self.release_id)
//         _write_standard_tag("TIT2", self.tracktitle)
//         _write_standard_tag("TDRC", str(self.releasedate))
//         _write_standard_tag("TDOR", str(self.originaldate))
//         _write_tag_with_description("TXXX:COMPOSITIONDATE", str(self.compositiondate))
//         _write_standard_tag("TRCK", self.tracknumber)
//         _write_standard_tag("TPOS", self.discnumber)
//         _write_standard_tag("TALB", self.releasetitle)
//         _write_standard_tag("TCON", _format_genre_tag(c, self.genre))
//         _write_tag_with_description("TXXX:SECONDARYGENRE", _format_genre_tag(c, self.secondarygenre))
//         _write_tag_with_description("TXXX:DESCRIPTOR", ";".join(self.descriptor))
//         _write_standard_tag("TPUB", ";".join(self.label))
//         _write_tag_with_description("TXXX:CATALOGNUMBER", self.catalognumber)
//         _write_tag_with_description("TXXX:EDITION", self.edition)
//         _write_tag_with_description("TXXX:RELEASETYPE", self.releasetype)
//         _write_standard_tag("TPE2", format_artist_string(self.releaseartists))
//         _write_standard_tag("TPE1", format_artist_string(self.trackartists))
//         # Wipe the alt. role artist tags, since we encode the full artist into the main tag.
//         m.tags.delall("TPE4")
//         m.tags.delall("TCOM")
//         m.tags.delall("TPE3")
//         # Delete all paired text frames, since these represent additional artist roles. We don't
//         # want to preserve them.
//         m.tags.delall("TIPL")
//         m.tags.delall("IPLS")
//         m.save()
//         return
//     if isinstance(m, mutagen.mp4.MP4):
//         if m.tags is None:
//             m.tags = mutagen.mp4.MP4Tags()
//         m.tags["----:net.sunsetglow.rose:ID"] = (self.id or "").encode()
//         m.tags["----:net.sunsetglow.rose:RELEASEID"] = (self.release_id or "").encode()
//         m.tags["\xa9nam"] = self.tracktitle or ""
//         m.tags["\xa9day"] = str(self.releasedate)
//         m.tags["----:net.sunsetglow.rose:ORIGINALDATE"] = str(self.originaldate).encode()
//         m.tags["----:net.sunsetglow.rose:COMPOSITIONDATE"] = str(self.compositiondate).encode()
//         m.tags["\xa9alb"] = self.releasetitle or ""
//         m.tags["\xa9gen"] = _format_genre_tag(c, self.genre)
//         m.tags["----:net.sunsetglow.rose:SECONDARYGENRE"] = _format_genre_tag(c, self.secondarygenre).encode()
//         m.tags["----:net.sunsetglow.rose:DESCRIPTOR"] = ";".join(self.descriptor).encode()
//         m.tags["----:com.apple.iTunes:LABEL"] = ";".join(self.label).encode()
//         m.tags["----:com.apple.iTunes:CATALOGNUMBER"] = (self.catalognumber or "").encode()
//         m.tags["----:net.sunsetglow.rose:EDITION"] = (self.edition or "").encode()
//         m.tags["----:com.apple.iTunes:RELEASETYPE"] = self.releasetype.encode()
//         m.tags["aART"] = format_artist_string(self.releaseartists)
//         m.tags["\xa9ART"] = format_artist_string(self.trackartists)
//         # Wipe the alt. role artist tags, since we encode the full artist into the main tag.
//         with contextlib.suppress(KeyError):
//             del m.tags["----:com.apple.iTunes:REMIXER"]
//         with contextlib.suppress(KeyError):
//             del m.tags["----:com.apple.iTunes:PRODUCER"]
//         with contextlib.suppress(KeyError):
//             del m.tags["\xa9wrt"]
//         with contextlib.suppress(KeyError):
//             del m.tags["----:com.apple.iTunes:CONDUCTOR"]
//         with contextlib.suppress(KeyError):
//             del m.tags["----:com.apple.iTunes:DJMIXER"]
//
//         # The track and disc numbers in MP4 are a bit annoying, because they must be a
//         # single-element list of 2-tuple ints. We preserve the previous tracktotal/disctotal (as
//         # Rose does not care about those values), and then attempt to write our own tracknumber
//         # and discnumber.
//         try:
//             prev_tracktotal = m.tags["trkn"][0][1]
//         except (KeyError, IndexError):
//             prev_tracktotal = 1
//         try:
//             prev_disctotal = m.tags["disk"][0][1]
//         except (KeyError, IndexError):
//             prev_disctotal = 1
//         try:
//             # Not sure why they can be a None string, but whatever...
//             if self.tracknumber == "None":
//                 self.tracknumber = None
//             if self.discnumber == "None":
//                 self.discnumber = None
//             m.tags["trkn"] = [(int(self.tracknumber or "0"), prev_tracktotal)]
//             m.tags["disk"] = [(int(self.discnumber or "0"), prev_disctotal)]
//         except ValueError as e:
//             raise UnsupportedTagValueTypeError(
//                 "Could not write m4a trackno/discno tags: must be integers. "
//                 f"Got: {self.tracknumber=} / {self.discnumber=}"
//             ) from e
//
//         m.save()
//         return
//     if isinstance(m, mutagen.flac.FLAC | mutagen.oggvorbis.OggVorbis | mutagen.oggopus.OggOpus):
//         if m.tags is None:
//             if isinstance(m, mutagen.flac.FLAC):
//                 m.tags = mutagen.flac.VCFLACDict()
//             elif isinstance(m, mutagen.oggvorbis.OggVorbis):
//                 m.tags = mutagen.oggvorbis.OggVCommentDict()
//             else:
//                 m.tags = mutagen.oggopus.OggOpusVComment()
//         assert not isinstance(m.tags, mutagen.flac.MetadataBlock)
//         m.tags["roseid"] = self.id or ""
//         m.tags["rosereleaseid"] = self.release_id or ""
//         m.tags["title"] = self.tracktitle or ""
//         m.tags["date"] = str(self.releasedate)
//         m.tags["originaldate"] = str(self.originaldate)
//         m.tags["compositiondate"] = str(self.compositiondate)
//         m.tags["tracknumber"] = self.tracknumber or ""
//         m.tags["discnumber"] = self.discnumber or ""
//         m.tags["album"] = self.releasetitle or ""
//         m.tags["genre"] = _format_genre_tag(c, self.genre)
//         m.tags["secondarygenre"] = _format_genre_tag(c, self.secondarygenre)
//         m.tags["descriptor"] = ";".join(self.descriptor)
//         m.tags["label"] = ";".join(self.label)
//         m.tags["catalognumber"] = self.catalognumber or ""
//         m.tags["edition"] = self.edition or ""
//         m.tags["releasetype"] = self.releasetype
//         m.tags["albumartist"] = format_artist_string(self.releaseartists)
//         m.tags["artist"] = format_artist_string(self.trackartists)
//         # Wipe the alt. role artist tags, since we encode the full artist into the main tag.
//         with contextlib.suppress(KeyError):
//             del m.tags["remixer"]
//         with contextlib.suppress(KeyError):
//             del m.tags["producer"]
//         with contextlib.suppress(KeyError):
//             del m.tags["composer"]
//         with contextlib.suppress(KeyError):
//             del m.tags["conductor"]
//         with contextlib.suppress(KeyError):
//             del m.tags["djmixer"]
//         m.save()
//         return
//
//     raise RoseError(f"Impossible: unknown mutagen type: {type(m)=} ({repr(m)=})")
