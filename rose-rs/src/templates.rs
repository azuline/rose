use crate::common::{Artist, ArtistMapping};
use crate::config::PathTemplate;
use crate::error::{RoseError, RoseExpectedError};
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tera::{Context, Filter, Tera, Value};

lazy_static! {
    static ref TEMPLATE_ENGINE: Mutex<Option<Tera>> = Mutex::new(None);
    static ref COLLAPSE_SPACING_REGEX: Regex = Regex::new(r"\s+").unwrap();
}

// Release type formatter mapping
pub fn releasetypefmt(x: &str) -> String {
    match x {
        "album" => "Album",
        "single" => "Single",
        "ep" => "EP",
        "compilation" => "Compilation",
        "anthology" => "Anthology",
        "soundtrack" => "Soundtrack",
        "live" => "Live",
        "remix" => "Remix",
        "djmix" => "DJ-Mix",
        "mixtape" => "Mixtape",
        "other" => "Other",
        "demo" => "Demo",
        "unknown" => "Unknown",
        _ => {
            // Title case the string
            return x
                .split_whitespace()
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
        }
    }
    .to_string()
}

// Format an array as "x, y & z"
pub fn arrayfmt(xs: &[String]) -> String {
    match xs.len() {
        0 => String::new(),
        1 => xs[0].clone(),
        _ => {
            let last = xs.last().unwrap();
            let init = &xs[..xs.len() - 1];
            format!("{} & {}", init.join(", "), last)
        }
    }
}

// Format an array of Artists
pub fn artistsarrayfmt(xs: &[Artist]) -> String {
    let strs: Vec<String> = xs
        .iter()
        .filter(|x| !x.alias)
        .map(|x| x.name.clone())
        .collect();
    if strs.len() <= 3 {
        arrayfmt(&strs)
    } else {
        format!("{} et al.", strs[0])
    }
}

// Format a mapping of artists
pub fn artistsfmt(a: &ArtistMapping, omit: Option<Vec<String>>) -> String {
    let omit = omit.unwrap_or_default();

    let mut r = artistsarrayfmt(&a.main);

    if !a.djmixer.is_empty() && !omit.contains(&"djmixer".to_string()) {
        r = format!("{} pres. {}", artistsarrayfmt(&a.djmixer), r);
    } else if !a.composer.is_empty() && !omit.contains(&"composer".to_string()) {
        r = format!("{} performed by {}", artistsarrayfmt(&a.composer), r);
    }

    if !a.conductor.is_empty() && !omit.contains(&"conductor".to_string()) {
        r = format!("{} under {}", r, artistsarrayfmt(&a.conductor));
    }

    if !a.guest.is_empty() && !omit.contains(&"guest".to_string()) {
        r = format!("{} (feat. {})", r, artistsarrayfmt(&a.guest));
    }

    if !a.producer.is_empty() && !omit.contains(&"producer".to_string()) {
        r = format!("{} (prod. {})", r, artistsarrayfmt(&a.producer));
    }

    if r.is_empty() {
        "Unknown Artists".to_string()
    } else {
        r
    }
}

// Sort order filter - "First Last" -> "Last, First"
pub fn sortorder(x: &str) -> String {
    match x.rsplit_once(' ') {
        Some((first, last)) => format!("{last}, {first}"),
        None => x.to_string(),
    }
}

// Last name filter
pub fn lastname(x: &str) -> String {
    match x.rsplit_once(' ') {
        Some((_, last)) => last.to_string(),
        None => x.to_string(),
    }
}

// Custom filters for Tera
struct ArrayFmtFilter;
impl Filter for ArrayFmtFilter {
    fn filter(&self, value: &Value, _args: &HashMap<String, Value>) -> tera::Result<Value> {
        match value {
            Value::Array(arr) => {
                let strs: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                Ok(Value::String(arrayfmt(&strs)))
            }
            _ => Err(tera::Error::msg("arrayfmt expects an array")),
        }
    }
}

