// Python: """
// Python: The releases module encapsulates all mutations that can occur on release and track entities.
// Python: """
//!
//! The releases module encapsulates all mutations that can occur on release and track entities.
//!

// Python: from __future__ import annotations
// Python:
// Python: import logging
// Python:
// Python: from rose.audiotags import AudioTags
// Python: from rose.cache import (
// Python:     Track,
// Python:     filter_tracks,
// Python:     get_track,
// Python:     list_tracks,
// Python: )
// Python: from rose.common import RoseExpectedError
// Python: from rose.config import Config
// Python: from rose.rule_parser import ALL_TAGS, Action, Matcher
// Python: from rose.rules import (
// Python:     execute_metadata_actions,
// Python:     fast_search_for_matching_tracks,
// Python:     filter_track_false_positives_using_read_cache,
// Python: )
// Python:
// Python: logger = logging.getLogger(__name__)

use tracing::debug;

use crate::audiotags::AudioTags;
use crate::cache::{filter_tracks, get_track, list_tracks, Track};
use crate::config::Config;
use crate::errors::{Result, RoseExpectedError};
use crate::rule_parser::{Action, ExpandableTag, Matcher, Tag, ALL_TAGS};
use crate::rules::{execute_metadata_actions, fast_search_for_matching_tracks, filter_track_false_positives_using_read_cache};

// Python:
// Python:
// Python: class TrackDoesNotExistError(RoseExpectedError):
// Python:     pass

// TrackDoesNotExistError is already defined in errors.rs as RoseExpectedError::TrackDoesNotExist

// Python:
// Python:
// Python: def find_tracks_matching_rule(c: Config, matcher: Matcher) -> list[Track]:
// Python:     # Implement optimizations for common lookups. Only applies to strict lookups.
// Python:     # TODO: Morning
// Python:     if matcher.pattern.strict_start and matcher.pattern.strict_end:
// Python:         if matcher.tags == ALL_TAGS["artist"]:
// Python:             return filter_tracks(c, all_artist_filter=matcher.pattern.needle)
// Python:         if matcher.tags == ALL_TAGS["trackartist"]:
// Python:             return filter_tracks(c, track_artist_filter=matcher.pattern.needle)
// Python:         if matcher.tags == ALL_TAGS["releaseartist"]:
// Python:             return filter_tracks(c, release_artist_filter=matcher.pattern.needle)
// Python:         if matcher.tags == ["genre"]:
// Python:             return filter_tracks(c, genre_filter=matcher.pattern.needle)
// Python:         if matcher.tags == ["label"]:
// Python:             return filter_tracks(c, label_filter=matcher.pattern.needle)
// Python:         if matcher.tags == ["descriptor"]:
// Python:             return filter_tracks(c, descriptor_filter=matcher.pattern.needle)
// Python:
// Python:     track_ids = [t.id for t in fast_search_for_matching_tracks(c, matcher)]
// Python:     tracks = list_tracks(c, track_ids)
// Python:     return filter_track_false_positives_using_read_cache(matcher, tracks)

/// Find tracks matching a given rule matcher
pub fn find_tracks_matching_rule(c: &Config, matcher: &Matcher) -> Result<Vec<Track>> {
    // Implement optimizations for common lookups. Only applies to strict lookups.
    // TODO: Morning
    if matcher.pattern.strict_start && matcher.pattern.strict_end {
        if matcher.tags == ALL_TAGS.get(&ExpandableTag::Artist).cloned().unwrap_or_default() {
            return filter_tracks(c, None, None, Some(&matcher.pattern.needle), None, None, None, None);
        }
        if matcher.tags == ALL_TAGS.get(&ExpandableTag::TrackArtist).cloned().unwrap_or_default() {
            return filter_tracks(c, Some(&matcher.pattern.needle), None, None, None, None, None, None);
        }
        if matcher.tags == ALL_TAGS.get(&ExpandableTag::ReleaseArtist).cloned().unwrap_or_default() {
            return filter_tracks(c, None, Some(&matcher.pattern.needle), None, None, None, None, None);
        }
        if matcher.tags == vec![Tag::Genre] {
            return filter_tracks(c, None, None, None, Some(&matcher.pattern.needle), None, None, None);
        }
        if matcher.tags == vec![Tag::Label] {
            return filter_tracks(c, None, None, None, None, None, Some(&matcher.pattern.needle), None);
        }
        if matcher.tags == vec![Tag::Descriptor] {
            return filter_tracks(c, None, None, None, None, Some(&matcher.pattern.needle), None, None);
        }
    }

    let search_results = fast_search_for_matching_tracks(c, matcher)?;
    let track_ids: Vec<String> = search_results.into_iter().map(|t| t.id).collect();
    let tracks = list_tracks(c, Some(track_ids))?;
    Ok(filter_track_false_positives_using_read_cache(matcher, tracks))
}

