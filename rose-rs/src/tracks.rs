// The tracks module provides functions for interacting with individual tracks.

use crate::audiotags::AudioTags;
use crate::cache::{connect, CachedTrack};
use crate::config::Config;
use crate::error::{Result, RoseError, RoseExpectedError};
use crate::rule_parser::{Action, Matcher, Tag};
use crate::rules::{
    execute_metadata_actions, fast_search_for_matching_tracks,
    filter_track_false_positives_using_read_cache,
};
use rusqlite::OptionalExtension;
use tracing::{debug, info};

#[derive(Debug)]
pub struct TrackDoesNotExistError(pub String);

impl std::fmt::Display for TrackDoesNotExistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Track does not exist: {}", self.0)
    }
}

impl std::error::Error for TrackDoesNotExistError {}

// Python: def find_tracks_matching_rule(c: Config, matcher: Matcher) -> list[Track]:
pub fn find_tracks_matching_rule(
    config: &Config,
    matcher: &Matcher,
) -> Result<Vec<CachedTrack>> {
    // Use the rules engine to find matching tracks
    let track_pairs = fast_search_for_matching_tracks(config, matcher)?;
    let tracks: Vec<CachedTrack> = track_pairs.into_iter().map(|(t, _)| t).collect();
    let filtered = filter_track_false_positives_using_read_cache(config, matcher, &tracks)?;
    
    Ok(filtered)
}

// Python: def run_actions_on_track(
pub fn run_actions_on_track(
    config: &Config,
    track_id: &str,
    actions: &[Action],
) -> Result<()> {
    let track = get_track(config, track_id)?
        .ok_or_else(|| RoseError::Expected(RoseExpectedError::Generic(
            format!("Track {} does not exist", track_id)
        )))?;
    
    // Execute the actions on just this track
    execute_metadata_actions(config, actions, &[(track.clone(), track.release.clone())], false)?;
    
    Ok(())
}

// Helper function to get a track (similar to the one in releases.rs)
pub fn get_track(config: &Config, track_id: &str) -> Result<Option<CachedTrack>> {
    let conn = connect(config)?;
    let mut stmt = conn.prepare(
        "SELECT tv.*, rv.*
         FROM tracks_view tv
         JOIN releases_view rv ON rv.id = tv.release_id
         WHERE tv.id = ?1"
    )?;
    
    let track = stmt.query_row([track_id], |row| {
        let release = crate::cache::cached_release_from_view(config, row, true)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let track = crate::cache::cached_track_from_view(config, row, release, true)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        Ok(track)
    }).optional()?;
    
    Ok(track)
}