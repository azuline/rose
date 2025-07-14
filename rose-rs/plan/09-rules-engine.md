# Milestone 9: Rules Engine

## Scope
Execute parsed rules against the cache and files.

## Components
- Matcher evaluation against cache
- Action execution
- Multi-value field handling
- Dry-run support
- Confirmation prompts

## Required Behaviors
- Case-sensitive matching for most fields
- Case-insensitive for genres
- Integer fields convert to strings for matching
- Multi-value fields match if ANY value matches
- Sed actions use regex crate
- Split actions handle delimiters correctly
- Triggers playlist regeneration when needed

## Functions to Implement
From `rules.py`:
- `rules.rs:execute_stored_metadata_rules`
- `rules.rs:execute_metadata_rule`
- `rules.rs:fast_search_for_matching_tracks`
- `rules.rs:filter_track_false_positives_using_tags`
- `rules.rs:execute_metadata_actions`
- `rules.rs:value_to_str`
- `rules.rs:matches_pattern`
- `rules.rs:execute_single_action`
- `rules.rs:execute_multi_value_action`
- `rules.rs:fast_search_for_matching_releases`
- `rules.rs:filter_track_false_positives_using_read_cache`
- `rules.rs:filter_release_false_positives_using_read_cache`

## Tests to Implement (selected from 45 total):
- `rules_test.rs:test_rules_execution_match_substring`
- `rules_test.rs:test_rules_execution_match_beginnning`
- `rules_test.rs:test_rules_execution_match_end`
- `rules_test.rs:test_rules_execution_match_superstrict`
- `rules_test.rs:test_rules_execution_match_case_insensitive`
- `rules_test.rs:test_rules_fields_match_tracktitle`
- `rules_test.rs:test_rules_fields_match_genre`
- `rules_test.rs:test_rules_fields_match_trackartist`
- `rules_test.rs:test_action_replace_with_delimiter`
- `rules_test.rs:test_sed_action`
- `rules_test.rs:test_split_action`
- `rules_test.rs:test_add_action`
- `rules_test.rs:test_delete_action`
- `rules_test.rs:test_preserves_unmatched_multitags`
- `rules_test.rs:test_chained_action`
- `rules_test.rs:test_dry_run`
- `rules_test.rs:test_run_stored_rules`
- `rules_test.rs:test_fast_search_for_matching_releases`
- `rules_test.rs:test_filter_release_false_positives_with_read_cache`
- `rules_test.rs:test_artist_matcher_on_trackartist_only`

## Python Tests: 45
## Minimum Rust Tests: 45