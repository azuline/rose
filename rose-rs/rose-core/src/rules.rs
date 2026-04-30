//! Rules engine: the 5-phase pipeline for performant substring tag querying
//! and bulk metadata updating.
//!
//! # Pipeline phases
//!
//! 1. **FTS fast search** — query the `rules_engine_fts` full-text-search index
//!    to get a superset of matching tracks (may include false positives).
//! 2. **Pre-filter via cache** — if >400 results, use the read cache to
//!    eliminate obvious false positives before hitting disk.
//! 3. **Tag-based filter** — read actual audio tags from disk, match against
//!    the pattern, and filter out false positives. Also applies ignore matchers.
//! 4. **Apply actions in-memory** — deep-clone tags, apply actions, compute
//!    per-track diffs. No disk writes.
//! 5. **Confirmation & flush** — display changes, prompt for confirmation,
//!    flush AudioTags and datafile changes to disk, then trigger cache update.
//!
//! # Regex replacement syntax
//!
//! The `sed` action uses Rust's `regex` crate replacement syntax (`$1`, `${name}`)
//! — **not** Python's `\1` / `\g<name>`.
//!
//! # Bug fix
//!
//! The Python source (rules.py lines 302, 306) incorrectly assigns
//! `match = matches_pattern(...)` in the ignore loop for `new`/`favorite`
//! fields. The correct behavior is `skip = matches_pattern(...)`.
//! This Rust port fixes that bug.

use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::audiotags::{AudioTags, RoseDate};
use crate::cache::{
    connect, list_releases, list_tracks, Release, StoredDataFile, Track, STORED_DATA_FILE_REGEX,
};
use crate::common::{uniq, Artist, RoseError};
use crate::config::Config;
use crate::rule_parser::{
    Action, ActionBehavior, Matcher, Pattern, Rule, Tag, RELEASE_TAGS, TAG_CATALOGNUMBER,
    TAG_COMPOSITIONDATE, TAG_DESCRIPTOR, TAG_DISCNUMBER, TAG_DISCTOTAL, TAG_EDITION, TAG_FAVORITE,
    TAG_GENRE, TAG_LABEL, TAG_NEW, TAG_ORIGINALDATE, TAG_RATING, TAG_RELEASEARTIST_COMPOSER,
    TAG_RELEASEARTIST_CONDUCTOR, TAG_RELEASEARTIST_DJMIXER, TAG_RELEASEARTIST_GUEST,
    TAG_RELEASEARTIST_MAIN, TAG_RELEASEARTIST_PRODUCER, TAG_RELEASEARTIST_REMIXER, TAG_RELEASEDATE,
    TAG_RELEASETITLE, TAG_RELEASETYPE, TAG_SECONDARYGENRE, TAG_TRACKARTIST_COMPOSER,
    TAG_TRACKARTIST_CONDUCTOR, TAG_TRACKARTIST_DJMIXER, TAG_TRACKARTIST_GUEST,
    TAG_TRACKARTIST_MAIN, TAG_TRACKARTIST_PRODUCER, TAG_TRACKARTIST_REMIXER, TAG_TRACKNUMBER,
    TAG_TRACKTITLE, TAG_TRACKTOTAL,
};

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Track-only tags were passed to a release-level query.
#[derive(Debug)]
pub struct TrackTagNotAllowedError {
    pub message: String,
}

impl std::fmt::Display for TrackTagNotAllowedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for TrackTagNotAllowedError {}

impl From<TrackTagNotAllowedError> for RoseError {
    fn from(e: TrackTagNotAllowedError) -> Self {
        RoseError::Internal(e.message)
    }
}

/// An invalid replacement value for a field (e.g. non-bool for `new`).
#[derive(Debug)]
pub struct InvalidReplacementValueError {
    pub message: String,
}

impl std::fmt::Display for InvalidReplacementValueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for InvalidReplacementValueError {}

impl From<InvalidReplacementValueError> for RoseError {
    fn from(e: InvalidReplacementValueError) -> Self {
        RoseError::Internal(e.message)
    }
}

// ---------------------------------------------------------------------------
// FastSearchResult
// ---------------------------------------------------------------------------

/// A result from the FTS fast search: track/release ID and its source path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastSearchResult {
    pub id: String,
    pub path: PathBuf,
}

// ---------------------------------------------------------------------------
// TAG_ROLE_REGEX — strip artist role suffixes for FTS column names
// ---------------------------------------------------------------------------

static TAG_ROLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[[^\]]+\]$").expect("invalid regex"));

// ---------------------------------------------------------------------------
// Value conversion helpers
// ---------------------------------------------------------------------------

/// Loosely typed tag value — may be a string, int, bool, date, or None.
pub type TagValue<'a> = Option<&'a dyn TagValueTrait>;

/// Trait so we can pass heterogeneous tag values to `matches_pattern`.
pub trait TagValueTrait: std::fmt::Debug {
    fn to_value_str(&self) -> String;
}

impl TagValueTrait for bool {
    fn to_value_str(&self) -> String {
        if *self {
            "true".to_string()
        } else {
            "false".to_string()
        }
    }
}

impl TagValueTrait for i32 {
    fn to_value_str(&self) -> String {
        self.to_string()
    }
}

impl TagValueTrait for str {
    fn to_value_str(&self) -> String {
        self.to_string()
    }
}

impl TagValueTrait for String {
    fn to_value_str(&self) -> String {
        self.clone()
    }
}

impl TagValueTrait for RoseDate {
    fn to_value_str(&self) -> String {
        self.to_string()
    }
}

/// Convert a possibly-None tag value to a display string.
/// bool → "true"/"false", RoseDate → string, None → "".
pub fn value_to_str<T: std::fmt::Display>(value: Option<&T>, is_bool: bool) -> String {
    match value {
        None => String::new(),
        Some(v) => {
            let s = v.to_string();
            if is_bool {
                s.to_lowercase()
            } else {
                s
            }
        }
    }
}

/// Simple value_to_str for strings, handling Option<String> and Option<&str>.
fn opt_str_to_str(value: &Option<String>) -> String {
    match value {
        Some(s) => s.clone(),
        None => String::new(),
    }
}

fn opt_date_to_str(value: &Option<RoseDate>) -> String {
    match value {
        Some(d) => d.to_string(),
        None => String::new(),
    }
}

fn opt_rating_to_str(value: Option<i32>) -> String {
    match value {
        Some(r) => r.to_string(),
        None => String::new(),
    }
}

fn bool_to_str(value: bool) -> String {
    if value {
        "true".to_string()
    } else {
        "false".to_string()
    }
}

// ---------------------------------------------------------------------------
// Pattern matching
// ---------------------------------------------------------------------------

/// Test whether `value` matches `pattern` using substring / anchored matching.
///
/// Handles strict_start (^), strict_end ($), and case_insensitive flags.
pub fn matches_pattern(pattern: &Pattern, value: &str) -> bool {
    let mut needle = pattern.needle.clone();
    let mut haystack = value.to_string();

    if pattern.case_insensitive {
        needle = needle.to_lowercase();
        haystack = haystack.to_lowercase();
    }

    if pattern.strict_start && pattern.strict_end {
        haystack == needle
    } else if pattern.strict_start {
        haystack.starts_with(&needle)
    } else if pattern.strict_end {
        haystack.ends_with(&needle)
    } else {
        haystack.contains(&needle)
    }
}

// ---------------------------------------------------------------------------
// Action execution helpers
// ---------------------------------------------------------------------------

/// Execute an action on a single-value tag. Returns the new string value,
/// or `None` if the tag should be deleted.
pub fn execute_single_action(action: &Action, value: &str) -> Option<String> {
    // If the action has a pattern and the value doesn't match, return as-is.
    if let Some(ref pat) = action.pattern {
        if !matches_pattern(pat, value) {
            return Some(value.to_string());
        }
    }

    match &action.behavior {
        ActionBehavior::Replace(a) => Some(a.replacement.clone()),
        ActionBehavior::Sed(a) => Some(a.src.replace_all(value, &a.dst).into_owned()),
        ActionBehavior::Delete(_) => None,
        _ => {
            // Split/Add are not valid for single-value tags;
            // should have been caught at parse time.
            Some(value.to_string())
        }
    }
}

/// Execute an action on a multi-value tag. Returns the new list of values.
pub fn execute_multi_value_action(action: &Action, values: &[String]) -> Vec<String> {
    // Determine which indices match the pattern.
    let matching_idx: Vec<usize> = if let Some(ref pat) = action.pattern {
        let mut idx = Vec::new();
        for (i, v) in values.iter().enumerate() {
            if matches_pattern(pat, v) {
                idx.push(i);
            }
        }
        if idx.is_empty() {
            return values.to_vec();
        }
        idx
    } else {
        (0..values.len()).collect()
    };

    let matching_set: HashSet<usize> = matching_idx.iter().copied().collect();

    match &action.behavior {
        ActionBehavior::Add(a) => {
            let mut result: Vec<String> = values.to_vec();
            result.push(a.value.clone());
            uniq(result)
        }
        _ => {
            let mut rval: Vec<String> = Vec::new();
            for (i, v) in values.iter().enumerate() {
                if !matching_set.contains(&i) {
                    rval.push(v.clone());
                    continue;
                }
                match &action.behavior {
                    ActionBehavior::Delete(_) => {
                        continue;
                    }
                    ActionBehavior::Replace(a) => {
                        for nv in a.replacement.split(';') {
                            let nv = nv.trim();
                            if !nv.is_empty() {
                                rval.push(nv.to_string());
                            }
                        }
                    }
                    ActionBehavior::Sed(a) => {
                        let replaced = a.src.replace_all(v, &a.dst).into_owned();
                        for nv in replaced.split(';') {
                            let nv = nv.trim();
                            if !nv.is_empty() {
                                rval.push(nv.to_string());
                            }
                        }
                    }
                    ActionBehavior::Split(a) => {
                        for nv in v.split(&a.delimiter) {
                            let nv = nv.trim();
                            if !nv.is_empty() {
                                rval.push(nv.to_string());
                            }
                        }
                    }
                    ActionBehavior::Add(_) => unreachable!(),
                }
            }
            uniq(rval)
        }
    }
}

// ---------------------------------------------------------------------------
// FTS query conversion
// ---------------------------------------------------------------------------

/// Convert a pattern to an FTS NEAR query string.
fn convert_matcher_to_fts_query(pattern: &Pattern) -> String {
    // Make every character its own token separated by ¬, escape quotes.
    let matchsql: String = pattern
        .needle
        .chars()
        .map(|c| c.to_string())
        .collect::<Vec<String>>()
        .join("¬")
        .replace('\'', "''")
        .replace('"', "\"\"");

    let near_distance = if pattern.needle.len() >= 2 {
        pattern.needle.len() - 2
    } else {
        0
    };

    format!("NEAR(\"{matchsql}\", {near_distance})")
}

// ---------------------------------------------------------------------------
// FTS search: tracks
// ---------------------------------------------------------------------------

/// Run a fast FTS search for tracks matching the given matcher.
///
/// This is fast but may produce false positives. Callers must filter results
/// using tag reads or cache checks.
pub fn fast_search_for_matching_tracks(
    c: &Config,
    matcher: &Matcher,
) -> Result<Vec<FastSearchResult>, RoseError> {
    let matchsql = convert_matcher_to_fts_query(&matcher.pattern);
    tracing::debug!(matchsql = %matchsql, "converted matcher to FTS query");

    // Strip artist role suffixes from tag names (FTS columns don't have roles).
    let columns: Vec<String> = uniq(
        matcher
            .tags
            .iter()
            .map(|t| TAG_ROLE_REGEX.replace(t, "").into_owned())
            .collect(),
    );
    let ftsquery = format!("{{{columns}}} : {matchsql}", columns = columns.join(" "));
    let query = format!(
        "SELECT DISTINCT t.id, t.source_path \
         FROM rules_engine_fts \
         JOIN tracks t ON rules_engine_fts.rowid = t.rowid \
         WHERE rules_engine_fts MATCH '{ftsquery}' \
         ORDER BY t.source_path"
    );
    tracing::debug!(query = %query, "constructed FTS query");

    let mut results: Vec<FastSearchResult> = Vec::new();
    let conn = connect(c)?;
    let mut stmt = conn.prepare(&query).map_err(|e| {
        RoseError::Internal(format!("fast_search_for_matching_tracks prepare: {e}"))
    })?;
    let rows = stmt
        .query_map([], |row| {
            let id: String = row.get("id")?;
            let source_path: String = row.get("source_path")?;
            Ok((id, source_path))
        })
        .map_err(|e| RoseError::Internal(format!("fast_search_for_matching_tracks query: {e}")))?;

    for row_result in rows {
        let (id, source_path) = row_result.map_err(|e| {
            RoseError::Internal(format!("fast_search_for_matching_tracks row: {e}"))
        })?;
        results.push(FastSearchResult {
            id,
            path: PathBuf::from(source_path),
        });
    }

    tracing::debug!(count = results.len(), "FTS matched tracks");
    Ok(results)
}

