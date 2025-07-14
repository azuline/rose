# Milestone 3: Configuration

## Scope
TOML configuration loading and validation.

## Components
- Config file discovery (XDG/platform-specific)
- Schema validation
- Default values
- Path templates
- Artist aliases

## Required Behaviors
- Creates config directory if missing
- Platform-specific paths (XDG_CONFIG_HOME, ~/Library, %APPDATA%)
- Environment variable expansion in paths
- Validates artist aliases reference real artists
- Validates UUID formats
- Invalid template patterns raise errors

## Functions to Implement
Note: config.py uses class methods. In Rust implement as:
- `config.rs:Config::new` (constructor)
- `config.rs:Config::default`
- `config.rs:Config::parse_file`
- `config.rs:Config::validate`
- `config.rs:Config::expand_paths`
- `config.rs:Config::get_config_path`

## Tests to Implement
From `config_test.py`:
- `config_test.rs:test_config_minimal`
- `config_test.rs:test_config_full`
- `config_test.rs:test_config_whitelist`
- `config_test.rs:test_config_not_found`
- `config_test.rs:test_config_missing_key_validation`
- `config_test.rs:test_config_value_validation`
- `config_test.rs:test_vfs_config_value_validation`

## Python Tests: 7
## Minimum Rust Tests: 7