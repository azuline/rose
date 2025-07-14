#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
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
    fn test_remove_release_from_collage() {
        let (config, _temp) = create_test_config();
        // TODO: Test removing a release from a collage
    }

    #[test]
    fn test_collage_lifecycle() {
        let (config, _temp) = create_test_config();
        // TODO: Test creating, modifying, and deleting a collage
    }

    #[test]
    fn test_collage_add_duplicate() {
        let (config, _temp) = create_test_config();
        // TODO: Test adding a duplicate release to a collage
    }

    #[test]
    fn test_rename_collage() {
        let (config, _temp) = create_test_config();
        // TODO: Test renaming a collage
    }

    #[test]
    fn test_edit_collages_ordering() {
        let (config, _temp) = create_test_config();
        // TODO: Test editing collage ordering
    }

    #[test]
    fn test_edit_collages_remove_release() {
        let (config, _temp) = create_test_config();
        // TODO: Test removing a release via editor
    }

    #[test]
    fn test_collage_handle_missing_release() {
        let (config, _temp) = create_test_config();
        // TODO: Test handling missing releases in collages
    }
}