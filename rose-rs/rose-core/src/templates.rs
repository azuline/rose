//! Path template rendering for release directories and track files.
//!
//! Templates use a Jinja-like syntax (via minijinja) and are user-configurable.
//! This module provides custom filters, template compilation, evaluation, sample
//! music data, and the `PathTemplateConfig` type with all default templates.

use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use minijinja::value::Kwargs;
use minijinja::{Environment, Error as MiniJinjaError, ErrorKind, Value};
use regex::Regex;

use crate::audiotags::RoseDate;
use crate::common::{Artist, ArtistMapping, RoseError};

// ---------------------------------------------------------------------------
// Release type formatter map
// ---------------------------------------------------------------------------

static RELEASE_TYPE_FORMATTER: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| {
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

// ---------------------------------------------------------------------------
// Custom filters
// ---------------------------------------------------------------------------

/// Format a release type string to its display form.
/// e.g. "djmix" -> "DJ-Mix", "ep" -> "EP"
fn filter_releasetypefmt(value: &str) -> String {
    RELEASE_TYPE_FORMATTER
        .get(value)
        .map(|s| s.to_string())
        .unwrap_or_else(|| titlecase(value))
}

/// Simple titlecase: capitalize first letter.
fn titlecase(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

/// Format a sequence of strings as "a, b & c".
fn filter_arrayfmt(value: Value) -> Result<String, MiniJinjaError> {
    let items: Vec<String> = value
        .try_iter()
        .map_err(|_| {
            MiniJinjaError::new(ErrorKind::InvalidOperation, "arrayfmt expects a sequence")
        })?
        .map(|v| v.to_string())
        .collect();
    Ok(arrayfmt_strings(&items))
}

fn arrayfmt_strings(items: &[String]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        _ => {
            let (last, rest) = items.split_last().unwrap();
            format!("{} & {}", rest.join(", "), last)
        }
    }
}

/// Format a Vec<Artist> (as minijinja Value) — exclude aliases, <=3 show
/// "A, B & C", >3 show "A et al."
fn filter_artistsarrayfmt(value: Value) -> Result<String, MiniJinjaError> {
    // The value is a sequence of Artist objects (structs with name/alias fields).
    let items: Vec<Value> = value
        .try_iter()
        .map_err(|_| {
            MiniJinjaError::new(
                ErrorKind::InvalidOperation,
                "artistsarrayfmt expects a sequence",
            )
        })?
        .collect();

    let mut names: Vec<String> = Vec::new();
    for item in &items {
        let alias = item
            .get_attr("alias")
            .ok()
            .and_then(|v| v.is_true().then_some(true))
            .unwrap_or(false);
        if !alias {
            let name = item
                .get_attr("name")
                .map_err(|_| {
                    MiniJinjaError::new(
                        ErrorKind::InvalidOperation,
                        "artistsarrayfmt: items must have a 'name' attribute",
                    )
                })?
                .to_string();
            names.push(name);
        }
    }

    if names.len() <= 3 {
        Ok(arrayfmt_strings(&names))
    } else {
        Ok(format!("{} et al.", names[0]))
    }
}

/// Format an ArtistMapping with role decorations.
/// Accepts an optional `omit` keyword argument (list of role names to skip).
fn filter_artistsfmt(value: Value, kwargs: Kwargs) -> Result<String, MiniJinjaError> {
    // Parse optional omit parameter
    let omit: Vec<String> = kwargs
        .get::<Option<Vec<String>>>("omit")?
        .unwrap_or_default();
    kwargs.assert_all_used()?;

    // Extract artist lists from the ArtistMapping value
    let get_artists = |role: &str| -> Vec<(String, bool)> {
        match value.get_attr(role) {
            Ok(v) => {
                if let Ok(iter) = v.try_iter() {
                    iter.map(|item| {
                        let name = item
                            .get_attr("name")
                            .map(|n| n.to_string())
                            .unwrap_or_default();
                        let alias = item
                            .get_attr("alias")
                            .ok()
                            .and_then(|v| v.is_true().then_some(true))
                            .unwrap_or(false);
                        (name, alias)
                    })
                    .collect()
                } else {
                    Vec::new()
                }
            }
            Err(_) => Vec::new(),
        }
    };

    let format_role = |artists: &[(String, bool)]| -> String {
        let names: Vec<String> = artists
            .iter()
            .filter(|(_, alias)| !*alias)
            .map(|(name, _)| name.clone())
            .collect();
        if names.len() <= 3 {
            arrayfmt_strings(&names)
        } else {
            format!("{} et al.", names[0])
        }
    };

    let main = get_artists("main");
    let djmixer = get_artists("djmixer");
    let composer = get_artists("composer");
    let conductor = get_artists("conductor");
    let guest = get_artists("guest");
    let producer = get_artists("producer");

    let mut r = format_role(&main);

    if !djmixer.is_empty() && !omit.contains(&"djmixer".to_string()) {
        r = format!("{} pres. {}", format_role(&djmixer), r);
    } else if !composer.is_empty() && !omit.contains(&"composer".to_string()) {
        r = format!("{} performed by {}", format_role(&composer), r);
    }
    if !conductor.is_empty() && !omit.contains(&"conductor".to_string()) {
        r = format!("{} under {}", r, format_role(&conductor));
    }
    if !guest.is_empty() && !omit.contains(&"guest".to_string()) {
        r = format!("{} (feat. {})", r, format_role(&guest));
    }
    if !producer.is_empty() && !omit.contains(&"producer".to_string()) {
        r = format!("{} (prod. {})", r, format_role(&producer));
    }

    if r.is_empty() {
        Ok("Unknown Artists".to_string())
    } else {
        Ok(r)
    }
}

/// "First Last" -> "Last, First"
fn filter_sortorder(value: &str) -> String {
    match value.rsplit_once(' ') {
        Some((first, last)) => format!("{}, {}", last, first),
        None => value.to_string(),
    }
}

/// "First Last" -> "Last"
fn filter_lastname(value: &str) -> String {
    match value.rsplit_once(' ') {
        Some((_, last)) => last.to_string(),
        None => value.to_string(),
    }
}

/// Left-pad with '0' to `width` characters. Replaces `.rjust(2, '0')`.
fn filter_pad(value: &str, width: usize) -> String {
    if value.len() >= width {
        value.to_string()
    } else {
        let padding = width - value.len();
        format!("{}{}", "0".repeat(padding), value)
    }
}

/// Take first `length` characters. Replaces `[:10]` slicing.
fn filter_truncate(value: &str, length: usize) -> String {
    value.chars().take(length).collect()
}

// ---------------------------------------------------------------------------
// Shared minijinja Environment
// ---------------------------------------------------------------------------

/// Build and return the shared minijinja Environment with all custom filters.
fn build_environment() -> Environment<'static> {
    let mut env = Environment::new();

    env.add_filter("releasetypefmt", filter_releasetypefmt);
    env.add_filter("arrayfmt", filter_arrayfmt);
    env.add_filter("artistsarrayfmt", filter_artistsarrayfmt);
    env.add_filter("artistsfmt", filter_artistsfmt);
    env.add_filter("sortorder", filter_sortorder);
    env.add_filter("lastname", filter_lastname);
    env.add_filter("pad", filter_pad);
    env.add_filter("truncate", filter_truncate);

    env
}

