# Milestone 1: Common Types and Utilities

## Overview
This milestone establishes the foundational data structures and utility functions that all other modules will depend on. It includes core types like `Artist` and `ArtistMapping`, error handling hierarchy, and utility functions for file operations.

## Dependencies
- serde (for serialization)
- thiserror (for error derivation)
- uuid (for UUID validation)

## Implementation Guide

### 1. Artist and ArtistMapping Types (`src/common.rs`)

#### Artist Struct
- Simple struct with `name: String` and `alias: bool` fields
- Implement `new()` constructor that takes a name and defaults alias to false
- Implement `with_alias()` builder method that sets the alias field
- Derive `Debug`, `Clone`, `PartialEq`, `Serialize`, `Deserialize`

#### ArtistMapping Struct
- Contains 7 Vec<Artist> fields for different artist roles:
  - main, guest, remixer, producer, composer, conductor, djmixer
- Implement `Default` trait to create empty vectors for all fields
- Implement `new()` that returns `Self::default()`
- All fields should be public for direct access

### 2. Error Hierarchy

The error system follows Python's hierarchy exactly:
```
RoseError (top level)
├── RoseExpectedError (user-facing errors)
│   ├── ConfigNotFound
│   ├── ConfigDecode
│   ├── InvalidPathTemplate
│   ├── UnsupportedAudioFormat
│   ├── TagNotAllowed
│   └── UnknownArtistRole
└── RoseUnexpectedError (internal errors)
    ├── FileNotFound
    └── Io
```

#### Implementation Notes:
- Use `thiserror` for deriving Error trait
- RoseError should have variants for Base(String), Expected, and Unexpected
- Use `#[error(transparent)]` for Expected and Unexpected variants
- Use `#[from]` to enable automatic conversion
- Each error should store relevant context (paths, format strings, etc.)

### 3. Utility Functions

#### `valid_uuid(s: &str) -> bool`
- Use `uuid::Uuid::parse_str()`
- Return true if parsing succeeds, false otherwise
- Handle empty strings (return false)

#### `sanitize_filename(s: &str) -> String`
- Replace these characters with underscore: `/\:*?"<>|`
- Special case: if entire string is "." or "..", replace with "_"
- Otherwise, leave dots untouched
- Iterate through chars and build new string

#### `musicfile(p: &Path) -> bool`
- Extract extension using `path.extension()`
- Convert to lowercase string
- Check if it's in SUPPORTED_AUDIO_EXTENSIONS
- Return false for no extension

#### `imagefile(p: &Path) -> bool`
- Same pattern as musicfile but check SUPPORTED_IMAGE_EXTENSIONS

### 4. Constants

Define these as public constants:
```rust
pub const SUPPORTED_AUDIO_EXTENSIONS: &[&str] = &["mp3", "m4a", "ogg", "opus", "flac"];
pub const SUPPORTED_IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png"];
```

### 5. Type Alias

Define Result type alias for convenience:
```rust
pub type Result<T> = std::result::Result<T, RoseError>;
```

## Test Implementation Guide (`src/common_test.rs`)

### Artist Tests

#### `test_artist_new`
- Create artist with name "BLACKPINK"
- Assert name equals "BLACKPINK"
- Assert alias is false

#### `test_artist_with_alias`
- Create artist, then call with_alias(true)
- Assert alias is now true
- Verify builder pattern works

#### `test_artist_mapping_new`
- Create new ArtistMapping
- Assert all vectors are empty
- Test Default trait implementation

#### `test_artist_mapping_builder`
- Create ArtistMapping
- Add artists to different role vectors
- Verify they're stored correctly

### UUID Tests

#### `test_valid_uuid`
- Test with valid UUID: "123e4567-e89b-12d3-a456-426614174000"
- Should return true

#### `test_invalid_uuid`
- Test with "not-a-uuid" - should return false
- Test with empty string - should return false
- Test with malformed UUID - should return false

### Filename Sanitization Tests

#### `test_sanitize_filename_basic`
- Test "a/b" -> "a_b"
- Test "a:b" -> "a_b"
- Test all forbidden characters

#### `test_sanitize_filename_dots`
- Test ".." -> "_"
- Test "." -> "_"
- Test "..test" -> "..test" (dots in middle are OK)

#### `test_sanitize_filename_unicode`
- Test Unicode characters are preserved
- Test "hello🎵world" stays unchanged
- Test mixed forbidden and Unicode

### File Type Tests

#### `test_musicfile`
- Test "test.mp3" -> true
- Test "test.FLAC" -> true (case insensitive)
- Test "test.txt" -> false
- Test "test" -> false (no extension)

#### `test_imagefile`
- Test "cover.jpg" -> true
- Test "cover.PNG" -> true (case insensitive)
- Test "cover.txt" -> false

### Error Hierarchy Test

#### `test_error_hierarchy`
- Create each error type
- Verify they can be converted to RoseError
- Test error messages are formatted correctly
- Test that From traits work

## Important Implementation Details

1. **Serialization Compatibility**: The serde derives must match Python's serialization format exactly. Artist and ArtistMapping will be serialized to/from TOML and JSON.

2. **Case Sensitivity**: File extension checks should be case-insensitive.

3. **Path Handling**: Use std::path::Path consistently, not strings.

4. **Error Context**: Always include relevant context in errors (file paths, format names, etc.)

5. **Public API**: All types and functions in this module are public and will be used throughout the codebase.

## Validation Checklist

- [ ] All 12 tests pass
- [ ] Serialization round-trips work (manually test with serde_json)
- [ ] Error messages match Python's format
- [ ] No unwrap() or panic!() in non-test code
- [ ] All public items have appropriate visibility
- [ ] Code follows Rust naming conventions