/// The templates module provides the ability to customize paths in the source directory and virtual
/// filesystem as Jinja templates. Users can specify different templates for different views in the
/// virtual filesystem.
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Mutex;
use tera::{Context, Tera};
use thiserror::Error;

use crate::common::{ArtistMapping, RoseDate};

#[derive(Error, Debug)]
pub struct InvalidPathTemplateError {
    pub key: String,
    message: String,
}

impl fmt::Display for InvalidPathTemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathTemplate {
    text: String,
}

impl PathTemplate {
    pub fn new(text: String) -> Self {
        PathTemplate { text }
    }

    /// Get the template text
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathTemplateTriad {
    pub release: PathTemplate,
    pub track: PathTemplate,
    pub all_tracks: PathTemplate,
}

#[derive(Debug, Clone)]
pub struct PathTemplateConfig {
    pub source: PathTemplateTriad,
    pub releases: PathTemplateTriad,
    pub releases_new: PathTemplateTriad,
    pub releases_added_on: PathTemplateTriad,
    pub releases_released_on: PathTemplateTriad,
    pub artists: PathTemplateTriad,
    pub genres: PathTemplateTriad,
    pub descriptors: PathTemplateTriad,
    pub labels: PathTemplateTriad,
    pub loose_tracks: PathTemplateTriad,
    pub collages: PathTemplateTriad,
    pub playlists: PathTemplate,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PathContext {
    pub genre: Option<String>,
    pub artist: Option<String>,
    pub label: Option<String>,
    pub descriptor: Option<String>,
    pub collage: Option<String>,
    pub playlist: Option<String>,
}

impl PathTemplateConfig {
    pub fn with_defaults(default_triad: PathTemplateTriad) -> Self {
        let releases_added_on = PathTemplateTriad {
            release: PathTemplate::new(format!("[{{{{ added_at[:10] }}}}] {}", default_triad.release.text)),
            track: default_triad.track.clone(),
            all_tracks: default_triad.all_tracks.clone(),
        };

        let releases_released_on = PathTemplateTriad {
            release: PathTemplate::new(format!("[{{{{ originaldate or releasedate or '0000-00-00' }}}}] {}", default_triad.release.text)),
            track: default_triad.track.clone(),
            all_tracks: default_triad.all_tracks.clone(),
        };

        let collages = PathTemplateTriad {
            release: PathTemplate::new(format!("{{{{ position }}}}. {}", default_triad.release.text)),
            track: default_triad.track.clone(),
            all_tracks: default_triad.all_tracks.clone(),
        };

        PathTemplateConfig {
            source: default_triad.clone(),
            releases: default_triad.clone(),
            releases_new: default_triad.clone(),
            releases_added_on,
            releases_released_on,
            artists: default_triad.clone(),
            genres: default_triad.clone(),
            descriptors: default_triad.clone(),
            labels: default_triad.clone(),
            loose_tracks: default_triad.clone(),
            collages,
            playlists: PathTemplate::new(
                r#"
{{ position }}.
{{ trackartists | artistsfmt }} -
{{ tracktitle }}
"#
                .to_string(),
            ),
        }
    }