static ENVIRONMENT: LazyLock<Environment<'static>> = LazyLock::new(build_environment);

// ---------------------------------------------------------------------------
// PathTemplate
// ---------------------------------------------------------------------------

/// A wrapper for a template string that compiles on-demand via the shared Environment.
#[derive(Debug, Clone)]
pub struct PathTemplate {
    pub text: String,
}

impl PathTemplate {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }

    /// Try to compile this template, returning an error if the syntax is invalid.
    pub fn compile_check(&self) -> Result<(), RoseError> {
        ENVIRONMENT
            .template_from_str(&self.text)
            .map(|_| ())
            .map_err(|e| RoseError::InvalidPathTemplate {
                key: String::new(),
                message: format!("Failed to compile template: {e}"),
            })
    }

    /// Render this template with the given context variables.
    fn render(&self, ctx: &Value) -> Result<String, MiniJinjaError> {
        let tmpl = ENVIRONMENT.template_from_str(&self.text)?;
        tmpl.render(ctx)
    }
}

impl std::hash::Hash for PathTemplate {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.text.hash(state);
    }
}

impl PartialEq for PathTemplate {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl Eq for PathTemplate {}

// ---------------------------------------------------------------------------
// PathTemplateTriad
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathTemplateTriad {
    pub release: PathTemplate,
    pub track: PathTemplate,
    pub all_tracks: PathTemplate,
}

// ---------------------------------------------------------------------------
// Default templates
// ---------------------------------------------------------------------------

pub const DEFAULT_RELEASE_TEMPLATE_TEXT: &str = "\
{{ releaseartists | artistsfmt }} - \
{% if releasedate %}{{ releasedate.year }}.{% endif %} \
{{ releasetitle }} \
{% if releasetype == \"single\" %}- {{ releasetype | releasetypefmt }}{% endif %} \
{% if favorite %} [FAVORITE]{% endif %}{% if new %} [NEW]{% endif %}";

pub const DEFAULT_TRACK_TEMPLATE_TEXT: &str = "\
{% if disctotal > 1 %}{{ discnumber | pad(2) }}-{% endif %}{{ tracknumber | pad(2) }}. \
{{ tracktitle }} \
{% if trackartists.guest %}(feat. {{ trackartists.guest | artistsarrayfmt }}){% endif %}";

pub const DEFAULT_ALL_TRACKS_TEMPLATE_TEXT: &str = "\
{{ trackartists | artistsfmt }} - \
{% if releasedate %}{{ releasedate.year }}.{% endif %} \
{{ releasetitle }} - \
{{ tracktitle }}";

fn default_release_template() -> PathTemplate {
    PathTemplate::new(DEFAULT_RELEASE_TEMPLATE_TEXT)
}

fn default_track_template() -> PathTemplate {
    PathTemplate::new(DEFAULT_TRACK_TEMPLATE_TEXT)
}

fn default_all_tracks_template() -> PathTemplate {
    PathTemplate::new(DEFAULT_ALL_TRACKS_TEMPLATE_TEXT)
}

fn default_template_triad() -> PathTemplateTriad {
    PathTemplateTriad {
        release: default_release_template(),
        track: default_track_template(),
        all_tracks: default_all_tracks_template(),
    }
}

const DEFAULT_PLAYLIST_TEMPLATE_TEXT: &str = "\
{{ position }}. \
{{ trackartists | artistsfmt }} - \
{{ tracktitle }}";

// ---------------------------------------------------------------------------
// PathTemplateConfig
// ---------------------------------------------------------------------------

/// Configuration for all path templates across VFS views.
#[derive(Debug, Clone)]
pub struct PathTemplateConfig {
    pub source: PathTemplateTriad,
    pub releases: PathTemplateTriad,
    pub releases_favorite: PathTemplateTriad,
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

impl PathTemplateConfig {
    /// Create a config populated with all default templates.
    /// If no custom default triad is provided, uses the built-in defaults.
    pub fn with_defaults() -> Self {
        Self::with_defaults_from(None)
    }

    /// Create a config populated with default templates, optionally using a
    /// custom default triad as the base for all views.
    pub fn with_defaults_from(custom_default: Option<PathTemplateTriad>) -> Self {
        let default = custom_default.unwrap_or_else(default_template_triad);

        PathTemplateConfig {
            source: default.clone(),
            releases: default.clone(),
            releases_favorite: default.clone(),
            releases_new: default.clone(),
            releases_added_on: PathTemplateTriad {
                release: PathTemplate::new(format!(
                    "[{{{{ added_at | truncate(10) }}}}] {}",
                    default.release.text
                )),
                track: default.track.clone(),
                all_tracks: default.all_tracks.clone(),
            },
            releases_released_on: PathTemplateTriad {
                release: PathTemplate::new(format!(
                    "[{{{{ originaldate or releasedate or '0000-00-00' }}}}] {}",
                    default.release.text
                )),
                track: default.track.clone(),
                all_tracks: default.all_tracks.clone(),
            },
            artists: default.clone(),
            genres: default.clone(),
            descriptors: default.clone(),
            labels: default.clone(),
            loose_tracks: default.clone(),
            collages: PathTemplateTriad {
                release: PathTemplate::new(format!("{{{{ position }}}}. {}", default.release.text)),
                track: default.track.clone(),
                all_tracks: default.all_tracks.clone(),
            },
            playlists: PathTemplate::new(DEFAULT_PLAYLIST_TEMPLATE_TEXT),
        }
    }

