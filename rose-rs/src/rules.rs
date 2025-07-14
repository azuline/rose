// The rules module implements the Rules Engine, which provides performant substring tag querying and
// bulk metadata updating.
//
// The first part of this file implements the Rule Engine pipeline, which:
//
// 1. Fetches a superset of possible tracks from the Read Cache.
// 2. Filters out false positives via tags.
// 3. Executes actions to update metadata.
//
// The second part of this file provides performant release/track querying entirely from the read
// cache, which is used by other modules to provide release/track filtering capabilities.

use crate::audiotags::{AudioTags, RoseDate};
use crate::cache::{
    connect, cached_release_from_view, cached_track_from_view,
    CachedRelease, CachedTrack, StoredDataFile, STORED_DATA_FILE_REGEX,
};
use crate::common::{Artist, uniq};
use crate::config::Config;
use crate::error::{Result, RoseError, RoseExpectedError};
use crate::rule_parser::{
    Action, ActionBehavior, AddAction, DeleteAction, Matcher, Pattern, ReplaceAction, Rule, 
    SedAction, SplitAction, Tag,
};
use regex::Regex;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info};

#[derive(Debug)]
pub struct TrackTagNotAllowedError(pub String);

impl std::fmt::Display for TrackTagNotAllowedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Track tag not allowed: {}", self.0)
    }
}

impl std::error::Error for TrackTagNotAllowedError {}

#[derive(Debug)]
pub struct InvalidReplacementValueError(pub String);

impl std::fmt::Display for InvalidReplacementValueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid replacement value: {}", self.0)
    }
}

impl std::error::Error for InvalidReplacementValueError {}

// Python: def execute_stored_metadata_rules(
pub fn execute_stored_metadata_rules(
    config: &Config,
    dry_run: bool,
    confirm_yes: bool,
) -> Result<()> {
    for config_rule in &config.stored_metadata_rules {
        info!("Executing stored metadata rule {:?}", config_rule);
        
        // Parse the rule from config
        let matcher = Matcher::parse(&config_rule.matcher)
            .map_err(|e| RoseError::Generic(format!("Failed to parse matcher: {}", e)))?;
            
        let mut actions = Vec::new();
        for (i, action_str) in config_rule.actions.iter().enumerate() {
            let action = Action::parse(action_str, i + 1, Some(&matcher))
                .map_err(|e| RoseError::Generic(format!("Failed to parse action: {}", e)))?;
            actions.push(action);
        }
        
        let rule = Rule {
            matcher,
            actions,
        };
        
        execute_metadata_rule(config, &rule, dry_run, confirm_yes, 25)?;
    }
    Ok(())
}

// Python: def execute_metadata_rule(
pub fn execute_metadata_rule(
    config: &Config,
    rule: &Rule,
    dry_run: bool,
    confirm_yes: bool,
    enter_number_to_confirm_above_count: usize,
) -> Result<()> {
    // This function executes a metadata update rule. It runs in five parts:
    //
    // 1. Run a search query on our Full Text Search index. This is far more performant than the SQL
    //    LIKE operation; however, it is also less precise. It produces false positives, but should not
    //    produce false negatives. So we then run:
    // 2. Read the files returned from the search query and remove all false positives.
    // 3. We then run the actions on each valid matched file and store all the intended changes
    //    in-memory. No changes are written to disk.
    // 4. We then prompt the user to confirm the changes, assuming confirm_yes is True.
    // 5. We then flush the intended changes to disk.
    
    info!("Executing metadata rule: {:?}", rule);
    
    let fast_search_results = fast_search_for_matching_tracks(config, &rule.matcher)?;
    if fast_search_results.is_empty() {
        info!("No matching tracks found");
        return Ok(());
    }
    
    debug!("Fast search found {} potential matching tracks", fast_search_results.len());
    
    let matching_tracks = filter_track_false_positives_using_tags(
        config,
        &rule.matcher,
        &fast_search_results,
    )?;
    
    if matching_tracks.is_empty() {
        info!("No tracks remaining after filtering false positives");
        return Ok(());
    }
    
    info!("Matched {} tracks", matching_tracks.len());
    
    // Apply the actions and collect changes
    let changes = execute_metadata_actions(
        config,
        &rule.actions,
        &matching_tracks,
        dry_run,
    )?;
    
    if changes.is_empty() {
        info!("No changes to apply");
        return Ok(());
    }
    
    // TODO: Implement confirmation prompt if needed
    if !confirm_yes && changes.len() > enter_number_to_confirm_above_count {
        // Would prompt user here
        unimplemented!("User confirmation prompts not yet implemented");
    }
    
    if !dry_run {
        // TODO: Actually write changes to disk
        info!("Would write {} changes to disk", changes.len());
    } else {
        info!("Dry run: would have made {} changes", changes.len());
    }
    
    Ok(())
}

