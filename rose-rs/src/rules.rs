use crate::audiotags::AudioTags;
use crate::cache::{list_releases, list_tracks, update_cache_for_releases, Release, StoredDataFile, Track};
use crate::common::{uniq, Artist, RoseDate};
use crate::config::Config;
use crate::rule_parser::{Action, ActionBehavior, Matcher, Pattern, Rule, Tag};
use crate::{Result, RoseError, RoseExpectedError};
use regex::Regex;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::{debug, info};

// Local copies of constants from cache module
const STORED_DATA_FILE_REGEX_STR: &str = r"^\.rose\.([^.]+)\.toml$";
lazy_static::lazy_static! {
    static ref STORED_DATA_FILE_REGEX: Regex = Regex::new(STORED_DATA_FILE_REGEX_STR).unwrap();
}
const RELEASE_TAGS: &[&str] = &[
    "releasetitle",
    "releasedate",
    "originaldate",
    "compositiondate",
    "edition",
    "catalognumber",
    "releasetype",
    "secondarygenre",
    "genre",
    "label",
    "releaseartist[main]",
    "releaseartist[guest]",
    "releaseartist[remixer]",
    "releaseartist[producer]",
    "releaseartist[composer]",
    "releaseartist[conductor]",
    "releaseartist[djmixer]",
    "new",
    "disctotal",
    "descriptor",
];

#[derive(Debug, Clone)]
pub struct TrackTagNotAllowedError(String);

impl std::fmt::Display for TrackTagNotAllowedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for TrackTagNotAllowedError {}

#[derive(Debug, Clone)]
pub struct InvalidReplacementValueError(String);

impl std::fmt::Display for InvalidReplacementValueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for InvalidReplacementValueError {}

#[derive(Debug, Clone, PartialEq)]
pub struct FastSearchResult {
    pub id: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TagValue {
    String(String),
    Int(i64),
    Bool(bool),
    Date(RoseDate),
    StringList(Vec<String>),
    None,
}

impl From<Option<String>> for TagValue {
    fn from(value: Option<String>) -> Self {
        match value {
            Some(s) => TagValue::String(s),
            None => TagValue::None,
        }
    }
}

impl From<String> for TagValue {
    fn from(value: String) -> Self {
        TagValue::String(value)
    }
}

impl From<&str> for TagValue {
    fn from(value: &str) -> Self {
        TagValue::String(value.to_string())
    }
}

impl From<i32> for TagValue {
    fn from(value: i32) -> Self {
        TagValue::Int(value as i64)
    }
}

impl From<Option<i32>> for TagValue {
    fn from(value: Option<i32>) -> Self {
        match value {
            Some(i) => TagValue::Int(i as i64),
            None => TagValue::None,
        }
    }
}

impl From<bool> for TagValue {
    fn from(value: bool) -> Self {
        TagValue::Bool(value)
    }
}

impl From<Option<RoseDate>> for TagValue {
    fn from(value: Option<RoseDate>) -> Self {
        match value {
            Some(date) => TagValue::Date(date),
            None => TagValue::None,
        }
    }
}

impl From<Vec<String>> for TagValue {
    fn from(value: Vec<String>) -> Self {
        TagValue::StringList(value)
    }
}

impl TagValue {
    fn to_option_string(&self) -> Option<String> {
        match self {
            TagValue::String(s) => Some(s.clone()),
            TagValue::Int(i) => Some(i.to_string()),
            TagValue::Bool(b) => Some(b.to_string()),
            TagValue::Date(d) => Some(d.to_string()),
            TagValue::StringList(v) => Some(v.join(";")),
            TagValue::None => None,
        }
    }

    fn as_str_option(&self) -> Option<&str> {
        match self {
            TagValue::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    fn to_string_or_default(&self) -> String {
        self.to_option_string().unwrap_or_default()
    }
}

impl std::fmt::Display for TagValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TagValue::String(s) => write!(f, "{}", s),
            TagValue::Int(i) => write!(f, "{}", i),
            TagValue::Bool(b) => write!(f, "{}", if *b { "true" } else { "false" }),
            TagValue::Date(d) => write!(f, "{}", d),
            TagValue::StringList(list) => write!(f, "{}", list.join(", ")),
            TagValue::None => write!(f, ""),
        }
    }
}

fn value_to_str(value: &TagValue) -> String {
    match value {
        TagValue::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        TagValue::None => String::new(),
        _ => value.to_string(),
    }
}