    pub fn parse(&self) -> Result<(), InvalidPathTemplateError> {
        // Attempt to parse all the templates into Tera templates (which will be cached).
        // This will raise an InvalidPathTemplateError if a template is invalid.
        let env = get_environment();
        let mut tera = env.lock().unwrap();

        macro_rules! validate_template {
            ($key_str:expr, $template:expr) => {
                tera.render_str(&$template.text, &Context::new()).map_err(|e| InvalidPathTemplateError {
                    key: $key_str.to_string(),
                    message: format!("Failed to compile template: {}", e),
                })?;
            };
        }

        validate_template!("source.release", self.source.release);
        validate_template!("source.track", self.source.track);
        validate_template!("source.all_tracks", self.source.all_tracks);
        validate_template!("releases.release", self.releases.release);
        validate_template!("releases.track", self.releases.track);
        validate_template!("releases.all_tracks", self.releases.all_tracks);
        validate_template!("releases_new.release", self.releases_new.release);
        validate_template!("releases_new.track", self.releases_new.track);
        validate_template!("releases_new.all_tracks", self.releases_new.all_tracks);
        validate_template!("releases_added_on.release", self.releases_added_on.release);
        validate_template!("releases_added_on.track", self.releases_added_on.track);
        validate_template!("releases_added_on.all_tracks", self.releases_added_on.all_tracks);
        validate_template!("releases_released_on.release", self.releases_released_on.release);
        validate_template!("releases_released_on.track", self.releases_released_on.track);
        validate_template!("releases_released_on.all_tracks", self.releases_released_on.all_tracks);
        validate_template!("artists.release", self.artists.release);
        validate_template!("artists.track", self.artists.track);
        validate_template!("artists.all_tracks", self.artists.all_tracks);
        validate_template!("genres.release", self.genres.release);
        validate_template!("genres.track", self.genres.track);
        validate_template!("genres.all_tracks", self.genres.all_tracks);
        validate_template!("descriptors.release", self.descriptors.release);
        validate_template!("descriptors.track", self.descriptors.track);
        validate_template!("descriptors.all_tracks", self.descriptors.all_tracks);
        validate_template!("labels.release", self.labels.release);
        validate_template!("labels.track", self.labels.track);
        validate_template!("labels.all_tracks", self.labels.all_tracks);
        validate_template!("loose_tracks.release", self.loose_tracks.release);
        validate_template!("loose_tracks.track", self.loose_tracks.track);
        validate_template!("loose_tracks.all_tracks", self.loose_tracks.all_tracks);
        validate_template!("collages.release", self.collages.release);
        validate_template!("collages.track", self.collages.track);
        validate_template!("collages.all_tracks", self.collages.all_tracks);
        validate_template!("playlists", self.playlists);

        Ok(())
    }
}

// Format mapping for release types
static RELEASE_TYPE_FORMATTER: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("album", "Album");
    m.insert("single", "Single");
    m.insert("ep", "EP");
    m.insert("compilation", "Compilation");
    m.insert("anthology", "Anthology");
    m.insert("soundtrack", "Soundtrack");
    m.insert("live", "Live");
    m.insert("remix", "Remix");
    m.insert("djmix", "DJ-Mix");
    m.insert("mixtape", "Mixtape");
    m.insert("other", "Other");
    m.insert("demo", "Demo");
    m.insert("unknown", "Unknown");
    m
});

pub static DEFAULT_RELEASE_TEMPLATE: Lazy<PathTemplate> = Lazy::new(|| {
    PathTemplate::new(
        r#"{{ releaseartists | artistsfmt }} -
{% if releasedate %}{{ releasedate.year }}.{% endif %}
{{ releasetitle }}
{% if releasetype == "single" %}- {{ releasetype | releasetypefmt }}{% endif %}
{% if new %}[NEW]{% endif %}"#
            .trim()
            .to_string(),
    )
});

pub static DEFAULT_TRACK_TEMPLATE: Lazy<PathTemplate> = Lazy::new(|| {
    PathTemplate::new(
        r#"{% if disctotal > 1 %}{{ discnumber | zerofill(width=2) }}-{% endif %}{{ tracknumber | zerofill(width=2) }}.
{{ tracktitle }}
{% if trackartists.guest %}(feat. {{ trackartists.guest | artistsarrayfmt }}){% endif %}"#
            .trim()
            .to_string(),
    )
});

pub static DEFAULT_ALL_TRACKS_TEMPLATE: Lazy<PathTemplate> = Lazy::new(|| {
    PathTemplate::new(
        r#"{{ trackartists | artistsfmt }} -
{% if releasedate %}{{ releasedate.year }}.{% endif %}
{{ releasetitle }} -
{{ tracktitle }}"#
            .trim()
            .to_string(),
    )
});

pub static DEFAULT_TEMPLATE_PAIR: Lazy<PathTemplateTriad> = Lazy::new(|| PathTemplateTriad {
    release: DEFAULT_RELEASE_TEMPLATE.clone(),
    track: DEFAULT_TRACK_TEMPLATE.clone(),
    all_tracks: DEFAULT_ALL_TRACKS_TEMPLATE.clone(),
});

// Template filter functions

