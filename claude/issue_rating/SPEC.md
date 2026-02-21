# Rating Feature Specification

## Problem Statement

Users can mark releases as "new" (automatic, togglable) and "favorite" (opt-in boolean), but there is
no mechanism for granular quality/preference scoring. A numeric rating (1-100) would allow users to
express fine-grained opinions about releases, usable in templates, rule engine matching, and filtering.

## Problem Context: Analogous Feature Analysis

The rating feature follows the same storage and data flow patterns as `favorite`. The key systems
involved are:

### 1. Storage (Dual Persistence: TOML + SQLite)

- **TOML files**: `.rose.{uuid}.toml` files in each release directory store per-release metadata.
  Currently stores: `new` (bool), `favorite` (bool), `added_at` (ISO8601 string).
- **SQLite cache**: The `releases` table caches these values for fast querying. Cache is rebuilt
  from TOML files when mtimes change.

### 2. Data Model

- `StoredDataFile` (cache.py:326): In-memory representation of `.rose.{uuid}.toml` contents.
- `Release` (cache.py:213): Full release model with all metadata fields.
- `MetadataRelease` (releases.py:236): Editable metadata model used in the interactive editor.

### 3. Business Logic

- `toggle_release_favorite()` (releases.py:117): Reads TOML, toggles boolean, writes back, refreshes cache.
- `filter_releases()` (cache.py:1742): SQL filtering with `favorite` parameter.
- `filter_tracks()` (cache.py:1840): SQL filtering with `favorite` parameter.

### 4. Rule Engine

- Tag registered in `ALL_TAGS` and `MODIFIABLE_TAGS` (rule_parser.py:136, 185).
- Matching in `find_tracks_matching_rule()` and `find_releases_matching_rule()`.
- Action execution in `execute_metadata_actions()` (rules.py:407-417).
- FTS5 index includes `favorite` column for substring search.

### 5. Template System

- `_calc_release_variables()` (templates.py:360): Exposes `favorite` to Jinja context.
- `_calc_track_variables()` (templates.py:384): Exposes `favorite` via `track.release.favorite`.
- Default template uses `{% if favorite %} [FAVORITE]{% endif %}`.

### 6. CLI

- `rose releases toggle-favorite <release>` command (cli.py:261-267).
- Metadata editor shows `favorite` field in TOML (releases.py:236-289).

### 7. JSON Dump

- `release_to_json()` (dump.py:44): Currently includes `new` but omits `favorite` (existing bug).

## Scope

### Files to Modify

**rose-py package:**
- `rose/cache.sql` — Add `rating` column to `releases` table, index, FTS5 table
- `rose/cache.py` — `StoredDataFile`, `Release`, `cached_release_from_view()`, cache update logic, `filter_releases()`, `filter_tracks()`, FTS sync, INSERT/UPDATE SQL
- `rose/releases.py` — New `set_release_rating()` function, `MetadataRelease` dataclass, `edit_release()`, `serialize()`, `from_toml()`, `from_cache()`
- `rose/templates.py` — `_calc_release_variables()`, `_calc_track_variables()`
- `rose/rules.py` — `_get_release_datafile_of_directory()`, `execute_metadata_actions()`, matching logic
- `rose/rule_parser.py` — `ALL_TAGS`, `MODIFIABLE_TAGS`
- `rose/__init__.py` — Export `set_release_rating`

**rose-cli package:**
- `rose_cli/cli.py` — New `set-rating` subcommand
- `rose_cli/dump.py` — Add `rating` to `release_to_json()` and `track_to_json()`

## Solution Proposal

### Rating Semantics

- **Type**: `int | None` (nullable integer, 1-100 range)
- **Default**: `None` (no rating / unrated)
- **Storage in TOML**: `rating = 85` or absent (meaning unrated)
- **Storage in SQLite**: `rating INTEGER` (nullable)
- **Validation**: Must be integer in range 1-100, or None to clear

### Differences from `favorite`

| Aspect | favorite | rating |
|--------|----------|--------|
| Type | `bool` | `int \| None` |
| Default | `false` | `None` (unrated) |
| CLI verb | `toggle-favorite` | `set-rating` (set or clear) |
| Template usage | `{% if favorite %}` | `{% if rating %}{{ rating }}{% endif %}` |
| Rule engine | Match `true`/`false` | Match numeric value as string |
| No VFS view | (has Favorites view) | No VFS view (per user request) |

### Systems Affected

1. **Storage** — NEW: `rating` field in `.rose.{uuid}.toml` and SQLite `releases` table
2. **Data Model** — MODIFIED: `StoredDataFile`, `Release`, `MetadataRelease` gain `rating` field
3. **Business Logic** — NEW: `set_release_rating()` function; MODIFIED: `filter_releases()`, `filter_tracks()`
4. **Template System** — MODIFIED: `rating` exposed as template variable
5. **Rule Engine** — MODIFIED: `rating` registered as tag for matching and actions
6. **CLI** — NEW: `rose releases set-rating <release> <rating>` command
7. **Metadata Editor** — MODIFIED: `rating` field shown in TOML editor
8. **JSON Dump** — MODIFIED: `rating` included in dump output

### Open Questions — ANSWERED

1. **Should rating have a VFS view?** No. The user explicitly stated no virtual filesystem folders.
2. **Should rating have a dedicated template triad?** No, since there's no VFS view for it.
3. **What happens when rating is cleared?** The key is removed from the TOML file; the DB stores NULL.
4. **Should we support filtering by rating range?** Not in initial implementation; simple equality/presence filter is sufficient. The rule engine's pattern matching handles this.
5. **What is the CLI syntax for clearing a rating?** `rose releases set-rating <release> --clear` or pass `0` / empty to clear.
