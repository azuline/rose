# Milestone 5: Datafiles

## Scope
Release metadata persistence in `.rose.{uuid}.toml` files.

## Components
- Datafile reading/writing
- Field validation
- Automatic upgrades
- Transaction safety

## Required Behaviors
- Filename format: `.rose.{uuid}.toml`
- Creates new UUID if missing
- Upgrades when required fields missing
- Preserves unknown fields
- Handles missing/corrupt files gracefully
- All fields optional except UUID

## Functions to Implement
Note: Datafile handling is embedded in releases.py and cache.py. Extract to dedicated module:
- `datafiles.rs:find_release_datafile`
- `datafiles.rs:read_datafile`
- `datafiles.rs:write_datafile`
- `datafiles.rs:create_datafile`
- `datafiles.rs:upgrade_datafile`
- `datafiles.rs:validate_datafile`

## Tests to Implement
Based on releases_test.py and cache_test.py datafile tests:
- `datafiles_test.rs:test_find_datafile_by_pattern`
- `datafiles_test.rs:test_read_valid_datafile`
- `datafiles_test.rs:test_read_missing_datafile`
- `datafiles_test.rs:test_read_corrupt_datafile`
- `datafiles_test.rs:test_create_new_datafile`
- `datafiles_test.rs:test_write_datafile`
- `datafiles_test.rs:test_upgrade_missing_fields`
- `datafiles_test.rs:test_preserve_unknown_fields`
- `datafiles_test.rs:test_uuid_validation`
- `datafiles_test.rs:test_filename_format`

## Critical: This module is missing from consultant's plan but essential

## Python Tests: ~15 (embedded in other modules)
## Minimum Rust Tests: 10