fn releasetypefmt(value: &tera::Value, _: &HashMap<String, tera::Value>) -> tera::Result<tera::Value> {
    let s = value.as_str().ok_or_else(|| tera::Error::msg("releasetypefmt: expected string"))?;
    let formatted = RELEASE_TYPE_FORMATTER.get(s).copied().unwrap_or_else(|| {
        // Title case conversion for unknown types
        let mut chars = s.chars();
        match chars.next() {
            None => "",
            Some(first) => {
                let mut result = first.to_uppercase().collect::<String>();
                result.push_str(&chars.as_str().to_lowercase());
                Box::leak(result.into_boxed_str())
            }
        }
    });
    Ok(tera::Value::String(formatted.to_string()))
}

fn arrayfmt(value: &tera::Value, _: &HashMap<String, tera::Value>) -> tera::Result<tera::Value> {
    let array = value.as_array().ok_or_else(|| tera::Error::msg("arrayfmt: expected array"))?;
    let strs: Vec<String> = array.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();

    let result = match strs.len() {
        0 => String::new(),
        1 => strs[0].clone(),
        _ => format!("{} & {}", strs[..strs.len() - 1].join(", "), strs[strs.len() - 1]),
    };
    Ok(tera::Value::String(result))
}

fn artistsarrayfmt(value: &tera::Value, _: &HashMap<String, tera::Value>) -> tera::Result<tera::Value> {
    let array = value.as_array().ok_or_else(|| tera::Error::msg("artistsarrayfmt: expected array"))?;
    let names: Vec<String> = array
        .iter()
        .filter_map(|v| {
            v.as_object().and_then(|obj| {
                if obj.get("alias").and_then(|v| v.as_bool()).unwrap_or(false) {
                    None
                } else {
                    obj.get("name").and_then(|v| v.as_str()).map(|s| s.to_string())
                }
            })
        })
        .collect();

    let result = if names.len() <= 3 {
        match names.len() {
            0 => String::new(),
            1 => names[0].clone(),
            _ => format!("{} & {}", names[..names.len() - 1].join(", "), names[names.len() - 1]),
        }
    } else {
        format!("{} et al.", names[0])
    };
    Ok(tera::Value::String(result))
}

fn artistsfmt(value: &tera::Value, args: &HashMap<String, tera::Value>) -> tera::Result<tera::Value> {
    let mapping = value.as_object().ok_or_else(|| tera::Error::msg("artistsfmt: expected object"))?;

    let omit = args.get("omit").and_then(|v| v.as_array()).map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>()).unwrap_or_default();

    let format_artists = |key: &str| -> String {
        mapping.get(key).and_then(|v| artistsarrayfmt(v, &HashMap::new()).ok()).and_then(|v| v.as_str().map(|s| s.to_string())).unwrap_or_default()
    };

    let mut result = format_artists("main");

    if !omit.contains(&"djmixer") && !format_artists("djmixer").is_empty() {
        result = format!("{} pres. {}", format_artists("djmixer"), result);
    } else if !omit.contains(&"composer") && !format_artists("composer").is_empty() {
        result = format!("{} performed by {}", format_artists("composer"), result);
    }

    if !omit.contains(&"conductor") && !format_artists("conductor").is_empty() {
        result = format!("{} under {}", result, format_artists("conductor"));
    }

    if !omit.contains(&"guest") && !format_artists("guest").is_empty() {
        result = format!("{} (feat. {})", result, format_artists("guest"));
    }

    if !omit.contains(&"producer") && !format_artists("producer").is_empty() {
        result = format!("{} (prod. {})", result, format_artists("producer"));
    }

    if result.is_empty() {
        result = "Unknown Artists".to_string();
    }

    Ok(tera::Value::String(result))
}

fn sortorder(value: &tera::Value, _: &HashMap<String, tera::Value>) -> tera::Result<tera::Value> {
    let s = value.as_str().ok_or_else(|| tera::Error::msg("sortorder: expected string"))?;
    let result = match s.rsplit_once(' ') {
        Some((first, last)) => format!("{last}, {first}"),
        None => s.to_string(),
    };
    Ok(tera::Value::String(result))
}

fn lastname(value: &tera::Value, _: &HashMap<String, tera::Value>) -> tera::Result<tera::Value> {
    let s = value.as_str().ok_or_else(|| tera::Error::msg("lastname: expected string"))?;
    let result = match s.rsplit_once(' ') {
        Some((_, last)) => last.to_string(),
        None => s.to_string(),
    };
    Ok(tera::Value::String(result))
}

