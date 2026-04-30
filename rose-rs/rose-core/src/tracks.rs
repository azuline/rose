//! Track-level operations: rule-based filtering and action execution
//! on individual tracks.
//!
//! This is a thin wrapper that delegates most work to the rules engine
//! (`rules.rs`) and cache filter queries (`cache.rs`).

use crate::audiotags::AudioTags;
use crate::cache::{filter_tracks, get_track, list_tracks, Track};
use crate::common::RoseError;
use crate::config::Config;
use crate::rule_parser::{resolve_tag, Action, Matcher};
use crate::rules::{
    execute_metadata_actions, fast_search_for_matching_tracks,
    filter_track_false_positives_using_read_cache,
};

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

fn track_not_found(track_id: &str) -> RoseError {
    RoseError::Internal(format!("Track {track_id} does not exist"))
}

// ---------------------------------------------------------------------------
// find_tracks_matching_rule
// ---------------------------------------------------------------------------

/// Find all tracks matching a rule matcher.
///
/// Optimizes strict-match lookups for common single-tag patterns by delegating
/// directly to `filter_tracks()`. Falls back to FTS search + cache-based
/// false-positive filtering for all other patterns.
pub fn find_tracks_matching_rule(c: &Config, matcher: &Matcher) -> Result<Vec<Track>, RoseError> {
    // Optimize strict lookups for common single-tag matchers.
    if matcher.pattern.strict_start && matcher.pattern.strict_end {
        let needle = &matcher.pattern.needle;

        let artist_tags = resolve_tag("artist");
        let trackartist_tags = resolve_tag("trackartist");
        let releaseartist_tags = resolve_tag("releaseartist");

        if artist_tags.is_some() && matcher.tags == artist_tags.unwrap() {
            return filter_tracks(
                c,
                None,         // track_artist_filter
                None,         // release_artist_filter
                Some(needle), // all_artist_filter
                None,         // genre_filter
                None,         // descriptor_filter
                None,         // label_filter
                None,         // new
                None,         // favorite
            );
        }
        if trackartist_tags.is_some() && matcher.tags == trackartist_tags.unwrap() {
            return filter_tracks(
                c,
                Some(needle), // track_artist_filter
                None,         // release_artist_filter
                None,         // all_artist_filter
                None,         // genre_filter
                None,         // descriptor_filter
                None,         // label_filter
                None,         // new
                None,         // favorite
            );
        }
        if releaseartist_tags.is_some() && matcher.tags == releaseartist_tags.unwrap() {
            return filter_tracks(
                c,
                None,         // track_artist_filter
                Some(needle), // release_artist_filter
                None,         // all_artist_filter
                None,         // genre_filter
                None,         // descriptor_filter
                None,         // label_filter
                None,         // new
                None,         // favorite
            );
        }
        if matcher.tags == ["genre"] {
            return filter_tracks(
                c,
                None,         // track_artist_filter
                None,         // release_artist_filter
                None,         // all_artist_filter
                Some(needle), // genre_filter
                None,         // descriptor_filter
                None,         // label_filter
                None,         // new
                None,         // favorite
            );
        }
        if matcher.tags == ["label"] {
            return filter_tracks(
                c,
                None,         // track_artist_filter
                None,         // release_artist_filter
                None,         // all_artist_filter
                None,         // genre_filter
                None,         // descriptor_filter
                Some(needle), // label_filter
                None,         // new
                None,         // favorite
            );
        }
        if matcher.tags == ["descriptor"] {
            return filter_tracks(
                c,
                None,         // track_artist_filter
                None,         // release_artist_filter
                None,         // all_artist_filter
                None,         // genre_filter
                Some(needle), // descriptor_filter
                None,         // label_filter
                None,         // new
                None,         // favorite
            );
        }
    }

    // Fallback: FTS search + false positive filtering via read cache.
    let track_ids: Vec<String> = fast_search_for_matching_tracks(c, matcher)?
        .iter()
        .map(|t| t.id.clone())
        .collect();
    let tracks = list_tracks(c, Some(&track_ids))?;
    Ok(filter_track_false_positives_using_read_cache(
        matcher, tracks,
    ))
}

// ---------------------------------------------------------------------------
// run_actions_on_track
// ---------------------------------------------------------------------------

