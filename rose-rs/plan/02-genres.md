# Milestone 2: Genre Hierarchy

## Scope
Hierarchical genre system with parent/child relationships.

## Components
- Genre validation against known set
- Parent genre lookups
- Genre hierarchy traversal
- Build-time data generation from Python

## Required Behaviors
- Case-insensitive genre matching
- Returns all parent genres in hierarchy
- "Dance" has special parent relationships
- Unknown genres return None for parents
- ~24,000 genres from Python source

## Functions to Implement
Note: genre_hierarchy.py only contains data (TRANSITIVE_PARENT_GENRES dict), no functions. In Rust:
- `genre_hierarchy.rs:get_parent_genres`
- `genre_hierarchy.rs:is_valid_genre`

## Tests to Implement
Note: No genre_hierarchy_test.py exists in Python. Based on cache.py usage patterns, implement:
- `genre_hierarchy_test.rs:test_valid_genre`
- `genre_hierarchy_test.rs:test_invalid_genre`
- `genre_hierarchy_test.rs:test_case_insensitive_genre`
- `genre_hierarchy_test.rs:test_parent_genres_single_level`
- `genre_hierarchy_test.rs:test_parent_genres_multi_level`
- `genre_hierarchy_test.rs:test_parent_genres_dance_special_case`
- `genre_hierarchy_test.rs:test_parent_genres_unknown`
- `genre_hierarchy_test.rs:test_transitive_parent_closure`

## Python Tests: 0 (no genre_hierarchy_test.py exists)
## Minimum Rust Tests: 8