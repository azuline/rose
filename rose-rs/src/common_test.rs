use crate::common::*;
use crate::error::*;

#[test]
fn test_artist_new() {
    let artist = Artist::new("Test Artist".to_string());
    assert_eq!(artist.name, "Test Artist");
    assert!(!artist.alias);
}

#[test]
fn test_artist_with_alias() {
    let artist = Artist::with_alias("Alias Name".to_string(), true);
    assert_eq!(artist.name, "Alias Name");
    assert!(artist.alias);
}

#[test]
fn test_artist_hash() {
    use std::collections::HashSet;

    let mut set = HashSet::new();
    let artist1 = Artist::new("Artist".to_string());
    let artist2 = Artist::new("Artist".to_string());
    let artist3 = Artist::with_alias("Artist".to_string(), true);

    set.insert(artist1.clone());
    assert!(set.contains(&artist2));
    assert!(!set.contains(&artist3));
}

#[test]
fn test_artist_mapping_default() {
    let mapping = ArtistMapping::default();
    assert!(mapping.main.is_empty());
    assert!(mapping.guest.is_empty());
    assert!(mapping.remixer.is_empty());
    assert!(mapping.producer.is_empty());
    assert!(mapping.composer.is_empty());
    assert!(mapping.conductor.is_empty());
    assert!(mapping.djmixer.is_empty());
}

#[test]
fn test_artist_mapping_all() {
    let mut mapping = ArtistMapping::new();

    let artist1 = Artist::new("Artist 1".to_string());
    let artist2 = Artist::new("Artist 2".to_string());
    let artist3 = Artist::new("Artist 3".to_string());

    mapping.main.push(artist1.clone());
    mapping.guest.push(artist2.clone());
    mapping.remixer.push(artist3.clone());
    mapping.composer.push(artist1.clone()); // Duplicate

    let all = mapping.all();
    assert_eq!(all.len(), 3); // Should be unique
    assert!(all.contains(&artist1));
    assert!(all.contains(&artist2));
    assert!(all.contains(&artist3));
}

#[test]
fn test_artist_mapping_items() {
    let mut mapping = ArtistMapping::new();
    mapping.main.push(Artist::new("Main Artist".to_string()));

    let items: Vec<_> = mapping.items().collect();
    assert_eq!(items.len(), 7);
    assert_eq!(items[0].0, "main");
    assert_eq!(items[1].0, "guest");
    assert_eq!(items[2].0, "remixer");
    assert_eq!(items[3].0, "producer");
    assert_eq!(items[4].0, "composer");
    assert_eq!(items[5].0, "conductor");
    assert_eq!(items[6].0, "djmixer");
}

