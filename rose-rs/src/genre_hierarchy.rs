use serde_json::Value;
use std::collections::HashMap;

const GENRE_JSON: &str = include_str!("genre_hierarchy.json");

lazy_static::lazy_static! {
    static ref GENRE_HIERARCHY: HashMap<String, Vec<String>> = {
        let value: Value = serde_json::from_str(GENRE_JSON)
            .expect("Failed to parse genre_hierarchy.json");

        match value {
            Value::Object(obj) => obj.into_iter()
                .filter_map(|(genre, parents)| match parents {
                    Value::Array(arr) => Some((genre, arr.into_iter()
                        .filter_map(|v| match v {
                            Value::String(s) => Some(s),
                            _ => None
                        }).collect())),
                    _ => None
                }).collect(),
            _ => HashMap::new()
        }
    };

    static ref GENRE_LOOKUP: HashMap<String, String> =
        GENRE_HIERARCHY.keys()
            .map(|g| (g.to_lowercase(), g.clone()))
            .collect();
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

pub fn get_all_parent_genres(genres: &[String]) -> Vec<String> {
    use crate::common::flatten;
    use std::collections::HashSet;

    let all_parents = flatten(genres.iter().filter_map(|g| get_parent_genres(g)).collect());
    let original_set: HashSet<String> = genres
        .iter()
        .map(|g| {
            GENRE_LOOKUP
                .get(&g.to_lowercase())
                .cloned()
                .unwrap_or_else(|| g.clone())
        })
        .collect();

    let mut result: Vec<String> = all_parents
        .into_iter()
        .filter(|p| !original_set.contains(p))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    result.sort();
    result
}

pub fn get_transitive_parent_genres(genre: &str) -> Option<Vec<String>> {
    let mut result = Vec::new();
    let mut to_process = vec![genre.to_string()];
    let mut seen = std::collections::HashSet::new();

    while let Some(current) = to_process.pop() {
        if !seen.insert(current.clone()) {
            continue;
        }

        if let Some(parents) = get_parent_genres(&current) {
            for parent in parents {
                if !seen.contains(&parent) {
                    result.push(parent.clone());
                    to_process.push(parent);
                }
            }
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}
