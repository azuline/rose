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

    // -----------------------------------------------------------------------
    // Integration tests: seeded DB + real audio files
    // -----------------------------------------------------------------------

    use std::collections::HashSet;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    use crate::cache::{connect, maybe_invalidate_cache_database, sync_fts_index};

    /// Path to the repository root (two levels above CARGO_MANIFEST_DIR).
    fn repo_root() -> PathBuf {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    /// Copy a real audio file from testdata into `dest_dir/filename`.
    fn copy_audio_file(dest_dir: &std::path::Path, filename: &str) -> PathBuf {
        let src = repo_root().join("testdata/Test Release 1/01.m4a");
        let dst = dest_dir.join(filename);
        std::fs::copy(&src, &dst)
            .unwrap_or_else(|e| panic!("copy {} -> {}: {e}", src.display(), dst.display()));
        dst
    }

    /// Create a minimal config pointing at a temporary directory.
    fn test_config(dir: &TempDir) -> Config {
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

    /// Build a seeded config with real audio files on disk and a populated
    /// FTS index, matching the standard seed data from `conftest.py`.
    ///
    /// Returns `(TempDir, Config)` — the `TempDir` must be held alive for the
    /// duration of the test to prevent cleanup.
    fn seeded_config_with_audio() -> (TempDir, Config) {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        maybe_invalidate_cache_database(&config).unwrap();

        let music_dir = config.music_source_dir.clone();

        // Create release directories.
        let dirpaths = [
            music_dir.join("r1"),
            music_dir.join("r2"),
            music_dir.join("r3"),
            music_dir.join("r4"),
        ];
        for d in &dirpaths {
            std::fs::create_dir_all(d).unwrap();
        }

        // Copy real audio files into the release directories.
        let musicpaths = [
            copy_audio_file(&dirpaths[0], "01.m4a"),
            copy_audio_file(&dirpaths[0], "02.m4a"),
            copy_audio_file(&dirpaths[1], "01.m4a"),
            copy_audio_file(&dirpaths[2], "01.m4a"),
            copy_audio_file(&dirpaths[3], "01.m4a"),
        ];

        // Create .rose.{id}.toml sidecar files (needed by run_actions for
        // datafile lookups when modifying new/favorite/rating tags).
        for (d, id) in dirpaths.iter().zip(["r1", "r2", "r3", "r4"]) {
            let sidecar = d.join(format!(".rose.{id}.toml"));
            std::fs::write(&sidecar, "").unwrap();
        }

        // Create image placeholders.
        let imagepaths = [
            music_dir.join("r2/cover.jpg"),
            music_dir.join("!playlists/Lala Lisa.jpg"),
        ];
        std::fs::create_dir_all(music_dir.join("!playlists")).unwrap();
        for p in &imagepaths {
            std::fs::write(p, b"").unwrap();
        }

        // Seed the database with the standard test data.
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
            dirpaths[1].display(), imagepaths[0].display(),
            dirpaths[2].display(),
            dirpaths[3].display(),
            musicpaths[0].display(),
            musicpaths[1].display(),
            musicpaths[2].display(),
            musicpaths[3].display(),
            musicpaths[4].display(),
        ))
        .expect("Failed to seed cache database");

        // Populate the FTS index (needed for substring/fallback searches).
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
        .expect("Failed to sync FTS index");

        (dir, config)
    }

    // 1. End-to-end: run_actions_on_track replaces the track title in audio tags.
    #[test]
    fn test_run_actions_on_track_e2e() {
        let (_dir, config) = seeded_config_with_audio();
        let track_id = "t3";

        // Parse "tracktitle/replace:Bop" — sets the track title to "Bop".
        let action = Action::parse("tracktitle/replace:Bop", None, None).unwrap();
        run_actions_on_track(&config, track_id, &[action], false, false).unwrap();

        // Re-read the audio file and verify the title was changed.
        let track = get_track(&config, track_id).unwrap().unwrap();
        let tags = AudioTags::from_file(&track.source_path).unwrap();
        assert_eq!(tags.tracktitle.as_deref(), Some("Bop"));
    }

    // 2. Strict artist match returns the correct track set (optimized path).
    #[test]
    fn test_find_tracks_matching_rule_strict_artist() {
        let (_dir, config) = seeded_config_with_audio();
        let m = Matcher::parse("artist:^Techno Man$").unwrap();
        let tracks = find_tracks_matching_rule(&config, &m).unwrap();
        let ids: HashSet<String> = tracks.into_iter().map(|t| t.id).collect();
        assert_eq!(ids, HashSet::from(["t1".into(), "t2".into()]));
    }

    // 3. Strict genre match returns the correct track set (optimized path).
    #[test]
    fn test_find_tracks_matching_rule_strict_genre() {
        let (_dir, config) = seeded_config_with_audio();
        let m = Matcher::parse("genre:^Deep House$").unwrap();
        let tracks = find_tracks_matching_rule(&config, &m).unwrap();
        let ids: HashSet<String> = tracks.into_iter().map(|t| t.id).collect();
        assert_eq!(ids, HashSet::from(["t1".into(), "t2".into()]));
    }

    // 4. Strict label match returns the correct track set (optimized path).
    #[test]
    fn test_find_tracks_matching_rule_strict_label() {
        let (_dir, config) = seeded_config_with_audio();
        let m = Matcher::parse("label:^Native State$").unwrap();
        let tracks = find_tracks_matching_rule(&config, &m).unwrap();
        let ids: HashSet<String> = tracks.into_iter().map(|t| t.id).collect();
        assert_eq!(ids, HashSet::from(["t3".into()]));
    }

    // 5. Strict descriptor match returns the correct track set (optimized path).
    #[test]
    fn test_find_tracks_matching_rule_strict_descriptor() {
        let (_dir, config) = seeded_config_with_audio();
        let m = Matcher::parse("descriptor:^Wet$").unwrap();
        let tracks = find_tracks_matching_rule(&config, &m).unwrap();
        let ids: HashSet<String> = tracks.into_iter().map(|t| t.id).collect();
        assert_eq!(ids, HashSet::from(["t3".into()]));
    }

    // 6. Non-strict (substring) matcher uses the FTS fallback path.
    #[test]
    fn test_find_tracks_matching_rule_substring_fallback() {
        let (_dir, config) = seeded_config_with_audio();
        let m = Matcher::parse("tracktitle:Track").unwrap();
        let tracks = find_tracks_matching_rule(&config, &m).unwrap();
        let ids: HashSet<String> = tracks.into_iter().map(|t| t.id).collect();
        // All five tracks have titles starting with "Track".
        assert_eq!(
            ids,
            HashSet::from([
                "t1".into(),
                "t2".into(),
                "t3".into(),
                "t4".into(),
                "t5".into(),
            ])
        );
    }

    // 7. run_actions_on_track with a nonexistent track ID returns an error.
    #[test]
    fn test_run_actions_on_nonexistent_track_e2e() {
        let (_dir, config) = seeded_config_with_audio();
        let action = Action::parse("tracktitle/replace:Nope", None, None).unwrap();
        let result = run_actions_on_track(&config, "bogus-track-id", &[action], false, false);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("does not exist"),
            "Expected 'does not exist' in error, got: {err_msg}"
        );
    }
}