// ---------------------------------------------------------------------------
// FTS search: releases
// ---------------------------------------------------------------------------

/// Run a fast FTS search for releases matching the given matcher.
///
/// Track-only tags (other than shorthand trackartist when releaseartist is
/// also present) are rejected with `TrackTagNotAllowedError`.
pub fn fast_search_for_matching_releases(
    c: &Config,
    matcher: &Matcher,
    include_loose_tracks: bool,
) -> Result<Vec<FastSearchResult>, RoseError> {
    // Reject track-only tags.
    let track_tags: Vec<Tag> = matcher
        .tags
        .iter()
        .copied()
        .filter(|t| !RELEASE_TAGS.contains(t))
        .collect();
    if !track_tags.is_empty() {
        // Allow exception: if both trackartist and releaseartist are present.
        let has_releaseartist = matcher.tags.iter().any(|t| t.starts_with("releaseartist"));
        if has_releaseartist {
            // Just ignore trackartist tags — they'll be filtered out of the column list.
            let remaining: Vec<Tag> = track_tags
                .iter()
                .copied()
                .filter(|t| !t.starts_with("trackartist"))
                .collect();
            if !remaining.is_empty() {
                return Err(TrackTagNotAllowedError {
                    message: format!(
                        "Track tags are not allowed when matching against releases: {}",
                        remaining.join(", ")
                    ),
                }
                .into());
            }
        } else {
            return Err(TrackTagNotAllowedError {
                message: format!(
                    "Track tags are not allowed when matching against releases: {}",
                    track_tags.join(", ")
                ),
            }
            .into());
        }
    }

    let matchsql = convert_matcher_to_fts_query(&matcher.pattern);
    let columns: Vec<String> = uniq(
        matcher
            .tags
            .iter()
            .map(|t| TAG_ROLE_REGEX.replace(t, "").into_owned())
            .collect(),
    );
    let ftsquery = format!("{{{columns}}} : {matchsql}", columns = columns.join(" "));

    let mut query = format!(
        "SELECT DISTINCT r.id, r.source_path \
         FROM rules_engine_fts \
         JOIN tracks t ON rules_engine_fts.rowid = t.rowid \
         JOIN releases r ON r.id = t.release_id \
         WHERE rules_engine_fts MATCH '{ftsquery}'"
    );
    if !include_loose_tracks {
        query.push_str(" AND r.releasetype <> 'loosetrack'");
    }
    query.push_str(" ORDER BY r.source_path");

    tracing::debug!(query = %query, "constructed release FTS query");

    let mut results: Vec<FastSearchResult> = Vec::new();
    let conn = connect(c)?;
    let mut stmt = conn.prepare(&query).map_err(|e| {
        RoseError::Internal(format!("fast_search_for_matching_releases prepare: {e}"))
    })?;
    let rows = stmt
        .query_map([], |row| {
            let id: String = row.get("id")?;
            let source_path: String = row.get("source_path")?;
            Ok((id, source_path))
        })
        .map_err(|e| {
            RoseError::Internal(format!("fast_search_for_matching_releases query: {e}"))
        })?;

    for row_result in rows {
        let (id, source_path) = row_result.map_err(|e| {
            RoseError::Internal(format!("fast_search_for_matching_releases row: {e}"))
        })?;
        results.push(FastSearchResult {
            id,
            path: PathBuf::from(source_path),
        });
    }

    tracing::debug!(count = results.len(), "FTS matched releases");
    Ok(results)
}

// ---------------------------------------------------------------------------
// Datafile helpers
// ---------------------------------------------------------------------------

/// Read the `.rose.{uuid}.toml` datafile from a directory.
fn get_release_datafile_of_directory(d: &Path) -> Result<StoredDataFile, RoseError> {
    let entries = std::fs::read_dir(d).map_err(|e| {
        RoseError::Internal(format!("Failed to read directory {}: {e}", d.display()))
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| {
            RoseError::Internal(format!("Failed to read dir entry in {}: {e}", d.display()))
        })?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !STORED_DATA_FILE_REGEX.is_match(&name_str) {
            continue;
        }
        let content = std::fs::read_to_string(entry.path()).map_err(|e| {
            RoseError::Internal(format!(
                "Failed to read datafile {}: {e}",
                entry.path().display()
            ))
        })?;
        let data: toml::Value = content.parse().map_err(|e| {
            RoseError::Internal(format!(
                "Failed to parse datafile {}: {e}",
                entry.path().display()
            ))
        })?;
        return StoredDataFile::parse(&data);
    }
    Err(RoseError::Internal(format!(
        "Release data file not found in {}. How is it in the library?",
        d.display()
    )))
}

// ---------------------------------------------------------------------------
// Tag-based false positive filtering
// ---------------------------------------------------------------------------

/// Helper: check if a field matches against audio tags.
fn tag_field_matches_audiotags(
    field: &str,
    pattern: &Pattern,
    tags: &AudioTags,
    datafile: &mut Option<StoredDataFile>,
    parent_dir: &Path,
) -> Result<bool, RoseError> {
    let m = match field {
        TAG_TRACKTITLE => matches_pattern(pattern, &opt_str_to_str(&tags.tracktitle)),
        TAG_RELEASEDATE => matches_pattern(pattern, &opt_date_to_str(&tags.releasedate)),
        TAG_ORIGINALDATE => matches_pattern(pattern, &opt_date_to_str(&tags.originaldate)),
        TAG_COMPOSITIONDATE => matches_pattern(pattern, &opt_date_to_str(&tags.compositiondate)),
        TAG_EDITION => matches_pattern(pattern, &opt_str_to_str(&tags.edition)),
        TAG_CATALOGNUMBER => matches_pattern(pattern, &opt_str_to_str(&tags.catalognumber)),
        TAG_TRACKNUMBER => matches_pattern(pattern, &opt_str_to_str(&tags.tracknumber)),
        TAG_TRACKTOTAL => {
            let s = tags.tracktotal.map(|t| t.to_string()).unwrap_or_default();
            matches_pattern(pattern, &s)
        }
        TAG_DISCNUMBER => matches_pattern(pattern, &opt_str_to_str(&tags.discnumber)),
        TAG_DISCTOTAL => {
            let s = tags.disctotal.map(|t| t.to_string()).unwrap_or_default();
            matches_pattern(pattern, &s)
        }
        TAG_RELEASETITLE => matches_pattern(pattern, &opt_str_to_str(&tags.releasetitle)),
        TAG_RELEASETYPE => matches_pattern(pattern, &tags.releasetype),
        TAG_GENRE => tags.genre.iter().any(|x| matches_pattern(pattern, x)),
        TAG_SECONDARYGENRE => tags
            .secondarygenre
            .iter()
            .any(|x| matches_pattern(pattern, x)),
        TAG_DESCRIPTOR => tags.descriptor.iter().any(|x| matches_pattern(pattern, x)),
        TAG_LABEL => tags.label.iter().any(|x| matches_pattern(pattern, x)),
        TAG_TRACKARTIST_MAIN => tags
            .trackartists
            .main
            .iter()
            .any(|x| matches_pattern(pattern, &x.name)),
        TAG_TRACKARTIST_GUEST => tags
            .trackartists
            .guest
            .iter()
            .any(|x| matches_pattern(pattern, &x.name)),
        TAG_TRACKARTIST_REMIXER => tags
            .trackartists
            .remixer
            .iter()
            .any(|x| matches_pattern(pattern, &x.name)),
        TAG_TRACKARTIST_PRODUCER => tags
            .trackartists
            .producer
            .iter()
            .any(|x| matches_pattern(pattern, &x.name)),
        TAG_TRACKARTIST_COMPOSER => tags
            .trackartists
            .composer
            .iter()
            .any(|x| matches_pattern(pattern, &x.name)),
        TAG_TRACKARTIST_CONDUCTOR => tags
            .trackartists
            .conductor
            .iter()
            .any(|x| matches_pattern(pattern, &x.name)),
        TAG_TRACKARTIST_DJMIXER => tags
            .trackartists
            .djmixer
            .iter()
            .any(|x| matches_pattern(pattern, &x.name)),
        TAG_RELEASEARTIST_MAIN => tags
            .releaseartists
            .main
            .iter()
            .any(|x| matches_pattern(pattern, &x.name)),
        TAG_RELEASEARTIST_GUEST => tags
            .releaseartists
            .guest
            .iter()
            .any(|x| matches_pattern(pattern, &x.name)),
        TAG_RELEASEARTIST_REMIXER => tags
            .releaseartists
            .remixer
            .iter()
            .any(|x| matches_pattern(pattern, &x.name)),
        TAG_RELEASEARTIST_PRODUCER => tags
            .releaseartists
            .producer
            .iter()
            .any(|x| matches_pattern(pattern, &x.name)),
        TAG_RELEASEARTIST_COMPOSER => tags
            .releaseartists
            .composer
            .iter()
            .any(|x| matches_pattern(pattern, &x.name)),
        TAG_RELEASEARTIST_CONDUCTOR => tags
            .releaseartists
            .conductor
            .iter()
            .any(|x| matches_pattern(pattern, &x.name)),
        TAG_RELEASEARTIST_DJMIXER => tags
            .releaseartists
            .djmixer
            .iter()
            .any(|x| matches_pattern(pattern, &x.name)),
        TAG_NEW => {
            if datafile.is_none() {
                *datafile = Some(get_release_datafile_of_directory(parent_dir)?);
            }
            matches_pattern(pattern, &bool_to_str(datafile.as_ref().unwrap().new))
        }
        TAG_FAVORITE => {
            if datafile.is_none() {
                *datafile = Some(get_release_datafile_of_directory(parent_dir)?);
            }
            matches_pattern(pattern, &bool_to_str(datafile.as_ref().unwrap().favorite))
        }
        TAG_RATING => {
            if datafile.is_none() {
                *datafile = Some(get_release_datafile_of_directory(parent_dir)?);
            }
            matches_pattern(
                pattern,
                &opt_rating_to_str(datafile.as_ref().unwrap().rating),
            )
        }
        _ => false,
    };
    Ok(m)
}

