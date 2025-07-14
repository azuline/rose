#[cfg(test)]
pub mod test_utils {
    use crate::config::{Config, PathTemplates, VirtualFSConfig};
    use std::collections::HashMap;
    use tempfile::TempDir;

    pub fn create_test_config(temp_dir: &TempDir) -> Config {
        Config {
            music_source_dir: temp_dir.path().join("music").to_path_buf(),
            cache_dir: temp_dir.path().join("cache").to_path_buf(),
            max_proc: 4,
            ignore_release_directories: vec![],
            rename_source_files: false,
            max_filename_bytes: 180,
            cover_art_stems: vec!["cover".to_string(), "folder".to_string()],
            valid_art_exts: vec!["jpg".to_string(), "jpeg".to_string(), "png".to_string()],
            write_parent_genres: false,
            artist_aliases_map: HashMap::new(),
            artist_aliases_parents_map: HashMap::new(),
            path_templates: PathTemplates::default(),
            stored_metadata_rules: vec![],
            vfs: VirtualFSConfig::default(),
        }
    }
}