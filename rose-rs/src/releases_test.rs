#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{connect, CachedRelease, CachedTrack};
    use crate::config::Config;
    use crate::rule_parser::{Matcher, Pattern, Tag};
    use std::fs;
    use tempfile::TempDir;

    fn create_test_config() -> (Config, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = Config {
            music_source_dir: temp_dir.path().join("music").to_path_buf(),
            cache_dir: temp_dir.path().join("cache").to_path_buf(),
            // ... other config fields
        };
        fs::create_dir_all(&config.music_source_dir).unwrap();
        fs::create_dir_all(&config.cache_dir).unwrap();
        (config, temp_dir)
    }

    #[test]
    fn test_delete_release() {
        let (config, _temp) = create_test_config();
        // TODO: Create a test release and delete it
    }

    #[test]
    fn test_toggle_release_new() {
        let (config, _temp) = create_test_config();
        // TODO: Create a test release and toggle its new status
    }

    #[test]
    fn test_set_release_cover_art() {
        let (config, _temp) = create_test_config();
        // TODO: Test setting cover art
    }

    #[test]
    fn test_delete_release_cover_art() {
        let (config, _temp) = create_test_config();
        // TODO: Test deleting cover art
    }

    #[test]
    fn test_edit_release() {
        let (config, _temp) = create_test_config();
        // TODO: Test editing release metadata
    }

    #[test]
    fn test_edit_release_failure_and_resume() {
        let (config, _temp) = create_test_config();
        // TODO: Test edit failure and resume functionality
    }

    #[test]
    fn test_create_single_release() {
        let (config, _temp) = create_test_config();
        // TODO: Test creating a single from a track
    }

    #[test]
    fn test_create_single_release_with_trailing_space() {
        let (config, _temp) = create_test_config();
        // TODO: Test creating a single with trailing space in title
    }

    #[test]
    fn test_run_action_on_release() {
        let (config, _temp) = create_test_config();
        // TODO: Test running actions on a release
    }

    #[test]
    fn test_find_matching_releases() {
        let (config, _temp) = create_test_config();
        // TODO: Test finding releases that match a rule
    }
}