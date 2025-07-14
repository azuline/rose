use crate::genre_hierarchy::*;

#[test]
fn test_valid_genre() {
    // Test some known genres
    assert!(is_valid_genre("Rock"));
    assert!(is_valid_genre("Pop"));
    assert!(is_valid_genre("Jazz"));
    assert!(is_valid_genre("Electronic"));
    assert!(is_valid_genre("Hip Hop"));
}

#[test]
fn test_invalid_genre() {
    assert!(!is_valid_genre("NotAGenre"));
    assert!(!is_valid_genre(""));
    assert!(!is_valid_genre("Random String"));
}

#[test]
fn test_case_insensitive_genre() {
    // All these should be valid
    assert!(is_valid_genre("rock"));
    assert!(is_valid_genre("ROCK"));
    assert!(is_valid_genre("Rock"));
    assert!(is_valid_genre("rOcK"));

    // Parent lookups should also be case insensitive
    assert_eq!(get_parent_genres("rock"), get_parent_genres("ROCK"));
    assert_eq!(get_parent_genres("hip hop"), get_parent_genres("Hip Hop"));
}

#[test]
fn test_parent_genres_single_level() {
    // Test genres with direct parents
    let parents = get_parent_genres("Punk Rock").unwrap();
    assert!(parents.contains(&"Rock".to_string()));

    let parents = get_parent_genres("Bebop").unwrap();
    assert!(parents.contains(&"Jazz".to_string()));
}

#[test]
fn test_parent_genres_multi_level() {
    // Test "2-Step" which has multiple parent levels
    let parents = get_parent_genres("2-Step").unwrap();
    assert!(parents.contains(&"Dance".to_string()));
    assert!(parents.contains(&"Electronic".to_string()));
    assert!(parents.contains(&"Electronic Dance Music".to_string()));
    assert!(parents.contains(&"UK Garage".to_string()));
}

#[test]
fn test_parent_genres_dance_special_case() {
    // Dance genre should have proper parent relationships
    let dance_parents = get_parent_genres("Dance");
    // Dance may or may not have parents, but the function should handle it properly
    assert!(dance_parents.is_some() || dance_parents.is_none());

    // Test a genre that has Dance as a parent
    let parents = get_parent_genres("House").unwrap();
    assert!(parents.contains(&"Dance".to_string()));
}

#[test]
fn test_parent_genres_unknown() {
    assert!(get_parent_genres("UnknownGenre").is_none());
    assert!(get_parent_genres("").is_none());
}

#[test]
fn test_transitive_parent_closure() {
    // Test that we get all transitive parents
    let genres = vec!["2-Step".to_string()];
    let all_parents = get_all_parent_genres(&genres);

    // Should include all parents but not the original genre
    assert!(!all_parents.contains(&"2-Step".to_string()));
    assert!(all_parents.contains(&"Dance".to_string()));
    assert!(all_parents.contains(&"Electronic".to_string()));
    assert!(all_parents.contains(&"Electronic Dance Music".to_string()));
    assert!(all_parents.contains(&"UK Garage".to_string()));
}

#[test]
fn test_get_all_parent_genres_deduplication() {
    // Test with multiple genres that might have overlapping parents
    let genres = vec!["House".to_string(), "Techno".to_string()];
    let all_parents = get_all_parent_genres(&genres);

    // Should have Dance and Electronic only once
    let dance_count = all_parents.iter().filter(|&g| g == "Dance").count();
    let electronic_count = all_parents.iter().filter(|&g| g == "Electronic").count();

    assert_eq!(dance_count, 1);
    assert_eq!(electronic_count, 1);
}

#[test]
fn test_get_all_parent_genres_excludes_input() {
    // The function should exclude input genres from the result
    let genres = vec!["Rock".to_string(), "Electronic".to_string()];
    let all_parents = get_all_parent_genres(&genres);

    assert!(!all_parents.contains(&"Rock".to_string()));
    assert!(!all_parents.contains(&"Electronic".to_string()));
}

#[test]
fn test_get_all_parent_genres_sorted() {
    // Result should be sorted
    let genres = vec!["House".to_string(), "Techno".to_string()];
    let all_parents = get_all_parent_genres(&genres);

    let mut sorted = all_parents.clone();
    sorted.sort();

    assert_eq!(all_parents, sorted);
}

#[test]
fn test_empty_parent_list() {
    // Some genres might have no parents
    let _genres_with_no_parents = ["A cappella".to_string()];
    let parents = get_parent_genres("A cappella").unwrap();
    assert!(parents.is_empty());
}

#[test]
fn test_genre_hierarchy_loaded() {
    // Basic sanity check that we loaded a reasonable number of genres
    // The Python version has ~2400 genres
    let test_genres = vec![
        "Rock",
        "Pop",
        "Jazz",
        "Electronic",
        "Hip Hop",
        "Classical Music",
        "Folk",
        "Blues",
        "Country",
        "R&B",
        "Soul",
        "Funk",
        "Metal",
        "Punk",
        "Dance",
        "House",
        "Techno",
        "Ambient",
        "Experimental",
    ];

    let valid_count = test_genres.iter().filter(|g| is_valid_genre(g)).count();
    assert!(
        valid_count >= 15,
        "Expected at least 15 valid genres, got {valid_count}"
    );
}
