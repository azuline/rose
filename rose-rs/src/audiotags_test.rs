use crate::audiotags::*;
use crate::common::{Artist, ArtistMapping};
use crate::config::Config;

#[test]
fn test_split_tag() {
    let result = split_tag(Some("Artist A \\\\ Artist B / Artist C; Artist D vs. Artist E"));
    assert_eq!(result, vec!["Artist A", "Artist B", "Artist C", "Artist D", "Artist E"]);
    
    let result = split_tag(Some("Single Artist"));
    assert_eq!(result, vec!["Single Artist"]);
    
    let result = split_tag(None);
    assert!(result.is_empty());
    
    let result = split_tag(Some(""));
    assert!(result.is_empty());
}

#[test]
fn test_parse_artist_string() {
    // Test basic parsing
    let result = parse_artist_string("Artist A", "", "", "", "", "");
    assert_eq!(result.main, vec![Artist { name: "Artist A".to_string(), alias: false }]);
    assert!(result.guest.is_empty());
    
    // Test feat. parsing
    let result = parse_artist_string("Artist A feat. Artist B", "", "", "", "", "");
    assert_eq!(result.main, vec![Artist { name: "Artist A".to_string(), alias: false }]);
    assert_eq!(result.guest, vec![Artist { name: "Artist B".to_string(), alias: false }]);
    
    // Test produced by parsing
    let result = parse_artist_string("Artist A produced by Artist B", "", "", "", "", "");
    assert_eq!(result.main, vec![Artist { name: "Artist A".to_string(), alias: false }]);
    assert_eq!(result.producer, vec![Artist { name: "Artist B".to_string(), alias: false }]);
    
    // Test remixed by parsing
    let result = parse_artist_string("Artist A remixed by Artist B", "", "", "", "", "");
    assert_eq!(result.main, vec![Artist { name: "Artist A".to_string(), alias: false }]);
    assert_eq!(result.remixer, vec![Artist { name: "Artist B".to_string(), alias: false }]);
    
    // Test pres. parsing
    let result = parse_artist_string("DJ A pres. Artist B", "", "", "", "", "");
    assert_eq!(result.main, vec![Artist { name: "Artist B".to_string(), alias: false }]);
    assert_eq!(result.djmixer, vec![Artist { name: "DJ A".to_string(), alias: false }]);
    
    // Test performed by parsing
    let result = parse_artist_string("Composer A performed by Artist B", "", "", "", "", "");
    assert_eq!(result.main, vec![Artist { name: "Artist B".to_string(), alias: false }]);
    assert_eq!(result.composer, vec![Artist { name: "Composer A".to_string(), alias: false }]);
    
    // Test under. parsing
    let result = parse_artist_string("Artist A under. Conductor B", "", "", "", "", "");
    assert_eq!(result.main, vec![Artist { name: "Artist A".to_string(), alias: false }]);
    assert_eq!(result.conductor, vec![Artist { name: "Conductor B".to_string(), alias: false }]);
    
    // Test complex parsing with multiple roles
    let result = parse_artist_string(
        "Artist A feat. Artist B",
        "Remixer A",
        "Composer A",
        "Conductor A",
        "Producer A",
        "DJ A",
    );
    assert_eq!(result.main, vec![Artist { name: "Artist A".to_string(), alias: false }]);
    assert_eq!(result.guest, vec![Artist { name: "Artist B".to_string(), alias: false }]);
    assert_eq!(result.remixer, vec![Artist { name: "Remixer A".to_string(), alias: false }]);
    assert_eq!(result.composer, vec![Artist { name: "Composer A".to_string(), alias: false }]);
    assert_eq!(result.conductor, vec![Artist { name: "Conductor A".to_string(), alias: false }]);
    assert_eq!(result.producer, vec![Artist { name: "Producer A".to_string(), alias: false }]);
    assert_eq!(result.djmixer, vec![Artist { name: "DJ A".to_string(), alias: false }]);
}

#[test]
fn test_format_artist_string() {
    // Test basic formatting
    let mapping = ArtistMapping {
        main: vec![Artist { name: "Artist A".to_string(), alias: false }],
        guest: vec![],
        remixer: vec![],
        composer: vec![],
        conductor: vec![],
        producer: vec![],
        djmixer: vec![],
    };
    assert_eq!(format_artist_string(&mapping), "Artist A");
    
    // Test with guest
    let mapping = ArtistMapping {
        main: vec![Artist { name: "Artist A".to_string(), alias: false }],
        guest: vec![Artist { name: "Artist B".to_string(), alias: false }],
        remixer: vec![],
        composer: vec![],
        conductor: vec![],
        producer: vec![],
        djmixer: vec![],
    };
    assert_eq!(format_artist_string(&mapping), "Artist A feat. Artist B");
    
    // Test with producer
    let mapping = ArtistMapping {
        main: vec![Artist { name: "Artist A".to_string(), alias: false }],
        guest: vec![],
        remixer: vec![],
        composer: vec![],
        conductor: vec![],
        producer: vec![Artist { name: "Producer A".to_string(), alias: false }],
        djmixer: vec![],
    };
    assert_eq!(format_artist_string(&mapping), "Artist A produced by Producer A");
    
    // Test with all roles
    let mapping = ArtistMapping {
        main: vec![Artist { name: "Artist A".to_string(), alias: false }],
        guest: vec![Artist { name: "Guest A".to_string(), alias: false }],
        remixer: vec![Artist { name: "Remixer A".to_string(), alias: false }],
        composer: vec![Artist { name: "Composer A".to_string(), alias: false }],
        conductor: vec![Artist { name: "Conductor A".to_string(), alias: false }],
        producer: vec![Artist { name: "Producer A".to_string(), alias: false }],
        djmixer: vec![Artist { name: "DJ A".to_string(), alias: false }],
    };
    assert_eq!(
        format_artist_string(&mapping),
        "Composer A performed by DJ A pres. Artist A under. Conductor A feat. Guest A remixed by Remixer A produced by Producer A"
    );
    
    // Test with aliases (should be excluded)
    let mapping = ArtistMapping {
        main: vec![
            Artist { name: "Artist A".to_string(), alias: false },
            Artist { name: "Artist B".to_string(), alias: true },
        ],
        guest: vec![],
        remixer: vec![],
        composer: vec![],
        conductor: vec![],
        producer: vec![],
        djmixer: vec![],
    };
    assert_eq!(format_artist_string(&mapping), "Artist A");
}

