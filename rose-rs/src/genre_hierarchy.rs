use serde_json::Value;
use std::collections::HashMap;

// Embed the JSON file at compile time
const GENRE_JSON: &str = include_str!("genre_hierarchy.json");

lazy_static::lazy_static! {
    // Parse the JSON and create the genre hierarchy map
    static ref GENRE_HIERARCHY: HashMap<String, Vec<String>> = {
        let value: Value = serde_json::from_str(GENRE_JSON)
            .expect("Failed to parse genre_hierarchy.json");

        let mut map = HashMap::new();

        if let Value::Object(obj) = value {
            for (genre, parents) in obj {
                if let Value::Array(parent_array) = parents {
                    let parent_vec: Vec<String> = parent_array
                        .into_iter()
                        .filter_map(|v| {
                            if let Value::String(s) = v {
                                Some(s)
                            } else {
                                None
                            }
                        })
                        .collect();
                    map.insert(genre, parent_vec);
                }
            }
        }

        map
    };

    // Create a case-insensitive lookup map
    static ref GENRE_LOOKUP: HashMap<String, String> = {
        let mut map = HashMap::new();
        for genre in GENRE_HIERARCHY.keys() {
            map.insert(genre.to_lowercase(), genre.clone());
        }
        map
    };
}

/// Check if a genre is valid (case-insensitive)
pub fn is_valid_genre(genre: &str) -> bool {
    GENRE_LOOKUP.contains_key(&genre.to_lowercase())
}

/// Get parent genres for a given genre (case-insensitive)
/// Returns None if the genre is not found
pub fn get_parent_genres(genre: &str) -> Option<Vec<String>> {
    let normalized = GENRE_LOOKUP.get(&genre.to_lowercase())?;
    GENRE_HIERARCHY.get(normalized).cloned()
}

/// Get all parent genres for multiple genres, handling the special case where
/// if "Dance" is in the input genres, we should use TRANSITIVE_PARENT_GENRES logic
pub fn get_all_parent_genres(genres: &[String]) -> Vec<String> {
    use crate::common::flatten;

    // Get all parent genres for each input genre
    let parent_lists: Vec<Vec<String>> =
        genres.iter().filter_map(|g| get_parent_genres(g)).collect();

    // Flatten and deduplicate
    let all_parents = flatten(parent_lists);

    // Return unique parents that are not in the original genres
    let original_set: std::collections::HashSet<String> = genres
        .iter()
        .map(|g| {
            // Normalize to the canonical case
            GENRE_LOOKUP
                .get(&g.to_lowercase())
                .cloned()
                .unwrap_or_else(|| g.clone())
        })
        .collect();

    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for parent in all_parents {
        if !original_set.contains(&parent) && seen.insert(parent.clone()) {
            result.push(parent);
        }
    }

    result.sort();
    result
}