struct ArtistsArrayFmtFilter;
impl Filter for ArtistsArrayFmtFilter {
    fn filter(&self, value: &Value, _args: &HashMap<String, Value>) -> tera::Result<Value> {
        match value {
            Value::Array(arr) => {
                let artists: Vec<Artist> = arr
                    .iter()
                    .filter_map(|v| {
                        v.as_object().and_then(|obj| {
                            obj.get("name").and_then(|n| n.as_str()).map(|name| {
                                let alias =
                                    obj.get("alias").and_then(|a| a.as_bool()).unwrap_or(false);
                                Artist::with_alias(name.to_string(), alias)
                            })
                        })
                    })
                    .collect();
                Ok(Value::String(artistsarrayfmt(&artists)))
            }
            _ => Err(tera::Error::msg("artistsarrayfmt expects an array")),
        }
    }
}

struct ArtistsFmtFilter;
impl Filter for ArtistsFmtFilter {
    fn filter(&self, value: &Value, args: &HashMap<String, Value>) -> tera::Result<Value> {
        let mapping = match value.as_object() {
            Some(obj) => {
                let mut mapping = ArtistMapping::new();

                // Helper to extract artists from object field
                let extract_artists = |field: &str| -> Vec<Artist> {
                    obj.get(field)
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| {
                                    v.as_object().and_then(|obj| {
                                        obj.get("name").and_then(|n| n.as_str()).map(|name| {
                                            let alias = obj
                                                .get("alias")
                                                .and_then(|a| a.as_bool())
                                                .unwrap_or(false);
                                            Artist::with_alias(name.to_string(), alias)
                                        })
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                };

                mapping.main = extract_artists("main");
                mapping.guest = extract_artists("guest");
                mapping.remixer = extract_artists("remixer");
                mapping.producer = extract_artists("producer");
                mapping.composer = extract_artists("composer");
                mapping.conductor = extract_artists("conductor");
                mapping.djmixer = extract_artists("djmixer");

                mapping
            }
            None => return Err(tera::Error::msg("artistsfmt expects an object")),
        };

        let omit = args.get("omit").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        });

        Ok(Value::String(artistsfmt(&mapping, omit)))
    }
}

struct ReleaseTypeFmtFilter;
impl Filter for ReleaseTypeFmtFilter {
    fn filter(&self, value: &Value, _args: &HashMap<String, Value>) -> tera::Result<Value> {
        match value.as_str() {
            Some(s) => Ok(Value::String(releasetypefmt(s))),
            None => Err(tera::Error::msg("releasetypefmt expects a string")),
        }
    }
}

struct SortOrderFilter;
impl Filter for SortOrderFilter {
    fn filter(&self, value: &Value, _args: &HashMap<String, Value>) -> tera::Result<Value> {
        match value.as_str() {
            Some(s) => Ok(Value::String(sortorder(s))),
            None => Err(tera::Error::msg("sortorder expects a string")),
        }
    }
}

struct LastNameFilter;
impl Filter for LastNameFilter {
    fn filter(&self, value: &Value, _args: &HashMap<String, Value>) -> tera::Result<Value> {
        match value.as_str() {
            Some(s) => Ok(Value::String(lastname(s))),
            None => Err(tera::Error::msg("lastname expects a string")),
        }
    }
}

struct RjustFilter;
impl Filter for RjustFilter {
    fn filter(&self, value: &Value, args: &HashMap<String, Value>) -> tera::Result<Value> {
        let s = value
            .as_str()
            .ok_or_else(|| tera::Error::msg("rjust expects a string"))?;
        let width = args
            .get("width")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| tera::Error::msg("rjust requires width argument"))?
            as usize;
        let fillchar = args.get("fillchar").and_then(|v| v.as_str()).unwrap_or(" ");

        if s.len() >= width {
            Ok(Value::String(s.to_string()))
        } else {
            let padding = fillchar.repeat(width - s.len());
            Ok(Value::String(format!("{padding}{s}")))
        }
    }
}

