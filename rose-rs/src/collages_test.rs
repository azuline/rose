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
    fn test_remove_release_from_collage() {
        let (_config, _temp) = setup_test();
        // TODO: Test removing a release from a collage
    }

    #[test]
    fn test_collage_lifecycle() {
        let (_config, _temp) = setup_test();
        // TODO: Test creating, modifying, and deleting a collage
    }

    #[test]
    fn test_collage_add_duplicate() {
        let (_config, _temp) = setup_test();
        // TODO: Test adding a duplicate release to a collage
    }

    #[test]
    fn test_rename_collage() {
        let (_config, _temp) = setup_test();
        // TODO: Test renaming a collage
    }

    #[test]
    fn test_edit_collages_ordering() {
        let (_config, _temp) = setup_test();
        // TODO: Test editing collage ordering
    }

    #[test]
    fn test_edit_collages_remove_release() {
        let (_config, _temp) = setup_test();
        // TODO: Test removing a release via editor
    }

    #[test]
    fn test_collage_handle_missing_release() {
        let (_config, _temp) = setup_test();
        // TODO: Test handling missing releases in collages
    }
}