// Python:
// Python:
// Python: def run_actions_on_track(
// Python:     c: Config,
// Python:     track_id: str,
// Python:     actions: list[Action],
// Python:     *,
// Python:     dry_run: bool = False,
// Python:     confirm_yes: bool = False,
// Python: ) -> None:
// Python:     """Run rule engine actions on a release."""
// Python:     track = get_track(c, track_id)
// Python:     if track is None:
// Python:         raise TrackDoesNotExistError(f"Track {track_id} does not exist")
// Python:     audiotag = AudioTags.from_file(track.source_path)
// Python:     execute_metadata_actions(c, actions, [audiotag], dry_run=dry_run, confirm_yes=confirm_yes)

/// Run rule engine actions on a track
pub fn run_actions_on_track(c: &Config, track_id: &str, actions: &[Action], dry_run: bool, confirm_yes: bool) -> Result<()> {
    debug!("running actions on track {}", track_id);

    let track = get_track(c, track_id)?.ok_or_else(|| RoseExpectedError::TrackDoesNotExist { id: track_id.to_string() })?;

    let audiotag = AudioTags::from_file(&track.source_path)?;
    execute_metadata_actions(c, actions, vec![audiotag], dry_run, confirm_yes, 15)?;

    Ok(())
}

// Python: # TESTS
// Python:
// Python: from pathlib import Path
// Python:
// Python: import pytest
// Python:
// Python: from rose.audiotags import AudioTags
// Python: from rose.config import Config
// Python: from rose.rule_parser import Action, Matcher
// Python: from rose.tracks import find_tracks_matching_rule, run_actions_on_track
// Python:
// Python:
// Python: def test_run_action_on_track(config: Config, source_dir: Path) -> None:
// Python:     action = Action.parse("tracktitle/replace:Bop")
// Python:     af = AudioTags.from_file(source_dir / "Test Release 2" / "01.m4a")
// Python:     assert af.id is not None
// Python:     run_actions_on_track(config, af.id, [action])
// Python:     af = AudioTags.from_file(source_dir / "Test Release 2" / "01.m4a")
// Python:     assert af.tracktitle == "Bop"
// Python:
// Python:
// Python: @pytest.mark.usefixtures("seeded_cache")
// Python: def test_find_matching_tracks(config: Config) -> None:
// Python:     results = find_tracks_matching_rule(config, Matcher.parse("releasetitle:Release 2"))
// Python:     assert {r.id for r in results} == {"t3"}
// Python:     results = find_tracks_matching_rule(config, Matcher.parse("tracktitle:Track 2"))
// Python:     assert {r.id for r in results} == {"t2"}
// Python:     results = find_tracks_matching_rule(config, Matcher.parse("artist:^Techno Man$"))
// Python:     assert {r.id for r in results} == {"t1", "t2"}
// Python:     results = find_tracks_matching_rule(config, Matcher.parse("artist:Techno Man"))
// Python:     assert {r.id for r in results} == {"t1", "t2"}
// Python:     results = find_tracks_matching_rule(config, Matcher.parse("genre:^Deep House$"))
// Python:     assert {r.id for r in results} == {"t1", "t2"}
// Python:     results = find_tracks_matching_rule(config, Matcher.parse("genre:Deep House"))
// Python:     assert {r.id for r in results} == {"t1", "t2"}
// Python:     results = find_tracks_matching_rule(config, Matcher.parse("descriptor:^Wet$"))
// Python:     assert {r.id for r in results} == {"t3"}
// Python:     results = find_tracks_matching_rule(config, Matcher.parse("descriptor:Wet"))
// Python:     assert {r.id for r in results} == {"t3"}
// Python:     results = find_tracks_matching_rule(config, Matcher.parse("label:^Native State$"))
// Python:     assert {r.id for r in results} == {"t3"}
// Python:     results = find_tracks_matching_rule(config, Matcher.parse("label:Native State"))
// Python:     assert {r.id for r in results} == {"t3"}
// Python:

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;
    use std::collections::HashSet;

    #[test]
    fn test_run_action_on_track() {
        let (config, _temp_dir) = testing::seeded_cache();

        // Get track t3 from the database which should have ID set
        let track = get_track(&config, "t3").unwrap().unwrap();

        let action = Action::parse("tracktitle/replace:Bop", None, None).unwrap();

        run_actions_on_track(&config, &track.id, &[action], false, false).unwrap();

        // Read the file to verify the change
        let af = AudioTags::from_file(&track.source_path).unwrap();
        assert_eq!(af.tracktitle, Some("Bop".to_string()));
    }

    #[test]
    fn test_find_matching_tracks() {
        let (config, _temp_dir) = testing::seeded_cache();

        let results = find_tracks_matching_rule(&config, &Matcher::parse("releasetitle:Release 2").unwrap()).unwrap();
        let ids: HashSet<String> = results.iter().map(|r| r.id.clone()).collect();
        assert_eq!(ids, HashSet::from(["t3".to_string()]));

        let results = find_tracks_matching_rule(&config, &Matcher::parse("tracktitle:Track 2").unwrap()).unwrap();
        let ids: HashSet<String> = results.iter().map(|r| r.id.clone()).collect();
        assert_eq!(ids, HashSet::from(["t2".to_string()]));

        let results = find_tracks_matching_rule(&config, &Matcher::parse("artist:^Techno Man$").unwrap()).unwrap();
        let ids: HashSet<String> = results.iter().map(|r| r.id.clone()).collect();
        assert_eq!(ids, HashSet::from(["t1".to_string(), "t2".to_string()]));

        let results = find_tracks_matching_rule(&config, &Matcher::parse("artist:Techno Man").unwrap()).unwrap();
        let ids: HashSet<String> = results.iter().map(|r| r.id.clone()).collect();
        assert_eq!(ids, HashSet::from(["t1".to_string(), "t2".to_string()]));

        let results = find_tracks_matching_rule(&config, &Matcher::parse("genre:^Deep House$").unwrap()).unwrap();
        let ids: HashSet<String> = results.iter().map(|r| r.id.clone()).collect();
        assert_eq!(ids, HashSet::from(["t1".to_string(), "t2".to_string()]));

        let results = find_tracks_matching_rule(&config, &Matcher::parse("genre:Deep House").unwrap()).unwrap();
        let ids: HashSet<String> = results.iter().map(|r| r.id.clone()).collect();
        assert_eq!(ids, HashSet::from(["t1".to_string(), "t2".to_string()]));

        let results = find_tracks_matching_rule(&config, &Matcher::parse("descriptor:^Wet$").unwrap()).unwrap();
        let ids: HashSet<String> = results.iter().map(|r| r.id.clone()).collect();
        assert_eq!(ids, HashSet::from(["t3".to_string()]));

        let results = find_tracks_matching_rule(&config, &Matcher::parse("descriptor:Wet").unwrap()).unwrap();
        let ids: HashSet<String> = results.iter().map(|r| r.id.clone()).collect();
        assert_eq!(ids, HashSet::from(["t3".to_string()]));

        let results = find_tracks_matching_rule(&config, &Matcher::parse("label:^Native State$").unwrap()).unwrap();
        let ids: HashSet<String> = results.iter().map(|r| r.id.clone()).collect();
        assert_eq!(ids, HashSet::from(["t3".to_string()]));

        let results = find_tracks_matching_rule(&config, &Matcher::parse("label:Native State").unwrap()).unwrap();
        let ids: HashSet<String> = results.iter().map(|r| r.id.clone()).collect();
        assert_eq!(ids, HashSet::from(["t3".to_string()]));
    }
}
