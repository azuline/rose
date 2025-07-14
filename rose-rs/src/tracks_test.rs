#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{connect, CachedTrack};
    use crate::config::Config;
    use crate::rule_parser::{Action, ActionBehavior, Matcher, Pattern, Tag};
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
    fn test_run_action_on_track() {
        let (config, _temp) = create_test_config();
        // TODO: Create a test track and run actions on it
    }

    #[test]
    fn test_find_matching_tracks() {
        let (config, _temp) = create_test_config();
        // TODO: Test finding tracks that match a rule
    }
}