struct MapFilter;
impl Filter for MapFilter {
    fn filter(&self, value: &Value, args: &HashMap<String, Value>) -> tera::Result<Value> {
        match value {
            Value::Array(arr) => {
                // Check if we have an attribute argument (for object mapping)
                if let Some(attribute) = args.get("attribute").and_then(|v| v.as_str()) {
                    let mapped: Vec<Value> = arr
                        .iter()
                        .filter_map(|v| v.as_object().and_then(|obj| obj.get(attribute)).cloned())
                        .collect();
                    Ok(Value::Array(mapped))
                } else {
                    // For now, just return the array as-is if no attribute specified
                    // The chained map('sortorder') filter will handle the transformation
                    Ok(value.clone())
                }
            }
            _ => Err(tera::Error::msg("map expects an array")),
        }
    }
}

// Get or create the template engine
pub fn get_environment() -> tera::Result<Tera> {
    let mut engine_guard = TEMPLATE_ENGINE.lock().unwrap();

    if let Some(ref engine) = *engine_guard {
        return Ok(engine.clone());
    }

    let mut tera = Tera::default();

    // Register custom filters
    tera.register_filter("arrayfmt", ArrayFmtFilter);
    tera.register_filter("artistsarrayfmt", ArtistsArrayFmtFilter);
    tera.register_filter("artistsfmt", ArtistsFmtFilter);
    tera.register_filter("releasetypefmt", ReleaseTypeFmtFilter);
    tera.register_filter("sortorder", SortOrderFilter);
    tera.register_filter("lastname", LastNameFilter);
    tera.register_filter("rjust", RjustFilter);
    tera.register_filter("map", MapFilter);

    *engine_guard = Some(tera.clone());
    Ok(tera)
}

// Collapse spacing in template output
pub fn collapse_spacing(x: &str) -> String {
    COLLAPSE_SPACING_REGEX
        .replace_all(x.trim(), " ")
        .to_string()
}

// Rose date type that can be partial
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoseDate {
    pub year: Option<i32>,
    pub month: Option<u32>,
    pub day: Option<u32>,
}

impl RoseDate {
    pub fn new(year: i32, month: Option<u32>, day: Option<u32>) -> Self {
        Self {
            year: Some(year),
            month,
            day,
        }
    }

    pub fn year_only(year: i32) -> Self {
        Self {
            year: Some(year),
            month: None,
            day: None,
        }
    }
}

// Placeholder types for Release and Track - will be implemented in milestone 10/11
#[derive(Debug, Clone)]
pub struct Release {
    pub id: String,
    pub source_path: std::path::PathBuf,
    pub cover_image_path: Option<std::path::PathBuf>,
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
    pub disctotal: u32,
    pub genres: Vec<String>,
    pub parent_genres: Vec<String>,
    pub secondary_genres: Vec<String>,
    pub parent_secondary_genres: Vec<String>,
    pub descriptors: Vec<String>,
    pub labels: Vec<String>,
    pub releaseartists: ArtistMapping,
    pub metahash: String,
}

