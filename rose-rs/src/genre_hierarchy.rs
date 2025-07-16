use once_cell::sync::Lazy;
use std::collections::HashMap;

// Load genre hierarchy from JSON at compile time
#[allow(dead_code)]
const GENRE_HIERARCHY_JSON: &str = include_str!("genre_hierarchy.json");

#[allow(dead_code)]
pub static TRANSITIVE_PARENT_GENRES: Lazy<HashMap<String, Vec<String>>> = Lazy::new(|| {
    let hierarchy: HashMap<String, Vec<String>> = serde_json::from_str(GENRE_HIERARCHY_JSON).expect("Failed to parse genre_hierarchy.json");

    let mut transitive_parents: HashMap<String, Vec<String>> = HashMap::new();

    // Build transitive parent relationships
    for (genre, parents) in &hierarchy {
        let mut all_parents = Vec::new();
        let mut to_process = parents.clone();
        let mut seen = std::collections::HashSet::new();

        while let Some(parent) = to_process.pop() {
            if seen.insert(parent.clone()) {
                all_parents.push(parent.clone());
                if let Some(grandparents) = hierarchy.get(&parent) {
                    to_process.extend(grandparents.clone());
                }
            }
        }

        transitive_parents.insert(genre.clone(), all_parents);
    }

    transitive_parents
});

#[allow(dead_code)]
pub static TRANSITIVE_CHILD_GENRES: Lazy<HashMap<String, Vec<String>>> = Lazy::new(|| {
    let mut transitive_children: HashMap<String, Vec<String>> = HashMap::new();

    // Build from transitive parents
    for (child, parents) in TRANSITIVE_PARENT_GENRES.iter() {
        for parent in parents {
            transitive_children.entry(parent.clone()).or_default().push(child.clone());
        }
    }

    transitive_children
});

#[allow(dead_code)]
pub static IMMEDIATE_CHILD_GENRES: Lazy<HashMap<String, Vec<String>>> = Lazy::new(|| {
    let hierarchy: HashMap<String, Vec<String>> = serde_json::from_str(GENRE_HIERARCHY_JSON).expect("Failed to parse genre_hierarchy.json");

    let mut immediate_children: HashMap<String, Vec<String>> = HashMap::new();

    // Build immediate child relationships
    for (child, parents) in &hierarchy {
        for parent in parents {
            immediate_children.entry(parent.clone()).or_default().push(child.clone());
        }
    }

    immediate_children
});

/// Wrapper for genre hierarchy operations
pub struct GenreHierarchy;

impl GenreHierarchy {
    /// Get the transitive parent genres for a given genre
    pub fn transitive_parents(genre: &str) -> Option<&Vec<String>> {
        TRANSITIVE_PARENT_GENRES.get(genre)
    }

    /// Get the parent genres for a given genre (alias for transitive_parents)
    pub fn parents(genre: &str) -> Option<&Vec<String>> {
        Self::transitive_parents(genre)
    }

    /// Get the transitive child genres for a given genre
    pub fn transitive_children(genre: &str) -> Option<&Vec<String>> {
        TRANSITIVE_CHILD_GENRES.get(genre)
    }

    /// Get the immediate child genres for a given genre
    pub fn immediate_children(genre: &str) -> Option<&Vec<String>> {
        IMMEDIATE_CHILD_GENRES.get(genre)
    }
}

// Create a static instance for convenience
pub static GENRE_HIERARCHY: GenreHierarchy = GenreHierarchy;

// Secondary genres (placeholder - would be populated from actual data)
pub static SECONDARY_GENRES: Lazy<std::collections::HashSet<&'static str>> = Lazy::new(|| {
    // These are genres that are considered secondary/sub-genres
    // TODO: Load from proper source
    ["psychedelic", "ambient", "minimal", "experimental", "atmospheric"].iter().copied().collect()
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_genre_hierarchy_loads() {
        let _ = crate::testing::init();
        // Test that the lazy statics initialize without panic
        assert!(!TRANSITIVE_PARENT_GENRES.is_empty());
        assert!(!TRANSITIVE_CHILD_GENRES.is_empty());
        assert!(!IMMEDIATE_CHILD_GENRES.is_empty());
    }

    #[test]
    fn test_transitive_parents() {
        let _ = crate::testing::init();
        // Test a known genre relationship from the JSON
        if let Some(parents) = TRANSITIVE_PARENT_GENRES.get("2-Step") {
            assert!(parents.contains(&"Dance".to_string()));
            assert!(parents.contains(&"Electronic".to_string()));
            assert!(parents.contains(&"UK Garage".to_string()));
        }
    }

    #[test]
    fn test_transitive_children() {
        let _ = crate::testing::init();
        // If "Electronic" is a parent of "16-bit", then "16-bit" should be in Electronic's children
        if TRANSITIVE_PARENT_GENRES.get("16-bit").map(|p| p.contains(&"Electronic".to_string())).unwrap_or(false) {
            if let Some(children) = TRANSITIVE_CHILD_GENRES.get("Electronic") {
                assert!(children.contains(&"16-bit".to_string()));
            }
        }
    }

    #[test]
    fn test_immediate_children() {
        let _ = crate::testing::init();
        // Test immediate relationships
        if let Some(parents) = TRANSITIVE_PARENT_GENRES.get("2 Tone") {
            for parent in parents {
                if let Some(children) = IMMEDIATE_CHILD_GENRES.get(parent) {
                    assert!(children.contains(&"2 Tone".to_string()));
                }
            }
        }
    }
}
