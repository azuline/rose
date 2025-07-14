#[cfg(test)]
mod tests {
    use crate::test_utils::test_utils::create_test_config;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test() -> (crate::config::Config, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        fs::create_dir_all(&config.music_source_dir).unwrap();
        fs::create_dir_all(&config.cache_dir).unwrap();
        (config, temp_dir)
    }

    #[test]
    fn test_run_action_on_track() {
        let (_config, _temp) = setup_test();
        // TODO: Create a test track and run actions on it
    }

    #[test]
    fn test_find_matching_tracks() {
        let (_config, _temp) = setup_test();
        // TODO: Test finding tracks that match a rule
    }
}