    /// Attempt to parse/compile all templates, returning an error for the first
    /// template that fails compilation.
    pub fn parse(&self) -> Result<(), RoseError> {
        let templates: Vec<(&str, &PathTemplate)> = vec![
            ("source.release", &self.source.release),
            ("source.track", &self.source.track),
            ("source.all_tracks", &self.source.all_tracks),
            ("releases.release", &self.releases.release),
            ("releases.track", &self.releases.track),
            ("releases.all_tracks", &self.releases.all_tracks),
            ("releases_favorite.release", &self.releases_favorite.release),
            ("releases_favorite.track", &self.releases_favorite.track),
            (
                "releases_favorite.all_tracks",
                &self.releases_favorite.all_tracks,
            ),
            ("releases_new.release", &self.releases_new.release),
            ("releases_new.track", &self.releases_new.track),
            ("releases_new.all_tracks", &self.releases_new.all_tracks),
            ("releases_added_on.release", &self.releases_added_on.release),
            ("releases_added_on.track", &self.releases_added_on.track),
            (
                "releases_added_on.all_tracks",
                &self.releases_added_on.all_tracks,
            ),
            (
                "releases_released_on.release",
                &self.releases_released_on.release,
            ),
            (
                "releases_released_on.track",
                &self.releases_released_on.track,
            ),
            (
                "releases_released_on.all_tracks",
                &self.releases_released_on.all_tracks,
            ),
            ("artists.release", &self.artists.release),
            ("artists.track", &self.artists.track),
            ("artists.all_tracks", &self.artists.all_tracks),
            ("genres.release", &self.genres.release),
            ("genres.track", &self.genres.track),
            ("genres.all_tracks", &self.genres.all_tracks),
            ("descriptors.release", &self.descriptors.release),
            ("descriptors.track", &self.descriptors.track),
            ("descriptors.all_tracks", &self.descriptors.all_tracks),
            ("labels.release", &self.labels.release),
            ("labels.track", &self.labels.track),
            ("labels.all_tracks", &self.labels.all_tracks),
            ("loose_tracks.release", &self.loose_tracks.release),
            ("loose_tracks.track", &self.loose_tracks.track),
            ("loose_tracks.all_tracks", &self.loose_tracks.all_tracks),
            ("collages.release", &self.collages.release),
            ("collages.track", &self.collages.track),
            ("collages.all_tracks", &self.collages.all_tracks),
            ("playlists", &self.playlists),
        ];

        for (key, tmpl) in templates {
            tmpl.compile_check()
                .map_err(|_| RoseError::InvalidPathTemplate {
                    key: key.to_string(),
                    message: format!("Failed to compile template for '{}': {}", key, tmpl.text),
                })?;
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PathContext
// ---------------------------------------------------------------------------

/// Additional context passed to template rendering for VFS views.
#[derive(Debug, Clone, Default)]
pub struct PathContext {
    pub genre: Option<String>,
    pub artist: Option<String>,
    pub label: Option<String>,
    pub descriptor: Option<String>,
    pub collage: Option<String>,
    pub playlist: Option<String>,
}

// ---------------------------------------------------------------------------
// Lightweight release/track types for template evaluation
// ---------------------------------------------------------------------------

/// Temporary Release struct for template evaluation. The real Release type will
/// be defined in the cache module later; this provides the fields templates need.
#[derive(Debug, Clone)]
pub struct Release {
    pub id: String,
    pub source_path: std::path::PathBuf,
    pub added_at: String,
    pub releasetitle: String,
    pub releasetype: String,
    pub releasedate: Option<RoseDate>,
    pub originaldate: Option<RoseDate>,
    pub compositiondate: Option<RoseDate>,
    pub edition: Option<String>,
    pub catalognumber: Option<String>,
    pub new: bool,
    pub favorite: bool,
    pub rating: Option<i32>,
    pub disctotal: i32,
    pub genres: Vec<String>,
    pub parent_genres: Vec<String>,
    pub secondary_genres: Vec<String>,
    pub parent_secondary_genres: Vec<String>,
    pub descriptors: Vec<String>,
    pub labels: Vec<String>,
    pub releaseartists: ArtistMapping,
}

/// Temporary Track struct for template evaluation. The real Track type will
/// be defined in the cache module later.
#[derive(Debug, Clone)]
pub struct Track {
    pub id: String,
    pub source_path: std::path::PathBuf,
    pub tracktitle: String,
    pub tracknumber: String,
    pub tracktotal: i32,
    pub discnumber: String,
    pub duration_seconds: i32,
    pub trackartists: ArtistMapping,
    pub release: Release,
}

// ---------------------------------------------------------------------------
// Template variable construction
// ---------------------------------------------------------------------------

/// Convert a RoseDate to a minijinja Value with a `year` attribute and string
/// representation.
fn rosedate_to_value(d: &RoseDate) -> Value {
    // Create an object with `year` attribute and proper Display.
    let mut map = std::collections::BTreeMap::new();
    map.insert("year".to_string(), Value::from(d.year));
    // Also store the full display string for `{{ originaldate or releasedate }}` usage
    map.insert("__str__".to_string(), Value::from(d.to_string()));
    Value::from(map)
}

/// Convert an Option<RoseDate> to a minijinja Value.
fn optional_rosedate_to_value(d: &Option<RoseDate>) -> Value {
    match d {
        Some(d) => rosedate_to_value(d),
        None => Value::from(()),
    }
}

/// Convert an Artist to a minijinja Value.
fn artist_to_value(a: &Artist) -> Value {
    let mut map = std::collections::BTreeMap::new();
    map.insert("name".to_string(), Value::from(a.name.as_str()));
    map.insert("alias".to_string(), Value::from(a.alias));
    Value::from(map)
}

/// Convert an ArtistMapping to a minijinja Value.
fn artist_mapping_to_value(m: &ArtistMapping) -> Value {
    let to_val_vec = |artists: &[Artist]| -> Value {
        Value::from(artists.iter().map(artist_to_value).collect::<Vec<_>>())
    };

    let mut map = std::collections::BTreeMap::new();
    map.insert("main".to_string(), to_val_vec(&m.main));
    map.insert("guest".to_string(), to_val_vec(&m.guest));
    map.insert("remixer".to_string(), to_val_vec(&m.remixer));
    map.insert("producer".to_string(), to_val_vec(&m.producer));
    map.insert("composer".to_string(), to_val_vec(&m.composer));
    map.insert("conductor".to_string(), to_val_vec(&m.conductor));
    map.insert("djmixer".to_string(), to_val_vec(&m.djmixer));
    Value::from(map)
}

/// Build the template variable map for a release.
fn calc_release_variables(release: &Release, position: Option<&str>) -> Value {
    let mut ctx = std::collections::BTreeMap::new();
    ctx.insert(
        "added_at".to_string(),
        Value::from(release.added_at.as_str()),
    );
    ctx.insert(
        "releasetitle".to_string(),
        Value::from(release.releasetitle.as_str()),
    );
    ctx.insert(
        "releasetype".to_string(),
        Value::from(release.releasetype.as_str()),
    );
    ctx.insert(
        "releasedate".to_string(),
        optional_rosedate_to_value(&release.releasedate),
    );
    ctx.insert(
        "originaldate".to_string(),
        optional_rosedate_to_value(&release.originaldate),
    );
    ctx.insert(
        "compositiondate".to_string(),
        optional_rosedate_to_value(&release.compositiondate),
    );
    ctx.insert(
        "edition".to_string(),
        Value::from(release.edition.as_deref().unwrap_or("")),
    );
    ctx.insert(
        "catalognumber".to_string(),
        Value::from(release.catalognumber.as_deref().unwrap_or("")),
    );
    ctx.insert("new".to_string(), Value::from(release.new));
    ctx.insert("favorite".to_string(), Value::from(release.favorite));
    ctx.insert(
        "rating".to_string(),
        match release.rating {
            Some(r) => Value::from(r),
            None => Value::from(()),
        },
    );
    ctx.insert("disctotal".to_string(), Value::from(release.disctotal));
    ctx.insert(
        "genres".to_string(),
        Value::from(
            release
                .genres
                .iter()
                .map(|s| Value::from(s.as_str()))
                .collect::<Vec<_>>(),
        ),
    );
    ctx.insert(
        "parentgenres".to_string(),
        Value::from(
            release
                .parent_genres
                .iter()
                .map(|s| Value::from(s.as_str()))
                .collect::<Vec<_>>(),
        ),
    );
    ctx.insert(
        "secondarygenres".to_string(),
        Value::from(
            release
                .secondary_genres
                .iter()
                .map(|s| Value::from(s.as_str()))
                .collect::<Vec<_>>(),
        ),
    );
    ctx.insert(
        "parentsecondarygenres".to_string(),
        Value::from(
            release
                .parent_secondary_genres
                .iter()
                .map(|s| Value::from(s.as_str()))
                .collect::<Vec<_>>(),
        ),
    );
    ctx.insert(
        "descriptors".to_string(),
        Value::from(
            release
                .descriptors
                .iter()
                .map(|s| Value::from(s.as_str()))
                .collect::<Vec<_>>(),
        ),
    );
    ctx.insert(
        "labels".to_string(),
        Value::from(
            release
                .labels
                .iter()
                .map(|s| Value::from(s.as_str()))
                .collect::<Vec<_>>(),
        ),
    );
    ctx.insert(
        "releaseartists".to_string(),
        artist_mapping_to_value(&release.releaseartists),
    );
    ctx.insert(
        "position".to_string(),
        match position {
            Some(p) => Value::from(p),
            None => Value::from(()),
        },
    );
    Value::from(ctx)
}

/// Build the template variable map for a track.
fn calc_track_variables(track: &Track, position: Option<&str>) -> Value {
    let release = &track.release;

    let mut ctx = std::collections::BTreeMap::new();
    ctx.insert(
        "added_at".to_string(),
        Value::from(release.added_at.as_str()),
    );
    ctx.insert(
        "tracktitle".to_string(),
        Value::from(track.tracktitle.as_str()),
    );
    ctx.insert(
        "tracknumber".to_string(),
        Value::from(track.tracknumber.as_str()),
    );
    ctx.insert("tracktotal".to_string(), Value::from(track.tracktotal));
    ctx.insert(
        "discnumber".to_string(),
        Value::from(track.discnumber.as_str()),
    );
    ctx.insert("disctotal".to_string(), Value::from(release.disctotal));
    ctx.insert(
        "duration_seconds".to_string(),
        Value::from(track.duration_seconds),
    );
    ctx.insert(
        "trackartists".to_string(),
        artist_mapping_to_value(&track.trackartists),
    );
    ctx.insert(
        "releasetitle".to_string(),
        Value::from(release.releasetitle.as_str()),
    );
    ctx.insert(
        "releasetype".to_string(),
        Value::from(release.releasetype.as_str()),
    );
    ctx.insert(
        "releasedate".to_string(),
        optional_rosedate_to_value(&release.releasedate),
    );
    ctx.insert(
        "originaldate".to_string(),
        optional_rosedate_to_value(&release.originaldate),
    );
    ctx.insert(
        "compositiondate".to_string(),
        optional_rosedate_to_value(&release.compositiondate),
    );
    ctx.insert(
        "edition".to_string(),
        Value::from(release.edition.as_deref().unwrap_or("")),
    );
    ctx.insert(
        "catalognumber".to_string(),
        Value::from(release.catalognumber.as_deref().unwrap_or("")),
    );
    ctx.insert("new".to_string(), Value::from(release.new));
    ctx.insert("favorite".to_string(), Value::from(release.favorite));
    ctx.insert(
        "rating".to_string(),
        match release.rating {
            Some(r) => Value::from(r),
            None => Value::from(()),
        },
    );
    ctx.insert(
        "genres".to_string(),
        Value::from(
            release
                .genres
                .iter()
                .map(|s| Value::from(s.as_str()))
                .collect::<Vec<_>>(),
        ),
    );
    ctx.insert(
        "parentgenres".to_string(),
        Value::from(
            release
                .parent_genres
                .iter()
                .map(|s| Value::from(s.as_str()))
                .collect::<Vec<_>>(),
        ),
    );
    ctx.insert(
        "secondarygenres".to_string(),
        Value::from(
            release
                .secondary_genres
                .iter()
                .map(|s| Value::from(s.as_str()))
                .collect::<Vec<_>>(),
        ),
    );
    ctx.insert(
        "parentsecondarygenres".to_string(),
        Value::from(
            release
                .parent_secondary_genres
                .iter()
                .map(|s| Value::from(s.as_str()))
                .collect::<Vec<_>>(),
        ),
    );
    ctx.insert(
        "descriptors".to_string(),
        Value::from(
            release
                .descriptors
                .iter()
                .map(|s| Value::from(s.as_str()))
                .collect::<Vec<_>>(),
        ),
    );
    ctx.insert(
        "labels".to_string(),
        Value::from(
            release
                .labels
                .iter()
                .map(|s| Value::from(s.as_str()))
                .collect::<Vec<_>>(),
        ),
    );
    ctx.insert(
        "releaseartists".to_string(),
        artist_mapping_to_value(&release.releaseartists),
    );
    ctx.insert(
        "position".to_string(),
        match position {
            Some(p) => Value::from(p),
            None => Value::from(()),
        },
    );
    Value::from(ctx)
}

// ---------------------------------------------------------------------------
// Whitespace collapsing
// ---------------------------------------------------------------------------

static COLLAPSE_SPACING_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+").expect("invalid regex"));

fn collapse_spacing(s: &str) -> String {
    COLLAPSE_SPACING_REGEX
        .replace_all(s, " ")
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// Template evaluation functions
// ---------------------------------------------------------------------------

/// Evaluate a release template, returning the rendered path component.
/// Whitespace is collapsed and the result is trimmed.
pub fn evaluate_release_template(
    template: &PathTemplate,
    release: &Release,
    context: Option<&PathContext>,
    position: Option<&str>,
) -> String {
    let ctx = calc_release_variables(release, position);
    let _ = context; // PathContext reserved for future VFS view usage

    let rendered = template
        .render(&ctx)
        .unwrap_or_else(|e| format!("TEMPLATE_ERROR: {e}"));

    collapse_spacing(&rendered)
}

/// Evaluate a track template, returning the rendered path component with file extension.
/// Whitespace is collapsed and the result is trimmed, then the source file extension
/// is appended.
pub fn evaluate_track_template(
    template: &PathTemplate,
    track: &Track,
    context: Option<&PathContext>,
    position: Option<&str>,
) -> String {
    let ctx = calc_track_variables(track, position);

    let _ = context; // PathContext reserved for future VFS view usage

    let rendered = template
        .render(&ctx)
        .unwrap_or_else(|e| format!("TEMPLATE_ERROR: {e}"));

    let ext = Path::new(&track.source_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();

    format!("{}{}", collapse_spacing(&rendered), ext)
}

// ---------------------------------------------------------------------------
// Sample music data
// ---------------------------------------------------------------------------

/// Return three sample (Release, Track) pairs used for template preview.
/// Kim Lip (single), BTS (multi-disc album), Debussy (classical with composer/conductor).
pub fn get_sample_music(music_source_dir: &Path) -> [(Release, Track); 3] {
    let kimlip_rls = Release {
        id: "018b268e-ff1e-7a0c-9ac8-7bbb282761f2".into(),
        source_path: music_source_dir.join("LOONA - 2017. Kim Lip"),
        added_at: "2023-04-20:23:45Z".into(),
        releasetitle: "Kim Lip".into(),
        releasetype: "single".into(),
        releasedate: Some(RoseDate {
            year: 2017,
            month: Some(5),
            day: Some(23),
        }),
        originaldate: Some(RoseDate {
            year: 2017,
            month: Some(5),
            day: Some(23),
        }),
        compositiondate: None,
        edition: None,
        catalognumber: Some("CMCC11088".into()),
        new: true,
        favorite: false,
        rating: None,
        disctotal: 1,
        genres: vec![
            "K-Pop".into(),
            "Dance-Pop".into(),
            "Contemporary R&B".into(),
        ],
        parent_genres: vec!["Pop".into(), "R&B".into()],
        secondary_genres: vec!["Synth Funk".into(), "Synthpop".into(), "Future Bass".into()],
        parent_secondary_genres: vec!["Funk".into(), "Pop".into()],
        descriptors: vec![
            "Female Vocalist".into(),
            "Mellow".into(),
            "Sensual".into(),
            "Ethereal".into(),
            "Love".into(),
            "Lush".into(),
            "Romantic".into(),
            "Warm".into(),
            "Melodic".into(),
            "Passionate".into(),
            "Nocturnal".into(),
            "Summer".into(),
        ],
        labels: vec!["BlockBerryCreative".into()],
        releaseartists: ArtistMapping {
            main: vec![Artist::new("Kim Lip")],
            ..Default::default()
        },
    };

    let bts_rls = Release {
        id: "018b6021-f1e5-7d4b-b796-440fbbea3b13".into(),
        source_path: music_source_dir.join("BTS - 2016. Young Forever (花樣年華)"),
        added_at: "2023-06-09:23:45Z".into(),
        releasetitle: "Young Forever (花樣年華)".into(),
        releasetype: "album".into(),
        releasedate: Some(RoseDate {
            year: 2016,
            month: None,
            day: None,
        }),
        originaldate: Some(RoseDate {
            year: 2016,
            month: None,
            day: None,
        }),
        compositiondate: None,
        edition: Some("Deluxe".into()),
        catalognumber: Some("L200001238".into()),
        new: false,
        favorite: false,
        rating: None,
        disctotal: 2,
        genres: vec!["K-Pop".into()],
        parent_genres: vec!["Pop".into()],
        secondary_genres: vec!["Pop Rap".into(), "Electropop".into()],
        parent_secondary_genres: vec!["Hip Hop".into(), "Electronic".into()],
        descriptors: vec![
            "Autumn".into(),
            "Passionate".into(),
            "Melodic".into(),
            "Romantic".into(),
            "Eclectic".into(),
            "Melancholic".into(),
            "Male Vocalist".into(),
            "Sentimental".into(),
            "Uplifting".into(),
            "Breakup".into(),
            "Love".into(),
            "Anthemic".into(),
            "Lush".into(),
            "Bittersweet".into(),
            "Spring".into(),
        ],
        labels: vec!["BIGHIT".into()],
        releaseartists: ArtistMapping {
            main: vec![Artist::new("BTS")],
            ..Default::default()
        },
    };

    let debussy_rls = Release {
        id: "018b268e-de0c-7cb2-8ffa-bcc2083c94e6".into(),
        source_path: music_source_dir.join(
            "Debussy - 1907. Images performed by Cleveland Orchestra under Pierre Boulez (1992)",
        ),
        added_at: "2023-09-06:23:45Z".into(),
        releasetitle: "Images".into(),
        releasetype: "album".into(),
        releasedate: Some(RoseDate {
            year: 1992,
            month: None,
            day: None,
        }),
        originaldate: Some(RoseDate {
            year: 1991,
            month: None,
            day: None,
        }),
        compositiondate: Some(RoseDate {
            year: 1907,
            month: None,
            day: None,
        }),
        edition: None,
        catalognumber: Some("435-766 2".into()),
        new: false,
        favorite: false,
        rating: None,
        disctotal: 2,
        genres: vec!["Impressionism, Orchestral".into()],
        parent_genres: vec!["Modern Classical".into()],
        secondary_genres: vec!["Tone Poem".into()],
        parent_secondary_genres: vec!["Orchestral Music".into()],
        descriptors: vec!["Orchestral Music".into()],
        labels: vec!["Deustche Grammophon".into()],
        releaseartists: ArtistMapping {
            main: vec![Artist::new("Cleveland Orchestra")],
            composer: vec![Artist::new("Claude Debussy")],
            conductor: vec![Artist::new("Pierre Boulez")],
            ..Default::default()
        },
    };

    let kimlip_trk = Track {
        id: "018b268e-ff1e-7a0c-9ac8-7bbb282761f1".into(),
        source_path: music_source_dir
            .join("LOONA - 2017. Kim Lip")
            .join("01. Eclipse.opus"),
        tracktitle: "Eclipse".into(),
        tracknumber: "1".into(),
        tracktotal: 2,
        discnumber: "1".into(),
        duration_seconds: 230,
        trackartists: ArtistMapping {
            main: vec![Artist::new("Kim Lip")],
            ..Default::default()
        },
        release: kimlip_rls.clone(),
    };

    let bts_trk = Track {
        id: "018b6021-f1e5-7d4b-b796-440fbbea3b15".into(),
        source_path: music_source_dir
            .join("BTS - 2016. Young Forever (花樣年華)")
            .join("02-05. House of Cards.opus"),
        tracktitle: "House of Cards".into(),
        tracknumber: "5".into(),
        tracktotal: 8,
        discnumber: "2".into(),
        duration_seconds: 226,
        trackartists: ArtistMapping {
            main: vec![Artist::new("BTS")],
            ..Default::default()
        },
        release: bts_rls.clone(),
    };

    let debussy_trk = Track {
        id: "018b6514-6e65-78cc-94a5-fdb17418f090".into(),
        source_path: music_source_dir
            .join("Debussy - 1907. Images performed by Cleveland Orchestra under Pierre Boulez (1992)")
            .join("01. Gigues: Modéré.opus"),
        tracktitle: "Gigues: Modéré".into(),
        tracknumber: "1".into(),
        tracktotal: 6,
        discnumber: "1".into(),
        duration_seconds: 444,
        trackartists: ArtistMapping {
            main: vec![Artist::new("Cleveland Orchestra")],
            composer: vec![Artist::new("Claude Debussy")],
            conductor: vec![Artist::new("Pierre Boulez")],
            ..Default::default()
        },
        release: debussy_rls.clone(),
    };

    [
        (kimlip_rls, kimlip_trk),
        (bts_rls, bts_trk),
        (debussy_rls, debussy_trk),
    ]
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn empty_release() -> Release {
        Release {
            id: String::new(),
            source_path: PathBuf::new(),
            added_at: "0000-01-01T00:00:00Z".into(),
            releasetitle: String::new(),
            releasetype: "unknown".into(),
            releasedate: None,
            originaldate: None,
            compositiondate: None,
            edition: None,
            catalognumber: None,
            new: false,
            favorite: false,
            rating: None,
            disctotal: 1,
            genres: Vec::new(),
            parent_genres: Vec::new(),
            secondary_genres: Vec::new(),
            parent_secondary_genres: Vec::new(),
            descriptors: Vec::new(),
            labels: Vec::new(),
            releaseartists: ArtistMapping::default(),
        }
    }

    fn empty_track() -> Track {
        Track {
            id: String::new(),
            source_path: PathBuf::from("hi.m4a"),
            tracktitle: String::new(),
            tracknumber: String::new(),
            tracktotal: 1,
            discnumber: String::new(),
            duration_seconds: 0,
            trackartists: ArtistMapping::default(),
            release: empty_release(),
        }
    }

    // 1. Default templates compile
    #[test]
    fn default_templates_compile() {
        let config = PathTemplateConfig::with_defaults();
        config.parse().expect("default templates should compile");
    }

    // 2. Release template evaluation: Kim Lip sample
    #[test]
    fn release_template_kim_lip() {
        let config = PathTemplateConfig::with_defaults();
        let music_dir = PathBuf::from("/tmp/music");
        let samples = get_sample_music(&music_dir);
        let (kim_lip_rls, _) = &samples[0];

        let result = evaluate_release_template(&config.source.release, kim_lip_rls, None, None);
        assert_eq!(result, "Kim Lip - 2017. Kim Lip - Single [NEW]");
    }

    // 3. Track template with multi-disc (BTS)
    #[test]
    fn track_template_multi_disc() {
        let config = PathTemplateConfig::with_defaults();
        let music_dir = PathBuf::from("/tmp/music");
        let samples = get_sample_music(&music_dir);
        let (_, bts_trk) = &samples[1];

        let result = evaluate_track_template(&config.source.track, bts_trk, None, None);
        assert_eq!(result, "02-05. House of Cards.opus");
    }

    // 4. Composer performed-by (Debussy)
    #[test]
    fn release_template_debussy() {
        let config = PathTemplateConfig::with_defaults();
        let music_dir = PathBuf::from("/tmp/music");
        let samples = get_sample_music(&music_dir);
        let (debussy_rls, _) = &samples[2];

        let result = evaluate_release_template(&config.source.release, debussy_rls, None, None);
        assert_eq!(
            result,
            "Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - 1992. Images"
        );
    }

    // 5. pad filter
    #[test]
    fn filter_pad_basic() {
        assert_eq!(filter_pad("3", 2), "03");
        assert_eq!(filter_pad("12", 2), "12");
        assert_eq!(filter_pad("1", 3), "001");
        assert_eq!(filter_pad("123", 2), "123");
    }

    // 6. truncate filter
    #[test]
    fn filter_truncate_basic() {
        assert_eq!(filter_truncate("2023-04-20:23:45Z", 10), "2023-04-20");
        assert_eq!(filter_truncate("short", 10), "short");
    }

    // 7. arrayfmt filter
    #[test]
    fn filter_arrayfmt_basic() {
        assert_eq!(arrayfmt_strings(&[]), "");
        assert_eq!(arrayfmt_strings(&["a".into()]), "a");
        assert_eq!(
            arrayfmt_strings(&["a".into(), "b".into(), "c".into()]),
            "a, b & c"
        );
        assert_eq!(arrayfmt_strings(&["a".into(), "b".into()]), "a & b");
    }

    // 8. releasetypefmt filter
    #[test]
    fn filter_releasetypefmt_basic() {
        assert_eq!(filter_releasetypefmt("djmix"), "DJ-Mix");
        assert_eq!(filter_releasetypefmt("ep"), "EP");
        assert_eq!(filter_releasetypefmt("album"), "Album");
        assert_eq!(filter_releasetypefmt("single"), "Single");
        assert_eq!(filter_releasetypefmt("unknown_type"), "Unknown_type");
    }

    // 9. Whitespace collapsing
    #[test]
    fn whitespace_collapsing() {
        assert_eq!(collapse_spacing("  hello   world  "), "hello world");
        assert_eq!(collapse_spacing("hello\n  world"), "hello world");
        assert_eq!(collapse_spacing("a\n\n\nb"), "a b");
    }

    // 10. Invalid template
    #[test]
    fn invalid_template_error() {
        let tmpl = PathTemplate::new("{{ unclosed");
        assert!(tmpl.compile_check().is_err());
    }

    // Test the default templates with empty data (like Python test_default_templates)
    #[test]
    fn default_templates_with_data() {
        let config = PathTemplateConfig::with_defaults();

        // Release with artists, date, and single type
        let mut release = empty_release();
        release.releasetitle = "Title".into();
        release.releasedate = Some(RoseDate {
            year: 2023,
            month: None,
            day: None,
        });
        release.releaseartists = ArtistMapping {
            main: vec![Artist::new("A1"), Artist::new("A2"), Artist::new("A3")],
            guest: vec![Artist::new("BB")],
            producer: vec![Artist::new("PP")],
            ..Default::default()
        };
        release.releasetype = "single".into();

        assert_eq!(
            evaluate_release_template(&config.source.release, &release, None, None),
            "A1, A2 & A3 (feat. BB) (prod. PP) - 2023. Title - Single"
        );
        assert_eq!(
            evaluate_release_template(&config.collages.release, &release, None, Some("4")),
            "4. A1, A2 & A3 (feat. BB) (prod. PP) - 2023. Title - Single"
        );

        // Release with empty artists
        let mut release = empty_release();
        release.releasetitle = "Title".into();
        assert_eq!(
            evaluate_release_template(&config.source.release, &release, None, None),
            "Unknown Artists - Title"
        );
        assert_eq!(
            evaluate_release_template(&config.collages.release, &release, None, Some("4")),
            "4. Unknown Artists - Title"
        );

        // Track with single disc
        let mut track = empty_track();
        track.tracknumber = "2".into();
        track.tracktitle = "Trick".into();
        assert_eq!(
            evaluate_track_template(&config.source.track, &track, None, None),
            "02. Trick.m4a"
        );
        assert_eq!(
            evaluate_track_template(&config.playlists, &track, None, Some("4")),
            "4. Unknown Artists - Trick.m4a"
        );

        // Track with multi-disc and guests
        let mut track = empty_track();
        track.release.disctotal = 2;
        track.discnumber = "4".into();
        track.tracknumber = "2".into();
        track.tracktitle = "Trick".into();
        track.trackartists = ArtistMapping {
            main: vec![Artist::new("Main")],
            guest: vec![Artist::new("Hi"), Artist::new("High"), Artist::new("Hye")],
            ..Default::default()
        };
        assert_eq!(
            evaluate_track_template(&config.source.track, &track, None, None),
            "04-02. Trick (feat. Hi, High & Hye).m4a"
        );
        assert_eq!(
            evaluate_track_template(&config.playlists, &track, None, Some("4")),
            "4. Main (feat. Hi, High & Hye) - Trick.m4a"
        );
    }

    // Test classical template (Debussy sample with sortorder/map filters)
    #[test]
    fn classical_template() {
        let music_dir = PathBuf::from("/tmp/music");
        let samples = get_sample_music(&music_dir);
        let (debussy_rls, _) = &samples[2];

        // This is the Python classical template ported to minijinja syntax.
        // Note: minijinja has `map(attribute='name')` built-in.
        let template = PathTemplate::new(
            "{% if new %}{{ '{N}' }}{% endif %}\
             {{ releaseartists.composer | map(attribute='name') | map('sortorder') | arrayfmt }} - \
             {% if compositiondate %}{{ compositiondate.year }}.{% endif %} \
             {{ releasetitle }} \
             performed by {{ releaseartists | artistsfmt(omit=[\"composer\"]) }} \
             {% if releasedate %}({{ releasedate.year }}){% endif %}",
        );

        let result = evaluate_release_template(&template, debussy_rls, None, None);
        assert_eq!(
            result,
            "Debussy, Claude - 1907. Images performed by Cleveland Orchestra under Pierre Boulez (1992)"
        );
    }

    // Test sortorder and lastname filters
    #[test]
    fn filter_sortorder_basic() {
        assert_eq!(filter_sortorder("Claude Debussy"), "Debussy, Claude");
        assert_eq!(filter_sortorder("Madonna"), "Madonna");
    }

    #[test]
    fn filter_lastname_basic() {
        assert_eq!(filter_lastname("Claude Debussy"), "Debussy");
        assert_eq!(filter_lastname("Madonna"), "Madonna");
    }

    // Test added_at truncation in releases_added_on default template
    #[test]
    fn releases_added_on_template() {
        let config = PathTemplateConfig::with_defaults();
        let music_dir = PathBuf::from("/tmp/music");
        let samples = get_sample_music(&music_dir);
        let (kim_lip_rls, _) = &samples[0];

        let result =
            evaluate_release_template(&config.releases_added_on.release, kim_lip_rls, None, None);
        assert!(
            result.starts_with("[2023-04-20"),
            "Expected added_on template to start with truncated date, got: {result}"
        );
    }

    // Test pad filter via minijinja rendering
    #[test]
    fn pad_filter_in_template() {
        let tmpl = PathTemplate::new("{{ val | pad(2) }}");
        let ctx = Value::from(std::collections::BTreeMap::from([(
            "val".to_string(),
            Value::from("3"),
        )]));
        let result = tmpl.render(&ctx).unwrap();
        assert_eq!(result, "03");
    }

    // Test truncate filter via minijinja rendering
    #[test]
    fn truncate_filter_in_template() {
        let tmpl = PathTemplate::new("{{ val | truncate(10) }}");
        let ctx = Value::from(std::collections::BTreeMap::from([(
            "val".to_string(),
            Value::from("2023-04-20:23:45Z"),
        )]));
        let result = tmpl.render(&ctx).unwrap();
        assert_eq!(result, "2023-04-20");
    }

    // Test arrayfmt filter via minijinja rendering
    #[test]
    fn arrayfmt_filter_in_template() {
        let tmpl = PathTemplate::new("{{ items | arrayfmt }}");
        let ctx = Value::from(std::collections::BTreeMap::from([(
            "items".to_string(),
            Value::from(vec![Value::from("a"), Value::from("b"), Value::from("c")]),
        )]));
        let result = tmpl.render(&ctx).unwrap();
        assert_eq!(result, "a, b & c");
    }

    // Test artistsarrayfmt with >3 artists
    #[test]
    fn artistsarrayfmt_et_al() {
        let artists = ArtistMapping {
            main: vec![
                Artist::new("A"),
                Artist::new("B"),
                Artist::new("C"),
                Artist::new("D"),
            ],
            ..Default::default()
        };
        let val = artist_mapping_to_value(&artists);
        let tmpl = PathTemplate::new("{{ artists.main | artistsarrayfmt }}");
        let ctx = Value::from(std::collections::BTreeMap::from([(
            "artists".to_string(),
            val,
        )]));
        let result = tmpl.render(&ctx).unwrap();
        assert_eq!(result, "A et al.");
    }

    // Test titlecase behavior for unknown release types.
    // Rust's titlecase only capitalizes the first character, so "some_unknown_type"
    // becomes "Some_unknown_type". Python's titlecase would produce "Some_Unknown_Type".
    // This is an intentional divergence: Rust uses simple first-char capitalization.
    #[test]
    fn test_titlecase_unknown_releasetype() {
        assert_eq!(
            filter_releasetypefmt("some_unknown_type"),
            "Some_unknown_type"
        );
        // Single word unknown type
        assert_eq!(filter_releasetypefmt("bootleg"), "Bootleg");
        // Empty string
        assert_eq!(filter_releasetypefmt(""), "");
    }

    // Test artistsfmt with djmixer role produces "DJ pres. Main" format.
    #[test]
    fn test_artistsfmt_with_djmixer() {
        let mut release = empty_release();
        release.releasetitle = "Mix Album".into();
        release.releasetype = "djmix".into();
        release.releasedate = Some(RoseDate {
            year: 2020,
            month: None,
            day: None,
        });
        release.releaseartists = ArtistMapping {
            main: vec![Artist::new("Various Artists")],
            djmixer: vec![Artist::new("Tiësto")],
            ..Default::default()
        };

        let config = PathTemplateConfig::with_defaults();
        let result = evaluate_release_template(&config.source.release, &release, None, None);
        assert!(
            result.contains("Tiësto pres. Various Artists"),
            "Expected djmixer 'pres.' format, got: {result}"
        );

        // Also test via a direct custom template
        let tmpl = PathTemplate::new("{{ releaseartists | artistsfmt }}");
        let ctx = calc_release_variables(&release, None);
        let rendered = tmpl.render(&ctx).unwrap();
        assert_eq!(rendered, "Tiësto pres. Various Artists");
    }

    // Test that favorite=true renders [FAVORITE] in the default release template.
    #[test]
    fn test_favorite_flag_in_template() {
        let mut release = empty_release();
        release.releasetitle = "Best Of".into();
        release.releasetype = "album".into();
        release.releasedate = Some(RoseDate {
            year: 2021,
            month: None,
            day: None,
        });
        release.favorite = true;
        release.releaseartists = ArtistMapping {
            main: vec![Artist::new("TestArtist")],
            ..Default::default()
        };

        let config = PathTemplateConfig::with_defaults();
        let result = evaluate_release_template(&config.source.release, &release, None, None);
        assert!(
            result.contains("[FAVORITE]"),
            "Expected [FAVORITE] in output, got: {result}"
        );
        assert_eq!(result, "TestArtist - 2021. Best Of [FAVORITE]");

        // When favorite=false, [FAVORITE] should not appear
        release.favorite = false;
        let result = evaluate_release_template(&config.source.release, &release, None, None);
        assert!(
            !result.contains("[FAVORITE]"),
            "Expected no [FAVORITE] when favorite=false, got: {result}"
        );
    }

    // Test all_tracks template evaluation includes both release and track info.
    #[test]
    fn test_all_tracks_template() {
        let mut track = empty_track();
        track.tracktitle = "My Song".into();
        track.tracknumber = "3".into();
        track.source_path = PathBuf::from("song.flac");
        track.release.releasetitle = "My Album".into();
        track.release.releasetype = "album".into();
        track.release.releasedate = Some(RoseDate {
            year: 2022,
            month: None,
            day: None,
        });
        track.trackartists = ArtistMapping {
            main: vec![Artist::new("TrackArtist")],
            ..Default::default()
        };
        track.release.releaseartists = ArtistMapping {
            main: vec![Artist::new("ReleaseArtist")],
            ..Default::default()
        };

        let config = PathTemplateConfig::with_defaults();
        let result = evaluate_track_template(&config.source.all_tracks, &track, None, None);
        // Default all_tracks template:
        // "{{ trackartists | artistsfmt }} - {{ releasedate.year }}. {{ releasetitle }} - {{ tracktitle }}"
        assert_eq!(result, "TrackArtist - 2022. My Album - My Song.flac");
    }

    // Test artistsarrayfmt filters out artists with alias=true.
    #[test]
    fn test_artistsarrayfmt_filters_aliases() {
        let artists = ArtistMapping {
            main: vec![
                Artist::new("RealArtist"),
                Artist {
                    name: "AliasArtist".into(),
                    alias: true,
                },
                Artist::new("AnotherReal"),
            ],
            ..Default::default()
        };
        let val = artist_mapping_to_value(&artists);
        let tmpl = PathTemplate::new("{{ artists.main | artistsarrayfmt }}");
        let ctx = Value::from(std::collections::BTreeMap::from([(
            "artists".to_string(),
            val,
        )]));
        let result = tmpl.render(&ctx).unwrap();
        // AliasArtist should be excluded; only RealArtist & AnotherReal remain
        assert_eq!(result, "RealArtist & AnotherReal");
    }

    // Test artistsfmt with >3 main artists triggers "et al." truncation.
    #[test]
    fn test_artistsfmt_et_al_main_artists() {
        let mut release = empty_release();
        release.releasetitle = "Collab".into();
        release.releaseartists = ArtistMapping {
            main: vec![
                Artist::new("Alpha"),
                Artist::new("Beta"),
                Artist::new("Gamma"),
                Artist::new("Delta"),
            ],
            ..Default::default()
        };

        let tmpl = PathTemplate::new("{{ releaseartists | artistsfmt }}");
        let ctx = calc_release_variables(&release, None);
        let rendered = tmpl.render(&ctx).unwrap();
        assert_eq!(rendered, "Alpha et al.");

        // With exactly 3 artists, no truncation
        release.releaseartists = ArtistMapping {
            main: vec![
                Artist::new("Alpha"),
                Artist::new("Beta"),
                Artist::new("Gamma"),
            ],
            ..Default::default()
        };
        let ctx = calc_release_variables(&release, None);
        let rendered = tmpl.render(&ctx).unwrap();
        assert_eq!(rendered, "Alpha, Beta & Gamma");
    }

    // Test released_on default template with or coalescing
    #[test]
    fn releases_released_on_template() {
        let config = PathTemplateConfig::with_defaults();

        let mut release = empty_release();
        release.releasetitle = "Title".into();
        release.releaseartists = ArtistMapping {
            main: vec![Artist::new("Artist")],
            ..Default::default()
        };
        // No originaldate or releasedate
        let result =
            evaluate_release_template(&config.releases_released_on.release, &release, None, None);
        assert!(
            result.starts_with("[0000-00-00]"),
            "Expected fallback date, got: {result}"
        );
    }
}