// Python: def fast_search_for_matching_tracks(
pub fn fast_search_for_matching_tracks(
    config: &Config,
    matcher: &Matcher,
) -> Result<Vec<(CachedTrack, CachedRelease)>> {
    let conn = connect(config)?;
    
    // Build the FTS query
    let fts_query = build_fts_query(matcher)?;
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }
    
    debug!("FTS query: {}", fts_query);
    
    // Execute the search
    let sql = r#"
        SELECT DISTINCT 
            tv.*, 
            rv.*
        FROM rules_engine_fts fts
        JOIN tracks_view tv ON tv.id = fts.rowid  
        JOIN releases_view rv ON rv.id = tv.release_id
        WHERE rules_engine_fts MATCH ?1
    "#;
    
    let mut stmt = conn.prepare(sql)?;
    let results = stmt.query_map(params![fts_query], |row| {
        // Need to parse both track and release from the joined view
        let release = cached_release_from_view(config, row, false)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let track = cached_track_from_view(config, row, release.clone(), false)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        Ok((track, release))
    })?;
    
    let mut tracks = Vec::new();
    for result in results {
        tracks.push(result?);
    }
    
    Ok(tracks)
}

// Build FTS query from matcher
fn build_fts_query(matcher: &Matcher) -> Result<String> {
    // In the actual implementation, Matcher has a single pattern and a list of tags
    let fts_pattern = pattern_to_fts_pattern(&matcher.pattern)?;
    if fts_pattern.is_empty() {
        return Ok(String::new());
    }
    
    let mut parts = Vec::new();
    for tag in &matcher.tags {
        let fts_column = tag_to_fts_column(tag)?;
        parts.push(format!("{}: {}", fts_column, fts_pattern));
    }
    
    Ok(parts.join(" OR "))
}

// Map tag names to FTS column names
fn tag_to_fts_column(tag: &Tag) -> Result<&'static str> {
    Ok(match tag {
        Tag::TrackTitle => "tracktitle",
        Tag::TrackNumber => "tracknumber",
        Tag::TrackTotal => "tracktotal",
        Tag::DiscNumber => "discnumber",
        Tag::DiscTotal => "disctotal",
        Tag::ReleaseTitle => "releasetitle",
        Tag::ReleaseType => "releasetype",
        Tag::ReleaseDate => "releasedate",
        Tag::OriginalDate => "originaldate",
        Tag::CompositionDate => "compositiondate",
        Tag::Edition => "edition",
        Tag::CatalogNumber => "catalognumber",
        Tag::Genre => "genre",
        Tag::SecondaryGenre => "secondarygenre",
        Tag::Descriptor => "descriptor",
        Tag::Label => "label",
        Tag::ReleaseArtistMain | Tag::ReleaseArtistGuest | Tag::ReleaseArtistRemixer |
        Tag::ReleaseArtistProducer | Tag::ReleaseArtistComposer | Tag::ReleaseArtistConductor |
        Tag::ReleaseArtistDjMixer => "releaseartist",
        Tag::TrackArtistMain | Tag::TrackArtistGuest | Tag::TrackArtistRemixer |
        Tag::TrackArtistProducer | Tag::TrackArtistComposer | Tag::TrackArtistConductor |
        Tag::TrackArtistDjMixer => "trackartist",
        Tag::New => "new",
    })
}

// Convert pattern to FTS query  
fn pattern_to_fts_pattern(pattern: &Pattern) -> Result<String> {
    // For FTS, we can only do basic substring matching
    // The strict_start and strict_end flags will need to be handled during filtering
    
    // Escape special FTS characters
    let escaped = pattern.needle
        .chars()
        .map(|c| match c {
            '"' | '\'' | '-' | '*' => format!("\\{}", c),
            _ => c.to_string(),
        })
        .collect::<String>();
    
    // FTS doesn't support anchors, so we'll do basic substring search
    Ok(format!("\"{}\"", escaped))
}

