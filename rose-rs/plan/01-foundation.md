# Milestone 1: Foundation

## Scope
Core types and utilities used throughout the codebase.

## Components
- Artist type with name and alias flag
- ArtistMapping for organizing artists by role (main, guest, remixer, etc.)
- Error hierarchy matching Python's structure
- Utility functions: UUID validation, filename sanitization, file type detection
- Constants for supported audio/image extensions

## Required Behaviors
- Filename sanitization replaces forbidden characters with underscore
- Special case: "." and ".." become "_" 
- File type checks are case-insensitive
- UUID validation accepts standard format
- Errors maintain context (paths, formats, etc.)

## Functions to Implement
- `common.rs:flatten`
- `common.rs:uniq`
- `common.rs:sanitize_dirname`
- `common.rs:sanitize_filename`
- `common.rs:sha256_dataclass`
- `common.rs:initialize_logging`

## Tests to Implement
Note: common_test.py does not exist in Python, but based on the module's functionality and other test patterns, these tests should be implemented:
- `common_test.rs:test_artist_new`
- `common_test.rs:test_artist_with_alias`
- `common_test.rs:test_artist_mapping_default`
- `common_test.rs:test_flatten`
- `common_test.rs:test_uniq`
- `common_test.rs:test_sanitize_dirname_basic`
- `common_test.rs:test_sanitize_dirname_dots`
- `common_test.rs:test_sanitize_dirname_unicode`
- `common_test.rs:test_sanitize_filename_basic`
- `common_test.rs:test_sanitize_filename_dots`
- `common_test.rs:test_sanitize_filename_unicode`
- `common_test.rs:test_sanitize_filename_maxlen`
- `common_test.rs:test_sha256_dataclass`
- `common_test.rs:test_valid_uuid`
- `common_test.rs:test_invalid_uuid`
- `common_test.rs:test_musicfile`
- `common_test.rs:test_imagefile`
- `common_test.rs:test_error_hierarchy`
- `common_test.rs:test_error_conversion`
- `common_test.rs:test_error_context`

## Python Tests: 0 (no common_test.py exists)
## Minimum Rust Tests: 20
