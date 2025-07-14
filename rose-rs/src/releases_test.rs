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
    fn test_delete_release() {
        let (_config, _temp) = setup_test();
        // TODO: Create a test release and delete it
    }

    #[test]
    fn test_toggle_release_new() {
        let (_config, _temp) = setup_test();
        // TODO: Create a test release and toggle its new status
    }

    #[test]
    fn test_set_release_cover_art() {
        let (_config, _temp) = setup_test();
        // TODO: Test setting cover art
    }

    #[test]
    fn test_delete_release_cover_art() {
        let (_config, _temp) = setup_test();
        // TODO: Test deleting cover art
    }

    #[test]
    fn test_edit_release() {
        let (_config, _temp) = setup_test();
        // TODO: Test editing release metadata
    }

    #[test]
    fn test_edit_release_failure_and_resume() {
        let (_config, _temp) = setup_test();
        // TODO: Test edit failure and resume functionality
    }

    #[test]
    fn test_create_single_release() {
        let (_config, _temp) = setup_test();
        // TODO: Test creating a single from a track
    }

    #[test]
    fn test_create_single_release_with_trailing_space() {
        let (_config, _temp) = setup_test();
        // TODO: Test creating a single with trailing space in title
    }

    #[test]
    fn test_run_action_on_release() {
        let (_config, _temp) = setup_test();
        // TODO: Test running actions on a release
    }

    #[test]
    fn test_find_matching_releases() {
        let (_config, _temp) = setup_test();
        // TODO: Test finding releases that match a rule
    }
}