// Python: def filter_track_false_positives_using_tags(
pub fn filter_track_false_positives_using_tags(
    config: &Config,
    matcher: &Matcher,
    tracks: &[(CachedTrack, CachedRelease)],
) -> Result<Vec<(CachedTrack, CachedRelease)>> {
    let mut filtered = Vec::new();
    
    for (track, release) in tracks {
        // Read the actual tags from the file
        let tags = AudioTags::from_file(&track.source_path)?;
        
        // Check if any of the tags match the pattern
        let mut any_match = false;
        for tag in &matcher.tags {
            let values = get_tag_value(&tags, track, release, tag)?;
            if matches_pattern(&values, &matcher.pattern, tag)? {
                any_match = true;
                break;
            }
        }
        
        if any_match {
            filtered.push((track.clone(), release.clone()));
        }
    }
    
    Ok(filtered)
}

// Get the value of a tag from AudioTags or cached data
fn get_tag_value(
    tags: &AudioTags,
    track: &CachedTrack,
    release: &CachedRelease,
    tag: &Tag,
) -> Result<Vec<String>> {
    Ok(match tag {
        Tag::TrackTitle => vec![tags.tracktitle.clone().unwrap_or_default()],
        Tag::TrackNumber => vec![tags.tracknumber.clone().unwrap_or_default()],
        Tag::TrackTotal => vec![track.tracktotal.to_string()],
        Tag::DiscNumber => vec![tags.discnumber.clone().unwrap_or_default()],
        Tag::DiscTotal => vec![release.disctotal.to_string()],
        
        Tag::TrackArtistMain => tags.trackartists.main.iter().map(|a| a.name.clone()).collect(),
        Tag::TrackArtistGuest => tags.trackartists.guest.iter().map(|a| a.name.clone()).collect(),
        Tag::TrackArtistRemixer => tags.trackartists.remixer.iter().map(|a| a.name.clone()).collect(),
        Tag::TrackArtistProducer => tags.trackartists.producer.iter().map(|a| a.name.clone()).collect(),
        Tag::TrackArtistComposer => tags.trackartists.composer.iter().map(|a| a.name.clone()).collect(),
        Tag::TrackArtistConductor => tags.trackartists.conductor.iter().map(|a| a.name.clone()).collect(),
        Tag::TrackArtistDjMixer => tags.trackartists.djmixer.iter().map(|a| a.name.clone()).collect(),
        
        // Release tags from cached data
        Tag::ReleaseTitle => vec![release.releasetitle.clone()],
        Tag::ReleaseType => vec![release.releasetype.clone()],
        Tag::ReleaseDate => vec![release.releasedate.as_ref().map(|d| d.to_string()).unwrap_or_default()],
        Tag::OriginalDate => vec![release.originaldate.as_ref().map(|d| d.to_string()).unwrap_or_default()],
        Tag::CompositionDate => vec![release.compositiondate.as_ref().map(|d| d.to_string()).unwrap_or_default()],
        Tag::Edition => vec![release.edition.clone().unwrap_or_default()],
        Tag::CatalogNumber => vec![release.catalognumber.clone().unwrap_or_default()],
        Tag::Genre => release.genres.clone(),
        Tag::SecondaryGenre => release.secondary_genres.clone(),
        Tag::Descriptor => release.descriptors.clone(),
        Tag::Label => release.labels.clone(),
        
        Tag::ReleaseArtistMain => release.releaseartists.main.iter().map(|a| a.name.clone()).collect(),
        Tag::ReleaseArtistGuest => release.releaseartists.guest.iter().map(|a| a.name.clone()).collect(),
        Tag::ReleaseArtistRemixer => release.releaseartists.remixer.iter().map(|a| a.name.clone()).collect(),
        Tag::ReleaseArtistProducer => release.releaseartists.producer.iter().map(|a| a.name.clone()).collect(),
        Tag::ReleaseArtistComposer => release.releaseartists.composer.iter().map(|a| a.name.clone()).collect(),
        Tag::ReleaseArtistConductor => release.releaseartists.conductor.iter().map(|a| a.name.clone()).collect(),
        Tag::ReleaseArtistDjMixer => release.releaseartists.djmixer.iter().map(|a| a.name.clone()).collect(),
        
        Tag::New => vec![if release.new { "true" } else { "false" }.to_string()],
    })
}

