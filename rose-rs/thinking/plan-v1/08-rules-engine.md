# Milestone 8: Rules Engine

## Overview
This milestone implements the rule execution engine that applies metadata transformations. It uses the parsed rules from the rule parser to modify tags in the cache and audio files.

## Dependencies
- regex (already used by rule parser)
- Previous modules: rule_parser, cache, audiotags

## Architecture Overview

The rules engine has two main phases:
1. **Matching Phase**: Use SQL and FTS to find candidates quickly
2. **Action Phase**: Apply actions to matched items and write changes

## Implementation Guide (`src/rules.rs`)

### 1. Main Execution Function

```rust
pub fn execute_rule(config: &Config, rule_str: &str) -> Result<()> {
    todo!()
}
```

Execution flow:
1. Parse rule string into (matcher, actions)
2. Find matching releases and tracks
3. Apply actions to each match
4. Update cache
5. Write changes to audio files
6. Handle confirmation prompts if needed

### 2. Fast Search Functions

```rust
pub fn fast_search_for_matching_releases(
    config: &Config,
    matcher: &Matcher,
) -> Result<Vec<String>> {
    todo!()
}

pub fn fast_search_for_matching_tracks(
    config: &Config,
    matcher: &Matcher,
) -> Result<Vec<String>> {
    todo!()
}
```

Search strategy:
1. Convert matcher to SQL/FTS query
2. Use full-text search for text matches
3. Get candidate IDs
4. Post-filter in Rust for exact matching (FTS is fuzzy)

### 3. Matcher to SQL Conversion

Convert matchers to SQL WHERE clauses:

- `Tag { field: "artist", pattern: Exact("BLACKPINK") }`
  → FTS query for "BLACKPINK" in artist fields

- `Tag { field: "year", pattern: Exact("2023") }`
  → `release_year = 2023`

- `And(matcher1, matcher2)`
  → Combine with AND

- `Or(matcher1, matcher2)`
  → Combine with OR

- `Not(matcher)`
  → Use NOT

### 4. Action Application

```rust
fn apply_action_to_release(
    config: &Config,
    release_id: &str,
    action: &Action,
) -> Result<()> {
    todo!()
}

fn apply_action_to_track(
    config: &Config,
    track_id: &str,
    action: &Action,
) -> Result<()> {
    todo!()
}
```

Action implementations:

#### Replace Action
```
field:='new value'
```
- Replace entire field value
- Handle multi-value fields (artists, genres)

#### Add Action
```
field+:'value'
```
- Add to multi-value field
- Avoid duplicates

#### Delete Action
```
field:=''
```
- Clear field value
- For multi-value, remove specific value

#### Delete Tag Action
```
field:
```
- Remove entire tag

#### Split Action
```
field/'delimiter'
```
- Split single value into multiple
- Useful for artists like "A & B" → ["A", "B"]

#### Sed Action
```
field/find/replace/flags
```
- Regex find and replace
- Handle flags: g (global), i (case insensitive)
- Apply to each value in multi-value fields

### 5. Field Mapping

Map rule field names to database/tag fields:

- `title` → track title
- `album` → release title
- `artist` → track artists
- `albumartist` → release artists
- `date` → release year
- `tracknumber` → track number
- `discnumber` → disc number
- `genre` → genres (multi-value)
- `label` → labels (multi-value)
- `new` → release new flag

### 6. Special Field Handling

#### Artist Fields
- Support role prefixes: `artist.main`, `artist.composer`, etc.
- Without prefix, default to main artists
- Handle both track and release artists

#### Genre Fields
- When adding genres, also add parent genres
- Use genre hierarchy from genre_hierarchy module

#### Date Fields
- Accept various formats: "2023", "2023-01-01"
- Extract year for release_year field

### 7. Confirmation and Dry Run

```rust
pub fn execute_rule_with_confirmation(
    config: &Config,
    rule_str: &str,
    dry_run: bool,
    confirm_fn: impl Fn(usize) -> bool,
) -> Result<()> {
    todo!()
}
```

Features:
- Show number of matches before applying
- Dry run shows what would change
- Confirmation callback for interactive use

### 8. Stored Rules

```rust
pub fn execute_stored_rule(
    config: &Config,
    rule_name: &str,
) -> Result<()> {
    todo!()
}
```

Look up rule by name in config and execute it.

## Test Implementation Guide (`src/rules_test.rs`)

### Matching Tests (6 tests)

Test different match patterns:
- `test_rules_execution_match_substring` - Match anywhere
- `test_rules_execution_match_beginning` - Match start (^)
- `test_rules_execution_match_end` - Match end ($)
- `test_rules_execution_match_superstrict` - Exact match (^$)
- `test_rules_execution_match_escaped_superstrict` - Escaped anchors
- `test_rules_execution_match_case_insensitive` - Case handling

### Field Matching Tests (13 tests)

Test each field can be matched:
- `test_rules_fields_match_tracktitle`
- `test_rules_fields_match_releasedate`
- `test_rules_fields_match_releasetype`
- `test_rules_fields_match_tracknumber`
- etc.

### Action Tests (15 tests)

Test each action type:
- `test_action_replace_with_delimiter`
- `test_action_replace_with_delimiters_empty_str`
- `test_sed_action`
- `test_sed_no_pattern`
- `test_split_action`
- `test_add_action`
- `test_delete_action`
- etc.

### Advanced Tests (13 tests)

Test complex scenarios:
- `test_preserves_unmatched_multitags`
- `test_action_on_different_tag`
- `test_chained_action`
- `test_confirmation_yes`
- `test_confirmation_no`
- `test_dry_run`
- `test_ignore_values`
- etc.

## Important Implementation Details

### 1. Transaction Safety
- All changes in a single transaction
- Rollback on any error
- Consistent state always

### 2. Multi-Value Field Handling
- Artists, genres, labels are multi-value
- Actions apply to all values or specific values
- Preserve order when possible

### 3. Performance Optimization
- Use FTS for initial filtering
- Batch database updates
- Only write changed audio files

### 4. Pattern Matching Accuracy
- FTS is fuzzy, post-filter for exact matches
- Handle special characters correctly
- Anchor patterns (^$) need special handling

### 5. Error Recovery
- Show which items failed
- Continue processing other items
- Detailed error messages

### 6. Change Tracking
- Track what changed for dry-run output
- Show before/after values
- Count of affected items

## SQL Query Examples

Finding tracks with artist "BLACKPINK":
```sql
SELECT t.id 
FROM tracks t
JOIN tracks_fts f ON t.id = f.id
WHERE tracks_fts MATCH 'BLACKPINK'
AND t.id IN (
    SELECT track_id 
    FROM tracks_artists 
    WHERE artist = 'BLACKPINK'
)
```

## Validation Checklist

- [ ] All 47 tests pass
- [ ] Actions modify both cache and files
- [ ] Multi-value fields handled correctly
- [ ] Dry run doesn't modify anything
- [ ] Performance acceptable on large libraries
- [ ] Error messages are helpful