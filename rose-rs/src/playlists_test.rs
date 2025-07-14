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
    fn test_remove_track_from_playlist() {
        let (_config, _temp) = setup_test();
        // TODO: Test removing a track from a playlist
    }

    #[test]
    fn test_playlist_lifecycle() {
        let (_config, _temp) = setup_test();
        // TODO: Test creating, modifying, and deleting a playlist
    }

    #[test]
    fn test_playlist_add_duplicate() {
        let (_config, _temp) = setup_test();
        // TODO: Test adding a duplicate track to a playlist
    }

    #[test]
    fn test_rename_playlist() {
        let (_config, _temp) = setup_test();
        // TODO: Test renaming a playlist
    }

    #[test]
    fn test_edit_playlists_ordering() {
        let (_config, _temp) = setup_test();
        // TODO: Test editing playlist ordering
    }

    #[test]
    fn test_edit_playlists_remove_track() {
        let (_config, _temp) = setup_test();
        // TODO: Test removing a track via editor
    }

    #[test]
    fn test_edit_playlists_duplicate_track_name() {
        let (_config, _temp) = setup_test();
        // TODO: Test handling duplicate track names in editor
    }

    #[test]
    fn test_playlist_handle_missing_track() {
        let (_config, _temp) = setup_test();
        // TODO: Test handling missing tracks in playlists
    }

    #[test]
    fn test_set_playlist_cover_art() {
        let (_config, _temp) = setup_test();
        // TODO: Test setting cover art for a playlist
    }

    #[test]
    fn test_remove_playlist_cover_art() {
        let (_config, _temp) = setup_test();
        // TODO: Test removing cover art from a playlist
    }
}