#[test]
fn test_flatten() {
    let nested = vec![vec![1, 2, 3], vec![4, 5], vec![6, 7, 8, 9]];
    let flat = flatten(nested);
    assert_eq!(flat, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
}

#[test]
fn test_uniq() {
    let items = vec![1, 2, 3, 2, 4, 3, 5, 1];
    let unique = uniq(items);
    assert_eq!(unique, vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_uniq_preserves_order() {
    let items = vec!["a", "b", "c", "b", "d", "a"];
    let unique = uniq(items.into_iter().map(String::from).collect());
    assert_eq!(unique, vec!["a", "b", "c", "d"]);
}

#[test]
fn test_sanitize_dirname_basic() {
    let result = sanitize_dirname("test:file?name", 180, false);
    assert_eq!(result, "test_file_name");
}

#[test]
fn test_sanitize_dirname_dots() {
    // The Python implementation doesn't replace dots
    let result = sanitize_dirname(".", 180, false);
    assert_eq!(result, ".");

    let result = sanitize_dirname("..", 180, false);
    assert_eq!(result, "..");

    let result = sanitize_dirname("file.name", 180, false);
    assert_eq!(result, "file.name");
}

#[test]
fn test_sanitize_dirname_unicode() {
    let result = sanitize_dirname("café", 180, false);
    // NFD normalization decomposes é into e + combining accent
    assert_eq!(result, "cafe\u{0301}"); // NFD normalized

    let result = sanitize_dirname("test/with*illegal|chars", 180, false);
    assert_eq!(result, "test_with_illegal_chars");
}

#[test]
fn test_sanitize_dirname_maxlen() {
    let long_name = "a".repeat(200);
    let result = sanitize_dirname(&long_name, 180, true);
    assert!(result.len() <= 180);
    assert!(result.starts_with("aaaa"));
}

#[test]
fn test_sanitize_filename_basic() {
    let result = sanitize_filename("test:file?name.mp3", 180, false);
    assert_eq!(result, "test_file_name.mp3");
}

#[test]
fn test_sanitize_filename_dots() {
    let result = sanitize_filename(".", 180, false);
    assert_eq!(result, ".");

    let result = sanitize_filename("..", 180, false);
    assert_eq!(result, "..");
}

#[test]
fn test_sanitize_filename_unicode() {
    let result = sanitize_filename("café.txt", 180, false);
    // NFD normalization decomposes é into e + combining accent
    assert_eq!(result, "cafe\u{0301}.txt"); // NFD normalized
}

#[test]
fn test_sanitize_filename_maxlen() {
    let long_name = format!("{}.mp3", "a".repeat(200));
    let result = sanitize_filename(&long_name, 180, true);
    assert!(result.ends_with(".mp3"));
    assert!(result.len() <= 186); // 180 + ".mp3"
}

#[test]
fn test_sanitize_filename_long_extension() {
    let name = "test.verylongextension";
    let result = sanitize_filename(name, 10, true);
    // Extension longer than 6 chars is ignored
    assert_eq!(result, "test.veryl");
}

#[test]
fn test_sha256_dataclass() {
    #[derive(Debug)]
    struct TestStruct {
        _field1: String,
        _field2: i32,
    }

    let data = TestStruct {
        _field1: "test".to_string(),
        _field2: 42,
    };

    let hash1 = sha256_dataclass(&data);
    let hash2 = sha256_dataclass(&data);

    assert_eq!(hash1, hash2);
    assert_eq!(hash1.len(), 64); // SHA256 produces 64 hex chars
}

#[test]
fn test_is_music_file() {
    assert!(is_music_file("song.mp3"));
    assert!(is_music_file("Song.MP3"));
    assert!(is_music_file("track.flac"));
    assert!(is_music_file("audio.opus"));
    assert!(is_music_file("music.ogg"));
    assert!(is_music_file("file.m4a"));

    assert!(!is_music_file("image.jpg"));
    assert!(!is_music_file("document.pdf"));
    assert!(!is_music_file("noextension"));
}

#[test]
fn test_is_image_file() {
    assert!(is_image_file("cover.jpg"));
    assert!(is_image_file("Cover.JPG"));
    assert!(is_image_file("folder.jpeg"));
    assert!(is_image_file("art.png"));

    assert!(!is_image_file("song.mp3"));
    assert!(!is_image_file("document.pdf"));
    assert!(!is_image_file("noextension"));
}

#[test]
fn test_error_hierarchy() {
    let genre_err = RoseExpectedError::GenreDoesNotExist {
        name: "Unknown".to_string(),
    };
    assert_eq!(genre_err.to_string(), "Genre does not exist: Unknown");

    let label_err = RoseExpectedError::LabelDoesNotExist {
        name: "Unknown Label".to_string(),
    };
    assert_eq!(label_err.to_string(), "Label does not exist: Unknown Label");

    let desc_err = RoseExpectedError::DescriptorDoesNotExist {
        name: "Unknown Desc".to_string(),
    };
    assert_eq!(
        desc_err.to_string(),
        "Descriptor does not exist: Unknown Desc"
    );

    let artist_err = RoseExpectedError::ArtistDoesNotExist {
        name: "Unknown Artist".to_string(),
    };
    assert_eq!(
        artist_err.to_string(),
        "Artist does not exist: Unknown Artist"
    );
}

#[test]
fn test_error_conversion() {
    let expected_err = RoseExpectedError::GenreDoesNotExist {
        name: "Test".to_string(),
    };
    let rose_err: RoseError = expected_err.into();

    match rose_err {
        RoseError::Expected(_) => {}
        _ => panic!("Expected RoseError::Expected variant"),
    }
}

#[test]
fn test_error_context() {
    use std::path::PathBuf;

    let generic_err = RoseError::Generic("Test error".to_string());
    assert_eq!(generic_err.to_string(), "Rose error: Test error");

    let uuid_err = RoseExpectedError::InvalidUuid {
        uuid: "not-a-uuid".to_string(),
    };
    assert_eq!(uuid_err.to_string(), "Invalid UUID: not-a-uuid");

    let file_err = RoseExpectedError::FileNotFound {
        path: PathBuf::from("/missing/file.mp3"),
    };
    assert_eq!(file_err.to_string(), "File not found: /missing/file.mp3");

    let format_err = RoseExpectedError::InvalidFileFormat {
        format: "unknown".to_string(),
    };
    assert_eq!(format_err.to_string(), "Invalid file format: unknown");
}
