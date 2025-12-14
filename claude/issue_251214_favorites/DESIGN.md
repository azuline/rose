# Problem Statement

We need to add a "favorite" release classification system to Rose that allows users to mark releases they particularly like for long-term preference tracking. The system will parallel the existing "new" architecture: a boolean field persisted in both TOML files and SQLite cache, exposed through a dedicated VirtualFS view (#1, shifting other views down), integrated into the rule engine, and accessible via CLI commands. Unlike "new" which defaults to `true`, favorites will default to `false` (opt-in). We will NOT implement the "only favorites" classifier hiding feature to keep this implementation simpler.

# Data Modeling

## Postgres Tables

Rose uses SQLite, not Postgres. The database schema is defined in `rose-py/rose/cache.sql`.

### Persistent State Requirements

We need to track:
1. **Release favorite status**: Boolean flag per release, defaults to `false`
2. **Fast querying**: Ability to efficiently filter releases by favorite status
3. **Full-text search**: Ability to match favorite status in rule engine queries

### Database Changes

**Modify `releases` table** (cache.sql line 8-26):
```sql
CREATE TABLE releases (
    id TEXT PRIMARY KEY,
    source_path TEXT NOT NULL UNIQUE,
    -- ... existing columns ...
    new BOOLEAN NOT NULL DEFAULT true,
    favorite BOOLEAN NOT NULL DEFAULT false  -- ADD THIS
);
CREATE INDEX releases_favorite ON releases(favorite);  -- ADD THIS INDEX
```

**Rationale**:
- `BOOLEAN NOT NULL DEFAULT false`: Explicit boolean type with opt-in semantics
- Index on `favorite`: Enables fast filtering for "Favorites" view queries
- Position after `new`: Logical grouping of classification fields

**Modify `rules_engine_fts` virtual table** (cache.sql line ~186):
```sql
CREATE VIRTUAL TABLE rules_engine_fts USING fts5 (
    id,
    source_path,
    -- ... many fields ...
    new,
    favorite,  -- ADD THIS
    -- ... more fields ...
    content=releases_view
);
```

**Rationale**: Enables full-text search matching on `favorite` field for rule engine patterns like `favorite:true`.

**Modify `releases_view`** (cache.sql line ~243):
```sql
CREATE VIEW releases_view AS
SELECT
    r.id,
    r.source_path,
    -- ... many fields ...
    r.new,
    r.favorite,  -- ADD THIS
    -- ... more fields ...
FROM releases r;
```

**Rationale**: View must expose `favorite` for FTS table and general querying.

### Query Analysis

**Query 1: Filter releases by favorite status**
```python
filter_releases(c, favorite=True)
# SQL: SELECT * FROM releases WHERE favorite = ?
```
**Ergonomic**: ✅ Simple indexed lookup

**Query 2: Filter tracks by parent release favorite status**
```python
filter_tracks(c, favorite=True)
# SQL: SELECT * FROM tracks JOIN releases ON tracks.release_id = releases.id WHERE releases.favorite = ?
```
**Ergonomic**: ✅ Standard join with index

**Query 3: Full-text search for rules**
```sql
SELECT * FROM rules_engine_fts WHERE rules_engine_fts MATCH 'favorite:true'
```
**Ergonomic**: ✅ Native FTS5 support

**Query 4: Update favorite status**
```sql
UPDATE releases SET favorite = ? WHERE id = ?
```
**Ergonomic**: ✅ Simple primary key update

All operations are ergonomic with proper indexing.

## Python Dataclasses

### Transient State Requirements

We need in-memory representations for:
1. **Release objects**: Include favorite status for filtering/display
2. **TOML file contents**: Serialize/deserialize favorite from disk
3. **Configuration**: Store template configuration for favorites view

### Dataclass Changes

**Modify `Release` dataclass** (cache.py line 208-231):
```python
@dataclasses.dataclass(slots=True)
class Release:
    id: str
    source_path: Path
    # ... many fields ...
    new: bool
    favorite: bool  # ADD THIS AFTER new
    disctotal: int
    genres: list[str]
    # ... more fields ...
```

**Operations**:
- Read from database: ✅ Ergonomic (single column fetch)
- Pass to templates: ✅ Ergonomic (direct field access)
- Filter in memory: ✅ Ergonomic (boolean check)

**Modify `StoredDataFile` dataclass** (cache.py line 318-323):
```python
@dataclasses.dataclass(slots=True)
class StoredDataFile:
    new: bool = True
    favorite: bool = False  # ADD THIS WITH DEFAULT FALSE
    added_at: str = dataclasses.field(
        default_factory=lambda: datetime.now().astimezone().replace(microsecond=0).isoformat()
    )
```

**Operations**:
- Serialize to TOML: ✅ Ergonomic (`tomli_w.dump(dataclasses.asdict(datafile))`)
- Deserialize from TOML: ✅ Ergonomic (`StoredDataFile(**diskdata)`)
- Toggle value: ✅ Ergonomic (`datafile.favorite = not datafile.favorite`)

**Rationale for default `False`**: Favorites are opt-in, unlike "new" which marks all newly added releases.

**Modify `PathTemplateConfig` dataclass** (templates.py line ~196):
```python
@dataclasses.dataclass
class PathTemplateConfig:
    releases: PathTemplateTriad
    releases_favorite: PathTemplateTriad  # ADD THIS
    releases_new: PathTemplateTriad
    releases_added_on: PathTemplateTriad
    # ... more fields ...
```

**Operations**:
- Select template by view: ✅ Ergonomic (`config.path_templates.releases_favorite.release`)
- Parse from config TOML: ✅ Ergonomic (existing parser handles new fields)

All operations are ergonomic.

# Data Flow

## Current Data Flow (Before Modification)

**Toggle "new" status**:
1. User runs `rose releases toggle-new {release_id}`
2. CLI calls `toggle_release_new(c, release_id)`
3. Function reads `.rose.{uuid}.toml` from disk
4. Toggles `data["new"]` in memory
5. Writes updated TOML back to disk
6. Calls `update_cache_for_releases()` to sync SQLite cache
7. Cache update reads TOML, updates `releases.new` column

**Browse "New" view in VirtualFS**:
1. User opens "1. Releases - New" directory
2. VirtualFS parses path → `VirtualPath(view="New")`
3. Creates `Matcher(["new"], Pattern("true"))`
4. Calls `filter_releases(c, matcher=matcher)`
5. Queries `releases` table with `WHERE new = true`
6. Returns matching releases
7. Applies `path_templates.releases_new` template for display names

**Rule engine matching**:
1. User creates rule: `new:true / replace:false`
2. Rule engine iterates tracks
3. For each track, reads parent release's `.rose.{uuid}.toml`
4. Checks if `datafile.new` matches pattern
5. If match, executes action → `datafile.new = False`
6. Writes updated TOML to disk
7. Cache update syncs changes

## Planned Data Flow (After Modification)

**Toggle "favorite" status** (NEW):
1. User runs `rose releases toggle-favorite {release_id}`
2. CLI calls `toggle_release_favorite(c, release_id)` (NEW FUNCTION)
3. Function reads `.rose.{uuid}.toml` from disk
4. Toggles `data["favorite"]` in memory
5. Writes updated TOML back to disk
6. Calls `update_cache_for_releases()` to sync SQLite cache
7. Cache update reads TOML, updates `releases.favorite` column

**Browse "Favorites" view in VirtualFS** (NEW):
1. User opens "1. Releases - Favorites" directory
2. VirtualFS parses path → `VirtualPath(view="Favorites")`
3. Creates `Matcher(["favorite"], Pattern("true"))`
4. Calls `filter_releases(c, matcher=matcher)`
5. Queries `releases` table with `WHERE favorite = true`
6. Returns matching releases
7. Applies `path_templates.releases_favorite` template for display names

**Rule engine matching** (NEW):
1. User creates rule: `favorite:false / replace:true`
2. Rule engine iterates tracks
3. For each track, reads parent release's `.rose.{uuid}.toml`
4. Checks if `datafile.favorite` matches pattern
5. If match, executes action → `datafile.favorite = True`
6. Writes updated TOML to disk
7. Cache update syncs changes

**VirtualFS view ordering** (MODIFIED):
- Current: "1. Releases", "1. Releases - New", "1. Releases - Added On", etc.
- New: Add "1. Releases - Favorites" (NEW), keep all others as "1." prefix
- Order: "1. Releases - Favorites", "1. Releases - New", "1. Releases - Added On", "1. Releases - Released On"
- No renumbering needed, just add new view before existing ones

This data flow is optimal because:
1. **Single source of truth**: TOML file is authoritative, cache is derivative
2. **Automatic synchronization**: Cache updates handle sync transparently
3. **Consistent patterns**: Favorites follow identical flow to "new"
4. **No breaking changes**: Existing data flows remain unchanged

# UI Changes

Rose is primarily a CLI application with a virtual filesystem interface. No web endpoints exist.

## CLI Commands

**New command**: `rose releases toggle-favorite`
```bash
rose releases toggle-favorite <release_id_or_path>
```
- **Parameters**:
  - `release_id_or_path`: Release UUID or filesystem path (positional, required)
- **Behavior**: Toggle boolean value, log change, update cache
- **Output**: Info log message with release name and new status

## VirtualFS Changes

**New view**: "1. Releases - Favorites"
- **Path**: `/path/to/mount/1. Releases - Favorites/`
- **Contents**: All releases where `favorite = true`
- **Template**: Uses `path_templates.releases_favorite` configuration
- **Behavior**: Identical to "1. Releases - New" but filters on `favorite` field

**View ordering**: All release views use "1." prefix
- "1. Releases - Favorites" (NEW, appears first)
- "1. Releases - New" (no change)
- "1. Releases - Added On" (no change)
- "1. Releases - Released On" (no change)
- No renumbering needed, just insert new view at the beginning

## Metadata Editor

**Modified**: `edit_release()` in releases.py
- Add `favorite` field to editable metadata (parallel to `new` field)
- User can toggle favorite status in editor
- On save, detect change and call `toggle_release_favorite()` if modified

## Configuration

**New config section**: `[path_templates.releases_favorite]`
```toml
[path_templates.releases_favorite]
release = """
{{ releaseartists | artistsfmt }} -
{% if releasedate %}{{ releasedate.year }}.{% endif %}
{{ releasetitle }}
{% if releasetype == "single" %}- {{ releasetype | releasetypefmt }}{% endif %}
[FAVORITE]
"""
releasetrack = "{{ discnumber }}.{{ tracknumber }}. {{ tracktitle }}"
collectiontrack = "{{ tracktitle }}"
```

**Default behavior**: If user doesn't configure, use above template

# Solution Restatement

We will implement a "favorite" boolean classification system that exactly parallels the "new" architecture: adding a `favorite BOOLEAN NOT NULL DEFAULT false` column to the `releases` table with index, adding `favorite: bool = False` to `Release` and `StoredDataFile` dataclasses, creating `toggle_release_favorite()` function, exposing a "1. Releases - Favorites" VirtualFS view (appearing first in the list, all views keep "1." prefix), registering "favorite" in the rule engine tag lists, providing a default `[FAVORITE]` template suffix, and adding a `rose releases toggle-favorite` CLI command. The key trapdoor decision is NOT implementing the "only favorites" classifier hiding feature—this simplifies implementation but means users cannot hide genres/descriptors/labels that only contain favorites like they can with "new".

# Observability

## Metrics

Rose does not currently use metrics/observability systems. All monitoring is via log messages.

**Log messages to add**:
1. **Toggle operation**: `Toggled "favorite"-ness of release {name} to {status}` (INFO level)
2. **Migration**: No logging (silent migration per requirements)
3. **Rule engine**: Use existing rule action logging, will automatically include `favorite` field changes

No new metrics infrastructure needed.

## Traces/Spans

Rose does not use distributed tracing. Debugging is via:
1. **Log files**: Existing logger infrastructure
2. **TOML inspection**: Users can manually read `.rose.{uuid}.toml` files
3. **Database queries**: `sqlite3` CLI for manual inspection

**Core flows for debugging**:
1. **Toggle flow**: Track via log message, verify TOML file updated, check cache sync
2. **VirtualFS flow**: Check filter_releases query, verify matcher logic, inspect template rendering
3. **Rule flow**: Check matcher evaluation, verify action execution, confirm TOML write

No new trace infrastructure needed.

# Testing Plans

## Net New Features

**Feature 1: Toggle favorite status**
- Property 1: Toggling from `false` → `true` updates TOML file
- Property 2: Toggling from `true` → `false` updates TOML file
- Property 3: Toggling updates SQLite cache
- Property 4: Multiple toggles alternate correctly
- Test: `test_toggle_release_favorite` (parallel to `test_toggle_release_new`)

**Feature 2: Filter releases by favorite**
- Property 1: `filter_releases(favorite=True)` returns only favorited releases
- Property 2: `filter_releases(favorite=False)` returns only non-favorited releases
- Property 3: `filter_releases(favorite=None)` returns all releases
- Test: Extend existing `test_filter_releases` with favorite parameter

**Feature 3: Rule engine favorite matching**
- Property 1: `favorite:true` pattern matches favorited releases
- Property 2: `favorite:false` pattern matches non-favorited releases
- Property 3: `favorite/replace:true` action sets favorite to true
- Property 4: `favorite/replace:false` action sets favorite to false
- Test: `test_rules_fields_match_favorite` (parallel to `test_rules_fields_match_new`)

**Feature 4: VirtualFS Favorites view**
- Property 1: "1. Releases - Favorites" directory appears in root
- Property 2: Directory contains only favorited releases
- Property 3: Template renders with `[FAVORITE]` suffix
- Property 4: Files are accessible and readable
- Test: Extend existing VirtualFS tests with Favorites view checks

Minimal test count: **4 new tests** (one per feature) + **extend 1 existing test**

## Modified Existing Features

**[EXISTING] test_update_cache**:
- Add property: Cache update correctly reads `favorite` field from TOML
- Add property: Missing `favorite` field defaults to `false`

**[EXISTING] test_release_edit**:
- Add property: Editing `favorite` in metadata editor triggers toggle
- Add property: Unchanged `favorite` does not trigger toggle

**[EXISTING] VirtualFS view enumeration tests**:
- Modify property: Root directory contains "1. Releases - Favorites" as first release view
- Modify property: "1. Releases - Favorites" appears before "1. Releases - New"
- Verify property: All views maintain "1." prefix (no renumbering)

**[NEW] test_schema_recreation**:
- Property 1: Schema hash changes when `cache.sql` is modified
- Property 2: Database is recreated with `favorite` column on next cache operation
- Property 3: All releases default to `favorite = false` after recreation
- Note: This test may already be covered by existing cache update tests

## Removed Features

None. No features are being removed.

# Files to Change

## rose-py/rose/cache.sql
- **Line ~25**: Add `favorite BOOLEAN NOT NULL DEFAULT false` column after `new` field
- **Line ~28**: Add `CREATE INDEX releases_favorite ON releases(favorite);` after `releases_new` index
- **Line ~186**: Add `favorite` field to `rules_engine_fts` FTS5 table
- **Line ~243**: Add `r.favorite` to `releases_view` SELECT clause

## rose-py/rose/cache.py
- **Line ~222**: Add `favorite: bool` field to `Release` dataclass after `new: bool`
- **Line ~320**: Add `favorite: bool = False` field to `StoredDataFile` dataclass after `new: bool = True`
- **Line ~650-669**: Update TOML deserialization to read `favorite` field with `diskdata.get("favorite", False)` (defaults to `False` if missing)
- **Line ~632-645**: Update TOML serialization to write `favorite` field
- **Line ~1807-1809**: Add `favorite: bool | None = None` parameter to `filter_releases()` function
- **Line ~1810**: Add query clause `if favorite is not None: query += " AND favorite = ?"; args.append(favorite)`
- **Line ~1905-1907**: Add `favorite: bool | None = None` parameter to `filter_tracks()` function
- **Line ~1908**: Add query clause `if favorite is not None: query += " AND favorite = ?"; args.append(favorite)`

## rose-py/rose/releases.py
- **After line ~115**: Add new function `toggle_release_favorite(c: Config, release_id: str) -> None`
  - Clone `toggle_release_new()` logic
  - Change field from `new` to `favorite`
  - Change log message to reference "favorite"-ness
- **Line ~421-422**: In `edit_release()`, add check for `release_meta.favorite != release.favorite` and call `toggle_release_favorite()`
- **Line ~563-570**: In `make_single_release()`, add logic to default extracted singles to `favorite=False` (toggle if parent was favorited)

## rose-py/rose/config.py
- No changes needed (no classifier hiding config)

## rose-py/rose/templates.py
- **Line ~196-197**: Add `releases_favorite: PathTemplateTriad` field to `PathTemplateConfig` dataclass (insert before `releases_new`)
- **Line ~160-166**: Add `DEFAULT_FAVORITE_RELEASE_TEMPLATE` constant with `[FAVORITE]` suffix (clone from `DEFAULT_RELEASE_TEMPLATE`)
- **Line ~365-368**: In `build_template_context()`, add `"favorite": release.favorite` to returned dict
- **Config parsing section**: Add parsing logic for `[path_templates.releases_favorite]` TOML section with default fallback

## rose-py/rose/rule_parser.py
- **Line ~78**: Add `"favorite"` to `ALL_QUERYABLE_TAGS` list (insert after `"new"`)
- **Line ~134**: Add `"favorite": ["favorite"]` to `ALL_TAGS` dict (insert after `"new"` entry)
- **Line ~182**: Add `"favorite"` to `MODIFIABLE_TAGS` list (insert after `"new"`)
- **Line ~198**: Add `"favorite"` to `SINGLE_VALUE_TAGS` list (insert after `"new"`)

## rose-py/rose/rules.py
- **Line ~255-258**: Add matching logic for `field == "favorite"` (clone from `new` logic)
  - Load `datafile` if not loaded
  - Match against `datafile.favorite`
- **Line ~392-402**: Add action execution for `field == "favorite"` (clone from `new` logic)
  - Execute action on `datafile.favorite`
  - Validate result is `"true"` or `"false"`
  - Convert to boolean and update `datafile.favorite`
  - Track change in `potential_datafile_changes`

## rose-py/rose/__init__.py
- **Line ~110**: Add `toggle_release_favorite` to imports from `rose.releases`
- **Line ~215**: Add `"toggle_release_favorite"` to `__all__` list

## rose-vfs/rose_vfs/virtualfs.py
- **Line ~184**: Add `"Favorites"` to view Literal type (before "New")
- **Line ~275**: Add path parsing for `"1. Releases - Favorites"` (clone "New" logic, insert before "New" block)
  - Parse 1-part path → `VirtualPath(view="Favorites")`
  - Parse 2-part path → `VirtualPath(view="Favorites", release=parts[1])`
  - Parse 3-part path → `VirtualPath(view="Favorites", release=parts[1], file=parts[2])`
- **Line ~560**: Add template selection for `elif release_parent.view == "Favorites": template = self._config.path_templates.releases_favorite.release`
- **Line ~1090-1093**: Update root directory listing to include `"1. Releases - Favorites"` (insert first, before "1. Releases - New")
- **Line ~1120-1162**: Add release filtering for `elif p.view == "Favorites": matcher = Matcher(["favorite"], Pattern("true", strict=True))`
- **Similar location**: Add track filtering for `if p.view == "Favorites": matcher = Matcher(["favorite"], Pattern("true", strict=True))`

## rose-cli/rose_cli/cli.py
- **After line ~259**: Add new command `@releases.command() def toggle_favorite(ctx: Context, release: str) -> None`
  - Clone `toggle_new` command
  - Change function name to `toggle_favorite`
  - Change docstring to reference "favorite"
  - Call `toggle_release_favorite(ctx.config, release)`

## rose-py/rose/releases_test.py
- **After line ~72**: Add `test_toggle_release_favorite(config: Config) -> None`
  - Clone `test_toggle_release_new` test
  - Change field from `new` to `favorite`
  - Verify default is `False` (not `True`)
  - Test toggling `False` → `True` → `False`

## rose-py/rose/rules_test.py
- **After line ~223**: Add `test_rules_fields_match_favorite(config: Config) -> None`
  - Clone `test_rules_fields_match_new` test
  - Change field from `new` to `favorite`
  - Test matching `favorite:false` and replacing with `true`

## Database Migration
- **No separate migration file needed**: Rose uses schema hash comparison (cache.py line 130-145)
- When `cache.sql` is modified, the schema hash changes
- On next cache operation, database is automatically recreated from `cache.sql`
- All releases will have `favorite = false` by default (per column definition)

# Open Questions - ANSWERED

1. **Migration Timing**: ~~Should the database migration run automatically on first cache update, or should it be a separate `rose migrate` command?~~ **RESOLVED**: Rose uses schema hash comparison - when schema changes, database is automatically recreated on next cache operation. No custom migration needed.

2. **Extracted Singles Inheritance**: Always default to `false` (consistent with "new" behavior)

3. **Template Context**: Yes, expose both `new` and `favorite` in same template context for flexibility (e.g., `{% if favorite %}★{% elif new %}[NEW]{% endif %}`)

4. **VirtualFS View Numbers**: All views keep "1." prefix, no renumbering needed

5. **Metadata Editor Format**: Match existing `new` field format (boolean: `true`/`false`)

6. **CLI Command Naming**: Use `toggle-favorite` for consistency with `toggle-new`

7. **TOML Field Ordering**: `favorite` comes after `new`, defaults to `false` if field is missing from TOML