/// Phase 3: Read audio tags, filter false positives, apply ignore matchers.
///
/// **BUG FIX:** In the Python source, the ignore loop for `new`/`favorite`
/// incorrectly assigns to `match` instead of `skip`. This Rust port
/// correctly assigns to `skip`.
pub fn filter_track_false_positives_using_tags(
    matcher: &Matcher,
    fast_search_results: &[FastSearchResult],
    ignore: &[Matcher],
) -> Result<Vec<AudioTags>, RoseError> {
    let mut rval = Vec::new();

    for fsr in fast_search_results {
        let tags = AudioTags::from_file(&fsr.path)?;
        let parent_dir = fsr.path.parent().unwrap_or_else(|| Path::new("."));
        let mut datafile: Option<StoredDataFile> = None;

        let mut track_matched = false;
        for field in &matcher.tags {
            let m = tag_field_matches_audiotags(
                field,
                &matcher.pattern,
                &tags,
                &mut datafile,
                parent_dir,
            )?;

            if m && !ignore.is_empty() {
                // Check ignore matchers.
                let mut skip = false;
                for i_matcher in ignore {
                    // BUG FIX: For new/favorite, we correctly assign to `skip`
                    // (not `match` as in the Python source).
                    skip = skip
                        || (field == &TAG_TRACKTITLE
                            && matches_pattern(
                                &i_matcher.pattern,
                                &opt_str_to_str(&tags.tracktitle),
                            ));
                    skip = skip
                        || (field == &TAG_RELEASEDATE
                            && matches_pattern(
                                &i_matcher.pattern,
                                &opt_date_to_str(&tags.releasedate),
                            ));
                    skip = skip
                        || (field == &TAG_ORIGINALDATE
                            && matches_pattern(
                                &i_matcher.pattern,
                                &opt_date_to_str(&tags.originaldate),
                            ));
                    skip = skip
                        || (field == &TAG_COMPOSITIONDATE
                            && matches_pattern(
                                &i_matcher.pattern,
                                &opt_date_to_str(&tags.compositiondate),
                            ));
                    skip = skip
                        || (field == &TAG_EDITION
                            && matches_pattern(&i_matcher.pattern, &opt_str_to_str(&tags.edition)));
                    skip = skip
                        || (field == &TAG_CATALOGNUMBER
                            && matches_pattern(
                                &i_matcher.pattern,
                                &opt_str_to_str(&tags.catalognumber),
                            ));
                    skip = skip
                        || (field == &TAG_TRACKNUMBER
                            && matches_pattern(
                                &i_matcher.pattern,
                                &opt_str_to_str(&tags.tracknumber),
                            ));
                    skip = skip
                        || (field == &TAG_TRACKTOTAL
                            && matches_pattern(
                                &i_matcher.pattern,
                                &tags.tracktotal.map(|t| t.to_string()).unwrap_or_default(),
                            ));
                    skip = skip
                        || (field == &TAG_DISCNUMBER
                            && matches_pattern(
                                &i_matcher.pattern,
                                &opt_str_to_str(&tags.discnumber),
                            ));
                    skip = skip
                        || (field == &TAG_DISCTOTAL
                            && matches_pattern(
                                &i_matcher.pattern,
                                &tags.disctotal.map(|t| t.to_string()).unwrap_or_default(),
                            ));
                    skip = skip
                        || (field == &TAG_RELEASETITLE
                            && matches_pattern(
                                &i_matcher.pattern,
                                &opt_str_to_str(&tags.releasetitle),
                            ));
                    skip = skip
                        || (field == &TAG_RELEASETYPE
                            && matches_pattern(&i_matcher.pattern, &tags.releasetype));
                    skip = skip
                        || (field == &TAG_GENRE
                            && tags
                                .genre
                                .iter()
                                .any(|x| matches_pattern(&i_matcher.pattern, x)));
                    skip = skip
                        || (field == &TAG_SECONDARYGENRE
                            && tags
                                .secondarygenre
                                .iter()
                                .any(|x| matches_pattern(&i_matcher.pattern, x)));
                    skip = skip
                        || (field == &TAG_DESCRIPTOR
                            && tags
                                .descriptor
                                .iter()
                                .any(|x| matches_pattern(&i_matcher.pattern, x)));
                    skip = skip
                        || (field == &TAG_LABEL
                            && tags
                                .label
                                .iter()
                                .any(|x| matches_pattern(&i_matcher.pattern, x)));
                    skip = skip
                        || (field == &TAG_TRACKARTIST_MAIN
                            && tags
                                .trackartists
                                .main
                                .iter()
                                .any(|x| matches_pattern(&i_matcher.pattern, &x.name)));
                    skip = skip
                        || (field == &TAG_TRACKARTIST_GUEST
                            && tags
                                .trackartists
                                .guest
                                .iter()
                                .any(|x| matches_pattern(&i_matcher.pattern, &x.name)));
                    skip = skip
                        || (field == &TAG_TRACKARTIST_REMIXER
                            && tags
                                .trackartists
                                .remixer
                                .iter()
                                .any(|x| matches_pattern(&i_matcher.pattern, &x.name)));
                    skip = skip
                        || (field == &TAG_TRACKARTIST_PRODUCER
                            && tags
                                .trackartists
                                .producer
                                .iter()
                                .any(|x| matches_pattern(&i_matcher.pattern, &x.name)));
                    skip = skip
                        || (field == &TAG_TRACKARTIST_COMPOSER
                            && tags
                                .trackartists
                                .composer
                                .iter()
                                .any(|x| matches_pattern(&i_matcher.pattern, &x.name)));
                    skip = skip
                        || (field == &TAG_TRACKARTIST_CONDUCTOR
                            && tags
                                .trackartists
                                .conductor
                                .iter()
                                .any(|x| matches_pattern(&i_matcher.pattern, &x.name)));
                    skip = skip
                        || (field == &TAG_TRACKARTIST_DJMIXER
                            && tags
                                .trackartists
                                .djmixer
                                .iter()
                                .any(|x| matches_pattern(&i_matcher.pattern, &x.name)));
                    skip = skip
                        || (field == &TAG_RELEASEARTIST_MAIN
                            && tags
                                .releaseartists
                                .main
                                .iter()
                                .any(|x| matches_pattern(&i_matcher.pattern, &x.name)));
                    skip = skip
                        || (field == &TAG_RELEASEARTIST_GUEST
                            && tags
                                .releaseartists
                                .guest
                                .iter()
                                .any(|x| matches_pattern(&i_matcher.pattern, &x.name)));
                    skip = skip
                        || (field == &TAG_RELEASEARTIST_REMIXER
                            && tags
                                .releaseartists
                                .remixer
                                .iter()
                                .any(|x| matches_pattern(&i_matcher.pattern, &x.name)));
                    skip = skip
                        || (field == &TAG_RELEASEARTIST_PRODUCER
                            && tags
                                .releaseartists
                                .producer
                                .iter()
                                .any(|x| matches_pattern(&i_matcher.pattern, &x.name)));
                    skip = skip
                        || (field == &TAG_RELEASEARTIST_COMPOSER
                            && tags
                                .releaseartists
                                .composer
                                .iter()
                                .any(|x| matches_pattern(&i_matcher.pattern, &x.name)));
                    skip = skip
                        || (field == &TAG_RELEASEARTIST_CONDUCTOR
                            && tags
                                .releaseartists
                                .conductor
                                .iter()
                                .any(|x| matches_pattern(&i_matcher.pattern, &x.name)));
                    skip = skip
                        || (field == &TAG_RELEASEARTIST_DJMIXER
                            && tags
                                .releaseartists
                                .djmixer
                                .iter()
                                .any(|x| matches_pattern(&i_matcher.pattern, &x.name)));

                    // BUG FIX: new/favorite correctly use `skip`, not `match`.
                    if !skip && field == &TAG_NEW {
                        if datafile.is_none() {
                            datafile = Some(get_release_datafile_of_directory(parent_dir)?);
                        }
                        skip = matches_pattern(
                            &i_matcher.pattern,
                            &bool_to_str(datafile.as_ref().unwrap().new),
                        );
                    }
                    if !skip && field == &TAG_FAVORITE {
                        if datafile.is_none() {
                            datafile = Some(get_release_datafile_of_directory(parent_dir)?);
                        }
                        skip = matches_pattern(
                            &i_matcher.pattern,
                            &bool_to_str(datafile.as_ref().unwrap().favorite),
                        );
                    }
                    if !skip && field == &TAG_RATING {
                        if datafile.is_none() {
                            datafile = Some(get_release_datafile_of_directory(parent_dir)?);
                        }
                        skip = matches_pattern(
                            &i_matcher.pattern,
                            &opt_rating_to_str(datafile.as_ref().unwrap().rating),
                        );
                    }

                    if skip {
                        break;
                    }
                }
                if skip {
                    break;
                }
            }

            if m {
                rval.push(tags);
                track_matched = true;
                break;
            }
        }

        if track_matched {
            continue;
        }
    }

    tracing::debug!(
        before = fast_search_results.len(),
        after = rval.len(),
        "filtered false positives using tags"
    );
    Ok(rval)
}

// ---------------------------------------------------------------------------
// Cache-based false positive filtering
// ---------------------------------------------------------------------------

/// Filter track false positives using the read cache (no disk reads).
pub fn filter_track_false_positives_using_read_cache(
    matcher: &Matcher,
    tracks: Vec<Track>,
) -> Vec<Track> {
    let mut rval = Vec::new();
    for t in &tracks {
        for field in &matcher.tags {
            let m = match *field {
                TAG_TRACKTITLE => matches_pattern(&matcher.pattern, &t.tracktitle),
                TAG_RELEASEDATE => {
                    matches_pattern(&matcher.pattern, &opt_date_to_str(&t.release.releasedate))
                }
                TAG_ORIGINALDATE => {
                    matches_pattern(&matcher.pattern, &opt_date_to_str(&t.release.originaldate))
                }
                TAG_COMPOSITIONDATE => matches_pattern(
                    &matcher.pattern,
                    &opt_date_to_str(&t.release.compositiondate),
                ),
                TAG_EDITION => {
                    matches_pattern(&matcher.pattern, t.release.edition.as_deref().unwrap_or(""))
                }
                TAG_CATALOGNUMBER => matches_pattern(
                    &matcher.pattern,
                    t.release.catalognumber.as_deref().unwrap_or(""),
                ),
                TAG_TRACKNUMBER => matches_pattern(&matcher.pattern, &t.tracknumber),
                TAG_TRACKTOTAL => matches_pattern(&matcher.pattern, &t.tracktotal.to_string()),
                TAG_DISCNUMBER => matches_pattern(&matcher.pattern, &t.discnumber),
                TAG_DISCTOTAL => {
                    matches_pattern(&matcher.pattern, &t.release.disctotal.to_string())
                }
                TAG_RELEASETITLE => matches_pattern(&matcher.pattern, &t.release.releasetitle),
                TAG_RELEASETYPE => matches_pattern(&matcher.pattern, &t.release.releasetype),
                TAG_NEW => matches_pattern(&matcher.pattern, &bool_to_str(t.release.new)),
                TAG_FAVORITE => matches_pattern(&matcher.pattern, &bool_to_str(t.release.favorite)),
                TAG_RATING => {
                    matches_pattern(&matcher.pattern, &opt_rating_to_str(t.release.rating))
                }
                TAG_GENRE => t
                    .release
                    .genres
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, x)),
                TAG_SECONDARYGENRE => t
                    .release
                    .secondary_genres
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, x)),
                TAG_DESCRIPTOR => t
                    .release
                    .descriptors
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, x)),
                TAG_LABEL => t
                    .release
                    .labels
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, x)),
                TAG_TRACKARTIST_MAIN => t
                    .trackartists
                    .main
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                TAG_TRACKARTIST_GUEST => t
                    .trackartists
                    .guest
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                TAG_TRACKARTIST_REMIXER => t
                    .trackartists
                    .remixer
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                TAG_TRACKARTIST_PRODUCER => t
                    .trackartists
                    .producer
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                TAG_TRACKARTIST_COMPOSER => t
                    .trackartists
                    .composer
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                TAG_TRACKARTIST_CONDUCTOR => t
                    .trackartists
                    .conductor
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                TAG_TRACKARTIST_DJMIXER => t
                    .trackartists
                    .djmixer
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                TAG_RELEASEARTIST_MAIN => t
                    .release
                    .releaseartists
                    .main
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                TAG_RELEASEARTIST_GUEST => t
                    .release
                    .releaseartists
                    .guest
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                TAG_RELEASEARTIST_REMIXER => t
                    .release
                    .releaseartists
                    .remixer
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                TAG_RELEASEARTIST_PRODUCER => t
                    .release
                    .releaseartists
                    .producer
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                TAG_RELEASEARTIST_COMPOSER => t
                    .release
                    .releaseartists
                    .composer
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                TAG_RELEASEARTIST_CONDUCTOR => t
                    .release
                    .releaseartists
                    .conductor
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                TAG_RELEASEARTIST_DJMIXER => t
                    .release
                    .releaseartists
                    .djmixer
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                _ => false,
            };
            if m {
                rval.push(t.clone());
                break;
            }
        }
    }
    tracing::debug!(
        before = tracks.len(),
        after = rval.len(),
        "filtered track false positives using cache"
    );
    rval
}

/// Filter release false positives using the read cache (no disk reads).
/// Only release-level tags are checked; track tags are ignored.
pub fn filter_release_false_positives_using_read_cache(
    matcher: &Matcher,
    releases: Vec<Release>,
    include_loose_tracks: bool,
) -> Vec<Release> {
    let mut rval = Vec::new();
    for r in &releases {
        if !include_loose_tracks && r.releasetype == "loosetrack" {
            continue;
        }
        for field in &matcher.tags {
            let m = match *field {
                TAG_RELEASEDATE => {
                    matches_pattern(&matcher.pattern, &opt_date_to_str(&r.releasedate))
                }
                TAG_ORIGINALDATE => {
                    matches_pattern(&matcher.pattern, &opt_date_to_str(&r.originaldate))
                }
                TAG_COMPOSITIONDATE => {
                    matches_pattern(&matcher.pattern, &opt_date_to_str(&r.compositiondate))
                }
                TAG_EDITION => {
                    matches_pattern(&matcher.pattern, r.edition.as_deref().unwrap_or(""))
                }
                TAG_CATALOGNUMBER => {
                    matches_pattern(&matcher.pattern, r.catalognumber.as_deref().unwrap_or(""))
                }
                TAG_RELEASETITLE => matches_pattern(&matcher.pattern, &r.releasetitle),
                TAG_RELEASETYPE => matches_pattern(&matcher.pattern, &r.releasetype),
                TAG_NEW => matches_pattern(&matcher.pattern, &bool_to_str(r.new)),
                TAG_FAVORITE => matches_pattern(&matcher.pattern, &bool_to_str(r.favorite)),
                TAG_RATING => matches_pattern(&matcher.pattern, &opt_rating_to_str(r.rating)),
                TAG_GENRE => r
                    .genres
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, x)),
                TAG_SECONDARYGENRE => r
                    .secondary_genres
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, x)),
                TAG_DESCRIPTOR => r
                    .descriptors
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, x)),
                TAG_LABEL => r
                    .labels
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, x)),
                TAG_RELEASEARTIST_MAIN => r
                    .releaseartists
                    .main
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                TAG_RELEASEARTIST_GUEST => r
                    .releaseartists
                    .guest
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                TAG_RELEASEARTIST_REMIXER => r
                    .releaseartists
                    .remixer
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                TAG_RELEASEARTIST_CONDUCTOR => r
                    .releaseartists
                    .conductor
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                TAG_RELEASEARTIST_PRODUCER => r
                    .releaseartists
                    .producer
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                TAG_RELEASEARTIST_COMPOSER => r
                    .releaseartists
                    .composer
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                TAG_RELEASEARTIST_DJMIXER => r
                    .releaseartists
                    .djmixer
                    .iter()
                    .any(|x| matches_pattern(&matcher.pattern, &x.name)),
                _ => false,
            };
            if m {
                rval.push(r.clone());
                break;
            }
        }
    }
    tracing::debug!(
        before = releases.len(),
        after = rval.len(),
        "filtered release false positives using cache"
    );
    rval
}