fn matches_pattern(pattern: &Pattern, value: &str) -> bool {
    let needle = if pattern.case_insensitive {
        pattern.needle.to_lowercase()
    } else {
        pattern.needle.clone()
    };

    let haystack = if pattern.case_insensitive { value.to_lowercase() } else { value.to_string() };

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

pub fn execute_stored_metadata_rules(config: &Config, dry_run: bool, confirm_yes: bool) -> Result<()> {
    for rule in &config.stored_metadata_rules {
        println!("\x1b[2mExecuting stored metadata rule {}\x1b[0m", rule);
        execute_metadata_rule(config, rule, dry_run, confirm_yes, 25)?;
    }
    Ok(())
}

pub fn execute_metadata_rule(config: &Config, rule: &Rule, dry_run: bool, confirm_yes: bool, enter_number_to_confirm_above_count: usize) -> Result<()> {
    println!();
    let fast_search_results = fast_search_for_matching_tracks(config, &rule.matcher)?;

    if fast_search_results.is_empty() {
        println!("\x1b[2;3mNo matching tracks found\x1b[0m");
        println!();
        return Ok(());
    }

    let filtered_search_results = if fast_search_results.len() > 400 {
        let time_start = Instant::now();
        let track_ids: Vec<String> = fast_search_results.iter().map(|t| t.id.clone()).collect();
        let tracks = list_tracks(config, Some(track_ids))?;
        debug!("fetched tracks from cache for filtering in {:?}", time_start.elapsed());
        let filtered_tracks = filter_track_false_positives_using_read_cache(&rule.matcher, tracks);
        let track_ids: std::collections::HashSet<String> = filtered_tracks.into_iter().map(|t| t.id).collect();
        fast_search_results.into_iter().filter(|t| track_ids.contains(&t.id)).collect()
    } else {
        fast_search_results
    };

    if filtered_search_results.is_empty() {
        println!("\x1b[2;3mNo matching tracks found\x1b[0m");
        println!();
        return Ok(());
    }

    let matcher_audiotags = filter_track_false_positives_using_tags(&rule.matcher, &filtered_search_results, &rule.ignore)?;

    if matcher_audiotags.is_empty() {
        println!("\x1b[2;3mNo matching tracks found\x1b[0m");
        println!();
        return Ok(());
    }

    execute_metadata_actions(config, &rule.actions, matcher_audiotags, dry_run, confirm_yes, enter_number_to_confirm_above_count)
}

lazy_static::lazy_static! {
    static ref TAG_ROLE_REGEX: Regex = Regex::new(r"\[[^\]]+\]$").unwrap();
}

pub fn fast_search_for_matching_tracks(config: &Config, matcher: &Matcher) -> Result<Vec<FastSearchResult>> {
    let time_start = Instant::now();
    let matchsql = convert_matcher_to_fts_query(&matcher.pattern);
    debug!("converted match {:?} to {:?}", matcher, matchsql);

    let columns: Vec<String> = uniq(matcher.tags.iter().map(|t| TAG_ROLE_REGEX.replace(&t.to_string(), "").to_string()).collect());
    let ftsquery = if columns.len() == 1 {
        format!("{}:{}", columns[0], matchsql)
    } else {
        format!("{{{}}} : {}", columns.join(" "), matchsql)
    };
    let query = format!(
        r#"
        SELECT DISTINCT t.id, t.source_path
        FROM rules_engine_fts
        JOIN tracks t ON rules_engine_fts.rowid = t.rowid
        WHERE rules_engine_fts MATCH '{}'
        ORDER BY t.source_path
    "#,
        ftsquery
    );

    debug!("constructed matching query {}", query);

    let mut results = Vec::new();
    let conn = Connection::open(config.cache_database_path())?;
    let mut stmt = conn.prepare(&query)?;
    let rows = stmt.query_map([], |row| {
        Ok(FastSearchResult {
            id: row.get(0)?,
            path: PathBuf::from(row.get::<_, String>(1)?),
        })
    })?;

    for row in rows {
        results.push(row?);
    }

    debug!("matched {} tracks from the read cache in {:?}", results.len(), time_start.elapsed());
    Ok(results)
}

fn convert_matcher_to_fts_query(pattern: &Pattern) -> String {
    // Join characters with "¬" separator to match the format used by process_string_for_fts
    let matchsql = pattern.needle.chars()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join("¬")
        .replace("'", "''")
        .replace("\"", "\"\"");
    format!("NEAR(\"{}\", {})", matchsql, pattern.needle.len().saturating_sub(2).max(0))
}

pub fn filter_track_false_positives_using_tags(matcher: &Matcher, fast_search_results: &[FastSearchResult], ignore: &[Matcher]) -> Result<Vec<AudioTags>> {
    let time_start = Instant::now();
    let mut rval = Vec::new();

    for fsr in fast_search_results {
        let tags = AudioTags::from_file(&fsr.path)?;
        let mut datafile: Option<StoredDataFile> = None;

        for field in &matcher.tags {
            let field_str = field.to_string();
            let mut is_match = false;

            is_match = is_match || (field_str == "tracktitle" && matches_pattern(&matcher.pattern, &value_to_str(&tags.tracktitle.clone().into())));
            is_match = is_match || (field_str == "releasedate" && matches_pattern(&matcher.pattern, &value_to_str(&tags.releasedate.into())));
            is_match = is_match || (field_str == "originaldate" && matches_pattern(&matcher.pattern, &value_to_str(&tags.originaldate.into())));
            is_match = is_match || (field_str == "compositiondate" && matches_pattern(&matcher.pattern, &value_to_str(&tags.compositiondate.into())));
            is_match = is_match || (field_str == "edition" && matches_pattern(&matcher.pattern, &value_to_str(&tags.edition.clone().into())));
            is_match = is_match || (field_str == "catalognumber" && matches_pattern(&matcher.pattern, &value_to_str(&tags.catalognumber.clone().into())));
            is_match = is_match || (field_str == "tracknumber" && tags.tracknumber.as_ref().is_some_and(|v| matches_pattern(&matcher.pattern, v)));
            is_match = is_match || (field_str == "tracktotal" && tags.tracktotal.is_some_and(|v| matches_pattern(&matcher.pattern, &v.to_string())));
            is_match = is_match || (field_str == "discnumber" && tags.discnumber.as_ref().is_some_and(|v| matches_pattern(&matcher.pattern, v)));
            is_match = is_match || (field_str == "disctotal" && tags.disctotal.is_some_and(|v| matches_pattern(&matcher.pattern, &v.to_string())));
            is_match = is_match || (field_str == "releasetitle" && tags.releasetitle.as_ref().is_some_and(|v| matches_pattern(&matcher.pattern, v)));
            is_match = is_match || (field_str == "releasetype" && matches_pattern(&matcher.pattern, &tags.releasetype));
            is_match = is_match || (field_str == "genre" && tags.genre.iter().any(|x| matches_pattern(&matcher.pattern, &value_to_str(&x.clone().into()))));
            is_match = is_match
                || (field_str == "secondarygenre" && tags.secondarygenre.iter().any(|x| matches_pattern(&matcher.pattern, &value_to_str(&x.clone().into()))));
            is_match =
                is_match || (field_str == "descriptor" && tags.descriptor.iter().any(|x| matches_pattern(&matcher.pattern, &value_to_str(&x.clone().into()))));
            is_match = is_match || (field_str == "label" && tags.label.iter().any(|x| matches_pattern(&matcher.pattern, &value_to_str(&x.clone().into()))));
            is_match = is_match
                || (field_str == "trackartist[main]"
                    && tags.trackartists.main.iter().any(|x| matches_pattern(&matcher.pattern, &value_to_str(&x.name.clone().into()))));
            is_match = is_match
                || (field_str == "trackartist[guest]"
                    && tags.trackartists.guest.iter().any(|x| matches_pattern(&matcher.pattern, &value_to_str(&x.name.clone().into()))));
            is_match = is_match
                || (field_str == "trackartist[remixer]"
                    && tags.trackartists.remixer.iter().any(|x| matches_pattern(&matcher.pattern, &value_to_str(&x.name.clone().into()))));
            is_match = is_match
                || (field_str == "trackartist[producer]"
                    && tags.trackartists.producer.iter().any(|x| matches_pattern(&matcher.pattern, &value_to_str(&x.name.clone().into()))));
            is_match = is_match
                || (field_str == "trackartist[composer]"
                    && tags.trackartists.composer.iter().any(|x| matches_pattern(&matcher.pattern, &value_to_str(&x.name.clone().into()))));
            is_match = is_match
                || (field_str == "trackartist[conductor]"
                    && tags.trackartists.conductor.iter().any(|x| matches_pattern(&matcher.pattern, &value_to_str(&x.name.clone().into()))));
            is_match = is_match
                || (field_str == "trackartist[djmixer]"
                    && tags.trackartists.djmixer.iter().any(|x| matches_pattern(&matcher.pattern, &value_to_str(&x.name.clone().into()))));
            is_match = is_match
                || (field_str == "releaseartist[main]"
                    && tags.releaseartists.main.iter().any(|x| matches_pattern(&matcher.pattern, &value_to_str(&x.name.clone().into()))));
            is_match = is_match
                || (field_str == "releaseartist[guest]"
                    && tags.releaseartists.guest.iter().any(|x| matches_pattern(&matcher.pattern, &value_to_str(&x.name.clone().into()))));
            is_match = is_match
                || (field_str == "releaseartist[remixer]"
                    && tags.releaseartists.remixer.iter().any(|x| matches_pattern(&matcher.pattern, &value_to_str(&x.name.clone().into()))));
            is_match = is_match
                || (field_str == "releaseartist[producer]"
                    && tags.releaseartists.producer.iter().any(|x| matches_pattern(&matcher.pattern, &value_to_str(&x.name.clone().into()))));
            is_match = is_match
                || (field_str == "releaseartist[composer]"
                    && tags.releaseartists.composer.iter().any(|x| matches_pattern(&matcher.pattern, &value_to_str(&x.name.clone().into()))));
            is_match = is_match
                || (field_str == "releaseartist[conductor]"
                    && tags.releaseartists.conductor.iter().any(|x| matches_pattern(&matcher.pattern, &value_to_str(&x.name.clone().into()))));
            is_match = is_match
                || (field_str == "releaseartist[djmixer]"
                    && tags.releaseartists.djmixer.iter().any(|x| matches_pattern(&matcher.pattern, &value_to_str(&x.name.clone().into()))));

            if !is_match && field_str == "new" {
                if datafile.is_none() {
                    datafile = get_release_datafile_of_directory(tags.path.parent().unwrap())?;
                }
                is_match = matches_pattern(&matcher.pattern, &value_to_str(&datafile.as_ref().unwrap().new.into()));
            }

            if is_match && !ignore.is_empty() {
                let mut skip = false;
                for i in ignore {
                    let field_str = field.to_string();
                    skip = skip || (field_str == "tracktitle" && matches_pattern(&i.pattern, &value_to_str(&tags.tracktitle.clone().into())));
                    skip = skip || (field_str == "releasedate" && matches_pattern(&i.pattern, &value_to_str(&tags.releasedate.into())));
                    skip = skip || (field_str == "originaldate" && matches_pattern(&i.pattern, &value_to_str(&tags.originaldate.into())));
                    skip = skip || (field_str == "compositiondate" && matches_pattern(&i.pattern, &value_to_str(&tags.compositiondate.into())));
                    skip = skip || (field_str == "edition" && matches_pattern(&i.pattern, &value_to_str(&tags.edition.clone().into())));
                    skip = skip || (field_str == "catalognumber" && matches_pattern(&i.pattern, &value_to_str(&tags.catalognumber.clone().into())));
                    skip = skip || (field_str == "tracknumber" && tags.tracknumber.as_ref().is_some_and(|v| matches_pattern(&i.pattern, v)));
                    skip = skip || (field_str == "tracktotal" && tags.tracktotal.is_some_and(|v| matches_pattern(&i.pattern, &v.to_string())));
                    skip = skip || (field_str == "discnumber" && tags.discnumber.as_ref().is_some_and(|v| matches_pattern(&i.pattern, v)));
                    skip = skip || (field_str == "disctotal" && tags.disctotal.is_some_and(|v| matches_pattern(&i.pattern, &v.to_string())));
                    skip = skip || (field_str == "releasetitle" && tags.releasetitle.as_ref().is_some_and(|v| matches_pattern(&i.pattern, v)));
                    skip = skip || (field_str == "releasetype" && matches_pattern(&i.pattern, &tags.releasetype));
                    skip = skip || (field_str == "genre" && tags.genre.iter().any(|x| matches_pattern(&i.pattern, &value_to_str(&x.clone().into()))));
                    skip = skip
                        || (field_str == "secondarygenre" && tags.secondarygenre.iter().any(|x| matches_pattern(&i.pattern, &value_to_str(&x.clone().into()))));
                    skip = skip || (field_str == "descriptor" && tags.descriptor.iter().any(|x| matches_pattern(&i.pattern, &value_to_str(&x.clone().into()))));
                    skip = skip || (field_str == "label" && tags.label.iter().any(|x| matches_pattern(&i.pattern, &value_to_str(&x.clone().into()))));
                    skip = skip
                        || (field_str == "trackartist[main]"
                            && tags.trackartists.main.iter().any(|x| matches_pattern(&i.pattern, &value_to_str(&x.name.clone().into()))));
                    skip = skip
                        || (field_str == "trackartist[guest]"
                            && tags.trackartists.guest.iter().any(|x| matches_pattern(&i.pattern, &value_to_str(&x.name.clone().into()))));
                    skip = skip
                        || (field_str == "trackartist[remixer]"
                            && tags.trackartists.remixer.iter().any(|x| matches_pattern(&i.pattern, &value_to_str(&x.name.clone().into()))));
                    skip = skip
                        || (field_str == "trackartist[producer]"
                            && tags.trackartists.producer.iter().any(|x| matches_pattern(&i.pattern, &value_to_str(&x.name.clone().into()))));
                    skip = skip
                        || (field_str == "trackartist[composer]"
                            && tags.trackartists.composer.iter().any(|x| matches_pattern(&i.pattern, &value_to_str(&x.name.clone().into()))));
                    skip = skip
                        || (field_str == "trackartist[conductor]"
                            && tags.trackartists.conductor.iter().any(|x| matches_pattern(&i.pattern, &value_to_str(&x.name.clone().into()))));
                    skip = skip
                        || (field_str == "trackartist[djmixer]"
                            && tags.trackartists.djmixer.iter().any(|x| matches_pattern(&i.pattern, &value_to_str(&x.name.clone().into()))));
                    skip = skip
                        || (field_str == "releaseartist[main]"
                            && tags.releaseartists.main.iter().any(|x| matches_pattern(&i.pattern, &value_to_str(&x.name.clone().into()))));
                    skip = skip
                        || (field_str == "releaseartist[guest]"
                            && tags.releaseartists.guest.iter().any(|x| matches_pattern(&i.pattern, &value_to_str(&x.name.clone().into()))));
                    skip = skip
                        || (field_str == "releaseartist[remixer]"
                            && tags.releaseartists.remixer.iter().any(|x| matches_pattern(&i.pattern, &value_to_str(&x.name.clone().into()))));
                    skip = skip
                        || (field_str == "releaseartist[producer]"
                            && tags.releaseartists.producer.iter().any(|x| matches_pattern(&i.pattern, &value_to_str(&x.name.clone().into()))));
                    skip = skip
                        || (field_str == "releaseartist[composer]"
                            && tags.releaseartists.composer.iter().any(|x| matches_pattern(&i.pattern, &value_to_str(&x.name.clone().into()))));
                    skip = skip
                        || (field_str == "releaseartist[conductor]"
                            && tags.releaseartists.conductor.iter().any(|x| matches_pattern(&i.pattern, &value_to_str(&x.name.clone().into()))));
                    skip = skip
                        || (field_str == "releaseartist[djmixer]"
                            && tags.releaseartists.djmixer.iter().any(|x| matches_pattern(&i.pattern, &value_to_str(&x.name.clone().into()))));

                    if !skip && field_str == "new" {
                        if datafile.is_none() {
                            datafile = get_release_datafile_of_directory(tags.path.parent().unwrap())?;
                        }
                        skip = matches_pattern(&i.pattern, &value_to_str(&datafile.as_ref().unwrap().new.into()));
                    }

                    if skip {
                        break;
                    }
                }
                if skip {
                    break;
                }
            }

            if is_match {
                rval.push(tags);
                break;
            }
        }
    }

    debug!("filtered {} tracks down to {} tracks in {:?}", fast_search_results.len(), rval.len(), time_start.elapsed());
    Ok(rval)
}

fn get_release_datafile_of_directory(dir: &Path) -> Result<Option<StoredDataFile>> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(filename) = path.file_name() {
                if STORED_DATA_FILE_REGEX.is_match(&filename.to_string_lossy()) {
                    let content = std::fs::read_to_string(&path)?;
                    let diskdata: toml::Value = toml::from_str(&content)?;
                    return Ok(Some(StoredDataFile {
                        new: diskdata.get("new").and_then(|v| v.as_bool()).unwrap_or(true),
                        added_at: diskdata
                            .get("added_at")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%z").to_string()),
                    }));
                }
            }
        }
    }
    Err(RoseError::Generic(format!("Release data file not found in {:?}. How is it in the library?", dir)))
}

