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
            valid_art_exts: vec!["jpg".to_string(), "png".to_string()],
            // ... other config fields
        };
        fs::create_dir_all(&config.music_source_dir).unwrap();
        fs::create_dir_all(&config.cache_dir).unwrap();
        (config, temp_dir)
    }

    #[test]
    fn test_remove_track_from_playlist() {
        let (config, _temp) = create_test_config();
        // TODO: Test removing a track from a playlist
    }

    #[test]
    fn test_playlist_lifecycle() {
        let (config, _temp) = create_test_config();
        // TODO: Test creating, modifying, and deleting a playlist
    }

    #[test]
    fn test_playlist_add_duplicate() {
        let (config, _temp) = create_test_config();
        // TODO: Test adding a duplicate track to a playlist
    }

    #[test]
    fn test_rename_playlist() {
        let (config, _temp) = create_test_config();
        // TODO: Test renaming a playlist
    }

    #[test]
    fn test_edit_playlists_ordering() {
        let (config, _temp) = create_test_config();
        // TODO: Test editing playlist ordering
    }

    #[test]
    fn test_edit_playlists_remove_track() {
        let (config, _temp) = create_test_config();
        // TODO: Test removing a track via editor
    }

    #[test]
    fn test_edit_playlists_duplicate_track_name() {
        let (config, _temp) = create_test_config();
        // TODO: Test handling duplicate track names in editor
    }

    #[test]
    fn test_playlist_handle_missing_track() {
        let (config, _temp) = create_test_config();
        // TODO: Test handling missing tracks in playlists
    }

    #[test]
    fn test_set_playlist_cover_art() {
        let (config, _temp) = create_test_config();
        // TODO: Test setting cover art for a playlist
    }

    #[test]
    fn test_remove_playlist_cover_art() {
        let (config, _temp) = create_test_config();
        // TODO: Test removing cover art from a playlist
    }
}