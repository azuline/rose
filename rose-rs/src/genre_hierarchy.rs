use std::collections::HashMap;
use once_cell::sync::Lazy;
use serde_json::Value;

// Load genre hierarchy from JSON at compile time
const GENRE_HIERARCHY_JSON: &str = include_str!("genre_hierarchy.json");

pub static TRANSITIVE_PARENT_GENRES: Lazy<HashMap<String, Vec<String>>> = Lazy::new(|| {
    let hierarchy: HashMap<String, Vec<String>> = serde_json::from_str(GENRE_HIERARCHY_JSON)
        .expect("Failed to parse genre_hierarchy.json");
    
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

pub static TRANSITIVE_CHILD_GENRES: Lazy<HashMap<String, Vec<String>>> = Lazy::new(|| {
    let mut transitive_children: HashMap<String, Vec<String>> = HashMap::new();
    
    // Build from transitive parents
    for (child, parents) in TRANSITIVE_PARENT_GENRES.iter() {
        for parent in parents {
            transitive_children
                .entry(parent.clone())
                .or_insert_with(Vec::new)
                .push(child.clone());
        }
    }
    
    transitive_children
});

pub static IMMEDIATE_CHILD_GENRES: Lazy<HashMap<String, Vec<String>>> = Lazy::new(|| {
    let hierarchy: HashMap<String, Vec<String>> = serde_json::from_str(GENRE_HIERARCHY_JSON)
        .expect("Failed to parse genre_hierarchy.json");
    
    let mut immediate_children: HashMap<String, Vec<String>> = HashMap::new();
    
    // Build immediate child relationships
    for (child, parents) in &hierarchy {
        for parent in parents {
            immediate_children
                .entry(parent.clone())
                .or_insert_with(Vec::new)
                .push(child.clone());
        }
    }
    
    immediate_children
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_genre_hierarchy_loads() {
        // Test that the lazy statics initialize without panic
        assert!(TRANSITIVE_PARENT_GENRES.len() > 0);
        assert!(TRANSITIVE_CHILD_GENRES.len() > 0);
        assert!(IMMEDIATE_CHILD_GENRES.len() > 0);
    }

    #[test]
    fn test_transitive_parents() {
        // Test a known genre relationship from the JSON
        if let Some(parents) = TRANSITIVE_PARENT_GENRES.get("2-Step") {
            assert!(parents.contains(&"Dance".to_string()));
            assert!(parents.contains(&"Electronic".to_string()));
            assert!(parents.contains(&"UK Garage".to_string()));
        }
    }

    #[test]
    fn test_transitive_children() {
        // If "Electronic" is a parent of "16-bit", then "16-bit" should be in Electronic's children
        if TRANSITIVE_PARENT_GENRES.get("16-bit").map(|p| p.contains(&"Electronic".to_string())).unwrap_or(false) {
            if let Some(children) = TRANSITIVE_CHILD_GENRES.get("Electronic") {
                assert!(children.contains(&"16-bit".to_string()));
            }
        }
    }

    #[test]
    fn test_immediate_children() {
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