type Changes = (String, TagValue, TagValue);

pub fn execute_metadata_actions(
    config: &Config,
    actions: &[Action],
    audiotags: Vec<AudioTags>,
    dry_run: bool,
    confirm_yes: bool,
    enter_number_to_confirm_above_count: usize,
) -> Result<()> {
    fn names(artists: &[Artist]) -> Vec<String> {
        artists.iter().map(|a| a.name.clone()).collect()
    }

    fn artists(names: Vec<String>) -> Vec<Artist> {
        names.into_iter().map(|n| Artist::new(&n)).collect()
    }

    let mut opened_datafiles: HashMap<String, StoredDataFile> = HashMap::new();

    let open_datafile = |path: &Path, opened_datafiles: &mut HashMap<String, StoredDataFile>| -> Result<StoredDataFile> {
        let parent_str = path.parent().unwrap().to_string_lossy().to_string();
        if let Some(datafile) = opened_datafiles.get(&parent_str) {
            Ok(datafile.clone())
        } else {
            let datafile =
                get_release_datafile_of_directory(path.parent().unwrap())?.ok_or_else(|| RoseError::Generic("Could not find datafile".to_string()))?;
            opened_datafiles.insert(parent_str, datafile.clone());
            Ok(datafile)
        }
    };

    let mut actionable_audiotags: Vec<(AudioTags, Vec<Changes>)> = Vec::new();
    // Map from parent directory to tuple.
    let mut actionable_datafiles: HashMap<String, (AudioTags, StoredDataFile, Vec<Changes>)> = HashMap::new();

    // We loop over audiotags as the main loop since the rules engine operates on tracks. Perhaps in
    // the future we better arrange this into release-level as well as track-level and make datafile
    // part of the release loop. We apply the datafile updates as-we-go, so even if we have 12 tracks
    // updating a datafile, the update should only apply and be shown once.
    for mut tags in audiotags {
        let origtags = tags.clone();
        let mut potential_audiotag_changes: Vec<Changes> = Vec::new();
        // Load the datafile if we use it. Then we know that we have potential datafile changes for
        // this datafile.
        let mut datafile: Option<StoredDataFile> = None;
        let mut potential_datafile_changes: Vec<Changes> = Vec::new();

        for act in actions {
            let fields_to_update = &act.tags;
            for field in fields_to_update {
                let field_str = field.to_string();
                // Datafile actions.
                // Only read the datafile if it's necessary; we don't want to pay the extra cost
                // every time for rarer fields. Store the opened datafiles in opened_datafiles.
                if field_str == "new" {
                    if datafile.is_none() {
                        datafile = Some(open_datafile(&tags.path, &mut opened_datafiles)?);
                    }
                    let datafile_ref = datafile.as_mut().unwrap();
                    let v = execute_single_action(act, &datafile_ref.new.into())?;
                    let v_str = v.as_str_option();
                    if v_str != Some("true") && v_str != Some("false") && !matches!(v, TagValue::None) {
                        return Err(RoseError::Expected(RoseExpectedError::Generic(format!(
                            "Failed to assign new value {} to new: value must be string `true` or `false`",
                            v.to_string_or_default()
                        ))));
                    }
                    let orig_value = datafile_ref.new;
                    datafile_ref.new = v_str == Some("true");
                    if orig_value != datafile_ref.new {
                        potential_datafile_changes.push(("new".to_string(), orig_value.into(), datafile_ref.new.into()));
                    }
                }

                // AudioTag Actions
                match field.to_string().as_str() {
                    "tracktitle" => {
                        let new_value = execute_single_action(act, &tags.tracktitle.clone().into())?;
                        tags.tracktitle = new_value.to_option_string();
                        potential_audiotag_changes.push(("title".to_string(), origtags.tracktitle.clone().into(), tags.tracktitle.clone().into()));
                    }
                    "releasedate" => {
                        let v = execute_single_action(act, &tags.releasedate.into())?;
                        tags.releasedate = RoseDate::parse(v.as_str_option());
                        if !matches!(v, TagValue::None) && tags.releasedate.is_none() {
                            return Err(RoseError::Expected(RoseExpectedError::Generic(format!(
                                "Failed to assign new value {} to releasedate: value must be date string",
                                v.to_string_or_default()
                            ))));
                        }
                        potential_audiotag_changes.push(("releasedate".to_string(), origtags.releasedate.into(), tags.releasedate.into()));
                    }
                    "originaldate" => {
                        let v = execute_single_action(act, &tags.originaldate.into())?;
                        tags.originaldate = RoseDate::parse(v.as_str_option());
                        if !matches!(v, TagValue::None) && tags.originaldate.is_none() {
                            return Err(RoseError::Expected(RoseExpectedError::Generic(format!(
                                "Failed to assign new value {} to originaldate: value must be date string",
                                v.to_string_or_default()
                            ))));
                        }
                        potential_audiotag_changes.push(("originaldate".to_string(), origtags.originaldate.into(), tags.originaldate.into()));
                    }
                    "compositiondate" => {
                        let v = execute_single_action(act, &tags.compositiondate.into())?;
                        tags.compositiondate = RoseDate::parse(v.as_str_option());
                        if !matches!(v, TagValue::None) && tags.compositiondate.is_none() {
                            return Err(RoseError::Expected(RoseExpectedError::Generic(format!(
                                "Failed to assign new value {} to compositiondate: value must be date string",
                                v.to_string_or_default()
                            ))));
                        }
                        potential_audiotag_changes.push(("compositiondate".to_string(), origtags.compositiondate.into(), tags.compositiondate.into()));
                    }
                    "edition" => {
                        let new_value = execute_single_action(act, &tags.edition.clone().into())?;
                        tags.edition = new_value.to_option_string();
                        potential_audiotag_changes.push(("edition".to_string(), origtags.edition.clone().into(), tags.edition.clone().into()));
                    }
                    "catalognumber" => {
                        let new_value = execute_single_action(act, &tags.catalognumber.clone().into())?;
                        tags.catalognumber = new_value.to_option_string();
                        potential_audiotag_changes.push((
                            "catalognumber".to_string(),
                            origtags.catalognumber.clone().into(),
                            tags.catalognumber.clone().into(),
                        ));
                    }
                    "tracknumber" => {
                        let new_value = execute_single_action(act, &tags.tracknumber.clone().into())?;
                        tags.tracknumber = match &new_value {
                            TagValue::String(s) => Some(s.clone()),
                            TagValue::None => None,
                            _ => Some(new_value.to_string()),
                        };
                        potential_audiotag_changes.push(("tracknumber".to_string(), origtags.tracknumber.clone().into(), tags.tracknumber.clone().into()));
                    }
                    "discnumber" => {
                        let new_value = execute_single_action(act, &tags.discnumber.clone().into())?;
                        tags.discnumber = match &new_value {
                            TagValue::String(s) => Some(s.clone()),
                            TagValue::None => None,
                            _ => Some(new_value.to_string()),
                        };
                        potential_audiotag_changes.push(("discnumber".to_string(), origtags.discnumber.clone().into(), tags.discnumber.clone().into()));
                    }
                    "releasetitle" => {
                        let new_value = execute_single_action(act, &tags.releasetitle.clone().into())?;
                        tags.releasetitle = new_value.to_option_string();
                        potential_audiotag_changes.push(("release".to_string(), origtags.releasetitle.clone().into(), tags.releasetitle.clone().into()));
                    }
                    "releasetype" => {
                        let new_value = execute_single_action(act, &tags.releasetype.clone().into())?;
                        tags.releasetype = new_value.to_option_string().unwrap_or_else(|| "unknown".to_string());
                        potential_audiotag_changes.push(("releasetype".to_string(), origtags.releasetype.clone().into(), tags.releasetype.clone().into()));
                    }
                    "genre" => {
                        let new_value = execute_multi_value_action(act, &tags.genre)?;
                        tags.genre = new_value;
                        potential_audiotag_changes.push(("genre".to_string(), origtags.genre.clone().into(), tags.genre.clone().into()));
                    }
                    "secondarygenre" => {
                        let new_value = execute_multi_value_action(act, &tags.secondarygenre)?;
                        tags.secondarygenre = new_value;
                        potential_audiotag_changes.push((
                            "secondarygenre".to_string(),
                            origtags.secondarygenre.clone().into(),
                            tags.secondarygenre.clone().into(),
                        ));
                    }
                    "descriptor" => {
                        let new_value = execute_multi_value_action(act, &tags.descriptor)?;
                        tags.descriptor = new_value;
                        potential_audiotag_changes.push(("descriptor".to_string(), origtags.descriptor.clone().into(), tags.descriptor.clone().into()));
                    }
                    "label" => {
                        let new_value = execute_multi_value_action(act, &tags.label)?;
                        tags.label = new_value;
                        potential_audiotag_changes.push(("label".to_string(), origtags.label.clone().into(), tags.label.clone().into()));
                    }
                    "trackartist[main]" => {
                        let current_names = names(&tags.trackartists.main);
                        let new_names = execute_multi_value_action(act, &current_names)?;
                        tags.trackartists.main = artists(new_names.clone());
                        potential_audiotag_changes.push(("trackartist[main]".to_string(), names(&origtags.trackartists.main).into(), new_names.into()));
                    }
                    "trackartist[guest]" => {
                        let current_names = names(&tags.trackartists.guest);
                        let new_names = execute_multi_value_action(act, &current_names)?;
                        tags.trackartists.guest = artists(new_names.clone());
                        potential_audiotag_changes.push(("trackartist[guest]".to_string(), names(&origtags.trackartists.guest).into(), new_names.into()));
                    }
                    "trackartist[remixer]" => {
                        let current_names = names(&tags.trackartists.remixer);
                        let new_names = execute_multi_value_action(act, &current_names)?;
                        tags.trackartists.remixer = artists(new_names.clone());
                        potential_audiotag_changes.push(("trackartist[remixer]".to_string(), names(&origtags.trackartists.remixer).into(), new_names.into()));
                    }
                    "trackartist[producer]" => {
                        let current_names = names(&tags.trackartists.producer);
                        let new_names = execute_multi_value_action(act, &current_names)?;
                        tags.trackartists.producer = artists(new_names.clone());
                        potential_audiotag_changes.push(("trackartist[producer]".to_string(), names(&origtags.trackartists.producer).into(), new_names.into()));
                    }
                    "trackartist[composer]" => {
                        let current_names = names(&tags.trackartists.composer);
                        let new_names = execute_multi_value_action(act, &current_names)?;
                        tags.trackartists.composer = artists(new_names.clone());
                        potential_audiotag_changes.push(("trackartist[composer]".to_string(), names(&origtags.trackartists.composer).into(), new_names.into()));
                    }
                    "trackartist[conductor]" => {
                        let current_names = names(&tags.trackartists.conductor);
                        let new_names = execute_multi_value_action(act, &current_names)?;
                        tags.trackartists.conductor = artists(new_names.clone());
                        potential_audiotag_changes.push((
                            "trackartist[conductor]".to_string(),
                            names(&origtags.trackartists.conductor).into(),
                            new_names.into(),
                        ));
                    }
                    "trackartist[djmixer]" => {
                        let current_names = names(&tags.trackartists.djmixer);
                        let new_names = execute_multi_value_action(act, &current_names)?;
                        tags.trackartists.djmixer = artists(new_names.clone());
                        potential_audiotag_changes.push(("trackartist[djmixer]".to_string(), names(&origtags.trackartists.djmixer).into(), new_names.into()));
                    }
                    "releaseartist[main]" => {
                        let current_names = names(&tags.releaseartists.main);
                        let new_names = execute_multi_value_action(act, &current_names)?;
                        tags.releaseartists.main = artists(new_names.clone());
                        potential_audiotag_changes.push(("releaseartist[main]".to_string(), names(&origtags.releaseartists.main).into(), new_names.into()));
                    }
                    "releaseartist[guest]" => {
                        let current_names = names(&tags.releaseartists.guest);
                        let new_names = execute_multi_value_action(act, &current_names)?;
                        tags.releaseartists.guest = artists(new_names.clone());
                        potential_audiotag_changes.push(("releaseartist[guest]".to_string(), names(&origtags.releaseartists.guest).into(), new_names.into()));
                    }
                    "releaseartist[remixer]" => {
                        let current_names = names(&tags.releaseartists.remixer);
                        let new_names = execute_multi_value_action(act, &current_names)?;
                        tags.releaseartists.remixer = artists(new_names.clone());
                        potential_audiotag_changes.push((
                            "releaseartist[remixer]".to_string(),
                            names(&origtags.releaseartists.remixer).into(),
                            new_names.into(),
                        ));
                    }
                    "releaseartist[producer]" => {
                        let current_names = names(&tags.releaseartists.producer);
                        let new_names = execute_multi_value_action(act, &current_names)?;
                        tags.releaseartists.producer = artists(new_names.clone());
                        potential_audiotag_changes.push((
                            "releaseartist[producer]".to_string(),
                            names(&origtags.releaseartists.producer).into(),
                            new_names.into(),
                        ));
                    }
                    "releaseartist[composer]" => {
                        let current_names = names(&tags.releaseartists.composer);
                        let new_names = execute_multi_value_action(act, &current_names)?;
                        tags.releaseartists.composer = artists(new_names.clone());
                        potential_audiotag_changes.push((
                            "releaseartist[composer]".to_string(),
                            names(&origtags.releaseartists.composer).into(),
                            new_names.into(),
                        ));
                    }
                    "releaseartist[conductor]" => {
                        let current_names = names(&tags.releaseartists.conductor);
                        let new_names = execute_multi_value_action(act, &current_names)?;
                        tags.releaseartists.conductor = artists(new_names.clone());
                        potential_audiotag_changes.push((
                            "releaseartist[conductor]".to_string(),
                            names(&origtags.releaseartists.conductor).into(),
                            new_names.into(),
                        ));
                    }
                    "releaseartist[djmixer]" => {
                        let current_names = names(&tags.releaseartists.djmixer);
                        let new_names = execute_multi_value_action(act, &current_names)?;
                        tags.releaseartists.djmixer = artists(new_names.clone());
                        potential_audiotag_changes.push((
                            "releaseartist[djmixer]".to_string(),
                            names(&origtags.releaseartists.djmixer).into(),
                            new_names.into(),
                        ));
                    }
                    _ => {
                        // Handle artist shorthand
                        if field_str == "artist" || field_str.starts_with("artist[") {
                            // This is handled by expanding to track/release artists in the parser
                        }
                    }
                }
            }
        }

        // Compute real changes by diffing the tags, and then store.
        let tag_changes: Vec<Changes> = potential_audiotag_changes.into_iter().filter(|(_, old, new)| old != new).collect();
        let has_tag_changes = !tag_changes.is_empty();
        if has_tag_changes {
            actionable_audiotags.push((tags.clone(), tag_changes));
        }

        // We already handled diffing for the datafile above. This moves the inner-track-loop
        // datafile updates to the outer scope.
        let has_datafile_changes = !potential_datafile_changes.is_empty();
        if let Some(df) = datafile {
            if has_datafile_changes {
                let parent_str = tags.path.parent().unwrap().to_string_lossy().to_string();
                match actionable_datafiles.get_mut(&parent_str) {
                    Some((_, _, datafile_changes)) => {
                        datafile_changes.extend(potential_datafile_changes);
                    }
                    None => {
                        actionable_datafiles.insert(parent_str, (tags.clone(), df, potential_datafile_changes));
                    }
                }
            }
        }

        if !has_tag_changes && !has_datafile_changes {
            debug!("skipping matched track {:?}: no changes calculated off tags and datafile", tags.path);
        }
    }

    if actionable_audiotags.is_empty() && actionable_datafiles.is_empty() {
        println!("\x1b[2;3mNo matching tracks found\x1b[0m");
        println!();
        return Ok(());
    }

    // Display changes and ask for user confirmation
    let mut todisplay: Vec<(String, Vec<Changes>)> = Vec::new();
    let mut maxpathwidth = 0;

    for (tags, tag_changes) in &actionable_audiotags {
        let mut pathtext = tags.path.strip_prefix(&config.music_source_dir).unwrap_or(&tags.path).to_string_lossy().to_string();
        if pathtext.len() >= 120 {
            pathtext = format!("{}..{}", &pathtext[..59], &pathtext[pathtext.len() - 59..]);
        }
        maxpathwidth = maxpathwidth.max(pathtext.len());
        todisplay.push((pathtext, tag_changes.clone()));
    }

    for (path, (_, _, datafile_changes)) in &actionable_datafiles {
        let mut pathtext = path.strip_prefix(&format!("{}/", config.music_source_dir.to_string_lossy())).unwrap_or(path).to_string();
        if pathtext.len() >= 120 {
            pathtext = format!("{}..{}", &pathtext[..59], &pathtext[pathtext.len() - 59..]);
        }
        maxpathwidth = maxpathwidth.max(pathtext.len());
        todisplay.push((pathtext, datafile_changes.clone()));
    }

    // And then display it.
    for (pathtext, tag_changes) in &todisplay {
        println!("\x1b[4m{}\x1b[0m", pathtext);
        for (name, old, new) in tag_changes {
            print!("      {}: ", name);
            print!("\x1b[31m{}\x1b[0m", old);
            print!(" -> ");
            println!("\x1b[32;1m{}\x1b[0m", new);
        }
    }

    // If we're dry-running, then abort here.
    if dry_run {
        println!();
        println!("\x1b[2mThis is a dry run, aborting. {} tracks would have been modified.\x1b[0m", actionable_audiotags.len());
        return Ok(());
    }

    // And then let's go for the confirmation.
    let num_changes = actionable_audiotags.len() + actionable_datafiles.len();
    if confirm_yes {
        println!();
        if num_changes > enter_number_to_confirm_above_count {
            loop {
                print!("Write changes to {} tracks? Enter \x1b[1m{}\x1b[0m to confirm (or 'no' to abort): ", num_changes, num_changes);
                use std::io::{self, Write};
                io::stdout().flush()?;
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                let input = input.trim();
                if input == "no" {
                    debug!("aborting planned tag changes after user confirmation");
                    return Ok(());
                }
                if input == num_changes.to_string() {
                    println!();
                    break;
                }
            }
        } else {
            print!("Write changes to \x1b[1m{}\x1b[0m tracks? [Y/n] ", num_changes);
            use std::io::{self, Write};
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim().to_lowercase();
            if input == "n" || input == "no" {
                debug!("aborting planned tag changes after user confirmation");
                return Ok(());
            }
            println!();
        }
    }

    // Flush writes to disk
    info!("writing tag changes for actions {:?}", actions.iter().map(|a| format!("{:?}", a)).collect::<Vec<_>>().join(" "));
    let mut changed_release_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (mut tags, tag_changes) in actionable_audiotags {
        if let Some(ref release_id) = tags.release_id {
            changed_release_ids.insert(release_id.clone());
        }
        let pathtext = tags.path.strip_prefix(&config.music_source_dir).unwrap_or(&tags.path).to_string_lossy().to_string();
        debug!(
            "attempting to write {} changes: {}",
            pathtext,
            tag_changes.iter().map(|(_, old, new)| format!("{} -> {}", old, new)).collect::<Vec<_>>().join(" //// ")
        );
        tags.flush(config, false)?;
        info!("wrote tag changes to {}", pathtext);
    }

    for (path, (tags, datafile, datafile_changes)) in actionable_datafiles {
        if let Some(ref release_id) = tags.release_id {
            changed_release_ids.insert(release_id.clone());
        }
        let pathtext = path.strip_prefix(&format!("{}/", config.music_source_dir.to_string_lossy())).unwrap_or(&path);
        debug!(
            "attempting to write {} changes: {}",
            pathtext,
            datafile_changes.iter().map(|(_, old, new)| format!("{} -> {}", old, new)).collect::<Vec<_>>().join(" //// ")
        );

        for entry in std::fs::read_dir(&path)? {
            let entry = entry?;
            let entry_path = entry.path();
            if entry_path.is_file() {
                if let Some(filename) = entry_path.file_name() {
                    if STORED_DATA_FILE_REGEX.is_match(&filename.to_string_lossy()) {
                        let mut toml_value = toml::value::Table::new();
                        toml_value.insert("new".to_string(), toml::Value::Boolean(datafile.new));
                        toml_value.insert("added_at".to_string(), toml::Value::String(datafile.added_at.clone()));
                        let toml_string = toml::to_string(&toml_value)?;
                        std::fs::write(&entry_path, toml_string)?;
                    }
                }
            }
        }
        info!("wrote datafile changes to {}", pathtext);
    }

    println!();
    println!("Applied tag changes to {} tracks!", num_changes);

    // Trigger cache update
    println!();
    let _release_ids: Vec<String> = changed_release_ids.into_iter().collect();
    let releases = list_releases(config, None, None, None)?;
    let source_paths: Vec<PathBuf> = releases.into_iter().filter(|r| _release_ids.contains(&r.id)).map(|r| r.source_path).collect();
    update_cache_for_releases(config, Some(source_paths), false, false)?;

    Ok(())
}

