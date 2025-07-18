# Comparison Report: Rules Unit Tests - Rust vs Python

This report compares the unit tests for the rules module between the Rust implementation (`src/rules.rs`) and the Python reference implementation (`py-impl-reference/rose/rules_test.py`).

## Test Coverage Summary

Both implementations provide comprehensive test coverage for the rules engine functionality, with tests for:

- Pattern matching (substring, beginning, end, superstrict, case-insensitive)
- Field matching (various metadata fields)
- Actions (replace, sed, split, add, delete)
- Fast search functionality
- False positive filtering
- Ignore values
- Dry run mode

## Key Differences

### 1. Test Data Setup

**Python:**

- Uses fixtures from `config: Config` and `source_dir: Path` parameters
- Test data setup appears to be in external fixture files
- Uses `AudioTags.from_file()` directly with paths

**Rust:**

- Uses `testing::seeded_cache()` and `testing::source_dir()` helper functions
- Explicitly manages test data within the tests
- Returns tuples like `(config, _tmpdir)`

### 2. Missing Tests in Rust

The following tests exist in Python but are missing from the Rust implementation:

1. **`test_rules_fields_match_originaldate`** - Tests matching on `originaldate` field
2. **`test_rules_fields_match_compositiondate`** - Tests matching on `compositiondate` field
3. **`test_rules_fields_match_edition`** - Tests matching on `edition` field
4. **`test_rules_fields_match_catalognumber`** - Tests matching on `catalognumber` field
5. **`test_rules_fields_match_tracktotal`** - Tests matching on `tracktotal` field with tag-specific action
6. **`test_rules_fields_match_disctotal`** - Tests matching on `disctotal` field with tag-specific action
7. **`test_match_backslash`** - Tests escaping backslashes in patterns
8. **`test_action_replace_with_delimiter`** - Tests replace action with semicolon delimiters
9. **`test_action_replace_with_delimiters_empty_str`** - Tests handling empty strings in delimited replacements
10. **`test_sed_no_pattern`** - Tests sed action with matched modifier
11. **`test_split_action_no_pattern`** - Tests split action with matched modifier
12. **`test_delete_action_no_pattern`** - Tests delete action with matched modifier
13. **`test_preserves_unmatched_multitags`** - Tests that unmatched tags in multi-value fields are preserved
14. **`test_action_on_different_tag`** - Tests applying action to a different tag than the match
15. **`test_action_no_pattern`** - Tests action with matched modifier
16. **`test_chained_action`** - Tests multiple chained actions
17. **`test_confirmation_yes`**, **`test_confirmation_no`**, **`test_confirmation_count`** - Interactive confirmation tests

### 3. Additional Tests in Rust

The Rust implementation includes these tests not found in Python:

1. **`test_action_multi_genre`** - Tests adding multiple genres with semicolon separation
2. **`test_action_descriptor`** - Tests matching and modifying descriptor fields
3. **`test_release_title_update`** - Tests updating release title at the release level
4. **`test_artist_role_split`** - Tests splitting artists with specific roles

### 4. API Differences

**Python:**

- `execute_metadata_rule(config, rule, confirm_yes=False)`
- `Rule.parse(matcher, actions)` for parsing rules
- Direct field assertions: `assert af.tracktitle == "lalala"`

**Rust:**

- `execute_metadata_rule(&config, &rule, false, false, 25).unwrap()`
- Additional parameters for dry_run and confirmation threshold
- Uses `unwrap()` for error handling
- Field assertions with `Option` types: `assert_eq!(af.tracktitle, Some("lalala".to_string()))`

### 5. Test Organization

**Python:**

- Uses pytest decorators like `@pytest.mark.usefixtures("seeded_cache")`
- `@pytest.mark.timeout(2)` for timeout handling
- Cleaner test function signatures with dependency injection

**Rust:**

- Manual test data setup with helper functions
- More verbose with explicit `unwrap()` calls
- No timeout annotations (likely handled differently)

### 6. Mock/Stub Differences

**Python:**

- Uses `monkeypatch` for mocking user input in confirmation tests
- Can mock `click.confirm` and `click.prompt`

**Rust:**

- No interactive confirmation tests implemented
- Missing the interactive user input testing capability

### 7. Error Type Differences

**Python:**

- Uses custom `TrackTagNotAllowedError` exception

**Rust:**

- Returns `Result` types with error variants
- Uses `.is_err()` for error checking

## Recommendations

1. **Add Missing Tests to Rust:** The Rust implementation should add the 17 missing tests to achieve feature parity with Python.

2. **Interactive Confirmation Tests:** Consider implementing interactive confirmation tests in Rust, possibly using a mock input mechanism.

3. **Test Helper Consistency:** The Rust test helpers (`testing::seeded_cache()`) could be documented to clarify what test data they provide.

4. **Error Message Testing:** Both implementations could benefit from testing specific error messages, not just that errors occur.

5. **Timeout Handling:** Consider adding timeout mechanisms to Rust tests that might hang.

## Conclusion

While both implementations cover the core functionality well, the Python test suite is more comprehensive with 17 additional test cases. The Rust implementation compensates with 4 unique tests focusing on multi-value operations and release-level updates. The main gaps in Rust are around edge cases (empty strings, backslash handling), action modifiers (matched:/, tag-specific actions), and interactive confirmation flows.
