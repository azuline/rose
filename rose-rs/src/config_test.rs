use crate::config::*;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn test_config_minimal() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
        music_source_dir = "~/.music-src"
        vfs.mount_dir = "~/music"
        "#,
    )
    .unwrap();

    let config = Config::parse(Some(&config_path)).unwrap();

    let home = dirs::home_dir().unwrap();
    assert_eq!(config.music_source_dir, home.join(".music-src"));
    assert_eq!(config.vfs.mount_dir, home.join("music"));
}

#[test]
fn test_config_full() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    let cache_dir = temp_dir.path().join("cache");

    let config_content = format!(
        r#"
        music_source_dir = "~/.music-src"
        cache_dir = "{}"
        max_proc = 8
        artist_aliases = [
          {{ artist = "Abakus", aliases = ["Cinnamon Chasers"] }},
          {{ artist = "tripleS", aliases = ["EVOLution", "LOVElution", "+(KR)ystal Eyes", "Acid Angel From Asia", "Acid Eyes"] }},
        ]

        cover_art_stems = [ "aa", "bb" ]
        valid_art_exts = [ "tiff" ]
        write_parent_genres = true
        max_filename_bytes = 255
        ignore_release_directories = [ "dummy boy" ]
        rename_source_files = true

        [[stored_metadata_rules]]
        matcher = "tracktitle:lala"
        actions = ["replace:hihi"]

        [[stored_metadata_rules]]
        matcher = "trackartist[main]:haha"
        actions = ["replace:bibi", "split: "]
        ignore = ["releasetitle:blabla"]

        [path_templates]
        source.release = "{{{{ title }}}}"
        source.track = "{{{{ title }}}}"
        source.all_tracks = "{{{{ title }}}}"
        releases.release = "{{{{ title }}}}"
        releases.track = "{{{{ title }}}}"
        releases.all_tracks = "{{{{ title }}}}"
        playlists = "{{{{ title }}}}"

        [vfs]
        mount_dir = "~/music"
        artists_whitelist = ["Beatles"]
        genres_blacklist = ["Techno"]
        hide_genres_with_only_new_releases = true
        "#,
        cache_dir.display()
    );

    std::fs::write(&config_path, config_content).unwrap();

    let config = Config::parse(Some(&config_path)).unwrap();

    // Basic fields
    let home = dirs::home_dir().unwrap();
    assert_eq!(config.music_source_dir, home.join(".music-src"));
    assert_eq!(config.cache_dir, cache_dir);
    assert_eq!(config.max_proc, 8);
    assert_eq!(config.max_filename_bytes, 255);
    assert!(config.rename_source_files);
    assert!(config.write_parent_genres);

    // Cover art config
    assert_eq!(config.cover_art_stems, vec!["aa", "bb"]);
    assert_eq!(config.valid_art_exts, vec!["tiff"]);

    // Ignore directories
    assert_eq!(config.ignore_release_directories, vec!["dummy boy"]);

    // Artist aliases
    assert_eq!(
        config.artist_aliases_map.get("Abakus"),
        Some(&vec!["Cinnamon Chasers".to_string()])
    );
    assert_eq!(
        config.artist_aliases_map.get("tripleS").map(|v| v.len()),
        Some(5)
    );
    assert!(config
        .artist_aliases_parents_map
        .get("Cinnamon Chasers")
        .unwrap()
        .contains(&"Abakus".to_string()));

    // Stored metadata rules
    assert_eq!(config.stored_metadata_rules.len(), 2);
    assert_eq!(config.stored_metadata_rules[0].matcher, "tracktitle:lala");
    assert_eq!(
        config.stored_metadata_rules[0].actions,
        vec!["replace:hihi"]
    );
    assert!(config.stored_metadata_rules[0].ignore.is_empty());

    assert_eq!(
        config.stored_metadata_rules[1].matcher,
        "trackartist[main]:haha"
    );
    assert_eq!(
        config.stored_metadata_rules[1].actions,
        vec!["replace:bibi", "split: "]
    );
    assert_eq!(
        config.stored_metadata_rules[1].ignore,
        vec!["releasetitle:blabla"]
    );

    // VFS config
    assert_eq!(config.vfs.mount_dir, home.join("music"));
    assert_eq!(
        config.vfs.artists_whitelist,
        Some(vec!["Beatles".to_string()])
    );
    assert_eq!(
        config.vfs.genres_blacklist,
        Some(vec!["Techno".to_string()])
    );
    assert!(config.vfs.hide_genres_with_only_new_releases);
}

