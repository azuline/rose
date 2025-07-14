# Milestone 3: Configuration System

## Overview
This milestone implements the configuration loading and validation system. Rose uses TOML configuration files with specific validation rules, default values, and computed fields like artist alias mappings.

## Dependencies
- toml (for parsing configuration files)
- serde (for deserialization)
- home (for expanding ~ in paths)
- std::collections::HashMap (for artist aliases)

## Implementation Guide (`src/config.rs`)

### 1. Configuration Structures

#### Main Config Struct
```rust
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub music_source_dir: PathBuf,
    
    #[serde(default = "default_cache_dir")]
    pub cache_dir: PathBuf,
    
    #[serde(default = "default_max_proc")]
    pub max_proc: usize,
    
    #[serde(default)]
    pub artist_aliases: Vec<ArtistAlias>,
    
    #[serde(default)]
    pub rules: Vec<StoredRule>,
    
    #[serde(default)]
    pub path_templates: PathTemplates,
    
    #[serde(default)]
    pub cover_art_regexes: Vec<String>,
    
    #[serde(default = "default_multi_disc_flag")]
    pub multi_disc_toggle_flag: String,
    
    #[serde(skip)]
    pub artist_aliases_map: HashMap<String, String>,
}
```

Key implementation details:
- `music_source_dir` is the only required field
- All other fields have defaults via serde attributes
- `artist_aliases_map` is computed from `artist_aliases` during validation

#### Supporting Structures

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ArtistAlias {
    pub artist: String,
    pub alias: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StoredRule {
    pub name: String,
    pub rule: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PathTemplates {
    #[serde(default = "default_release_template")]
    pub release: String,
    
    #[serde(default = "default_track_template")]
    pub track: String,
    
    #[serde(default = "default_all_pattern")]
    pub all_patterns: String,
}
```

### 2. Configuration Loading (`Config::parse`)

The main entry point takes an optional path override:

```rust
impl Config {
    pub fn parse(config_path_override: Option<&Path>) -> Result<Self> {
        todo!()
    }
}
```

Loading steps:
1. Determine config path:
   - If override provided, use it
   - Otherwise call `find_config_path()`
2. Read file to string
   - Return `ConfigNotFound` error if file doesn't exist
3. Parse TOML
   - Return `ConfigDecode` error with toml error message if parsing fails
4. Process the config:
   - Call `validate_and_process()` on the parsed config
5. Return processed config

### 3. Configuration Discovery (`find_config_path`)

```rust
fn find_config_path() -> Result<PathBuf> {
    todo!()
}
```

Search order (Linux):
1. Check `XDG_CONFIG_HOME` environment variable
   - If set, use `{XDG_CONFIG_HOME}/rose/config.toml`
2. Otherwise check home directory
   - Use `~/.config/rose/config.toml`

Search order (macOS):
1. Use `~/Library/Preferences/rose/config.toml`

Return `ConfigNotFound` error with expected path if not found.

### 4. Path Expansion (`expand_home`)

```rust
fn expand_home(path: &Path) -> PathBuf {
    todo!()
}
```

Implementation:
1. Check if path starts with "~"
2. If yes, strip "~" prefix and prepend home directory
3. Use `home::home_dir()` to get home directory
4. If no home dir found, return path unchanged

### 5. Configuration Validation (`validate_and_process`)

```rust
impl Config {
    fn validate_and_process(&mut self) -> Result<()> {
        todo!()
    }
}
```

Processing steps:
1. Expand ~ in all path fields:
   - `music_source_dir`
   - `cache_dir`
2. Validate and build artist aliases map:
   - Call `validate_artist_aliases()`
   - Store result in `artist_aliases_map`

### 6. Artist Alias Validation (`validate_artist_aliases`)

```rust
fn validate_artist_aliases(aliases: &[ArtistAlias]) -> Result<HashMap<String, String>> {
    todo!()
}
```

Validation rules:
1. No self-aliases: alias cannot equal artist
   - Return `ConfigDecode` error: "Artist alias cannot resolve to itself: {artist}"
2. No duplicate aliases: each alias can only point to one artist
   - Return `ConfigDecode` error: "Duplicate alias '{alias}' for artists '{artist1}' and '{artist2}'"
3. Build HashMap: alias -> artist

### 7. Default Functions

Implement these default value functions:
- `default_cache_dir()` -> `~/.cache/rose` (expand ~)
- `default_max_proc()` -> 4
- `default_multi_disc_flag()` -> "DEFAULT_MULTI_DISC"
- `default_release_template()` -> "[{release_year}] {album}{multi_disc_flag}"
- `default_track_template()` -> "{track_number}. {title}"
- `default_all_pattern()` -> "{source_dir}/{release}/{track}"

### 8. Default Trait Implementation

```rust
impl Default for Config {
    fn default() -> Self {
        todo!()
    }
}
```

Create config with all default values and empty music_source_dir.

## Test Implementation Guide (`src/config_test.rs`)

### `test_config_minimal`
- Create TOML with only `music_source_dir = "~/music"`
- Parse it
- Verify all defaults are applied correctly
- Verify ~ is expanded in music_source_dir

### `test_config_full`
- Create TOML with all fields populated:
  ```toml
  music_source_dir = "~/.music-source"
  cache_dir = "~/.cache/rose"
  max_proc = 8
  multi_disc_toggle_flag = "MULTIDISC"
  
  [[artist_aliases]]
  artist = "Blackpink"
  alias = "BLACKPINK"
  
  [[rules]]
  name = "fix-kpop"
  rule = "genre:K-Pop genre:='K-Pop'"
  
  [path_templates]
  release = "[{release_year}] {album}"
  track = "{track_number}. {title}"
  
  cover_art_regexes = ["cover", "folder", "album"]
  ```
- Verify all fields are parsed correctly
- Verify artist_aliases_map contains "BLACKPINK" -> "Blackpink"

### `test_config_whitelist`
- Test that unknown fields are ignored (forward compatibility)
- Add extra fields to TOML and ensure parsing still works

### `test_config_not_found`
- Call parse with non-existent path
- Verify `ConfigNotFound` error is returned
- Check error message contains the path

### `test_config_missing_key_validation`
- Create TOML missing `music_source_dir`
- Verify parsing fails with appropriate error

### `test_config_value_validation`
- Test various validation failures:
  - Self-referencing artist alias
  - Invalid path templates (if any validation exists)

### `test_vfs_config_value_validation`
- Test VFS-specific configuration if present
- May be empty if VFS not implemented

## Important Implementation Details

1. **Path Handling**: Always expand ~ after parsing, not during serialization

2. **Error Messages**: Include context in all errors (paths, field names, etc.)

3. **Forward Compatibility**: Unknown fields should be ignored

4. **Atomicity**: Validation should be all-or-nothing

5. **Immutability**: Config should be immutable after creation (except for tests)

## Validation Checklist

- [ ] All 7 tests pass
- [ ] Configuration round-trips through TOML
- [ ] ~ expansion works on all platforms
- [ ] Artist alias validation catches all error cases
- [ ] Default values match Python exactly
- [ ] Unknown fields are ignored