fn zerofill(value: &tera::Value, args: &HashMap<String, tera::Value>) -> tera::Result<tera::Value> {
    // Handle both string and number inputs
    let s = if let Some(s) = value.as_str() {
        s.to_string()
    } else if let Some(n) = value.as_u64() {
        n.to_string()
    } else if let Some(n) = value.as_i64() {
        n.to_string()
    } else {
        return Err(tera::Error::msg("zerofill: expected string or number"));
    };

    // In Tera, the first positional argument is passed with key "0"
    // Also check for named "width" parameter for compatibility
    let width = args.get("width").or_else(|| args.get("0")).and_then(|v| v.as_u64()).unwrap_or(2) as usize;

    let result = format!("{s:0>width$}");
    Ok(tera::Value::String(result))
}

fn composersfmt(value: &tera::Value, _: &HashMap<String, tera::Value>) -> tera::Result<tera::Value> {
    let array = value.as_array().ok_or_else(|| tera::Error::msg("composersfmt: expected array"))?;
    let names: Vec<String> = array
        .iter()
        .filter_map(|v| {
            v.as_object().and_then(|obj| {
                obj.get("name").and_then(|v| v.as_str()).map(|s| {
                    // Apply sortorder transformation
                    match s.rsplit_once(' ') {
                        Some((first, last)) => format!("{last}, {first}"),
                        None => s.to_string(),
                    }
                })
            })
        })
        .collect();

    let result = match names.len() {
        0 => String::new(),
        1 => names[0].clone(),
        _ => format!("{} & {}", names[..names.len() - 1].join(", "), names[names.len() - 1]),
    };
    Ok(tera::Value::String(result))
}

// Global Tera environment with lazy initialization
static ENVIRONMENT: Lazy<Mutex<Tera>> = Lazy::new(|| {
    let mut tera = Tera::default();
    tera.register_filter("arrayfmt", arrayfmt);
    tera.register_filter("artistsarrayfmt", artistsarrayfmt);
    tera.register_filter("artistsfmt", artistsfmt);
    tera.register_filter("releasetypefmt", releasetypefmt);
    tera.register_filter("sortorder", sortorder);
    tera.register_filter("lastname", lastname);
    tera.register_filter("zerofill", zerofill);
    tera.register_filter("composersfmt", composersfmt);
    Mutex::new(tera)
});

fn get_environment() -> &'static Mutex<Tera> {
    &ENVIRONMENT
}