// Python: def matches_pattern(
pub fn matches_pattern(values: &[String], pattern: &Pattern, tag: &Tag) -> Result<bool> {
    // For multi-value fields, ANY value matching means success
    for value in values {
        let value_to_check = if pattern.case_insensitive || matches!(tag, Tag::Genre | Tag::SecondaryGenre) {
            value.to_lowercase()
        } else {
            value.clone()
        };
        
        let needle = if pattern.case_insensitive || matches!(tag, Tag::Genre | Tag::SecondaryGenre) {
            pattern.needle.to_lowercase()
        } else {
            pattern.needle.clone()
        };
        
        let matches = if pattern.strict_start && pattern.strict_end {
            value_to_check == needle
        } else if pattern.strict_start {
            value_to_check.starts_with(&needle)
        } else if pattern.strict_end {
            value_to_check.ends_with(&needle)
        } else {
            value_to_check.contains(&needle)
        };
        
        if matches {
            return Ok(true);
        }
    }
    
    Ok(false)
}

// Python: def execute_metadata_actions(
pub fn execute_metadata_actions(
    config: &Config,
    actions: &[Action],
    tracks: &[(CachedTrack, CachedRelease)],
    dry_run: bool,
) -> Result<Vec<(CachedTrack, HashMap<String, Vec<String>>)>> {
    let mut changes = Vec::new();
    
    for (track, release) in tracks {
        let mut tags = AudioTags::from_file(&track.source_path)?;
        let mut modified = false;
        let mut changes_map = HashMap::new();
        
        // Apply each action
        for action in actions {
            let (action_modified, action_changes) = execute_single_action(
                config,
                action,
                &mut tags,
                track,
                release,
            )?;
            
            if action_modified {
                modified = true;
                for (tag, values) in action_changes {
                    changes_map.insert(tag, values);
                }
            }
        }
        
        if modified {
            if !dry_run {
                tags.flush(config)?;
            }
            changes.push((track.clone(), changes_map));
        }
    }
    
    Ok(changes)
}

// Python: def execute_single_action(
fn execute_single_action(
    config: &Config,
    action: &Action,
    tags: &mut AudioTags,
    track: &CachedTrack,
    release: &CachedRelease,
) -> Result<(bool, HashMap<String, Vec<String>>)> {
    let mut changes = HashMap::new();
    let modified = match &action.behavior {
        ActionBehavior::Replace(replace_action) => {
            execute_replace_action(replace_action, tags, track, release, &mut changes)?
        }
        ActionBehavior::Sed(sed_action) => {
            execute_sed_action(sed_action, tags, track, release, &mut changes)?
        }
        ActionBehavior::Split(split_action) => {
            execute_split_action(split_action, tags, track, release, &mut changes)?
        }
        ActionBehavior::Add(add_action) => {
            execute_add_action(add_action, tags, track, release, &mut changes)?
        }
        ActionBehavior::Delete(delete_action) => {
            execute_delete_action(delete_action, tags, track, release, &mut changes)?
        }
    };
    
    Ok((modified, changes))
}

// Execute a replace action
fn execute_replace_action(
    action: &ReplaceAction,
    tags: &mut AudioTags,
    track: &CachedTrack,
    release: &CachedRelease,
    changes: &mut HashMap<String, Vec<String>>,
) -> Result<bool> {
    // TODO: Implement replace action
    Ok(false)
}

// Execute a sed action
fn execute_sed_action(
    action: &SedAction,
    tags: &mut AudioTags,
    track: &CachedTrack,
    release: &CachedRelease,
    changes: &mut HashMap<String, Vec<String>>,
) -> Result<bool> {
    // TODO: Implement sed action
    Ok(false)
}

// Execute a split action
fn execute_split_action(
    action: &SplitAction,
    tags: &mut AudioTags,
    track: &CachedTrack,
    release: &CachedRelease,
    changes: &mut HashMap<String, Vec<String>>,
) -> Result<bool> {
    // TODO: Implement split action
    Ok(false)
}

