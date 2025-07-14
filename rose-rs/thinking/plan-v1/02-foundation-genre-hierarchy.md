# Milestone 2: Genre Hierarchy

## Overview
This milestone implements the genre classification system, which includes over 24,000 genre names and their hierarchical relationships. The data is generated from Python at build time and embedded into the Rust binary at compile time.

## Dependencies
- serde_json (for parsing the generated JSON)
- lazy_static (for compile-time data initialization)
- Python script for data generation

## Build System Setup

### 1. Create Build Script (`build.rs`)

The build script runs during compilation to generate genre data:

```rust
fn main() {
    todo!() // Implement build script
}
```

#### Build Script Tasks:
1. Get `OUT_DIR` from environment
2. Create output path: `{OUT_DIR}/genres.json`
3. Execute Python script: `python3 scripts/generate_genres.py {output_path}`
4. Check if Python script succeeded
5. Set `cargo:rerun-if-changed=scripts/generate_genres.py`

### 2. Python Script (`scripts/generate_genres.py`)

Port the genre generation logic from rose-py:
- Read genre data from source (likely `rym-genres.txt` or similar)
- Build genre list and parent relationships
- Output as JSON with structure:
```json
{
    "genres": ["K-Pop", "Pop", "Dance-Pop", ...],
    "parents": {
        "K-Pop": ["Pop"],
        "Dance-Pop": ["Pop", "Dance"],
        ...
    }
}
```

## Implementation Guide (`src/genre_hierarchy.rs`)

### 1. Data Loading with lazy_static

```rust
use lazy_static::lazy_static;
use std::collections::{HashMap, HashSet};
use serde_json::Value;

lazy_static! {
    static ref GENRE_DATA: Value = {
        todo!() // Load and parse JSON
    };
    
    pub static ref GENRES: Vec<&'static str> = {
        todo!() // Extract genres array from GENRE_DATA
    };
    
    pub static ref GENRE_PARENTS: HashMap<&'static str, Vec<&'static str>> = {
        todo!() // Extract parents mapping from GENRE_DATA
    };
}
```

#### Implementation Steps:

1. **Load JSON at compile time**:
   - Use `include_str!(concat!(env!("OUT_DIR"), "/genres.json"))`
   - This embeds the JSON as a string in the binary

2. **Parse JSON into Value**:
   - Use `serde_json::from_str()`
   - Use `.expect()` since this is compile-time - failures should halt compilation

3. **Extract genres array**:
   - Access `GENRE_DATA["genres"]` as array
   - Convert each item to `&'static str`
   - Use `.expect()` liberally - malformed data should fail at compile time

4. **Extract parents mapping**:
   - Access `GENRE_DATA["parents"]` as object
   - Iterate over key-value pairs
   - Convert values (arrays) to `Vec<&'static str>`
   - Build HashMap

### 2. Public Functions

#### `genre_exists(genre: &str) -> bool`
- Simple linear search through GENRES vector
- Consider using `GENRES.iter().any(|&g| g == genre)`
- Case-sensitive comparison (genres are case-sensitive in rose)

#### `get_all_parents(genre: &str) -> Vec<&str>`
- Implement recursive parent resolution
- Use a work queue (Vec) and visited set (HashSet) to avoid cycles
- Algorithm:
  1. Initialize empty parents vector and work queue with input genre
  2. While work queue not empty:
     - Pop genre from queue
     - Skip if already visited
     - Add to visited set
     - Look up direct parents in GENRE_PARENTS
     - Add new parents to result and queue
  3. Sort and deduplicate results
  4. Return parents vector

## Test Implementation Guide (`src/genre_hierarchy_test.rs`)

### `test_genres_loaded`
- Assert GENRES.len() > 20000
- Verify specific genres exist: "K-Pop", "Dance-Pop", "Electronic"
- This tests that build process worked

### `test_genre_parents`
- Test known relationships:
  - "K-Pop" has parent "Pop"
  - "Dance-Pop" has parents including "Pop" and "Dance"
- Verify GENRE_PARENTS contains expected entries

### `test_genre_exists`
- Test existing genres return true
- Test non-existent genres return false
- Test case sensitivity

### `test_get_all_parents`
- Test single-level parents (K-Pop -> Pop)
- Test multi-level parents (traverse hierarchy)
- Test genres with no parents return empty vec
- Test cycle handling (if any exist in data)

## Important Implementation Details

1. **Memory Efficiency**: The lazy_static data is shared and immutable. Using `&'static str` avoids allocations.

2. **Build-Time Validation**: The build script should fail if Python script fails, preventing bad builds.

3. **JSON Structure**: Must match exact structure expected by Rust code.

4. **Performance**: Linear search is OK for genre_exists since it's not a hot path. Could optimize with HashSet if needed.

5. **Case Sensitivity**: Genres are case-sensitive. "K-Pop" != "k-pop".

6. **Static Lifetimes**: All string slices are `'static` since data is embedded in binary.

## Error Handling

Build script errors should panic with clear messages:
- "Failed to run genre generation script"
- "Genre generation script failed with: {error}"
- "Failed to create output directory"

Runtime errors should use expect() in lazy_static blocks:
- "Failed to parse genre JSON data"
- "Genre data missing 'genres' array"
- "Genre data missing 'parents' object"

## Performance Considerations

1. **Compile Time**: JSON parsing happens once at program start
2. **Runtime Lookup**: O(n) for genre_exists, O(p*d) for get_all_parents where p=parents, d=depth
3. **Memory**: Approximately 1-2 MB for all genre data

## Validation Checklist

- [ ] Build script runs successfully
- [ ] Generated JSON is valid
- [ ] All 4 tests pass
- [ ] No runtime allocations in hot paths
- [ ] Genre count matches Python (24,000+)
- [ ] Parent relationships match Python exactly