fn execute_single_action(action: &Action, value: &TagValue) -> Result<TagValue> {
    if let Some(pattern) = &action.pattern {
        if !matches_pattern(pattern, &value_to_str(value)) {
            return Ok(value.clone());
        }
    }

    let strvalue = value_to_str(value);

    match &action.behavior {
        ActionBehavior::Replace(replace_action) => Ok(TagValue::String(replace_action.replacement.clone())),
        ActionBehavior::Sed(sed_action) => {
            if strvalue.is_empty() {
                Ok(TagValue::None)
            } else {
                Ok(TagValue::String(sed_action.src.replace_all(&strvalue, &sed_action.dst).to_string()))
            }
        }
        ActionBehavior::Delete(_) => Ok(TagValue::None),
        _ => Err(RoseError::Generic(format!("Invalid action {:?} for single-value tag: Should have been caught in parsing", action.behavior))),
    }
}

fn execute_multi_value_action(action: &Action, values: &[String]) -> Result<Vec<String>> {
    let mut matching_idx = (0..values.len()).collect::<Vec<_>>();
    if let Some(pattern) = &action.pattern {
        matching_idx = values.iter().enumerate().filter(|(_, v)| matches_pattern(pattern, v)).map(|(i, _)| i).collect();
        if matching_idx.is_empty() {
            return Ok(values.to_vec());
        }
    }

    match &action.behavior {
        ActionBehavior::Add(add_action) => Ok(uniq([values.to_vec(), vec![add_action.value.clone()]].concat())),
        _ => {
            let mut rval = Vec::new();
            for (i, v) in values.iter().enumerate() {
                if !matching_idx.contains(&i) {
                    rval.push(v.clone());
                    continue;
                }
                match &action.behavior {
                    ActionBehavior::Delete(_) => continue,
                    ActionBehavior::Replace(replace_action) => {
                        let replacement_str = &replace_action.replacement;
                        for nv in replacement_str.split(';') {
                            let nv = nv.trim();
                            if !nv.is_empty() {
                                rval.push(nv.to_string());
                            }
                        }
                    }
                    ActionBehavior::Sed(sed_action) => {
                        let replaced = sed_action.src.replace_all(v, &sed_action.dst).to_string();
                        for nv in replaced.split(';') {
                            let nv = nv.trim();
                            if !nv.is_empty() {
                                rval.push(nv.to_string());
                            }
                        }
                    }
                    ActionBehavior::Split(split_action) => {
                        for nv in v.split(&split_action.delimiter) {
                            let nv = nv.trim();
                            if !nv.is_empty() {
                                rval.push(nv.to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(uniq(rval))
        }
    }
}

// Query engine functions

pub fn fast_search_for_matching_releases(config: &Config, matcher: &Matcher, include_loose_tracks: bool) -> Result<Vec<FastSearchResult>> {
    let time_start = Instant::now();

    let track_tags: Vec<&Tag> = matcher.tags.iter().filter(|t| !RELEASE_TAGS.contains(&&*t.to_string())).collect();

    if !track_tags.is_empty() {
        // But allow an exception if both trackartist and releaseartist are defined
        let has_releaseartist = matcher.tags.iter().any(|t| t.to_string().starts_with("releaseartist"));
        let filtered_track_tags: Vec<&Tag> = if has_releaseartist {
            track_tags.into_iter().filter(|t| !t.to_string().starts_with("trackartist")).collect()
        } else {
            track_tags
        };

        if !filtered_track_tags.is_empty() {
            return Err(RoseError::Expected(RoseExpectedError::Generic(format!(
                "Track tags are not allowed when matching against releases: {}",
                filtered_track_tags.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(", ")
            ))));
        }
    }

    let matchsql = convert_matcher_to_fts_query(&matcher.pattern);
    debug!("converted match {:?} to {:?}", matcher, matchsql);

    let columns: Vec<String> = uniq(matcher.tags.iter().map(|t| TAG_ROLE_REGEX.replace(&t.to_string(), "").to_string()).collect());
    let ftsquery = if columns.len() == 1 {
        format!("{}:{}", columns[0], matchsql)
    } else {
        format!("{{{}}} : {}", columns.join(" "), matchsql)
    };
    let mut query = format!(
        r#"
        SELECT DISTINCT r.id, r.source_path
        FROM rules_engine_fts
        JOIN tracks t ON rules_engine_fts.rowid = t.rowid
        JOIN releases r ON r.id = t.release_id
        WHERE rules_engine_fts MATCH '{}'
    "#,
        ftsquery
    );

    if !include_loose_tracks {
        query.push_str(" AND r.releasetype <> 'loosetrack'");
    }
    query.push_str(" ORDER BY r.source_path");

    debug!("constructed matching query {}", query);

    let mut results = Vec::new();
    let conn = Connection::open(config.cache_database_path())?;
    let mut stmt = conn.prepare(&query)?;
    let rows = stmt.query_map([], |row| {
        Ok(FastSearchResult {
            id: row.get(0)?,
            path: PathBuf::from(row.get::<_, String>(1)?),
        })
    })?;

    for row in rows {
        results.push(row?);
    }

    debug!("matched {} releases from the read cache in {:?}", results.len(), time_start.elapsed());
    Ok(results)
}

pub fn filter_track_false_positives_using_read_cache(matcher: &Matcher, tracks: Vec<Track>) -> Vec<Track> {
    let time_start = Instant::now();
    let _tracks_len = tracks.len();
    let mut result = Vec::new();

    for t in tracks.into_iter() {
        for field in &matcher.tags {
            let mut is_match = false;
            match field.to_string().as_str() {
                "tracktitle" => is_match = matches_pattern(&matcher.pattern, &value_to_str(&t.tracktitle.clone().into())),
                "releasedate" => is_match = matches_pattern(&matcher.pattern, &value_to_str(&t.release.releasedate.into())),
                "originaldate" => is_match = matches_pattern(&matcher.pattern, &value_to_str(&t.release.originaldate.into())),
                "compositiondate" => is_match = matches_pattern(&matcher.pattern, &value_to_str(&t.release.compositiondate.into())),
                "edition" => is_match = matches_pattern(&matcher.pattern, &value_to_str(&t.release.edition.clone().into())),
                "catalognumber" => is_match = matches_pattern(&matcher.pattern, &value_to_str(&t.release.catalognumber.clone().into())),
                "tracknumber" => is_match = matches_pattern(&matcher.pattern, &t.tracknumber),
                "tracktotal" => is_match = matches_pattern(&matcher.pattern, &t.tracktotal.to_string()),
                "discnumber" => is_match = matches_pattern(&matcher.pattern, &t.discnumber),
                "disctotal" => is_match = matches_pattern(&matcher.pattern, &t.release.disctotal.to_string()),
                "releasetitle" => is_match = matches_pattern(&matcher.pattern, &t.release.releasetitle),
                "releasetype" => is_match = matches_pattern(&matcher.pattern, &t.release.releasetype),
                "new" => is_match = matches_pattern(&matcher.pattern, &t.release.new.to_string()),
                "genre" => is_match = t.release.genres.iter().any(|x| matches_pattern(&matcher.pattern, x)),
                "secondarygenre" => is_match = t.release.secondary_genres.iter().any(|x| matches_pattern(&matcher.pattern, x)),
                "descriptor" => is_match = t.release.descriptors.iter().any(|x| matches_pattern(&matcher.pattern, x)),
                "label" => is_match = t.release.labels.iter().any(|x| matches_pattern(&matcher.pattern, x)),
                "trackartist[main]" => is_match = t.trackartists.main.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                "trackartist[guest]" => is_match = t.trackartists.guest.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                "trackartist[remixer]" => is_match = t.trackartists.remixer.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                "trackartist[producer]" => is_match = t.trackartists.producer.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                "trackartist[composer]" => is_match = t.trackartists.composer.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                "trackartist[conductor]" => is_match = t.trackartists.conductor.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                "trackartist[djmixer]" => is_match = t.trackartists.djmixer.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                "releaseartist[main]" => is_match = t.release.releaseartists.main.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                "releaseartist[guest]" => is_match = t.release.releaseartists.guest.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                "releaseartist[remixer]" => is_match = t.release.releaseartists.remixer.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                "releaseartist[producer]" => is_match = t.release.releaseartists.producer.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                "releaseartist[composer]" => is_match = t.release.releaseartists.composer.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                "releaseartist[conductor]" => is_match = t.release.releaseartists.conductor.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                "releaseartist[djmixer]" => is_match = t.release.releaseartists.djmixer.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                _ => {}
            }
            if is_match {
                result.push(t);
                break;
            }
        }
    }

    debug!("filtered {} tracks down to {} tracks in {:?}", _tracks_len, result.len(), time_start.elapsed());
    result
}

pub fn filter_release_false_positives_using_read_cache(matcher: &Matcher, releases: Vec<Release>, include_loose_tracks: bool) -> Vec<Release> {
    let time_start = Instant::now();
    let releases_len = releases.len();
    let mut result = Vec::new();

    for r in releases.into_iter() {
        if !include_loose_tracks && r.releasetype == "loosetrack" {
            continue;
        }
        for field in &matcher.tags {
            let mut is_match = false;
            // Only attempt to match the release tags; ignore track tags.
            match field.to_string().as_str() {
                "releasedate" => is_match = matches_pattern(&matcher.pattern, &value_to_str(&r.releasedate.into())),
                "originaldate" => is_match = matches_pattern(&matcher.pattern, &value_to_str(&r.originaldate.into())),
                "compositiondate" => is_match = matches_pattern(&matcher.pattern, &value_to_str(&r.compositiondate.into())),
                "edition" => is_match = matches_pattern(&matcher.pattern, &value_to_str(&r.edition.clone().into())),
                "catalognumber" => is_match = matches_pattern(&matcher.pattern, &value_to_str(&r.catalognumber.clone().into())),
                "releasetitle" => is_match = matches_pattern(&matcher.pattern, &r.releasetitle),
                "releasetype" => is_match = matches_pattern(&matcher.pattern, &r.releasetype),
                "new" => is_match = matches_pattern(&matcher.pattern, &r.new.to_string()),
                "genre" => is_match = r.genres.iter().any(|x| matches_pattern(&matcher.pattern, x)),
                "secondarygenre" => is_match = r.secondary_genres.iter().any(|x| matches_pattern(&matcher.pattern, x)),
                "descriptor" => is_match = r.descriptors.iter().any(|x| matches_pattern(&matcher.pattern, x)),
                "label" => is_match = r.labels.iter().any(|x| matches_pattern(&matcher.pattern, x)),
                "releaseartist[main]" => is_match = r.releaseartists.main.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                "releaseartist[guest]" => is_match = r.releaseartists.guest.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                "releaseartist[remixer]" => is_match = r.releaseartists.remixer.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                "releaseartist[producer]" => is_match = r.releaseartists.producer.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                "releaseartist[composer]" => is_match = r.releaseartists.composer.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                "releaseartist[conductor]" => is_match = r.releaseartists.conductor.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                "releaseartist[djmixer]" => is_match = r.releaseartists.djmixer.iter().any(|x| matches_pattern(&matcher.pattern, &x.name)),
                _ => {}
            }
            if is_match {
                result.push(r);
                break;
            }
        }
    }

    debug!("filtered {} releases down to {} releases in {:?}", releases_len, result.len(), time_start.elapsed());
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{connect, get_release, list_releases, list_tracks, update_cache};
    use crate::testing;

    fn parse_rule(matcher: &str, actions: &[&str]) -> Rule {
        Rule::parse(matcher, actions.iter().map(|s| s.to_string()).collect(), None).unwrap()
    }

    fn parse_rule_with_ignore(matcher: &str, actions: &[&str], ignore: &[&str]) -> Rule {
        Rule::parse(matcher, actions.iter().map(|s| s.to_string()).collect(), Some(ignore.iter().map(|s| s.to_string()).collect())).unwrap()
    }

    #[test]
    fn test_rules_execution_match_substring() {
        let (config, _tmpdir) = testing::source_dir();

        // No match
        let rule = parse_rule("tracktitle:bbb", &["replace:lalala"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_ne!(af.tracktitle, Some("lalala".to_string()));

        // Match
        let rule = parse_rule("tracktitle:rack", &["replace:lalala"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.tracktitle, Some("lalala".to_string()));
    }

    #[test]
    fn test_rules_execution_match_beginnning() {
        let (config, _tmpdir) = testing::source_dir();

        // No match
        let rule = parse_rule("tracktitle:^rack", &["replace:lalala"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_ne!(af.tracktitle, Some("lalala".to_string()));

        // Match
        let rule = parse_rule("tracktitle:^Track", &["replace:lalala"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.tracktitle, Some("lalala".to_string()));
    }

    #[test]
    fn test_rules_execution_match_end() {
        let (config, _tmpdir) = testing::source_dir();

        // No match - track titles are 'Track 1' and 'Track 2'
        let rule = parse_rule("tracktitle:rack$", &["replace:lalala"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_ne!(af.tracktitle, Some("lalala".to_string()));

        // Match - 'Track 1' ends with 'rack 1'
        let rule = parse_rule("tracktitle:rack 1$", &["replace:lalala"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.tracktitle, Some("lalala".to_string()));
    }

    #[test]
    fn test_rules_execution_match_superstrict() {
        let (config, _tmpdir) = testing::source_dir();

        // No match - 'Track ' doesn't match 'Track 1' exactly
        let rule = parse_rule("tracktitle:^Track $", &["replace:lalala"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_ne!(af.tracktitle, Some("lalala".to_string()));

        // Match - exact match for 'Track 1'
        let rule = parse_rule("tracktitle:^Track 1$", &["replace:lalala"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.tracktitle, Some("lalala".to_string()));
    }

    #[test]
    fn test_rules_execution_match_escaped_superstrict() {
        let (config, _tmpdir) = testing::source_dir();

        let mut af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        af.tracktitle = Some("hi^Test$bye".to_string());
        af.flush(&config, false).unwrap();
        
        // Force cache update with force=true
        update_cache(&config, true, false).unwrap();

        // No match
        let rule = parse_rule("tracktitle:^Test$", &["replace:lalala"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_ne!(af.tracktitle, Some("lalala".to_string()));

        // Match
        let rule = parse_rule(r"tracktitle:\^Test\$", &["replace:lalala"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.tracktitle, Some("lalala".to_string()));
    }

    #[test]
    fn test_rules_execution_match_case_insensitive() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("tracktitle:tRaCk:i", &["replace:lalala"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.tracktitle, Some("lalala".to_string()));
    }

    #[test]
    fn test_rules_fields_match_tracktitle() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("tracktitle:Track", &["replace:8"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.tracktitle, Some("8".to_string()));
    }

    #[test]
    fn test_rules_fields_match_releasedate() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("releasedate:1990", &["replace:8"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.releasedate, Some(RoseDate::new(Some(8), None, None)));
    }


    #[test]
    fn test_rules_fields_match_releasetitle() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("releasetitle:Love Blackpink", &["replace:8"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.releasetitle, Some("8".to_string()));
    }


    #[test]
    fn test_rules_fields_match_tracknumber() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("tracknumber:1", &["replace:8"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.tracknumber, Some("8".to_string()));
    }

    #[test]
    fn test_rules_fields_match_discnumber() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("discnumber:1", &["replace:8"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.discnumber, Some("8".to_string()));
    }

    #[test]
    fn test_rules_fields_match_releasetype() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("releasetype:album", &["replace:live"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.releasetype, "live");
    }

    #[test]
    fn test_rules_fields_match_tracktotal() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("tracktotal:2", &["tracktitle/replace:8"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.tracktitle, Some("8".to_string()));
    }

    #[test]
    fn test_rules_fields_match_disctotal() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("disctotal:1", &["tracktitle/replace:8"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.tracktitle, Some("8".to_string()));
    }

    #[test]
    fn test_rules_fields_match_genre() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("genre:K-Pop", &["replace:8"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.genre, vec!["8".to_string(), "Pop".to_string()]);
    }

    #[test]
    fn test_rules_fields_match_label() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("label:Cool", &["replace:8"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.label, vec!["8".to_string()]);
    }

    #[test]
    fn test_rules_fields_match_releaseartist() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("releaseartist:BLACKPINK", &["replace:8"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.releaseartists.main, vec![Artist::new("8")]);
    }

    #[test]
    fn test_rules_fields_match_trackartist() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("trackartist:BLACKPINK", &["replace:8"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.trackartists.main, vec![Artist::new("8")]);
    }

    #[test]
    fn test_rules_fields_match_new() {
        let (config, _tmpdir) = testing::source_dir();

        // First set all releases to new: false to match Python test expectations
        let conn = connect(&config).unwrap();
        conn.execute("UPDATE releases SET new = false", []).unwrap();
        drop(conn);

        let rule = parse_rule("new:false", &["replace:true"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let release = get_release(&config, "ilovecarly").unwrap();
        assert!(release.unwrap().new);
        let release = get_release(&config, "ilovenewjeans").unwrap();
        assert!(release.unwrap().new);

        let rule = parse_rule("new:true", &["replace:false"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let release = get_release(&config, "ilovecarly").unwrap();
        assert!(!release.unwrap().new);
        let release = get_release(&config, "ilovenewjeans").unwrap();
        assert!(!release.unwrap().new);

        let rule = parse_rule("releasetitle:Carly", &["new/replace:true"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let release = get_release(&config, "ilovecarly").unwrap();
        assert!(release.unwrap().new);
        let release = get_release(&config, "ilovenewjeans").unwrap();
        assert!(!release.unwrap().new);
    }

    #[test]
    fn test_match_backslash() {
        let (config, _tmpdir) = testing::source_dir();
        
        let mut af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        af.tracktitle = Some(r"X \\ Y".to_string());
        af.flush(&config, false).unwrap();
        
        // Force cache update with force=true
        update_cache(&config, true, false).unwrap();

        let rule = parse_rule(r"tracktitle: \\ ", &[r"sed: \\\\ : // "]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.tracktitle, Some("X / Y".to_string()));
    }

    #[test]
    fn test_action_replace_with_delimiter() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("genre:K-Pop", &["replace:Hip-Hop;Rap"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.genre, vec!["Hip-Hop".to_string(), "Rap".to_string(), "Pop".to_string()]);
    }

    #[test]
    fn test_action_replace_with_delimiters_empty_str() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("genre:K-Pop", &["matched:/replace:Hip-Hop;;;;"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.genre, vec!["Hip-Hop".to_string()]);
    }

    #[test]
    fn test_sed_action() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("tracktitle:Track", &["sed:ack:ip"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.tracktitle, Some("Trip 1".to_string()));
    }

    #[test]
    fn test_sed_no_pattern() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("genre:P", &[r"matched:/sed:^(.*)$:i$1"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.genre, vec!["iK-Pop".to_string(), "iPop".to_string()]);
    }

    #[test]
    fn test_split_action() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("label:Cool", &["split:Cool"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.label, vec!["A".to_string(), "Label".to_string()]);
    }

    #[test]
    fn test_split_action_no_pattern() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("genre:K-Pop", &["matched:/split:P"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.genre, vec!["K-".to_string(), "op".to_string()]);
    }

    #[test]
    fn test_add_action() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("label:Cool", &["add:Even Cooler Label"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.label, vec!["A Cool Label".to_string(), "Even Cooler Label".to_string()]);
    }

    #[test]
    fn test_delete_action() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("genre:^Pop$", &["delete"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.genre, vec!["K-Pop".to_string()]);
    }

    #[test]
    fn test_delete_action_no_pattern() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("genre:^Pop$", &["matched:/delete"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.genre, Vec::<String>::new());
    }

    #[test]
    fn test_preserves_unmatched_multitags() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("genre:^Pop$", &["replace:lalala"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.genre, vec!["K-Pop".to_string(), "lalala".to_string()]);
    }

    #[test]
    fn test_action_on_different_tag() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("label:A Cool Label", &["genre/replace:hi"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.genre, vec!["hi".to_string()]);
    }

    #[test]
    fn test_action_no_pattern() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("genre:K-Pop", &["matched:/sed:P:B"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.genre, vec!["K-Bop".to_string(), "Bop".to_string()]);
    }

    #[test]
    fn test_chained_action() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule(
            "label:A Cool Label",
            &[
                "replace:Jennie",
                "label:^Jennie$/replace:Jisoo",
                "label:nomatch/replace:Rose",
                "genre/replace:haha",
            ],
        );
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.label, vec!["Jisoo".to_string()]);
        assert_eq!(af.genre, vec!["haha".to_string()]);
    }


    #[test]
    fn test_dry_run() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule("tracktitle:Track", &["replace:lalala"]);
        execute_metadata_rule(&config, &rule, true, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_ne!(af.tracktitle, Some("lalala".to_string()));
    }

    #[test]
    fn test_run_stored_rules() {
        let (mut config, _tmpdir) = testing::source_dir();
        config.stored_metadata_rules = vec![parse_rule("tracktitle:Track", &["replace:lalala"])];

        execute_stored_metadata_rules(&config, false, false).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.tracktitle, Some("lalala".to_string()));
    }

    #[test]
    fn test_fast_search_for_matching_releases() {
        let (config, _tmpdir) = testing::seeded_cache();

        // 'Techno Man' is a release artist for r1 in the test data
        let matcher = Matcher::parse("releaseartist:Techno Man").unwrap();
        let results = fast_search_for_matching_releases(&config, &matcher, true).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "r1");
    }

    #[test]
    fn test_fast_search_for_matching_releases_invalid_tag() {
        let (config, _tmpdir) = testing::seeded_cache();

        let matcher = Matcher::parse("tracktitle:x").unwrap();
        assert!(fast_search_for_matching_releases(&config, &matcher, true).is_err());

        let matcher = Matcher::parse("trackartist:x").unwrap();
        assert!(fast_search_for_matching_releases(&config, &matcher, true).is_err());

        // But allow artist tag
        let matcher = Matcher::parse("artist:x").unwrap();
        assert!(fast_search_for_matching_releases(&config, &matcher, true).is_ok());
    }

    #[test]
    fn test_filter_release_false_positives_with_read_cache() {
        let (config, _tmpdir) = testing::seeded_cache();

        let matcher = Matcher::parse("releaseartist:^Man").unwrap();
        let fsresults = fast_search_for_matching_releases(&config, &matcher, true).unwrap();
        assert_eq!(fsresults.len(), 2);
        eprintln!("fsresults: {:?}", fsresults.iter().map(|r| &r.id).collect::<Vec<_>>());
        let cacheresults = list_releases(&config, Some(fsresults.iter().map(|r| r.id.clone()).collect()), None, None).unwrap();
        eprintln!("cacheresults len: {}", cacheresults.len());
        assert_eq!(cacheresults.len(), 2);
        let filteredresults = filter_release_false_positives_using_read_cache(&matcher, cacheresults, true);
        assert!(filteredresults.is_empty());
    }

    #[test]
    fn test_filter_track_false_positives_with_read_cache() {
        let (config, _tmpdir) = testing::seeded_cache();

        let matcher = Matcher::parse("trackartist:^Man").unwrap();
        let fsresults = fast_search_for_matching_tracks(&config, &matcher).unwrap();
        assert_eq!(fsresults.len(), 3);
        let tracks = list_tracks(&config, Some(fsresults.iter().map(|r| r.id.clone()).collect())).unwrap();
        assert_eq!(tracks.len(), 3);
        let filteredresults = filter_track_false_positives_using_read_cache(&matcher, tracks);
        assert!(filteredresults.is_empty());
    }

    #[test]
    fn test_ignore_values() {
        let (config, _tmpdir) = testing::source_dir();

        let rule = parse_rule_with_ignore("tracktitle:rack", &["replace:lalala"], &["tracktitle:^Track 1$"]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.tracktitle, Some("Track 1".to_string()));
    }

    #[test]
    fn test_artist_matcher_on_trackartist_only() {
        let (config, _tmpdir) = testing::source_dir();
        
        let mut af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        af.trackartists.main = vec![Artist::new("BIGBANG & 2NE1")];
        af.releaseartists.main = vec![Artist::new("BIGBANG"), Artist::new("2NE1")];
        af.flush(&config, false).unwrap();
        update_cache(&config, false, false).unwrap();
        
        let rule = parse_rule("artist: & ", &["split: & "]);
        execute_metadata_rule(&config, &rule, false, false, 25).unwrap();
        let af = AudioTags::from_file(&config.music_source_dir.join("Test Release 1").join("01.m4a")).unwrap();
        assert_eq!(af.trackartists.main, vec![Artist::new("BIGBANG"), Artist::new("2NE1")]);
    }
}