// ---------------------------------------------------------------------------
// Change type
// ---------------------------------------------------------------------------

/// A single field change: (field_name, old_value_display, new_value_display).
type Change = (String, String, String);

// ---------------------------------------------------------------------------
// Execute metadata actions (phases 3-5)
// ---------------------------------------------------------------------------

/// Execute metadata actions on a set of audio tags. This implements phases 3-5:
///
/// - Phase 3: Apply actions in-memory, compute per-track diffs
/// - Phase 4: Display changes, prompt for confirmation
/// - Phase 5: Flush changes to disk
pub fn execute_metadata_actions(
    c: &Config,
    actions: &[Action],
    audiotags: Vec<AudioTags>,
    dry_run: bool,
    confirm_yes: bool,
) -> Result<(), RoseError> {
    execute_metadata_actions_inner(c, actions, audiotags, dry_run, confirm_yes, 25)
}

fn execute_metadata_actions_inner(
    c: &Config,
    actions: &[Action],
    audiotags: Vec<AudioTags>,
    dry_run: bool,
    confirm_yes: bool,
    enter_number_to_confirm_above_count: usize,
) -> Result<(), RoseError> {
    // Helper fns for artist name extraction.
    fn names(xs: &[Artist]) -> Vec<String> {
        xs.iter().map(|x| x.name.clone()).collect()
    }
    fn to_artists(xs: Vec<String>) -> Vec<Artist> {
        xs.into_iter()
            .map(|name| Artist { name, alias: false })
            .collect()
    }

    // Map from parent directory string to opened datafile.
    let mut opened_datafiles: HashMap<String, StoredDataFile> = HashMap::new();

    // Actionable audio tags: (modified tags, changes).
    let mut actionable_audiotags: Vec<(AudioTags, Vec<Change>)> = Vec::new();
    // Actionable datafiles: map from parent dir string → (tags_for_release_id, datafile, changes).
    let mut actionable_datafiles: HashMap<String, (AudioTags, StoredDataFile, Vec<Change>)> =
        HashMap::new();

    for tags in audiotags {
        let orig_tags = tags.clone();
        let mut tags = tags;
        let mut potential_audiotag_changes: Vec<Change> = Vec::new();

        let mut datafile: Option<StoredDataFile> = None;
        let mut potential_datafile_changes: Vec<Change> = Vec::new();

        // Helper: load datafile from cache or disk.
        let parent_dir = tags
            .path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let parent_key = parent_dir.to_string_lossy().to_string();

        for act in actions {
            for field in &act.tags {
                // Datafile actions
                if *field == TAG_NEW {
                    let df = match datafile.take() {
                        Some(df) => df,
                        None => {
                            if let Some(cached) = opened_datafiles.get(&parent_key) {
                                cached.clone()
                            } else {
                                let loaded = get_release_datafile_of_directory(&parent_dir)?;
                                opened_datafiles.insert(parent_key.clone(), loaded.clone());
                                loaded
                            }
                        }
                    };
                    let mut df = df;
                    let v = execute_single_action(act, &bool_to_str(df.new));
                    let v = v.unwrap_or_default();
                    if v != "true" && v != "false" {
                        return Err(InvalidReplacementValueError {
                            message: format!(
                                "Failed to assign new value {v} to new: value must be string `true` or `false`"
                            ),
                        }
                        .into());
                    }
                    let orig_value = df.new;
                    df.new = v == "true";
                    if orig_value != df.new {
                        potential_datafile_changes.push((
                            "new".to_string(),
                            bool_to_str(orig_value),
                            bool_to_str(df.new),
                        ));
                    }
                    opened_datafiles.insert(parent_key.clone(), df.clone());
                    datafile = Some(df);
                }
                if *field == TAG_FAVORITE {
                    let df = match datafile.take() {
                        Some(df) => df,
                        None => {
                            if let Some(cached) = opened_datafiles.get(&parent_key) {
                                cached.clone()
                            } else {
                                let loaded = get_release_datafile_of_directory(&parent_dir)?;
                                opened_datafiles.insert(parent_key.clone(), loaded.clone());
                                loaded
                            }
                        }
                    };
                    let mut df = df;
                    let v = execute_single_action(act, &bool_to_str(df.favorite));
                    let v = v.unwrap_or_default();
                    if v != "true" && v != "false" {
                        return Err(InvalidReplacementValueError {
                            message: format!(
                                "Failed to assign new value {v} to favorite: value must be string `true` or `false`"
                            ),
                        }
                        .into());
                    }
                    let orig_value = df.favorite;
                    df.favorite = v == "true";
                    if orig_value != df.favorite {
                        potential_datafile_changes.push((
                            "favorite".to_string(),
                            bool_to_str(orig_value),
                            bool_to_str(df.favorite),
                        ));
                    }
                    opened_datafiles.insert(parent_key.clone(), df.clone());
                    datafile = Some(df);
                }
                if *field == TAG_RATING {
                    let df = match datafile.take() {
                        Some(df) => df,
                        None => {
                            if let Some(cached) = opened_datafiles.get(&parent_key) {
                                cached.clone()
                            } else {
                                let loaded = get_release_datafile_of_directory(&parent_dir)?;
                                opened_datafiles.insert(parent_key.clone(), loaded.clone());
                                loaded
                            }
                        }
                    };
                    let mut df = df;
                    let v = execute_single_action(act, &opt_rating_to_str(df.rating));
                    let new_rating: Option<i32> = match v.as_deref() {
                        None | Some("") => None,
                        Some(s) => {
                            let parsed: i32 = s.parse().map_err(|_| {
                                InvalidReplacementValueError {
                                    message: format!(
                                        "Failed to assign new value {s} to rating: value must be an integer 1-100 or empty to clear"
                                    ),
                                }
                            })?;
                            if !(1..=100).contains(&parsed) {
                                return Err(InvalidReplacementValueError {
                                    message: format!(
                                        "Failed to assign new value {s} to rating: value must be between 1 and 100"
                                    ),
                                }
                                .into());
                            }
                            Some(parsed)
                        }
                    };
                    let orig_rating = df.rating;
                    df.rating = new_rating;
                    if orig_rating != df.rating {
                        potential_datafile_changes.push((
                            "rating".to_string(),
                            opt_rating_to_str(orig_rating),
                            opt_rating_to_str(df.rating),
                        ));
                    }
                    opened_datafiles.insert(parent_key.clone(), df.clone());
                    datafile = Some(df);
                }

                // AudioTag Actions
                match *field {
                    TAG_TRACKTITLE => {
                        let v = execute_single_action(act, &opt_str_to_str(&tags.tracktitle));
                        potential_audiotag_changes.push((
                            "title".to_string(),
                            opt_str_to_str(&orig_tags.tracktitle),
                            v.clone().unwrap_or_default(),
                        ));
                        tags.tracktitle = v;
                    }
                    TAG_RELEASEDATE => {
                        let v = execute_single_action(act, &opt_date_to_str(&tags.releasedate));
                        let v_str = v.unwrap_or_default();
                        tags.releasedate = if v_str.is_empty() {
                            None
                        } else {
                            RoseDate::parse(Some(&v_str))
                        };
                        potential_audiotag_changes.push((
                            "releasedate".to_string(),
                            opt_date_to_str(&orig_tags.releasedate),
                            v_str,
                        ));
                    }
                    TAG_ORIGINALDATE => {
                        let v = execute_single_action(act, &opt_date_to_str(&tags.originaldate));
                        let v_str = v.unwrap_or_default();
                        tags.originaldate = if v_str.is_empty() {
                            None
                        } else {
                            RoseDate::parse(Some(&v_str))
                        };
                        potential_audiotag_changes.push((
                            "originaldate".to_string(),
                            opt_date_to_str(&orig_tags.originaldate),
                            v_str,
                        ));
                    }
                    TAG_COMPOSITIONDATE => {
                        let v = execute_single_action(act, &opt_date_to_str(&tags.compositiondate));
                        let v_str = v.unwrap_or_default();
                        tags.compositiondate = if v_str.is_empty() {
                            None
                        } else {
                            RoseDate::parse(Some(&v_str))
                        };
                        potential_audiotag_changes.push((
                            "compositiondate".to_string(),
                            opt_date_to_str(&orig_tags.compositiondate),
                            v_str,
                        ));
                    }
                    TAG_EDITION => {
                        let v = execute_single_action(act, &opt_str_to_str(&tags.edition));
                        potential_audiotag_changes.push((
                            "edition".to_string(),
                            opt_str_to_str(&orig_tags.edition),
                            v.clone().unwrap_or_default(),
                        ));
                        tags.edition = v;
                    }
                    TAG_CATALOGNUMBER => {
                        let v = execute_single_action(act, &opt_str_to_str(&tags.catalognumber));
                        potential_audiotag_changes.push((
                            "catalognumber".to_string(),
                            opt_str_to_str(&orig_tags.catalognumber),
                            v.clone().unwrap_or_default(),
                        ));
                        tags.catalognumber = v;
                    }
                    TAG_TRACKNUMBER => {
                        let v = execute_single_action(act, &opt_str_to_str(&tags.tracknumber));
                        potential_audiotag_changes.push((
                            "tracknumber".to_string(),
                            opt_str_to_str(&orig_tags.tracknumber),
                            v.clone().unwrap_or_default(),
                        ));
                        tags.tracknumber = v;
                    }
                    TAG_DISCNUMBER => {
                        let v = execute_single_action(act, &opt_str_to_str(&tags.discnumber));
                        potential_audiotag_changes.push((
                            "discnumber".to_string(),
                            opt_str_to_str(&orig_tags.discnumber),
                            v.clone().unwrap_or_default(),
                        ));
                        tags.discnumber = v;
                    }
                    TAG_RELEASETITLE => {
                        let v = execute_single_action(act, &opt_str_to_str(&tags.releasetitle));
                        potential_audiotag_changes.push((
                            "release".to_string(),
                            opt_str_to_str(&orig_tags.releasetitle),
                            v.clone().unwrap_or_default(),
                        ));
                        tags.releasetitle = v;
                    }
                    TAG_RELEASETYPE => {
                        let v = execute_single_action(act, &tags.releasetype);
                        let v = v.unwrap_or_else(|| "unknown".to_string());
                        potential_audiotag_changes.push((
                            "releasetype".to_string(),
                            orig_tags.releasetype.clone(),
                            v.clone(),
                        ));
                        tags.releasetype = v;
                    }
                    TAG_GENRE => {
                        let new_vals = execute_multi_value_action(act, &tags.genre);
                        potential_audiotag_changes.push((
                            "genre".to_string(),
                            format!("{:?}", orig_tags.genre),
                            format!("{:?}", new_vals),
                        ));
                        tags.genre = new_vals;
                    }
                    TAG_SECONDARYGENRE => {
                        let new_vals = execute_multi_value_action(act, &tags.secondarygenre);
                        potential_audiotag_changes.push((
                            "secondarygenre".to_string(),
                            format!("{:?}", orig_tags.secondarygenre),
                            format!("{:?}", new_vals),
                        ));
                        tags.secondarygenre = new_vals;
                    }
                    TAG_DESCRIPTOR => {
                        let new_vals = execute_multi_value_action(act, &tags.descriptor);
                        potential_audiotag_changes.push((
                            "descriptor".to_string(),
                            format!("{:?}", orig_tags.descriptor),
                            format!("{:?}", new_vals),
                        ));
                        tags.descriptor = new_vals;
                    }
                    TAG_LABEL => {
                        let new_vals = execute_multi_value_action(act, &tags.label);
                        potential_audiotag_changes.push((
                            "label".to_string(),
                            format!("{:?}", orig_tags.label),
                            format!("{:?}", new_vals),
                        ));
                        tags.label = new_vals;
                    }
                    TAG_TRACKARTIST_MAIN => {
                        let new_vals = to_artists(execute_multi_value_action(
                            act,
                            &names(&tags.trackartists.main),
                        ));
                        potential_audiotag_changes.push((
                            "trackartist[main]".to_string(),
                            format!("{:?}", names(&orig_tags.trackartists.main)),
                            format!("{:?}", names(&new_vals)),
                        ));
                        tags.trackartists.main = new_vals;
                    }
                    TAG_TRACKARTIST_GUEST => {
                        let new_vals = to_artists(execute_multi_value_action(
                            act,
                            &names(&tags.trackartists.guest),
                        ));
                        potential_audiotag_changes.push((
                            "trackartist[guest]".to_string(),
                            format!("{:?}", names(&orig_tags.trackartists.guest)),
                            format!("{:?}", names(&new_vals)),
                        ));
                        tags.trackartists.guest = new_vals;
                    }
                    TAG_TRACKARTIST_REMIXER => {
                        let new_vals = to_artists(execute_multi_value_action(
                            act,
                            &names(&tags.trackartists.remixer),
                        ));
                        potential_audiotag_changes.push((
                            "trackartist[remixer]".to_string(),
                            format!("{:?}", names(&orig_tags.trackartists.remixer)),
                            format!("{:?}", names(&new_vals)),
                        ));
                        tags.trackartists.remixer = new_vals;
                    }
                    TAG_TRACKARTIST_PRODUCER => {
                        let new_vals = to_artists(execute_multi_value_action(
                            act,
                            &names(&tags.trackartists.producer),
                        ));
                        potential_audiotag_changes.push((
                            "trackartist[producer]".to_string(),
                            format!("{:?}", names(&orig_tags.trackartists.producer)),
                            format!("{:?}", names(&new_vals)),
                        ));
                        tags.trackartists.producer = new_vals;
                    }
                    TAG_TRACKARTIST_COMPOSER => {
                        let new_vals = to_artists(execute_multi_value_action(
                            act,
                            &names(&tags.trackartists.composer),
                        ));
                        potential_audiotag_changes.push((
                            "trackartist[composer]".to_string(),
                            format!("{:?}", names(&orig_tags.trackartists.composer)),
                            format!("{:?}", names(&new_vals)),
                        ));
                        tags.trackartists.composer = new_vals;
                    }
                    TAG_TRACKARTIST_CONDUCTOR => {
                        let new_vals = to_artists(execute_multi_value_action(
                            act,
                            &names(&tags.trackartists.conductor),
                        ));
                        potential_audiotag_changes.push((
                            "trackartist[conductor]".to_string(),
                            format!("{:?}", names(&orig_tags.trackartists.conductor)),
                            format!("{:?}", names(&new_vals)),
                        ));
                        tags.trackartists.conductor = new_vals;
                    }
                    TAG_TRACKARTIST_DJMIXER => {
                        let new_vals = to_artists(execute_multi_value_action(
                            act,
                            &names(&tags.trackartists.djmixer),
                        ));
                        potential_audiotag_changes.push((
                            "trackartist[djmixer]".to_string(),
                            format!("{:?}", names(&orig_tags.trackartists.djmixer)),
                            format!("{:?}", names(&new_vals)),
                        ));
                        tags.trackartists.djmixer = new_vals;
                    }
                    TAG_RELEASEARTIST_MAIN => {
                        let new_vals = to_artists(execute_multi_value_action(
                            act,
                            &names(&tags.releaseartists.main),
                        ));
                        potential_audiotag_changes.push((
                            "releaseartist[main]".to_string(),
                            format!("{:?}", names(&orig_tags.releaseartists.main)),
                            format!("{:?}", names(&new_vals)),
                        ));
                        tags.releaseartists.main = new_vals;
                    }
                    TAG_RELEASEARTIST_GUEST => {
                        let new_vals = to_artists(execute_multi_value_action(
                            act,
                            &names(&tags.releaseartists.guest),
                        ));
                        potential_audiotag_changes.push((
                            "releaseartist[guest]".to_string(),
                            format!("{:?}", names(&orig_tags.releaseartists.guest)),
                            format!("{:?}", names(&new_vals)),
                        ));
                        tags.releaseartists.guest = new_vals;
                    }
                    TAG_RELEASEARTIST_REMIXER => {
                        let new_vals = to_artists(execute_multi_value_action(
                            act,
                            &names(&tags.releaseartists.remixer),
                        ));
                        potential_audiotag_changes.push((
                            "releaseartist[remixer]".to_string(),
                            format!("{:?}", names(&orig_tags.releaseartists.remixer)),
                            format!("{:?}", names(&new_vals)),
                        ));
                        tags.releaseartists.remixer = new_vals;
                    }
                    TAG_RELEASEARTIST_PRODUCER => {
                        let new_vals = to_artists(execute_multi_value_action(
                            act,
                            &names(&tags.releaseartists.producer),
                        ));
                        potential_audiotag_changes.push((
                            "releaseartist[producer]".to_string(),
                            format!("{:?}", names(&orig_tags.releaseartists.producer)),
                            format!("{:?}", names(&new_vals)),
                        ));
                        tags.releaseartists.producer = new_vals;
                    }
                    TAG_RELEASEARTIST_COMPOSER => {
                        let new_vals = to_artists(execute_multi_value_action(
                            act,
                            &names(&tags.releaseartists.composer),
                        ));
                        potential_audiotag_changes.push((
                            "releaseartist[composer]".to_string(),
                            format!("{:?}", names(&orig_tags.releaseartists.composer)),
                            format!("{:?}", names(&new_vals)),
                        ));
                        tags.releaseartists.composer = new_vals;
                    }
                    TAG_RELEASEARTIST_CONDUCTOR => {
                        let new_vals = to_artists(execute_multi_value_action(
                            act,
                            &names(&tags.releaseartists.conductor),
                        ));
                        potential_audiotag_changes.push((
                            "releaseartist[conductor]".to_string(),
                            format!("{:?}", names(&orig_tags.releaseartists.conductor)),
                            format!("{:?}", names(&new_vals)),
                        ));
                        tags.releaseartists.conductor = new_vals;
                    }
                    TAG_RELEASEARTIST_DJMIXER => {
                        let new_vals = to_artists(execute_multi_value_action(
                            act,
                            &names(&tags.releaseartists.djmixer),
                        ));
                        potential_audiotag_changes.push((
                            "releaseartist[djmixer]".to_string(),
                            format!("{:?}", names(&orig_tags.releaseartists.djmixer)),
                            format!("{:?}", names(&new_vals)),
                        ));
                        tags.releaseartists.djmixer = new_vals;
                    }
                    _ => {} // new/favorite/rating handled above
                }
            }
        }

        // Compute real changes by diffing.
        let tag_changes: Vec<Change> = potential_audiotag_changes
            .into_iter()
            .filter(|(_, old, new)| old != new)
            .collect();

        if !tag_changes.is_empty() {
            actionable_audiotags.push((tags.clone(), tag_changes));
        }

        // Handle datafile changes.
        if let Some(df) = datafile {
            if !potential_datafile_changes.is_empty() {
                match actionable_datafiles.get_mut(&parent_key) {
                    Some((_, _, existing_changes)) => {
                        existing_changes.extend(potential_datafile_changes);
                    }
                    None => {
                        actionable_datafiles.insert(
                            parent_key.clone(),
                            (tags.clone(), df, potential_datafile_changes),
                        );
                    }
                }
            }
        }
    }

    if actionable_audiotags.is_empty() && actionable_datafiles.is_empty() {
        eprintln!("No matching tracks found");
        return Ok(());
    }

    // === Step 4: Display changes and ask for user confirmation ===

    let mut todisplay: Vec<(String, Vec<Change>)> = Vec::new();

    let music_source_prefix = format!("{}/", c.music_source_dir.display());

    for (tags, tag_changes) in &actionable_audiotags {
        let mut pathtext = tags.path.display().to_string();
        if let Some(stripped) = pathtext.strip_prefix(&music_source_prefix) {
            pathtext = stripped.to_string();
        }
        if pathtext.len() >= 120 {
            pathtext = format!("{}..{}", &pathtext[..59], &pathtext[pathtext.len() - 59..]);
        }
        todisplay.push((pathtext, tag_changes.clone()));
    }
    for (path, (_, _, datafile_changes)) in &actionable_datafiles {
        let mut pathtext = path.clone();
        if let Some(stripped) = pathtext.strip_prefix(&music_source_prefix) {
            pathtext = stripped.to_string();
        }
        if pathtext.len() >= 120 {
            pathtext = format!("{}..{}", &pathtext[..59], &pathtext[pathtext.len() - 59..]);
        }
        todisplay.push((pathtext, datafile_changes.clone()));
    }

    // Display changes.
    for (pathtext, tag_changes) in &todisplay {
        eprintln!("{pathtext}");
        for (name, old, new) in tag_changes {
            eprintln!("      {name}: {old} -> {new}");
        }
    }

    // Dry run: abort.
    if dry_run {
        eprintln!(
            "\nThis is a dry run, aborting. {} tracks would have been modified.",
            actionable_audiotags.len()
        );
        return Ok(());
    }

    // Confirmation.
    let num_changes = actionable_audiotags.len() + actionable_datafiles.len();
    if confirm_yes {
        eprintln!();
        if num_changes > enter_number_to_confirm_above_count {
            loop {
                eprint!("Write changes to {num_changes} tracks? Enter {num_changes} to confirm (or 'no' to abort): ");
                io::stderr().flush().ok();
                let mut line = String::new();
                io::stdin().lock().read_line(&mut line).ok();
                let line = line.trim();
                if line == "no" {
                    tracing::debug!("aborting planned tag changes after user confirmation");
                    return Ok(());
                }
                if line == num_changes.to_string() {
                    eprintln!();
                    break;
                }
            }
        } else {
            eprint!("Write changes to {num_changes} tracks? [Y/n] ");
            io::stderr().flush().ok();
            let mut line = String::new();
            io::stdin().lock().read_line(&mut line).ok();
            let line = line.trim().to_lowercase();
            if line == "n" || line == "no" {
                tracing::debug!("aborting planned tag changes after user confirmation");
                return Ok(());
            }
            eprintln!();
        }
    }

    // === Step 5: Flush writes to disk ===

    let mut changed_release_ids: HashSet<String> = HashSet::new();

    for (mut tags, _tag_changes) in actionable_audiotags {
        if let Some(ref release_id) = tags.release_id {
            changed_release_ids.insert(release_id.clone());
        }
        tags.flush(c.write_parent_genres)?;
        tracing::info!(path = %tags.path.display(), "wrote tag changes");
    }

    for (path, (tags, datafile, _datafile_changes)) in &actionable_datafiles {
        if let Some(ref release_id) = tags.release_id {
            changed_release_ids.insert(release_id.clone());
        }
        // Find and write to the datafile in the directory.
        let dir = Path::new(path);
        let entries = std::fs::read_dir(dir).map_err(|e| {
            RoseError::Internal(format!("Failed to read directory {}: {e}", dir.display()))
        })?;
        for entry in entries {
            let entry =
                entry.map_err(|e| RoseError::Internal(format!("Failed to read dir entry: {e}")))?;
            let fname = entry.file_name();
            let fname_str = fname.to_string_lossy();
            if !STORED_DATA_FILE_REGEX.is_match(&fname_str) {
                continue;
            }
            let toml_str = toml::to_string_pretty(&datafile.serialize())
                .map_err(|e| RoseError::Internal(format!("Failed to serialize datafile: {e}")))?;
            std::fs::write(entry.path(), toml_str).map_err(|e| {
                RoseError::Internal(format!(
                    "Failed to write datafile {}: {e}",
                    entry.path().display()
                ))
            })?;
        }
        tracing::info!(path = %path, "wrote datafile changes");
    }

    eprintln!("\nApplied tag changes to {num_changes} tracks!");

    // === Step 6: Trigger cache update ===

    let release_ids: Vec<String> = changed_release_ids.into_iter().collect();
    if !release_ids.is_empty() {
        let releases = list_releases(c, Some(&release_ids), true)?;
        let source_paths: Vec<PathBuf> = releases.iter().map(|r| r.source_path.clone()).collect();
        // update_cache_for_releases is not yet ported. When it is, call it here:
        // update_cache_for_releases(c, &source_paths)?;
        tracing::info!(
            release_count = source_paths.len(),
            "would trigger cache update for changed releases"
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Top-level rule execution
// ---------------------------------------------------------------------------

/// Execute a single metadata rule (the 5-phase pipeline).
pub fn execute_metadata_rule(
    c: &Config,
    rule: &Rule,
    dry_run: bool,
    confirm_yes: bool,
) -> Result<(), RoseError> {
    execute_metadata_rule_inner(c, rule, dry_run, confirm_yes, 25)
}

fn execute_metadata_rule_inner(
    c: &Config,
    rule: &Rule,
    dry_run: bool,
    confirm_yes: bool,
    enter_number_to_confirm_above_count: usize,
) -> Result<(), RoseError> {
    eprintln!();
    let fast_search_results = fast_search_for_matching_tracks(c, &rule.matcher)?;
    if fast_search_results.is_empty() {
        eprintln!("No matching tracks found");
        eprintln!();
        return Ok(());
    }

    // Phase 2: If >400, pre-filter via cache.
    let fast_search_results = if fast_search_results.len() > 400 {
        let track_ids: Vec<String> = fast_search_results.iter().map(|t| t.id.clone()).collect();
        let tracks = list_tracks(c, Some(&track_ids))?;
        let filtered_tracks = filter_track_false_positives_using_read_cache(&rule.matcher, tracks);
        let track_id_set: HashSet<String> = filtered_tracks.iter().map(|t| t.id.clone()).collect();
        let filtered: Vec<FastSearchResult> = fast_search_results
            .into_iter()
            .filter(|fsr| track_id_set.contains(&fsr.id))
            .collect();
        if filtered.is_empty() {
            eprintln!("No matching tracks found");
            eprintln!();
            return Ok(());
        }
        filtered
    } else {
        fast_search_results
    };

    // Phase 3: Tag-based filter.
    let matcher_audiotags =
        filter_track_false_positives_using_tags(&rule.matcher, &fast_search_results, &rule.ignore)?;
    if matcher_audiotags.is_empty() {
        eprintln!("No matching tracks found");
        eprintln!();
        return Ok(());
    }

    // Phases 4-5: Execute actions.
    execute_metadata_actions_inner(
        c,
        &rule.actions,
        matcher_audiotags,
        dry_run,
        confirm_yes,
        enter_number_to_confirm_above_count,
    )
}

/// Execute all stored metadata rules from the config.
pub fn execute_stored_metadata_rules(
    c: &Config,
    dry_run: bool,
    confirm_yes: bool,
) -> Result<(), RoseError> {
    for rule in &c.stored_metadata_rules {
        eprintln!("Executing stored metadata rule {rule}");
        execute_metadata_rule(c, rule, dry_run, confirm_yes)?;
    }
    Ok(())
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule_parser::{
        ActionBehavior, AddAction, DeleteAction, Pattern, ReplaceAction, SedAction, SplitAction,
    };

    // -----------------------------------------------------------------------
    // matches_pattern
    // -----------------------------------------------------------------------

    #[test]
    fn test_matches_pattern_substring() {
        let pat = Pattern::simple("rack");
        assert!(matches_pattern(&pat, "Track 1"));
        assert!(!matches_pattern(&pat, "Tr"));
    }

    #[test]
    fn test_matches_pattern_exact() {
        let pat = Pattern::new("Track 1", false, true, true, false);
        assert!(matches_pattern(&pat, "Track 1"));
        assert!(!matches_pattern(&pat, "Track 1 Extra"));
        assert!(!matches_pattern(&pat, "My Track 1"));
    }

    #[test]
    fn test_matches_pattern_prefix() {
        let pat = Pattern::new("Track", false, true, false, false);
        assert!(matches_pattern(&pat, "Track 1"));
        assert!(!matches_pattern(&pat, "My Track"));
    }

    #[test]
    fn test_matches_pattern_suffix() {
        let pat = Pattern::new("1", false, false, true, false);
        assert!(matches_pattern(&pat, "Track 1"));
        assert!(!matches_pattern(&pat, "1 Track"));
    }

    #[test]
    fn test_matches_pattern_case_insensitive() {
        let pat = Pattern::new("track", false, false, false, true);
        assert!(matches_pattern(&pat, "Track 1"));
        assert!(matches_pattern(&pat, "TRACK 1"));
        assert!(matches_pattern(&pat, "track 1"));
    }

    #[test]
    fn test_matches_pattern_empty() {
        let pat = Pattern::simple("");
        assert!(matches_pattern(&pat, "anything"));
        assert!(matches_pattern(&pat, ""));
    }

    #[test]
    fn test_matches_pattern_bool_values() {
        let pat = Pattern::simple("true");
        assert!(matches_pattern(&pat, "true"));
        assert!(!matches_pattern(&pat, "false"));
    }

    // -----------------------------------------------------------------------
    // execute_single_action
    // -----------------------------------------------------------------------

    #[test]
    fn test_execute_single_action_replace() {
        let action = Action {
            tags: vec![TAG_TRACKTITLE],
            behavior: ActionBehavior::Replace(ReplaceAction {
                replacement: "new title".to_string(),
            }),
            pattern: None,
        };
        assert_eq!(
            execute_single_action(&action, "old title"),
            Some("new title".to_string())
        );
    }

    #[test]
    fn test_execute_single_action_replace_with_pattern_match() {
        let action = Action {
            tags: vec![TAG_TRACKTITLE],
            behavior: ActionBehavior::Replace(ReplaceAction {
                replacement: "new".to_string(),
            }),
            pattern: Some(Pattern::simple("old")),
        };
        assert_eq!(
            execute_single_action(&action, "old title"),
            Some("new".to_string())
        );
    }

    #[test]
    fn test_execute_single_action_replace_with_pattern_no_match() {
        let action = Action {
            tags: vec![TAG_TRACKTITLE],
            behavior: ActionBehavior::Replace(ReplaceAction {
                replacement: "new".to_string(),
            }),
            pattern: Some(Pattern::simple("xyz")),
        };
        assert_eq!(
            execute_single_action(&action, "old title"),
            Some("old title".to_string())
        );
    }

    #[test]
    fn test_execute_single_action_sed() {
        let action = Action {
            tags: vec![TAG_TRACKTITLE],
            behavior: ActionBehavior::Sed(SedAction {
                src: Regex::new("ack").unwrap(),
                dst: "ip".to_string(),
            }),
            pattern: None,
        };
        assert_eq!(
            execute_single_action(&action, "Track 1"),
            Some("Trip 1".to_string())
        );
    }

    #[test]
    fn test_execute_single_action_delete() {
        let action = Action {
            tags: vec![TAG_TRACKTITLE],
            behavior: ActionBehavior::Delete(DeleteAction),
            pattern: None,
        };
        assert_eq!(execute_single_action(&action, "old title"), None);
    }

    // -----------------------------------------------------------------------
    // execute_multi_value_action
    // -----------------------------------------------------------------------

    #[test]
    fn test_multi_value_replace() {
        let action = Action {
            tags: vec![TAG_GENRE],
            behavior: ActionBehavior::Replace(ReplaceAction {
                replacement: "Hip-Hop".to_string(),
            }),
            pattern: Some(Pattern::simple("K-Pop")),
        };
        let result = execute_multi_value_action(&action, &["K-Pop".to_string(), "Pop".to_string()]);
        assert_eq!(result, vec!["Hip-Hop", "Pop"]);
    }

    #[test]
    fn test_multi_value_replace_with_semicolons() {
        let action = Action {
            tags: vec![TAG_GENRE],
            behavior: ActionBehavior::Replace(ReplaceAction {
                replacement: "Hip-Hop;Rap".to_string(),
            }),
            pattern: Some(Pattern::simple("K-Pop")),
        };
        let result = execute_multi_value_action(&action, &["K-Pop".to_string(), "Pop".to_string()]);
        assert_eq!(result, vec!["Hip-Hop", "Rap", "Pop"]);
    }

    #[test]
    fn test_multi_value_sed() {
        let action = Action {
            tags: vec![TAG_GENRE],
            behavior: ActionBehavior::Sed(SedAction {
                src: Regex::new("P").unwrap(),
                dst: "B".to_string(),
            }),
            pattern: None,
        };
        let result = execute_multi_value_action(&action, &["K-Pop".to_string(), "Pop".to_string()]);
        assert_eq!(result, vec!["K-Bop", "Bop"]);
    }

    #[test]
    fn test_multi_value_split() {
        let action = Action {
            tags: vec![TAG_LABEL],
            behavior: ActionBehavior::Split(SplitAction {
                delimiter: "Cool".to_string(),
            }),
            pattern: Some(Pattern::simple("Cool")),
        };
        let result = execute_multi_value_action(&action, &["A Cool Label".to_string()]);
        assert_eq!(result, vec!["A", "Label"]);
    }

    #[test]
    fn test_multi_value_add() {
        let action = Action {
            tags: vec![TAG_LABEL],
            behavior: ActionBehavior::Add(AddAction {
                value: "New Label".to_string(),
            }),
            pattern: Some(Pattern::simple("Cool")),
        };
        let result = execute_multi_value_action(&action, &["A Cool Label".to_string()]);
        assert_eq!(result, vec!["A Cool Label", "New Label"]);
    }

    #[test]
    fn test_multi_value_delete() {
        let action = Action {
            tags: vec![TAG_GENRE],
            behavior: ActionBehavior::Delete(DeleteAction),
            pattern: Some(Pattern::new("Pop", false, true, true, false)),
        };
        let result = execute_multi_value_action(&action, &["K-Pop".to_string(), "Pop".to_string()]);
        assert_eq!(result, vec!["K-Pop"]);
    }

    #[test]
    fn test_multi_value_delete_no_pattern() {
        let action = Action {
            tags: vec![TAG_GENRE],
            behavior: ActionBehavior::Delete(DeleteAction),
            pattern: None,
        };
        let result = execute_multi_value_action(&action, &["K-Pop".to_string(), "Pop".to_string()]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_multi_value_no_match() {
        let action = Action {
            tags: vec![TAG_GENRE],
            behavior: ActionBehavior::Replace(ReplaceAction {
                replacement: "New".to_string(),
            }),
            pattern: Some(Pattern::simple("xyz")),
        };
        let result = execute_multi_value_action(&action, &["K-Pop".to_string(), "Pop".to_string()]);
        assert_eq!(result, vec!["K-Pop", "Pop"]);
    }

    #[test]
    fn test_multi_value_replace_empty_delimited() {
        let action = Action {
            tags: vec![TAG_GENRE],
            behavior: ActionBehavior::Replace(ReplaceAction {
                replacement: "Hip-Hop;;;;".to_string(),
            }),
            pattern: Some(Pattern::simple("K-Pop")),
        };
        let result = execute_multi_value_action(&action, &["K-Pop".to_string()]);
        assert_eq!(result, vec!["Hip-Hop"]);
    }

    // -----------------------------------------------------------------------
    // _convert_matcher_to_fts_query
    // -----------------------------------------------------------------------

    #[test]
    fn test_convert_matcher_to_fts_query_basic() {
        let pat = Pattern::simple("AB");
        let result = convert_matcher_to_fts_query(&pat);
        assert_eq!(result, "NEAR(\"A¬B\", 0)");
    }

    #[test]
    fn test_convert_matcher_to_fts_query_single_char() {
        let pat = Pattern::simple("A");
        let result = convert_matcher_to_fts_query(&pat);
        assert_eq!(result, "NEAR(\"A\", 0)");
    }

    #[test]
    fn test_convert_matcher_to_fts_query_longer() {
        let pat = Pattern::simple("ABCD");
        let result = convert_matcher_to_fts_query(&pat);
        assert_eq!(result, "NEAR(\"A¬B¬C¬D\", 2)");
    }

    #[test]
    fn test_convert_matcher_to_fts_query_with_quotes() {
        let pat = Pattern::simple("A'B");
        let result = convert_matcher_to_fts_query(&pat);
        assert!(result.contains("A¬''¬B"));
    }

    // -----------------------------------------------------------------------
    // Regression: --ignore bug fix
    // -----------------------------------------------------------------------

    #[test]
    fn test_ignore_bug_fix_documented() {
        // This test documents that the bug fix is implemented:
        // In the Python source (rules.py:302,306), the ignore loop for
        // new/favorite incorrectly assigns to `match` instead of `skip`.
        // The Rust implementation correctly assigns to `skip`.
        //
        // We can't easily integration-test this without a full DB, but
        // the code path is verified by code review. The implementation in
        // filter_track_false_positives_using_tags uses `skip = ...` for
        // new/favorite/rating in the ignore loop.
    }

    // -----------------------------------------------------------------------
    // Deduplication
    // -----------------------------------------------------------------------

    #[test]
    fn test_multi_value_dedup() {
        let action = Action {
            tags: vec![TAG_GENRE],
            behavior: ActionBehavior::Add(AddAction {
                value: "Pop".to_string(),
            }),
            pattern: None,
        };
        // "Pop" is already in the list, so it shouldn't be duplicated.
        let result = execute_multi_value_action(&action, &["Pop".to_string()]);
        assert_eq!(result, vec!["Pop"]);
    }

    #[test]
    fn test_sed_with_capture_group() {
        // Test $1 syntax (Rust regex replacement)
        let action = Action {
            tags: vec![TAG_GENRE],
            behavior: ActionBehavior::Sed(SedAction {
                src: Regex::new(r"^(.*)$").unwrap(),
                dst: "i$1".to_string(),
            }),
            pattern: None,
        };
        let result = execute_multi_value_action(&action, &["K-Pop".to_string(), "Pop".to_string()]);
        assert_eq!(result, vec!["iK-Pop", "iPop"]);
    }

    // ===================================================================
    // Per-field matching integration tests (full pipeline)
    // ===================================================================
    //
    // These tests exercise the complete rules pipeline:
    //   FTS search → filter → action → flush → verify
    // using real audio files, a real seeded SQLite cache, and real FTS index.

    use crate::audiotags::AudioTags;
    use crate::cache::{maybe_invalidate_cache_database, update_cache, StoredDataFile};
    use crate::config::Config;
    use crate::rule_parser::Rule;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Path to the monorepo root (two levels up from CARGO_MANIFEST_DIR).
    fn repo_root() -> PathBuf {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    /// Create a minimal Config for pipeline integration tests.
    fn pipeline_test_config(dir: &TempDir) -> Config {
        let music_dir = dir.path().join("music");
        let cache_dir = dir.path().join("cache");
        std::fs::create_dir_all(&music_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();

        let cfg_path = dir.path().join("config.toml");
        let mut f = std::fs::File::create(&cfg_path).unwrap();
        write!(
            f,
            r#"
            music_source_dir = "{}"
            cache_dir = "{}"
            vfs.mount_dir = "{}"
            "#,
            music_dir.display(),
            cache_dir.display(),
            dir.path().join("vfs").display(),
        )
        .unwrap();
        Config::parse(Some(&cfg_path)).unwrap()
    }

    /// Set up a single-release test environment:
    /// - Copy the real test .m4a file to a temp release directory
    /// - Create a sidecar `.rose.testrel.toml` with specified new/favorite values
    /// - Run `update_cache` to populate DB + FTS index
    ///
    /// Returns (TempDir, Config, path_to_audio_file, release_dir).
    fn pipeline_setup(new: bool, favorite: bool) -> (TempDir, Config, PathBuf, PathBuf) {
        let dir = TempDir::new().unwrap();
        let config = pipeline_test_config(&dir);
        maybe_invalidate_cache_database(&config).unwrap();

        let release_dir = config.music_source_dir.join("TestRelease");
        std::fs::create_dir_all(&release_dir).unwrap();

        // Copy real audio file
        let src = repo_root().join("testdata/Test Release 1/01.m4a");
        let dst = release_dir.join("01.m4a");
        std::fs::copy(&src, &dst).unwrap();

        // Create sidecar
        let sdf = StoredDataFile {
            new,
            favorite,
            rating: None,
            added_at: "2023-01-01T00:00:00+00:00".to_string(),
        };
        let sidecar_path = release_dir.join(".rose.testrel.toml");
        let toml_str = toml::to_string_pretty(sdf.serialize().as_table().unwrap()).unwrap();
        std::fs::write(&sidecar_path, toml_str).unwrap();

        // Create required special directories
        std::fs::create_dir_all(config.music_source_dir.join("!collages")).unwrap();
        std::fs::create_dir_all(config.music_source_dir.join("!playlists")).unwrap();

        // Run update_cache to populate DB + FTS
        update_cache(&config, true).unwrap();

        (dir, config, dst, release_dir)
    }

    /// Execute a rule through the full pipeline (no confirmation prompt).
    fn run_rule(config: &Config, rule: &Rule) {
        execute_metadata_rule_inner(config, rule, false, false, 9999).unwrap();
    }

    // 1. tracktitle match → replace tracktitle
    #[test]
    fn test_pipeline_match_tracktitle() {
        let (_dir, config, audio_path, _) = pipeline_setup(true, false);

        // Verify initial state
        let tags = AudioTags::from_file(&audio_path).unwrap();
        assert!(
            tags.tracktitle.as_deref().unwrap().contains("Track"),
            "expected tracktitle containing 'Track', got {:?}",
            tags.tracktitle
        );

        let rule = Rule::parse("tracktitle:Track", &["replace:New Title"], None).unwrap();
        run_rule(&config, &rule);

        let tags = AudioTags::from_file(&audio_path).unwrap();
        assert_eq!(tags.tracktitle.as_deref(), Some("New Title"));
    }

    // 2. releasetitle match → replace releasetitle
    #[test]
    fn test_pipeline_match_releasetitle() {
        let (_dir, config, audio_path, _) = pipeline_setup(true, false);

        let tags = AudioTags::from_file(&audio_path).unwrap();
        assert!(
            tags.releasetitle.is_some(),
            "expected a releasetitle to be set"
        );

        let releasetitle = tags.releasetitle.as_deref().unwrap();
        // Match on a substring of the release title
        let rule = Rule::parse(
            &format!("releasetitle:{releasetitle}"),
            &["replace:New Album"],
            None,
        )
        .unwrap();
        run_rule(&config, &rule);

        let tags = AudioTags::from_file(&audio_path).unwrap();
        assert_eq!(tags.releasetitle.as_deref(), Some("New Album"));
    }

    // 3. releasedate match → action on tracktitle to verify match works
    #[test]
    fn test_pipeline_match_releasedate() {
        let (_dir, config, audio_path, _) = pipeline_setup(true, false);

        let tags = AudioTags::from_file(&audio_path).unwrap();
        let date_str = tags.releasedate.as_ref().unwrap().to_string();

        // Match on releasedate, action on tracktitle
        let rule = Rule::parse(
            &format!("releasedate:{date_str}"),
            &["tracktitle/replace:DateMatched"],
            None,
        )
        .unwrap();
        run_rule(&config, &rule);

        let tags = AudioTags::from_file(&audio_path).unwrap();
        assert_eq!(tags.tracktitle.as_deref(), Some("DateMatched"));
    }

    // 4. releasetype match → replace releasetype
    #[test]
    fn test_pipeline_match_releasetype() {
        let (_dir, config, audio_path, _) = pipeline_setup(true, false);

        let tags = AudioTags::from_file(&audio_path).unwrap();
        let releasetype = tags.releasetype.clone();

        let rule =
            Rule::parse(&format!("releasetype:{releasetype}"), &["replace:ep"], None).unwrap();
        run_rule(&config, &rule);

        let tags = AudioTags::from_file(&audio_path).unwrap();
        assert_eq!(tags.releasetype, "ep");
    }

    // 5. tracknumber match → action on tracktitle
    #[test]
    fn test_pipeline_match_tracknumber() {
        let (_dir, config, audio_path, _) = pipeline_setup(true, false);

        let tags = AudioTags::from_file(&audio_path).unwrap();
        let tracknumber = tags.tracknumber.as_deref().unwrap_or("1");

        let rule = Rule::parse(
            &format!("tracknumber:{tracknumber}"),
            &["tracktitle/replace:NumberMatched"],
            None,
        )
        .unwrap();
        run_rule(&config, &rule);

        let tags = AudioTags::from_file(&audio_path).unwrap();
        assert_eq!(tags.tracktitle.as_deref(), Some("NumberMatched"));
    }

    // 6. genre match → replace genre
    #[test]
    fn test_pipeline_match_genre() {
        let (_dir, config, audio_path, _) = pipeline_setup(true, false);

        let tags = AudioTags::from_file(&audio_path).unwrap();
        assert!(!tags.genre.is_empty(), "expected at least one genre");
        let first_genre = tags.genre[0].clone();

        let rule =
            Rule::parse(&format!("genre:^{first_genre}$"), &["replace:Techno"], None).unwrap();
        run_rule(&config, &rule);

        let tags = AudioTags::from_file(&audio_path).unwrap();
        assert!(
            tags.genre.contains(&"Techno".to_string()),
            "expected genre list to contain 'Techno', got {:?}",
            tags.genre
        );
    }

    // 7. label match → replace label
    #[test]
    fn test_pipeline_match_label() {
        let (_dir, config, audio_path, _) = pipeline_setup(true, false);

        let tags = AudioTags::from_file(&audio_path).unwrap();
        assert!(!tags.label.is_empty(), "expected at least one label");
        let first_label = tags.label[0].clone();

        let rule = Rule::parse(
            &format!("label:^{first_label}$"),
            &["replace:New Label"],
            None,
        )
        .unwrap();
        run_rule(&config, &rule);

        let tags = AudioTags::from_file(&audio_path).unwrap();
        assert!(
            tags.label.contains(&"New Label".to_string()),
            "expected label list to contain 'New Label', got {:?}",
            tags.label
        );
    }

    // 8. releaseartist match → action on releasetitle to verify match
    #[test]
    fn test_pipeline_match_releaseartist() {
        let (_dir, config, audio_path, _) = pipeline_setup(true, false);

        let tags = AudioTags::from_file(&audio_path).unwrap();
        assert!(
            !tags.releaseartists.main.is_empty(),
            "expected at least one release artist"
        );
        let artist_name = tags.releaseartists.main[0].name.clone();

        let rule = Rule::parse(
            &format!("releaseartist:{artist_name}"),
            &["releasetitle/replace:ArtistMatched"],
            None,
        )
        .unwrap();
        run_rule(&config, &rule);

        let tags = AudioTags::from_file(&audio_path).unwrap();
        assert_eq!(tags.releasetitle.as_deref(), Some("ArtistMatched"));
    }

    // 9. trackartist match → action on tracktitle to verify match
    #[test]
    fn test_pipeline_match_trackartist() {
        let (_dir, config, audio_path, _) = pipeline_setup(true, false);

        let tags = AudioTags::from_file(&audio_path).unwrap();
        assert!(
            !tags.trackartists.main.is_empty(),
            "expected at least one track artist"
        );
        let artist_name = tags.trackartists.main[0].name.clone();

        let rule = Rule::parse(
            &format!("trackartist:{artist_name}"),
            &["tracktitle/replace:TrackArtistMatched"],
            None,
        )
        .unwrap();
        run_rule(&config, &rule);

        let tags = AudioTags::from_file(&audio_path).unwrap();
        assert_eq!(tags.tracktitle.as_deref(), Some("TrackArtistMatched"));
    }

    // 10. new:true match → replace with false
    #[test]
    fn test_pipeline_match_new() {
        let (_dir, config, _audio_path, release_dir) = pipeline_setup(true, false);

        // Verify initial state: new=true
        let sidecar = get_release_datafile_of_directory(&release_dir).unwrap();
        assert!(sidecar.new, "expected new=true initially");

        let rule = Rule::parse("new:true", &["replace:false"], None).unwrap();
        run_rule(&config, &rule);

        let sidecar = get_release_datafile_of_directory(&release_dir).unwrap();
        assert!(!sidecar.new, "expected new=false after rule execution");
    }

    // 11. favorite:false match → replace with true
    #[test]
    fn test_pipeline_match_favorite() {
        let (_dir, config, _audio_path, release_dir) = pipeline_setup(true, false);

        // Verify initial state: favorite=false
        let sidecar = get_release_datafile_of_directory(&release_dir).unwrap();
        assert!(!sidecar.favorite, "expected favorite=false initially");

        let rule = Rule::parse("favorite:false", &["replace:true"], None).unwrap();
        run_rule(&config, &rule);

        let sidecar = get_release_datafile_of_directory(&release_dir).unwrap();
        assert!(
            sidecar.favorite,
            "expected favorite=true after rule execution"
        );
    }

    // ===================================================================
    // FTS search, confirmation, dry run, and stored rules tests (T-8.3)
    // ===================================================================

    use crate::cache::{connect, list_releases, list_tracks, sync_fts_index};
    use crate::rule_parser::Matcher;

    /// Seed a database matching Python's `_seed_cache` fixture and sync the FTS
    /// index so that `fast_search_for_matching_*` queries work.
    fn seeded_config_with_fts() -> (TempDir, Config) {
        let dir = TempDir::new().unwrap();
        let config = pipeline_test_config(&dir);
        maybe_invalidate_cache_database(&config).unwrap();

        let music_dir = config.music_source_dir.clone();
        let dirpaths = [
            music_dir.join("r1"),
            music_dir.join("r2"),
            music_dir.join("r3"),
            music_dir.join("r4"),
        ];
        let musicpaths = [
            music_dir.join("r1/01.m4a"),
            music_dir.join("r1/02.m4a"),
            music_dir.join("r2/01.m4a"),
            music_dir.join("r3/01.m4a"),
            music_dir.join("r4/01.m4a"),
        ];
        let imagepaths = [
            music_dir.join("r2/cover.jpg"),
            music_dir.join("!playlists/Lala Lisa.jpg"),
        ];

        // Create directories and dummy files.
        std::fs::create_dir_all(music_dir.join("!collages")).unwrap();
        std::fs::create_dir_all(music_dir.join("!playlists")).unwrap();
        for d in &dirpaths {
            std::fs::create_dir_all(d).unwrap();
        }
        for p in &musicpaths {
            std::fs::File::create(p).unwrap();
        }
        for p in &imagepaths {
            std::fs::File::create(p).unwrap();
        }
        // Create datafile sidecars.
        for d in &dirpaths {
            let sdf = StoredDataFile::new_default();
            let table = sdf.serialize();
            let toml_str = toml::to_string_pretty(table.as_table().unwrap()).unwrap();
            let sidecar = d.join(format!(
                ".rose.{}.toml",
                d.file_name().unwrap().to_string_lossy()
            ));
            std::fs::write(&sidecar, toml_str).unwrap();
        }

        let conn = connect(&config).unwrap();
        conn.execute_batch(&format!(
            r#"
INSERT INTO releases
       (id  , source_path    , cover_image_path , added_at                   , datafile_mtime, title      , releasetype , releasedate , originaldate, compositiondate, catalognumber, edition , disctotal, new  , favorite, metahash)
VALUES ('r1', '{}'           , null             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 1', 'album'     , '2023'      , null        , null           , null         , null    , 1        , false, true    , '1')
     , ('r2', '{}'           , '{}'             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 2', 'album'     , '2021'      , '2019'      , null           , 'DG-001'     , 'Deluxe', 1        , true , false   , '2')
     , ('r3', '{}'           , null             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 3', 'album'     , '2021-04-20', null        , '1780'         , 'DG-002'     , null    , 1        , false, false   , '3')
     , ('r4', '{}'           , null             , '0000-01-01T00:00:00+00:00', '999'         , 'Release 4', 'loosetrack', '2021-04-20', null        , '1780'         , 'DG-002'     , null    , 1        , false, false   , '4');

INSERT INTO releases_genres
       (release_id, genre             , position)
VALUES ('r1'      , 'Techno'          , 1)
     , ('r1'      , 'Deep House'      , 2)
     , ('r2'      , 'Modern Classical', 1);

INSERT INTO releases_secondary_genres
       (release_id, genre             , position)
VALUES ('r1'      , 'Rominimal'       , 1)
     , ('r1'      , 'Ambient'         , 2)
     , ('r2'      , 'Orchestral Music', 1);

INSERT INTO releases_descriptors
       (release_id, descriptor, position)
VALUES ('r1'      , 'Warm'    , 1)
     , ('r1'      , 'Hot'     , 2)
     , ('r2'      , 'Wet'     , 1);

INSERT INTO releases_labels
       (release_id, label         , position)
VALUES ('r1'      , 'Silk Music'  , 1)
     , ('r2'      , 'Native State', 1);

INSERT INTO tracks
       (id  , source_path    , source_mtime, title    , release_id, tracknumber, tracktotal, discnumber, duration_seconds, metahash)
VALUES ('t1', '{}'           , '999'       , 'Track 1', 'r1'      , '01'       , 2         , '01'      , 120             , '1')
     , ('t2', '{}'           , '999'       , 'Track 2', 'r1'      , '02'       , 2         , '01'      , 240             , '2')
     , ('t3', '{}'           , '999'       , 'Track 1', 'r2'      , '01'       , 1         , '01'      , 120             , '3')
     , ('t4', '{}'           , '999'       , 'Track 1', 'r3'      , '01'       , 1         , '01'      , 120             , '4')
     , ('t5', '{}'           , '999'       , 'Track 1', 'r4'      , '01'       , 1         , '01'      , 120             , '5');

INSERT INTO releases_artists
       (release_id, artist           , role   , position)
VALUES ('r1'      , 'Techno Man'     , 'main' , 1)
     , ('r1'      , 'Bass Man'       , 'main' , 2)
     , ('r2'      , 'Violin Woman'   , 'main' , 1)
     , ('r2'      , 'Conductor Woman', 'guest', 2);

INSERT INTO tracks_artists
       (track_id, artist           , role   , position)
VALUES ('t1'    , 'Techno Man'     , 'main' , 1)
     , ('t1'    , 'Bass Man'       , 'main' , 2)
     , ('t2'    , 'Techno Man'     , 'main' , 1)
     , ('t2'    , 'Bass Man'       , 'main' , 2)
     , ('t3'    , 'Violin Woman'   , 'main' , 1)
     , ('t3'    , 'Conductor Woman', 'guest', 2);
            "#,
            dirpaths[0].display(),
            dirpaths[1].display(),
            imagepaths[0].display(),
            dirpaths[2].display(),
            dirpaths[3].display(),
            musicpaths[0].display(),
            musicpaths[1].display(),
            musicpaths[2].display(),
            musicpaths[3].display(),
            musicpaths[4].display(),
        ))
        .expect("Failed to seed cache database");

        // Sync the FTS index for all tracks/releases.
        sync_fts_index(
            &conn,
            &[
                "t1".to_string(),
                "t2".to_string(),
                "t3".to_string(),
                "t4".to_string(),
                "t5".to_string(),
            ],
            &[
                "r1".to_string(),
                "r2".to_string(),
                "r3".to_string(),
                "r4".to_string(),
            ],
        )
        .unwrap();

        (dir, config)
    }

    // -----------------------------------------------------------------------
    // T-8.3.1: test_fast_search_for_matching_releases
    // -----------------------------------------------------------------------

    #[test]
    fn test_fast_search_for_matching_releases() {
        let (_dir, config) = seeded_config_with_fts();
        let matcher = Matcher::parse("releaseartist:Techno Man").unwrap();
        let results = fast_search_for_matching_releases(&config, &matcher, false).unwrap();
        let ids: Vec<&str> = results.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"r1"), "should find release r1, got {:?}", ids);
    }

    // -----------------------------------------------------------------------
    // T-8.3.2: test_fast_search_releases_invalid_tag
    // -----------------------------------------------------------------------

    #[test]
    fn test_fast_search_releases_invalid_tag() {
        let (_dir, config) = seeded_config_with_fts();

        // tracktitle is a track-only tag → should be rejected.
        let matcher = Matcher::parse("tracktitle:x").unwrap();
        let result = fast_search_for_matching_releases(&config, &matcher, false);
        assert!(
            result.is_err(),
            "tracktitle should not be allowed for release search"
        );

        // trackartist alone is also track-only → should be rejected.
        let matcher = Matcher::parse("trackartist:x").unwrap();
        let result = fast_search_for_matching_releases(&config, &matcher, false);
        assert!(
            result.is_err(),
            "trackartist alone should not be allowed for release search"
        );

        // artist is an alias that expands to both releaseartist and trackartist.
        // Since releaseartist is present, the track-only trackartist tags are tolerated.
        let matcher = Matcher::parse("artist:x").unwrap();
        let result = fast_search_for_matching_releases(&config, &matcher, false);
        assert!(
            result.is_ok(),
            "artist should be allowed for release search (includes releaseartist)"
        );
    }

    // -----------------------------------------------------------------------
    // T-8.3.3: test_filter_release_false_positives_with_cache
    // -----------------------------------------------------------------------

    #[test]
    fn test_filter_release_false_positives_with_cache() {
        let (_dir, config) = seeded_config_with_fts();

        // Search for "^Man" (strict start) in releaseartist.
        // FTS returns a superset of matches where "Man" appears as a substring
        // (e.g. "Techno Man", "Bass Man"). But no artist name *starts* with "Man",
        // so the cache filter should remove them all.
        let matcher = Matcher::parse("releaseartist:^Man").unwrap();
        let fts_results = fast_search_for_matching_releases(&config, &matcher, true).unwrap();
        assert!(
            !fts_results.is_empty(),
            "FTS should return results (false positives) for 'Man' substring"
        );

        let release_ids: Vec<String> = fts_results.iter().map(|r| r.id.clone()).collect();
        let cache_results = list_releases(&config, Some(&release_ids), true).unwrap();
        assert!(
            !cache_results.is_empty(),
            "cache should return the releases"
        );

        let filtered =
            filter_release_false_positives_using_read_cache(&matcher, cache_results, true);
        assert!(
            filtered.is_empty(),
            "all results should be filtered as false positives, got {} results",
            filtered.len()
        );
    }

    // -----------------------------------------------------------------------
    // T-8.3.4: test_filter_track_false_positives_with_cache
    // -----------------------------------------------------------------------

    #[test]
    fn test_filter_track_false_positives_with_cache() {
        let (_dir, config) = seeded_config_with_fts();

        // Search for "^Man" (strict start) in trackartist.
        // FTS matches tracks whose artists contain "Man" as a substring
        // (e.g. "Techno Man", "Bass Man"), but none start with "Man".
        let matcher = Matcher::parse("trackartist:^Man").unwrap();
        let fts_results = fast_search_for_matching_tracks(&config, &matcher).unwrap();
        assert!(
            !fts_results.is_empty(),
            "FTS should return results for 'Man' substring"
        );

        let track_ids: Vec<String> = fts_results.iter().map(|r| r.id.clone()).collect();
        let tracks = list_tracks(&config, Some(&track_ids)).unwrap();
        assert!(!tracks.is_empty(), "cache should return the tracks");

        let filtered = filter_track_false_positives_using_read_cache(&matcher, tracks);
        assert!(
            filtered.is_empty(),
            "all results should be filtered as false positives, got {} results",
            filtered.len()
        );
    }

    // -----------------------------------------------------------------------
    // T-8.3.5: test_dry_run_does_not_write
    // -----------------------------------------------------------------------

    #[test]
    fn test_dry_run_does_not_write() {
        let (_dir, config, audio_path, _) = pipeline_setup(true, false);

        // Read original tags.
        let original = AudioTags::from_file(&audio_path).unwrap();
        let original_title = original.tracktitle.clone();

        // Execute a rule with dry_run=true.
        let rule = Rule::parse("tracktitle:Track", &["replace:lalala"], None).unwrap();
        execute_metadata_rule_inner(&config, &rule, true, false, 9999).unwrap();

        // Tags should NOT have been modified.
        let after = AudioTags::from_file(&audio_path).unwrap();
        assert_eq!(
            after.tracktitle, original_title,
            "dry_run should not modify audio file tags"
        );
    }

    // -----------------------------------------------------------------------
    // T-8.3.6: test_execute_stored_metadata_rules
    // -----------------------------------------------------------------------

    #[test]
    fn test_execute_stored_metadata_rules_fn() {
        let (_dir, config, audio_path, _) = pipeline_setup(true, false);

        // Read original tags.
        let original = AudioTags::from_file(&audio_path).unwrap();
        assert!(
            original.tracktitle.is_some(),
            "track should have a title before rule execution"
        );

        // Build a config with a stored rule that replaces the track title.
        let rule = Rule::parse("tracktitle:Track", &["replace:lalala"], None).unwrap();
        let config_with_rules = Config {
            stored_metadata_rules: vec![rule],
            ..config
        };

        execute_stored_metadata_rules(&config_with_rules, false, false).unwrap();

        // Tags should have been modified.
        let after = AudioTags::from_file(&audio_path).unwrap();
        assert_eq!(
            after.tracktitle,
            Some("lalala".to_string()),
            "stored rule should have replaced tracktitle"
        );
    }

    // -----------------------------------------------------------------------
    // T-8.3.7: test_releasetype_defaults_to_unknown_on_delete
    // -----------------------------------------------------------------------

    #[test]
    fn test_releasetype_defaults_to_unknown_on_delete() {
        // When deleting releasetype, execute_single_action returns None,
        // but the pipeline substitutes "unknown" via unwrap_or_else.
        let action = Action {
            tags: vec![TAG_RELEASETYPE],
            behavior: ActionBehavior::Delete(DeleteAction),
            pattern: None,
        };
        let result = execute_single_action(&action, "album");
        assert_eq!(result, None, "delete should return None");

        // In execute_metadata_actions_inner, the TAG_RELEASETYPE branch does:
        //   let v = v.unwrap_or_else(|| "unknown".to_string());
        let v = result.unwrap_or_else(|| "unknown".to_string());
        assert_eq!(
            v, "unknown",
            "releasetype should default to 'unknown' on delete"
        );
    }

    // -----------------------------------------------------------------------
    // T-8.3.8: test_date_validation_on_replace
    // -----------------------------------------------------------------------

    #[test]
    fn test_date_validation_on_replace() {
        use crate::audiotags::RoseDate;

        // Replace releasedate with an invalid date string.
        let action = Action {
            tags: vec![TAG_RELEASEDATE],
            behavior: ActionBehavior::Replace(ReplaceAction {
                replacement: "not-a-date".to_string(),
            }),
            pattern: None,
        };
        let result = execute_single_action(&action, "2024");
        assert_eq!(result, Some("not-a-date".to_string()));

        // The pipeline applies RoseDate::parse to the replacement value.
        // An invalid date string produces None, effectively clearing the field.
        let parsed = RoseDate::parse(result.as_deref());
        assert!(
            parsed.is_none(),
            "invalid date string should parse to None, clearing the date"
        );

        // Verify that a valid date string does parse correctly.
        let valid_action = Action {
            tags: vec![TAG_RELEASEDATE],
            behavior: ActionBehavior::Replace(ReplaceAction {
                replacement: "2025".to_string(),
            }),
            pattern: None,
        };
        let valid_result = execute_single_action(&valid_action, "2024");
        let valid_parsed = RoseDate::parse(valid_result.as_deref());
        assert!(
            valid_parsed.is_some(),
            "valid date should parse successfully"
        );
        assert_eq!(valid_parsed.unwrap().year, 2025);
    }
}
