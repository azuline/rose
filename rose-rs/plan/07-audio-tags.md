# Milestone 7: Audio Tags

## Scope
Unified interface for reading/writing audio metadata across formats.

## Components
- Format detection
- Tag reading for all supported formats
- Tag writing with format-specific handling
- Rose UUID storage in custom fields

## Required Behaviors
- Supports MP3, M4A, FLAC, OGG, OPUS
- Empty strings treated as None
- Track numbers accept multiple formats (1, 01, 01/12)
- Multi-value fields (artists, genres) preserved
- Format-specific tag mappings maintained
- Unknown tags preserved when possible
- Special handling for albumartist

## Functions to Implement
From `audiotags.py`:
- `audiotags.rs:parse_artist_string`
- `audiotags.rs:format_artist_string`
- `audiotags.rs:AudioTags::new` (factory for format)
- `audiotags.rs:AudioTags::get_*` (all getters)
- `audiotags.rs:AudioTags::set_*` (all setters)
- `audiotags.rs:AudioTags::flush`

## Tests to Implement
From `audiotags_test.py`:
- `audiotags_test.rs:test_getters` (parameterized)
- `audiotags_test.rs:test_flush` (parameterized)
- `audiotags_test.rs:test_write_parent_genres`
- `audiotags_test.rs:test_id_assignment` (parameterized)
- `audiotags_test.rs:test_releasetype_normalization` (parameterized)
- `audiotags_test.rs:test_split_tag`
- `audiotags_test.rs:test_parse_artist_string`
- `audiotags_test.rs:test_format_artist_string`

## Python Tests: 8 (plus parameterized tests)
## Minimum Rust Tests: 8