// Execute an add action
fn execute_add_action(
    action: &AddAction,
    tags: &mut AudioTags,
    track: &CachedTrack,
    release: &CachedRelease,
    changes: &mut HashMap<String, Vec<String>>,
) -> Result<bool> {
    // TODO: Implement add action
    Ok(false)
}

// Execute a delete action
fn execute_delete_action(
    action: &DeleteAction,
    tags: &mut AudioTags,
    track: &CachedTrack,
    release: &CachedRelease,
    changes: &mut HashMap<String, Vec<String>>,
) -> Result<bool> {
    // TODO: Implement delete action
    Ok(false)
}

// Python: def fast_search_for_matching_releases(
pub fn fast_search_for_matching_releases(
    config: &Config,
    matcher: &Matcher,
) -> Result<Vec<CachedRelease>> {
    let conn = connect(config)?;
    
    // Build the FTS query
    let fts_query = build_fts_query(matcher)?;
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }
    
    debug!("FTS query for releases: {}", fts_query);
    
    // Execute the search - releases are found via their tracks in the FTS index
    let sql = r#"
        SELECT DISTINCT rv.*
        FROM rules_engine_fts fts
        JOIN tracks_view tv ON tv.id = fts.rowid  
        JOIN releases_view rv ON rv.id = tv.release_id
        WHERE rules_engine_fts MATCH ?1
    "#;
    
    let mut stmt = conn.prepare(sql)?;
    let results = stmt.query_map(params![fts_query], |row| {
        cached_release_from_view(config, row, false)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
    })?;
    
    let mut releases = Vec::new();
    for result in results {
        releases.push(result?);
    }
    
    Ok(releases)
}

// Python: def filter_release_false_positives_using_read_cache(
pub fn filter_release_false_positives_using_read_cache(
    config: &Config,
    matcher: &Matcher,
    releases: &[CachedRelease],
) -> Result<Vec<CachedRelease>> {
    let mut filtered = Vec::new();
    
    for release in releases {
        // Check if any of the tags match the pattern using cached data
        let mut any_match = false;
        for tag in &matcher.tags {
            let values = get_release_tag_value(release, tag)?;
            if matches_pattern(&values, &matcher.pattern, tag)? {
                any_match = true;
                break;
            }
        }
        
        if any_match {
            filtered.push(release.clone());
        }
    }
    
    Ok(filtered)
}

// Get the value of a tag from cached release data
fn get_release_tag_value(release: &CachedRelease, tag: &Tag) -> Result<Vec<String>> {
    Ok(match tag {
        Tag::ReleaseTitle => vec![release.releasetitle.clone()],
        Tag::ReleaseType => vec![release.releasetype.clone()],
        Tag::ReleaseDate => vec![release.releasedate.as_ref().map(|d| d.to_string()).unwrap_or_default()],
        Tag::OriginalDate => vec![release.originaldate.as_ref().map(|d| d.to_string()).unwrap_or_default()],
        Tag::CompositionDate => vec![release.compositiondate.as_ref().map(|d| d.to_string()).unwrap_or_default()],
        Tag::Edition => vec![release.edition.clone().unwrap_or_default()],
        Tag::CatalogNumber => vec![release.catalognumber.clone().unwrap_or_default()],
        Tag::Genre => release.genres.clone(),
        Tag::SecondaryGenre => release.secondary_genres.clone(),
        Tag::Descriptor => release.descriptors.clone(),
        Tag::Label => release.labels.clone(),
        Tag::ReleaseArtistMain => release.releaseartists.main.iter().map(|a| a.name.clone()).collect(),
        Tag::ReleaseArtistGuest => release.releaseartists.guest.iter().map(|a| a.name.clone()).collect(),
        Tag::ReleaseArtistRemixer => release.releaseartists.remixer.iter().map(|a| a.name.clone()).collect(),
        Tag::ReleaseArtistProducer => release.releaseartists.producer.iter().map(|a| a.name.clone()).collect(),
        Tag::ReleaseArtistComposer => release.releaseartists.composer.iter().map(|a| a.name.clone()).collect(),
        Tag::ReleaseArtistConductor => release.releaseartists.conductor.iter().map(|a| a.name.clone()).collect(),
        Tag::ReleaseArtistDjMixer => release.releaseartists.djmixer.iter().map(|a| a.name.clone()).collect(),
        Tag::New => vec![if release.new { "true" } else { "false" }.to_string()],
        Tag::DiscTotal => vec![release.disctotal.to_string()],
        _ => return Err(RoseError::Generic(format!("Tag {:?} not available for releases", tag))),
    })
}