#[test]
fn test_config_whitelist() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
        music_source_dir = "~/.music-src"
        
        [vfs]
        mount_dir = "~/music"
        artists_whitelist = ["Artist1", "Artist2"]
        genres_whitelist = ["Genre1", "Genre2"]
        descriptors_whitelist = ["Desc1", "Desc2"]
        labels_whitelist = ["Label1", "Label2"]
        "#,
    )
    .unwrap();

    let config = Config::parse(Some(&config_path)).unwrap();

    assert_eq!(
        config.vfs.artists_whitelist,
        Some(vec!["Artist1".to_string(), "Artist2".to_string()])
    );
    assert_eq!(
        config.vfs.genres_whitelist,
        Some(vec!["Genre1".to_string(), "Genre2".to_string()])
    );
    assert_eq!(
        config.vfs.descriptors_whitelist,
        Some(vec!["Desc1".to_string(), "Desc2".to_string()])
    );
    assert_eq!(
        config.vfs.labels_whitelist,
        Some(vec!["Label1".to_string(), "Label2".to_string()])
    );
}

#[test]
fn test_config_not_found() {
    let result = Config::parse(Some(Path::new("/nonexistent/config.toml")));
    assert!(matches!(result, Err(ConfigError::NotFound(_))));
}

#[test]
fn test_config_missing_key_validation() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    // Missing music_source_dir
    std::fs::write(
        &config_path,
        r#"
        [vfs]
        mount_dir = "~/music"
        "#,
    )
    .unwrap();

    let result = Config::parse(Some(&config_path));
    assert!(result.is_err());

    // Missing vfs.mount_dir
    std::fs::write(
        &config_path,
        r#"
        music_source_dir = "~/.music-src"
        [vfs]
        "#,
    )
    .unwrap();

    let result = Config::parse(Some(&config_path));
    assert!(result.is_err());
}

#[test]
fn test_config_value_validation() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    // Invalid max_proc (negative)
    std::fs::write(
        &config_path,
        r#"
        music_source_dir = "~/.music-src"
        max_proc = -1
        
        [vfs]
        mount_dir = "~/music"
        "#,
    )
    .unwrap();

    let result = Config::parse(Some(&config_path));
    assert!(matches!(result, Err(ConfigError::InvalidValue { key, .. }) if key == "max_proc"));

    // Invalid max_proc (zero)
    std::fs::write(
        &config_path,
        r#"
        music_source_dir = "~/.music-src"
        max_proc = 0
        
        [vfs]
        mount_dir = "~/music"
        "#,
    )
    .unwrap();

    let result = Config::parse(Some(&config_path));
    assert!(matches!(result, Err(ConfigError::InvalidValue { key, .. }) if key == "max_proc"));
}

#[test]
fn test_vfs_config_value_validation() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    // Test that unknown fields in vfs section cause error
    std::fs::write(
        &config_path,
        r#"
        music_source_dir = "~/.music-src"
        
        [vfs]
        mount_dir = "~/music"
        unknown_field = "value"
        "#,
    )
    .unwrap();

    let result = Config::parse(Some(&config_path));
    assert!(result.is_err());
}

#[test]
fn test_default_values() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
        music_source_dir = "~/.music-src"
        
        [vfs]
        mount_dir = "~/music"
        "#,
    )
    .unwrap();

    let config = Config::parse(Some(&config_path)).unwrap();

    // Check default values
    assert_eq!(config.max_filename_bytes, 180);
    assert_eq!(
        config.cover_art_stems,
        vec!["folder", "cover", "art", "front"]
    );
    assert_eq!(config.valid_art_exts, vec!["jpg", "jpeg", "png"]);
    assert!(!config.write_parent_genres);
    assert!(!config.rename_source_files);
    assert!(config.ignore_release_directories.is_empty());
    assert!(config.stored_metadata_rules.is_empty());
    assert!(config.artist_aliases_map.is_empty());

    // Test valid_cover_arts method
    let valid_arts = config.valid_cover_arts();
    assert!(valid_arts.contains(&"folder.jpg".to_string()));
    assert!(valid_arts.contains(&"cover.png".to_string()));
    assert!(valid_arts.contains(&"art.jpeg".to_string()));
    assert_eq!(valid_arts.len(), 12); // 4 stems × 3 extensions
}

#[test]
fn test_cache_dir_default() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
        music_source_dir = "~/.music-src"
        
        [vfs]
        mount_dir = "~/music"
        "#,
    )
    .unwrap();

    let config = Config::parse(Some(&config_path)).unwrap();

    // Should use default cache dir
    assert!(config.cache_dir.ends_with("rose"));
}

#[test]
fn test_max_proc_default() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
        music_source_dir = "~/.music-src"
        
        [vfs]
        mount_dir = "~/music"
        "#,
    )
    .unwrap();

    let config = Config::parse(Some(&config_path)).unwrap();

    // Should be at least 1
    assert!(config.max_proc >= 1);
}
