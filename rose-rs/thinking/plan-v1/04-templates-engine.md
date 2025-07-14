# Milestone 4: Templates Engine

## Overview
This milestone implements the path templating system using Tera (Jinja2-like template engine). Templates are used to generate file paths for releases and tracks based on their metadata.

## Dependencies
- tera (Jinja2-compatible template engine)
- lazy_static (for template engine initialization)
- regex (for template validation if needed)

## Implementation Guide (`src/templates.rs`)

### 1. Template Engine Setup

```rust
use lazy_static::lazy_static;
use tera::{Context, Tera};

lazy_static! {
    static ref TEMPLATE_ENGINE: Tera = {
        todo!() // Initialize Tera engine
    };
}
```

Setup steps:
1. Create new Tera instance with `Tera::default()`
2. Register custom filter "sanitize" using `tera.register_filter()`
3. Return configured engine

### 2. Release Template Execution

```rust
pub fn execute_release_template(config: &Config, release: &CachedRelease) -> Result<String> {
    todo!()
}
```

Implementation steps:

1. **Create Tera context**:
   ```
   let mut context = Context::new();
   ```

2. **Add release fields to context**:
   - `album` -> release.title
   - `release_year` -> release.release_year.unwrap_or(0)
   - `release_type` -> release.release_type (or empty string if None)
   - `multi_disc_flag` -> config.multi_disc_toggle_flag
   - `catalog_number` -> release.catalog_number (or empty string)

3. **Add artist information**:
   - If release has main artists:
     - Join artist names with "; "
     - Add as `albumartist` in context
   - Handle other artist roles similarly if needed

4. **Add label information**:
   - Join labels with "; " if multiple
   - Add as `label` in context

5. **Add genre information**:
   - First genre as `genre` (or empty string)
   - All genres joined as `genres`

6. **Render template**:
   ```rust
   TEMPLATE_ENGINE.render_str(&config.path_templates.release, &context)
   ```

7. **Handle errors**:
   - Convert Tera errors to `InvalidPathTemplate` error

8. **Sanitize output**:
   - Call `sanitize_filename()` on the result
   - This ensures no invalid path characters

### 3. Track Template Execution

```rust
pub fn execute_track_template(config: &Config, track: &CachedTrack) -> Result<String> {
    todo!()
}
```

Implementation steps:

1. **Create context**:
   ```
   let mut context = Context::new();
   ```

2. **Add track fields**:
   - `track_number` -> track.track_number
   - `title` -> track.title
   - `disc_number` -> track.disc_number
   - `duration` -> track.duration_seconds (or 0)

3. **Add artist information**:
   - Main artists joined with "; " as `artist`
   - Other roles if needed

4. **Render and sanitize**:
   - Same pattern as release template

### 4. Custom Filter Implementation

```rust
fn filter_sanitize(
    value: &tera::Value, 
    _: &HashMap<String, tera::Value>
) -> tera::Result<tera::Value> {
    todo!()
}
```

Implementation:
1. Check if value is a string using `value.as_str()`
2. If yes, call `sanitize_filename()` from common module
3. Return sanitized string as `tera::Value::String`
4. If not a string, return value unchanged

## Test Implementation Guide (`src/templates_test.rs`)

### `test_default_templates`

Test the default templates with typical data:

1. **Setup**:
   - Create config with default templates
   - Create test release with:
     - title: "Test Album"
     - release_year: Some(2023)
     - release_type: Some("album")
   - Create test track with:
     - track_number: "01"
     - title: "Test Track"

2. **Release template test**:
   - Execute release template
   - Expected: "[2023] Test AlbumDEFAULT_MULTI_DISC"
   - Note: No space between album and flag

3. **Track template test**:
   - Execute track template
   - Expected: "01. Test Track"

### `test_classical`

Test templates with classical music metadata patterns:

1. **Setup**:
   - Create release with:
     - Multiple composers in artists
     - Classical-style album name
     - Catalog number

2. **Test special fields**:
   - Verify composer information is accessible
   - Test catalog number in templates
   - Test multi-artist handling

## Template Variables Reference

### Release Templates

Available variables:
- `album` - Release title
- `release_year` - Year (0 if missing)
- `release_type` - Type (album, single, etc.)
- `albumartist` - Main artists joined with "; "
- `label` - Labels joined with "; "
- `genre` - First genre
- `genres` - All genres joined
- `catalog_number` - Catalog number
- `multi_disc_flag` - From config

### Track Templates

Available variables:
- `track_number` - Track number (may include leading zeros)
- `title` - Track title
- `disc_number` - Disc number
- `artist` - Track artists joined with "; "
- `duration` - Duration in seconds

## Important Implementation Details

1. **Default Values**: Use unwrap_or with sensible defaults for Option fields

2. **Path Safety**: Always sanitize final output to prevent directory traversal

3. **Template Syntax**: Tera uses Jinja2 syntax: `{{ variable }}`

4. **Error Context**: Include template string in error messages

5. **Performance**: Template engine is initialized once via lazy_static

6. **Character Encoding**: Ensure UTF-8 handling throughout

## Common Issues and Solutions

1. **Missing Variables**: Tera fails if variable doesn't exist
   - Solution: Add all variables even if empty

2. **Invalid Characters**: Some metadata may contain path separators
   - Solution: Sanitize all output

3. **Template Syntax Errors**: Invalid template syntax
   - Solution: Validate templates during config loading (future)

## Validation Checklist

- [ ] Both tests pass
- [ ] Templates match Python output exactly
- [ ] Sanitization prevents path traversal
- [ ] Missing metadata doesn't crash
- [ ] Custom filter works correctly
- [ ] Error messages are helpful