#[derive(Debug, Clone)]
pub struct Track {
    pub id: String,
    pub source_path: std::path::PathBuf,
    pub source_mtime: String,
    pub tracktitle: String,
    pub tracknumber: String,
    pub tracktotal: u32,
    pub discnumber: String,
    pub duration_seconds: u32,
    pub trackartists: ArtistMapping,
    pub metahash: String,
    pub release: Release,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathContext {
    pub genre: Option<String>,
    pub artist: Option<String>,
    pub label: Option<String>,
    pub descriptor: Option<String>,
    pub collage: Option<String>,
    pub playlist: Option<String>,
}

// Evaluate a release template
pub fn evaluate_release_template(
    template: &PathTemplate,
    release: &Release,
    context: Option<&PathContext>,
    position: Option<&str>,
) -> Result<String, RoseError> {
    let mut tera = get_environment()
        .map_err(|e| RoseError::Expected(RoseExpectedError::Generic(e.to_string())))?;

    // Add the template dynamically
    tera.add_raw_template("release_template", &template.0)
        .map_err(|e| RoseError::Expected(RoseExpectedError::Generic(e.to_string())))?;

    let mut ctx = Context::new();

    // Add context if provided
    if let Some(path_ctx) = context {
        ctx.insert("context", path_ctx);
    }

    // Add position if provided
    if let Some(pos) = position {
        ctx.insert("position", pos);
    }

    // Add release fields
    ctx.insert("added_at", &release.added_at);
    ctx.insert("releasetitle", &release.releasetitle);
    ctx.insert("releasetype", &release.releasetype);
    ctx.insert("releasedate", &release.releasedate);
    ctx.insert("originaldate", &release.originaldate);
    ctx.insert("compositiondate", &release.compositiondate);
    ctx.insert("edition", &release.edition);
    ctx.insert("catalognumber", &release.catalognumber);
    ctx.insert("new", &release.new);
    ctx.insert("disctotal", &release.disctotal);
    ctx.insert("genres", &release.genres);
    ctx.insert("parentgenres", &release.parent_genres);
    ctx.insert("secondarygenres", &release.secondary_genres);
    ctx.insert("parentsecondarygenres", &release.parent_secondary_genres);
    ctx.insert("descriptors", &release.descriptors);
    ctx.insert("labels", &release.labels);
    ctx.insert("releaseartists", &release.releaseartists);

    let rendered = tera
        .render("release_template", &ctx)
        .map_err(|e| RoseError::Expected(RoseExpectedError::Generic(e.to_string())))?;

    Ok(collapse_spacing(&rendered))
}

// Evaluate a track template
pub fn evaluate_track_template(
    template: &PathTemplate,
    track: &Track,
    context: Option<&PathContext>,
    position: Option<&str>,
) -> Result<String, RoseError> {
    let mut tera = get_environment()
        .map_err(|e| RoseError::Expected(RoseExpectedError::Generic(e.to_string())))?;

    // Add the template dynamically
    tera.add_raw_template("track_template", &template.0)
        .map_err(|e| RoseError::Expected(RoseExpectedError::Generic(e.to_string())))?;

    let mut ctx = Context::new();

    // Add context if provided
    if let Some(path_ctx) = context {
        ctx.insert("context", path_ctx);
    }

    // Add position if provided
    if let Some(pos) = position {
        ctx.insert("position", pos);
    }

    // Add track fields
    ctx.insert("added_at", &track.release.added_at);
    ctx.insert("tracktitle", &track.tracktitle);
    ctx.insert("tracknumber", &track.tracknumber);
    ctx.insert("tracktotal", &track.tracktotal);
    ctx.insert("discnumber", &track.discnumber);
    ctx.insert("disctotal", &track.release.disctotal);
    ctx.insert("duration_seconds", &track.duration_seconds);
    ctx.insert("trackartists", &track.trackartists);

    // Add release fields
    ctx.insert("releasetitle", &track.release.releasetitle);
    ctx.insert("releasetype", &track.release.releasetype);
    ctx.insert("releasedate", &track.release.releasedate);
    ctx.insert("originaldate", &track.release.originaldate);
    ctx.insert("compositiondate", &track.release.compositiondate);
    ctx.insert("edition", &track.release.edition);
    ctx.insert("catalognumber", &track.release.catalognumber);
    ctx.insert("new", &track.release.new);
    ctx.insert("genres", &track.release.genres);
    ctx.insert("parentgenres", &track.release.parent_genres);
    ctx.insert("secondarygenres", &track.release.secondary_genres);
    ctx.insert(
        "parentsecondarygenres",
        &track.release.parent_secondary_genres,
    );
    ctx.insert("descriptors", &track.release.descriptors);
    ctx.insert("labels", &track.release.labels);
    ctx.insert("releaseartists", &track.release.releaseartists);

    let rendered = tera
        .render("track_template", &ctx)
        .map_err(|e| RoseError::Expected(RoseExpectedError::Generic(e.to_string())))?;

    // Add file extension
    let ext = track
        .source_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    Ok(format!("{}.{}", collapse_spacing(&rendered), ext))
}

// Get sample music for testing
pub fn get_sample_music(
    music_source_dir: &std::path::Path,
) -> ((Release, Track), (Release, Track), (Release, Track)) {
    let kimlip_rls = Release {
        id: "018b268e-ff1e-7a0c-9ac8-7bbb282761f2".to_string(),
        source_path: music_source_dir.join("LOONA - 2017. Kim Lip"),
        cover_image_path: None,
        added_at: "2023-04-20:23:45Z".to_string(),
        datafile_mtime: "999".to_string(),
        releasetitle: "Kim Lip".to_string(),
        releasetype: "single".to_string(),
        releasedate: Some(RoseDate::new(2017, Some(5), Some(23))),
        originaldate: Some(RoseDate::new(2017, Some(5), Some(23))),
        compositiondate: None,
        edition: None,
        catalognumber: Some("CMCC11088".to_string()),
        new: true,
        disctotal: 1,
        genres: vec![
            "K-Pop".to_string(),
            "Dance-Pop".to_string(),
            "Contemporary R&B".to_string(),
        ],
        parent_genres: vec!["Pop".to_string(), "R&B".to_string()],
        secondary_genres: vec![
            "Synth Funk".to_string(),
            "Synthpop".to_string(),
            "Future Bass".to_string(),
        ],
        parent_secondary_genres: vec!["Funk".to_string(), "Pop".to_string()],
        descriptors: vec![
            "Female Vocalist".to_string(),
            "Mellow".to_string(),
            "Sensual".to_string(),
            "Ethereal".to_string(),
            "Love".to_string(),
            "Lush".to_string(),
            "Romantic".to_string(),
            "Warm".to_string(),
            "Melodic".to_string(),
            "Passionate".to_string(),
            "Nocturnal".to_string(),
            "Summer".to_string(),
        ],
        labels: vec!["BlockBerryCreative".to_string()],
        releaseartists: ArtistMapping {
            main: vec![Artist::new("Kim Lip".to_string())],
            ..ArtistMapping::new()
        },
        metahash: "0".to_string(),
    };

    let bts_rls = Release {
        id: "018b6021-f1e5-7d4b-b796-440fbbea3b13".to_string(),
        source_path: music_source_dir.join("BTS - 2016. Young Forever (花樣年華)"),
        cover_image_path: None,
        added_at: "2023-06-09:23:45Z".to_string(),
        datafile_mtime: "999".to_string(),
        releasetitle: "Young Forever (花樣年華)".to_string(),
        releasetype: "album".to_string(),
        releasedate: Some(RoseDate::year_only(2016)),
        originaldate: Some(RoseDate::year_only(2016)),
        compositiondate: None,
        edition: Some("Deluxe".to_string()),
        catalognumber: Some("L200001238".to_string()),
        new: false,
        disctotal: 2,
        genres: vec!["K-Pop".to_string()],
        parent_genres: vec!["Pop".to_string()],
        secondary_genres: vec!["Pop Rap".to_string(), "Electropop".to_string()],
        parent_secondary_genres: vec!["Hip Hop".to_string(), "Electronic".to_string()],
        descriptors: vec![
            "Autumn".to_string(),
            "Passionate".to_string(),
            "Melodic".to_string(),
            "Romantic".to_string(),
            "Eclectic".to_string(),
            "Melancholic".to_string(),
            "Male Vocalist".to_string(),
            "Sentimental".to_string(),
            "Uplifting".to_string(),
            "Breakup".to_string(),
            "Love".to_string(),
            "Anthemic".to_string(),
            "Lush".to_string(),
            "Bittersweet".to_string(),
            "Spring".to_string(),
        ],
        labels: vec!["BIGHIT".to_string()],
        releaseartists: ArtistMapping {
            main: vec![Artist::new("BTS".to_string())],
            ..ArtistMapping::new()
        },
        metahash: "0".to_string(),
    };

    let debussy_rls = Release {
        id: "018b268e-de0c-7cb2-8ffa-bcc2083c94e6".to_string(),
        source_path: music_source_dir.join(
            "Debussy - 1907. Images performed by Cleveland Orchestra under Pierre Boulez (1992)",
        ),
        cover_image_path: None,
        added_at: "2023-09-06:23:45Z".to_string(),
        datafile_mtime: "999".to_string(),
        releasetitle: "Images".to_string(),
        releasetype: "album".to_string(),
        releasedate: Some(RoseDate::year_only(1992)),
        originaldate: Some(RoseDate::year_only(1991)),
        compositiondate: Some(RoseDate::year_only(1907)),
        edition: None,
        catalognumber: Some("435-766 2".to_string()),
        new: false,
        disctotal: 2,
        genres: vec!["Impressionism, Orchestral".to_string()],
        parent_genres: vec!["Modern Classical".to_string()],
        secondary_genres: vec!["Tone Poem".to_string()],
        parent_secondary_genres: vec!["Orchestral Music".to_string()],
        descriptors: vec!["Orchestral Music".to_string()],
        labels: vec!["Deustche Grammophon".to_string()],
        releaseartists: ArtistMapping {
            main: vec![Artist::new("Cleveland Orchestra".to_string())],
            composer: vec![Artist::new("Claude Debussy".to_string())],
            conductor: vec![Artist::new("Pierre Boulez".to_string())],
            ..ArtistMapping::new()
        },
        metahash: "0".to_string(),
    };

    let kimlip_trk = Track {
        id: "018b268e-ff1e-7a0c-9ac8-7bbb282761f1".to_string(),
        source_path: music_source_dir
            .join("LOONA - 2017. Kim Lip")
            .join("01. Eclipse.opus"),
        source_mtime: "999".to_string(),
        tracktitle: "Eclipse".to_string(),
        tracknumber: "1".to_string(),
        tracktotal: 2,
        discnumber: "1".to_string(),
        duration_seconds: 230,
        trackartists: ArtistMapping {
            main: vec![Artist::new("Kim Lip".to_string())],
            ..ArtistMapping::new()
        },
        metahash: "0".to_string(),
        release: kimlip_rls.clone(),
    };

    let bts_trk = Track {
        id: "018b6021-f1e5-7d4b-b796-440fbbea3b15".to_string(),
        source_path: music_source_dir
            .join("BTS - 2016. Young Forever (花樣年華)")
            .join("02-05. House of Cards.opus"),
        source_mtime: "999".to_string(),
        tracktitle: "House of Cards".to_string(),
        tracknumber: "5".to_string(),
        tracktotal: 8,
        discnumber: "2".to_string(),
        duration_seconds: 226,
        trackartists: ArtistMapping {
            main: vec![Artist::new("BTS".to_string())],
            ..ArtistMapping::new()
        },
        metahash: "0".to_string(),
        release: bts_rls.clone(),
    };

    let debussy_trk = Track {
        id: "018b6514-6e65-78cc-94a5-fdb17418f090".to_string(),
        source_path: music_source_dir
            .join("Debussy - 1907. Images performed by Cleveland Orchestra under Pierre Boulez (1992)")
            .join("01. Gigues: Modéré.opus"),
        source_mtime: "999".to_string(),
        tracktitle: "Gigues: Modéré".to_string(),
        tracknumber: "1".to_string(),
        tracktotal: 6,
        discnumber: "1".to_string(),
        duration_seconds: 444,
        trackartists: ArtistMapping {
            main: vec![Artist::new("Cleveland Orchestra".to_string())],
            composer: vec![Artist::new("Claude Debussy".to_string())],
            conductor: vec![Artist::new("Pierre Boulez".to_string())],
            ..ArtistMapping::new()
        },
        metahash: "0".to_string(),
        release: debussy_rls.clone(),
    };

    (
        (kimlip_rls, kimlip_trk),
        (bts_rls, bts_trk),
        (debussy_rls, debussy_trk),
    )
}