// Placeholder structs for Release and Track - these will be defined in the cache module
#[derive(Debug, Clone)]
pub struct Release {
    pub id: String,
    pub source_path: PathBuf,
    pub cover_image_path: Option<PathBuf>,
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
    pub source_path: PathBuf,
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

// Template evaluation functions

pub fn evaluate_release_template(template: &PathTemplate, release: &Release, context: Option<&PathContext>, position: Option<&str>) -> String {
    let env = get_environment();
    let mut tera = env.lock().unwrap();
    let ctx = calc_release_variables(release, position, context);

    match tera.render_str(&template.text, &ctx) {
        Ok(rendered) => {
            // Collapse whitespace - all newlines and multi-spaces are replaced with a single space
            let spacing_regex = Regex::new(r"\s+").unwrap();
            spacing_regex.replace_all(&rendered, " ").trim().to_string()
        }
        Err(e) => {
            tracing::error!("Failed to render release template: {}", e);
            String::new()
        }
    }
}

pub fn evaluate_track_template(template: &PathTemplate, track: &Track, context: Option<&PathContext>, position: Option<&str>) -> String {
    let env = get_environment();
    let mut tera = env.lock().unwrap();
    let ctx = calc_track_variables(track, position, context);

    let mut result = match tera.render_str(&template.text, &ctx) {
        Ok(rendered) => {
            // Collapse whitespace - all newlines and multi-spaces are replaced with a single space
            let spacing_regex = Regex::new(r"\s+").unwrap();
            spacing_regex.replace_all(&rendered, " ").trim().to_string()
        }
        Err(e) => {
            tracing::error!("Failed to render track template: {}", e);
            tracing::error!("Template text: {}", template.text);
            tracing::error!("Context: {:?}", ctx);
            String::new()
        }
    };

    // Append file extension
    if let Some(extension) = track.source_path.extension() {
        result.push('.');
        result.push_str(&extension.to_string_lossy());
    }

    result
}

fn calc_release_variables(release: &Release, position: Option<&str>, context: Option<&PathContext>) -> Context {
    let mut ctx = Context::new();

    // Add context if provided
    if let Some(context) = context {
        ctx.insert("context", context);
    }

    // Basic release fields
    ctx.insert("added_at", &release.added_at);
    ctx.insert("releasetitle", &release.releasetitle);
    ctx.insert("releasetype", &release.releasetype);
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

    // Date fields with special handling - serialize as both object and string
    if let Some(date) = &release.releasedate {
        let mut date_obj = tera::Map::new();
        if let Some(year) = date.year {
            date_obj.insert("year".to_string(), tera::Value::Number(year.into()));
        }
        if let Some(month) = date.month {
            date_obj.insert("month".to_string(), tera::Value::Number(month.into()));
        }
        if let Some(day) = date.day {
            date_obj.insert("day".to_string(), tera::Value::Number(day.into()));
        }
        // Also add string representation for direct display
        date_obj.insert("__str__".to_string(), tera::Value::String(date.to_string()));
        ctx.insert("releasedate", &date_obj);
    }
    if let Some(date) = &release.originaldate {
        let mut date_obj = tera::Map::new();
        if let Some(year) = date.year {
            date_obj.insert("year".to_string(), tera::Value::Number(year.into()));
        }
        if let Some(month) = date.month {
            date_obj.insert("month".to_string(), tera::Value::Number(month.into()));
        }
        if let Some(day) = date.day {
            date_obj.insert("day".to_string(), tera::Value::Number(day.into()));
        }
        date_obj.insert("__str__".to_string(), tera::Value::String(date.to_string()));
        ctx.insert("originaldate", &date_obj);
    }
    if let Some(date) = &release.compositiondate {
        let mut date_obj = tera::Map::new();
        if let Some(year) = date.year {
            date_obj.insert("year".to_string(), tera::Value::Number(year.into()));
        }
        if let Some(month) = date.month {
            date_obj.insert("month".to_string(), tera::Value::Number(month.into()));
        }
        if let Some(day) = date.day {
            date_obj.insert("day".to_string(), tera::Value::Number(day.into()));
        }
        date_obj.insert("__str__".to_string(), tera::Value::String(date.to_string()));
        ctx.insert("compositiondate", &date_obj);
    }

    // Position if provided
    if let Some(pos) = position {
        ctx.insert("position", pos);
    }

    ctx
}

fn calc_track_variables(track: &Track, position: Option<&str>, context: Option<&PathContext>) -> Context {
    let mut ctx = Context::new();

    // Add context if provided
    if let Some(context) = context {
        ctx.insert("context", context);
    }

    // Track-specific fields
    ctx.insert("tracktitle", &track.tracktitle);
    ctx.insert("tracknumber", &track.tracknumber);
    ctx.insert("tracktotal", &track.tracktotal);
    ctx.insert("discnumber", &track.discnumber);
    ctx.insert("duration_seconds", &track.duration_seconds);
    ctx.insert("trackartists", &track.trackartists);

    // Release fields from the track's release
    ctx.insert("added_at", &track.release.added_at);
    ctx.insert("releasetitle", &track.release.releasetitle);
    ctx.insert("releasetype", &track.release.releasetype);
    ctx.insert("edition", &track.release.edition);
    ctx.insert("catalognumber", &track.release.catalognumber);
    ctx.insert("new", &track.release.new);
    ctx.insert("disctotal", &track.release.disctotal);
    ctx.insert("genres", &track.release.genres);
    ctx.insert("parentgenres", &track.release.parent_genres);
    ctx.insert("secondarygenres", &track.release.secondary_genres);
    ctx.insert("parentsecondarygenres", &track.release.parent_secondary_genres);
    ctx.insert("descriptors", &track.release.descriptors);
    ctx.insert("labels", &track.release.labels);
    ctx.insert("releaseartists", &track.release.releaseartists);

    // Date fields with special handling - serialize as both object and string
    if let Some(date) = &track.release.releasedate {
        let mut date_obj = tera::Map::new();
        if let Some(year) = date.year {
            date_obj.insert("year".to_string(), tera::Value::Number(year.into()));
        }
        if let Some(month) = date.month {
            date_obj.insert("month".to_string(), tera::Value::Number(month.into()));
        }
        if let Some(day) = date.day {
            date_obj.insert("day".to_string(), tera::Value::Number(day.into()));
        }
        date_obj.insert("__str__".to_string(), tera::Value::String(date.to_string()));
        ctx.insert("releasedate", &date_obj);
    }
    if let Some(date) = &track.release.originaldate {
        let mut date_obj = tera::Map::new();
        if let Some(year) = date.year {
            date_obj.insert("year".to_string(), tera::Value::Number(year.into()));
        }
        if let Some(month) = date.month {
            date_obj.insert("month".to_string(), tera::Value::Number(month.into()));
        }
        if let Some(day) = date.day {
            date_obj.insert("day".to_string(), tera::Value::Number(day.into()));
        }
        date_obj.insert("__str__".to_string(), tera::Value::String(date.to_string()));
        ctx.insert("originaldate", &date_obj);
    }
    if let Some(date) = &track.release.compositiondate {
        let mut date_obj = tera::Map::new();
        if let Some(year) = date.year {
            date_obj.insert("year".to_string(), tera::Value::Number(year.into()));
        }
        if let Some(month) = date.month {
            date_obj.insert("month".to_string(), tera::Value::Number(month.into()));
        }
        if let Some(day) = date.day {
            date_obj.insert("day".to_string(), tera::Value::Number(day.into()));
        }
        date_obj.insert("__str__".to_string(), tera::Value::String(date.to_string()));
        ctx.insert("compositiondate", &date_obj);
    }

    // Position if provided
    if let Some(pos) = position {
        ctx.insert("position", pos);
    }

    ctx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::{Artist, ArtistMapping, RoseDate};

    fn empty_cached_release() -> Release {
        Release {
            id: String::new(),
            source_path: PathBuf::new(),
            cover_image_path: None,
            added_at: "0000-01-01T00:00:00Z".to_string(),
            datafile_mtime: "999".to_string(),
            releasetitle: String::new(),
            releasetype: "unknown".to_string(),
            releasedate: None,
            originaldate: None,
            compositiondate: None,
            edition: None,
            catalognumber: None,
            new: false,
            disctotal: 1,
            genres: Vec::new(),
            parent_genres: Vec::new(),
            secondary_genres: Vec::new(),
            parent_secondary_genres: Vec::new(),
            descriptors: Vec::new(),
            labels: Vec::new(),
            releaseartists: ArtistMapping::default(),
            metahash: "0".to_string(),
        }
    }

    fn empty_cached_track() -> Track {
        Track {
            id: String::new(),
            source_path: PathBuf::from("hi.m4a"),
            source_mtime: String::new(),
            tracktitle: String::new(),
            tracknumber: String::new(),
            tracktotal: 1,
            discnumber: String::new(),
            duration_seconds: 0,
            trackartists: ArtistMapping::default(),
            metahash: "0".to_string(),
            release: empty_cached_release(),
        }
    }

    #[test]
    fn test_default_templates() {
        // Initialize tracing for tests
        let _ = tracing_subscriber::fmt::try_init();

        let templates = PathTemplateConfig::with_defaults(DEFAULT_TEMPLATE_PAIR.clone());

        let mut release = empty_cached_release();
        release.releasetitle = "Title".to_string();
        release.releasedate = Some(RoseDate::new(Some(2023), None, None));
        release.releaseartists = ArtistMapping {
            main: vec![Artist::new("A1"), Artist::new("A2"), Artist::new("A3")],
            guest: vec![Artist::new("BB")],
            producer: vec![Artist::new("PP")],
            ..Default::default()
        };
        release.releasetype = "single".to_string();

        assert_eq!(evaluate_release_template(&templates.source.release, &release, None, None), "A1, A2 & A3 (feat. BB) (prod. PP) - 2023. Title - Single");
        assert_eq!(
            evaluate_release_template(&templates.collages.release, &release, None, Some("4")),
            "4. A1, A2 & A3 (feat. BB) (prod. PP) - 2023. Title - Single"
        );

        let mut release = empty_cached_release();
        release.releasetitle = "Title".to_string();
        assert_eq!(evaluate_release_template(&templates.source.release, &release, None, None), "Unknown Artists - Title");
        assert_eq!(evaluate_release_template(&templates.collages.release, &release, None, Some("4")), "4. Unknown Artists - Title");

        let mut track = empty_cached_track();
        track.tracknumber = "2".to_string();
        track.tracktitle = "Trick".to_string();
        assert_eq!(evaluate_track_template(&templates.source.track, &track, None, None), "02. Trick.m4a");
        assert_eq!(evaluate_track_template(&templates.playlists, &track, None, Some("4")), "4. Unknown Artists - Trick.m4a");

        let mut track = empty_cached_track();
        track.release.disctotal = 2;
        track.discnumber = "4".to_string();
        track.tracknumber = "2".to_string();
        track.tracktitle = "Trick".to_string();
        track.trackartists = ArtistMapping {
            main: vec![Artist::new("Main")],
            guest: vec![Artist::new("Hi"), Artist::new("High"), Artist::new("Hye")],
            ..Default::default()
        };
        assert_eq!(evaluate_track_template(&templates.source.track, &track, None, None), "04-02. Trick (feat. Hi, High & Hye).m4a");
        assert_eq!(evaluate_track_template(&templates.playlists, &track, None, Some("4")), "4. Main (feat. Hi, High & Hye) - Trick.m4a");
    }

    #[test]
    fn test_simple_template() {
        let _ = tracing_subscriber::fmt::try_init();

        // Test simple template without any filters
        let template = PathTemplate::new("{{ tracknumber }}. {{ tracktitle }}".to_string());
        let mut track = empty_cached_track();
        track.tracknumber = "2".to_string();
        track.tracktitle = "Trick".to_string();

        let result = evaluate_track_template(&template, &track, None, None);
        assert_eq!(result, "2. Trick.m4a");
    }

    #[test]
    fn test_zerofill_direct() {
        let _ = tracing_subscriber::fmt::try_init();

        // Test zerofill filter directly
        let env = get_environment();
        let mut tera = env.lock().unwrap();

        let mut ctx = Context::new();
        ctx.insert("tracknumber", &"2");

        // Test with zerofill - try different syntaxes
        match tera.render_str("{{ tracknumber | zerofill(width=2) }}", &ctx) {
            Ok(result) => println!("zerofill with width=2 result: '{result}'"),
            Err(e) => println!("zerofill with width=2 error: {e}"),
        }

        // Test without arguments (should use default width of 2)
        match tera.render_str("{{ tracknumber | zerofill }}", &ctx) {
            Ok(result) => println!("zerofill no args result: '{result}'"),
            Err(e) => println!("zerofill no args error: {e}"),
        }

        // Test without zerofill
        match tera.render_str("{{ tracknumber }}", &ctx) {
            Ok(result) => println!("no filter result: '{result}'"),
            Err(e) => println!("no filter error: {e}"),
        }
    }

    #[test]
    fn test_classical() {
        let template = PathTemplate::new(
            r#"
        {% if new %}{{ '{N}' }}{% endif %}
        {{ releaseartists.composer | composersfmt }} -
        {% if compositiondate %}{{ compositiondate.year }}.{% endif %}
        {{ releasetitle }}
        performed by {{ releaseartists | artistsfmt(omit=["composer"]) }}
        {% if releasedate %}({{ releasedate.year }}){% endif %}
        "#
            .to_string(),
        );

        let mut release = empty_cached_release();
        release.releasetitle = "Images".to_string();
        release.releasetype = "album".to_string();
        release.releasedate = Some(RoseDate::new(Some(1992), None, None));
        release.compositiondate = Some(RoseDate::new(Some(1907), None, None));
        release.releaseartists = ArtistMapping {
            main: vec![Artist::new("Cleveland Orchestra")],
            composer: vec![Artist::new("Claude Debussy")],
            conductor: vec![Artist::new("Pierre Boulez")],
            ..Default::default()
        };

        assert_eq!(
            evaluate_release_template(&template, &release, None, None),
            "Debussy, Claude - 1907. Images performed by Cleveland Orchestra under Pierre Boulez (1992)"
        );
    }
}