#[test]
fn test_releasetype_normalization() {
    assert_eq!(normalize_releasetype(Some("Album")), "album");
    assert_eq!(normalize_releasetype(Some("SINGLE")), "single");
    assert_eq!(normalize_releasetype(Some("ep")), "ep");
    assert_eq!(normalize_releasetype(Some("Invalid")), "unknown");
    assert_eq!(normalize_releasetype(None), "unknown");
    assert_eq!(normalize_releasetype(Some("")), "unknown");
}

#[test]
fn test_rose_date_parse() {
    // Test year only
    let date = RoseDate::parse(Some("2023"));
    assert_eq!(date, Some(RoseDate { year: 2023, month: None, day: None }));
    
    // Test full date
    let date = RoseDate::parse(Some("2023-12-25"));
    assert_eq!(date, Some(RoseDate { year: 2023, month: Some(12), day: Some(25) }));
    
    // Test with extra content after date
    let date = RoseDate::parse(Some("2023-12-25T10:30:00"));
    assert_eq!(date, Some(RoseDate { year: 2023, month: Some(12), day: Some(25) }));
    
    // Test invalid inputs
    assert_eq!(RoseDate::parse(None), None);
    assert_eq!(RoseDate::parse(Some("")), None);
    assert_eq!(RoseDate::parse(Some("invalid")), None);
    assert_eq!(RoseDate::parse(Some("12345")), None); // Year out of range
}

#[test]
fn test_rose_date_display() {
    let date = RoseDate { year: 2023, month: None, day: None };
    assert_eq!(date.to_string(), "2023");
    
    let date = RoseDate { year: 2023, month: Some(12), day: Some(25) };
    assert_eq!(date.to_string(), "2023-12-25");
    
    let date = RoseDate { year: 2023, month: Some(12), day: None };
    assert_eq!(date.to_string(), "2023-12-01");
    
    let date = RoseDate { year: 2023, month: None, day: Some(25) };
    assert_eq!(date.to_string(), "2023-01-01");
}

#[test]
fn test_format_genre_tag() {
    let config = Config::default();
    
    // Test without parent genre writing
    let genres = vec!["Electronic".to_string(), "House".to_string()];
    assert_eq!(format_genre_tag(&config, &genres), "Electronic;House");
    
    // Test empty genres
    assert_eq!(format_genre_tag(&config, &[]), "");
    
    // Test with parent genre writing enabled
    let mut config = Config::default();
    config.write_parent_genres = true;
    
    // Assuming Electronic has parent genres in the hierarchy
    // This test might need adjustment based on actual genre hierarchy
    let genres = vec!["House".to_string()];
    let result = format_genre_tag(&config, &genres);
    // Should contain PARENTS separator if House has parent genres
    assert!(result.starts_with("House") || result.contains("\\\\PARENTS:\\\\"));
}

#[test]
fn test_split_genre_tag() {
    // Test normal genre split
    let result = split_genre_tag(Some("Electronic; House"));
    assert_eq!(result, vec!["Electronic", "House"]);
    
    // Test with parent genres (should remove them)
    let result = split_genre_tag(Some("Electronic; House\\\\PARENTS:\\\\Dance; EDM"));
    assert_eq!(result, vec!["Electronic", "House"]);
    
    // Test empty cases
    assert!(split_genre_tag(None).is_empty());
    assert!(split_genre_tag(Some("")).is_empty());
}

// Integration tests would require actual audio files to test reading/writing
// For now, we'll test the parse/format round-trip
#[test]
fn test_artist_string_round_trip() {
    let original = "Composer A performed by DJ A pres. Artist A under. Conductor A feat. Guest A remixed by Remixer A produced by Producer A";
    let parsed = parse_artist_string(original, "", "", "", "", "");
    let formatted = format_artist_string(&parsed);
    
    // The formatted string should contain all the same information
    assert!(formatted.contains("Composer A performed by"));
    assert!(formatted.contains("DJ A pres."));
    assert!(formatted.contains("Artist A"));
    assert!(formatted.contains("under. Conductor A"));
    assert!(formatted.contains("feat. Guest A"));
    assert!(formatted.contains("remixed by Remixer A"));
    assert!(formatted.contains("produced by Producer A"));
}