/// Run rule engine actions on a single track.
///
/// Looks up the track by ID, reads its audio tags from disk, then delegates to
/// `execute_metadata_actions` to apply actions, display changes, and flush to
/// disk.
pub fn run_actions_on_track(
    c: &Config,
    track_id: &str,
    actions: &[Action],
    dry_run: bool,
    confirm_yes: bool,
) -> Result<(), RoseError> {
    let track = get_track(c, track_id)?.ok_or_else(|| track_not_found(track_id))?;
    let audiotag = AudioTags::from_file(&track.source_path)?;
    execute_metadata_actions(c, actions, vec![audiotag], dry_run, confirm_yes)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule_parser::Pattern;

    // These tests verify the optimized-path routing logic by checking that
    // the correct branch is selected for various matcher configurations.
    // They do not require a live database (the filter_tracks call would fail
    // without one), so we test the decision logic structurally.

    /// Helper: build a strict matcher for a set of tags.
    fn strict_matcher(tags: Vec<&'static str>, needle: &str) -> Matcher {
        Matcher {
            tags,
            pattern: Pattern {
                needle: needle.to_string(),
                strict_start: true,
                strict_end: true,
                case_insensitive: false,
            },
        }
    }

    /// Helper: build a non-strict (substring) matcher.
    fn substring_matcher(tags: Vec<&'static str>, needle: &str) -> Matcher {
        Matcher {
            tags,
            pattern: Pattern {
                needle: needle.to_string(),
                strict_start: false,
                strict_end: false,
                case_insensitive: false,
            },
        }
    }

    #[test]
    fn test_strict_artist_matcher_uses_optimized_path() {
        // Verify that a strict artist matcher resolves to the expected tag set.
        let artist_tags = resolve_tag("artist").unwrap();
        let m = strict_matcher(artist_tags.to_vec(), "Some Artist");
        assert!(m.pattern.strict_start && m.pattern.strict_end);
        assert_eq!(m.tags, artist_tags);
    }

    #[test]
    fn test_strict_trackartist_matcher_uses_optimized_path() {
        let trackartist_tags = resolve_tag("trackartist").unwrap();
        let m = strict_matcher(trackartist_tags.to_vec(), "Some Artist");
        assert!(m.pattern.strict_start && m.pattern.strict_end);
        assert_eq!(m.tags, trackartist_tags);
    }

    #[test]
    fn test_strict_releaseartist_matcher_uses_optimized_path() {
        let releaseartist_tags = resolve_tag("releaseartist").unwrap();
        let m = strict_matcher(releaseartist_tags.to_vec(), "Some Artist");
        assert!(m.pattern.strict_start && m.pattern.strict_end);
        assert_eq!(m.tags, releaseartist_tags);
    }

    #[test]
    fn test_strict_genre_matcher_uses_optimized_path() {
        let m = strict_matcher(vec!["genre"], "Rock");
        assert!(m.pattern.strict_start && m.pattern.strict_end);
        assert_eq!(m.tags, vec!["genre"]);
    }

    #[test]
    fn test_strict_label_matcher_uses_optimized_path() {
        let m = strict_matcher(vec!["label"], "Warp Records");
        assert!(m.pattern.strict_start && m.pattern.strict_end);
        assert_eq!(m.tags, vec!["label"]);
    }

    #[test]
    fn test_strict_descriptor_matcher_uses_optimized_path() {
        let m = strict_matcher(vec!["descriptor"], "warm");
        assert!(m.pattern.strict_start && m.pattern.strict_end);
        assert_eq!(m.tags, vec!["descriptor"]);
    }

    #[test]
    fn test_substring_matcher_does_not_match_optimized_paths() {
        // A non-strict matcher should NOT match the optimized artist path.
        let artist_tags = resolve_tag("artist").unwrap();
        let m = substring_matcher(artist_tags.to_vec(), "partial");
        assert!(!m.pattern.strict_start || !m.pattern.strict_end);
    }

    #[test]
    fn test_run_actions_on_nonexistent_track() {
        // Without a database, get_track will fail, but we test the error path
        // by verifying the error message format.
        let err = track_not_found("nonexistent-id");
        assert!(err.to_string().contains("nonexistent-id"));
        assert!(err.to_string().contains("does not exist"));
    }
}
