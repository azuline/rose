# Rating Feature — Technical Design

## Problem Statement

Add a numeric rating (1-100, nullable) to releases, stored in `.rose.{uuid}.toml` files and cached
in SQLite, exposed to templates, the rule engine, CLI, metadata editor, and JSON dump.

## Data Modeling

### Database Changes

**`releases` table** — Add column:
```sql
rating INTEGER
```
No DEFAULT (NULL = unrated). Add index:
```sql
CREATE INDEX releases_rating ON releases(rating);
```

**FTS5 table** (`rules_engine_fts`) — Add column:
```sql
, rating
```

**Views** (`releases_view`) — Add to SELECT:
```sql
, r.rating
```

### Python Dataclasses

**`StoredDataFile`** (cache.py:326) — Add field:
```python
rating: int | None = None
```

**`Release`** (cache.py:213) — Add field after `favorite`:
```python
rating: int | None
```

**`MetadataRelease`** (releases.py:236) — Add field after `favorite`:
```python
rating: int | None
```

### Query Analysis

1. **Filter releases by rating presence**: `AND rating IS NOT NULL` or `AND rating IS NULL`
2. **Filter releases by rating value**: `AND rating = ?`
3. **Update rating**: Write to TOML file, refresh cache (same pattern as favorite toggle)
4. **FTS search**: Rating stored as string in FTS; matched via rule engine pattern

## Data Flow

### Set Rating Flow

1. User runs `rose releases set-rating <release_id> <value>`
2. `set_release_rating(c, release_id, rating)` called
3. Find `.rose.{uuid}.toml` in release directory
4. Acquire lock, read TOML, set `rating` key (or delete if clearing), write TOML
5. Call `update_cache_for_releases()` to refresh SQLite cache
6. Rating propagated to `Release` object → templates, rule engine, dump

### Cache Update Flow (reading rating from disk)

In `_update_cache_for_releases_executor()` (cache.py:670-682):
1. Read TOML file via `tomllib.load()`
2. Construct `StoredDataFile` with `rating=diskdata.get("rating", None)`
3. Copy to `release.rating = datafile.rating`
4. Write resolved data back if changed

### Template Flow

1. `_calc_release_variables()` returns `"rating": release.rating`
2. `_calc_track_variables()` returns `"rating": track.release.rating`
3. Users can use `{% if rating %}[{{ rating }}]{% endif %}` in templates

### Rule Engine Flow

1. Tag `"rating"` registered in `ALL_TAGS` and `MODIFIABLE_TAGS`
2. Matching: `matches_pattern(matcher.pattern, str(release.rating))` — rating converted to string for matching
3. Actions: `execute_single_action(act, str(datafile.rating or ""))` — validates result is integer 1-100 or empty

## UI Changes

### CLI Command

```
rose releases set-rating <release_id_or_path> <rating>
```

Where `<rating>` is an integer 1-100, or `--clear` flag to remove rating.

### Metadata Editor

Rating field appears in the TOML editor between `favorite` and `releasetype`:
```toml
title = "Album Name"
new = true
favorite = false
rating = 85
releasetype = "album"
```

When unrated, serialized as:
```toml
rating = 0
```
And `from_toml()` treats 0 as None (unrated).

### JSON Dump

`release_to_json()` includes `"rating": r.rating` (int or null).
`track_to_json()` includes `"rating": t.release.rating` (int or null).

## Testing Plans

### New Tests
- `test_set_release_rating`: Set rating, verify TOML and cache updated
- `test_set_release_rating_clear`: Clear rating, verify TOML and cache updated

### Modified Tests
- Cache tests: Verify rating field read/written correctly
- Template tests: Verify `rating` variable available
- Rule engine tests: Verify `rating` tag matching and actions
- Metadata editor tests: Verify rating editable

## Files to Change

### rose-py/rose/cache.sql
- Add `rating INTEGER` to `releases` table (line 26)
- Add `CREATE INDEX releases_rating ON releases(rating);` (after line 30)
- Add `rating` column to `rules_engine_fts` (after line 189)
- Add `r.rating` to `releases_view` SELECT (after line 247)

### rose-py/rose/cache.py
- `StoredDataFile` (line 326): Add `rating: int | None = None`
- `Release` (line 228): Add `rating: int | None` after `favorite`
- `cached_release_from_view()` (line 240): Add `rating=row["rating"]`
- Cache update logic (line 672-682): Read `rating` from TOML, propagate to release
- `_update_cache_for_releases_executor` INSERT SQL (line 1041-1075): Add `rating` column
- FTS sync INSERT (line 1221-1265): Add `rating` column
- `filter_releases()` (line 1742): Add `rating` parameter
- `filter_tracks()` (line 1840): Add `rating` parameter

### rose-py/rose/releases.py
- New function `set_release_rating(c, release_id, rating)` (after line 139)
- `MetadataRelease` (line 236): Add `rating: int | None` field
- `MetadataRelease.from_cache()` (line 254): Add `rating=release.rating`
- `MetadataRelease.serialize()` (line 281): Serialize rating (0 = unrated)
- `MetadataRelease.from_toml()` (line 292): Parse rating (0 = None)
- `edit_release()` (line 449-452): Handle rating changes

### rose-py/rose/templates.py
- `_calc_release_variables()` (line 360): Add `"rating": release.rating`
- `_calc_track_variables()` (line 384): Add `"rating": track.release.rating`

### rose-py/rose/rules.py
- `_get_release_datafile_of_directory()` (line 320): Add `rating` to StoredDataFile construction
- `execute_metadata_actions()` (line 341): Add `rating` field handling
- Track matching (line 804): Add rating matching
- Release matching (line 854): Add rating matching

### rose-py/rose/rule_parser.py
- `ALL_TAGS` (line 136): Add `"rating": ["rating"]`
- `MODIFIABLE_TAGS` (line 185): Add `"rating"`

### rose-py/rose/__init__.py
- Export `set_release_rating`

### rose-cli/rose_cli/cli.py
- Import `set_release_rating`
- New `set_rating` command under `releases` group

### rose-cli/rose_cli/dump.py
- `release_to_json()`: Add `"rating": r.rating`
- `track_to_json()`: Add `"rating": t.release.rating`