// Python: def filter_track_false_positives_using_read_cache(
pub fn filter_track_false_positives_using_read_cache(
    config: &Config,
    matcher: &Matcher,
    tracks: &[CachedTrack],
) -> Result<Vec<CachedTrack>> {
    let mut result = Vec::new();
    
    for track in tracks {
        let mut matched = false;
        
        for tag in &matcher.tags {
            let values = match tag {
                Tag::TrackTitle => vec![track.tracktitle.clone()],
                Tag::TrackNumber => vec![track.tracknumber.clone()],
                Tag::DiscNumber => vec![track.discnumber.clone()],
                Tag::TrackArtistMain => track.trackartists.main.iter().map(|a| a.name.clone()).collect(),
                Tag::TrackArtistGuest => track.trackartists.guest.iter().map(|a| a.name.clone()).collect(),
                Tag::TrackArtistRemixer => track.trackartists.remixer.iter().map(|a| a.name.clone()).collect(),
                Tag::TrackArtistProducer => track.trackartists.producer.iter().map(|a| a.name.clone()).collect(),
                Tag::TrackArtistComposer => track.trackartists.composer.iter().map(|a| a.name.clone()).collect(),
                Tag::TrackArtistConductor => track.trackartists.conductor.iter().map(|a| a.name.clone()).collect(),
                Tag::TrackArtistDjMixer => track.trackartists.djmixer.iter().map(|a| a.name.clone()).collect(),
                // Release tags
                Tag::ReleaseTitle => vec![track.release.releasetitle.clone()],
                Tag::ReleaseType => vec![track.release.releasetype.clone()],
                Tag::ReleaseDate => track.release.releasedate.as_ref().map(|d| vec![d.to_string()]).unwrap_or_default(),
                Tag::OriginalDate => track.release.originaldate.as_ref().map(|d| vec![d.to_string()]).unwrap_or_default(),
                Tag::CompositionDate => track.release.compositiondate.as_ref().map(|d| vec![d.to_string()]).unwrap_or_default(),
                Tag::Edition => track.release.edition.as_ref().map(|e| vec![e.clone()]).unwrap_or_default(),
                Tag::CatalogNumber => track.release.catalognumber.as_ref().map(|c| vec![c.clone()]).unwrap_or_default(),
                Tag::Genre => track.release.genres.clone(),
                Tag::SecondaryGenre => track.release.secondary_genres.clone(),
                Tag::Descriptor => track.release.descriptors.clone(),
                Tag::Label => track.release.labels.clone(),
                Tag::ReleaseArtistMain => track.release.releaseartists.main.iter().map(|a| a.name.clone()).collect(),
                Tag::ReleaseArtistGuest => track.release.releaseartists.guest.iter().map(|a| a.name.clone()).collect(),
                Tag::ReleaseArtistRemixer => track.release.releaseartists.remixer.iter().map(|a| a.name.clone()).collect(),
                Tag::ReleaseArtistProducer => track.release.releaseartists.producer.iter().map(|a| a.name.clone()).collect(),
                Tag::ReleaseArtistComposer => track.release.releaseartists.composer.iter().map(|a| a.name.clone()).collect(),
                Tag::ReleaseArtistConductor => track.release.releaseartists.conductor.iter().map(|a| a.name.clone()).collect(),
                Tag::ReleaseArtistDjMixer => track.release.releaseartists.djmixer.iter().map(|a| a.name.clone()).collect(),
                Tag::New => vec![if track.release.new { "true" } else { "false" }.to_string()],
                Tag::DiscTotal => vec![track.release.disctotal.to_string()],
                Tag::TrackTotal => vec![track.tracktotal.to_string()],
            };
            
            if matches_pattern(&values, &matcher.pattern, tag)? {
                matched = true;
                break;
            }
        }
        
        if matched {
            result.push(track.clone());
        }
    }
    
    Ok(result)
}