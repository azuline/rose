# Problem Statement

We need to add a "favorite" release classification system to Rose that allows users to mark releases they particularly like, similar to how the existing "new" release system works. This is for music library users who want to quickly access and organize their preferred releases. Currently, users can only mark releases as "new" to distinguish recently added content, but they lack a mechanism to mark releases for long-term preference tracking. This problem is worth solving because it enables better music library curation and provides users with a personalized view of their most valued content, which enhances the overall library browsing experience.

# Problem Context

Based on extensive codebase exploration documented in `claude/251214_favorites/REPORT.md`, the "new" release feature provides the exact architectural pattern we need to follow. Here's the concrete understanding:

## Scope

The favorite feature will touch the following areas of the codebase:

**Core Data & Storage Layer (rose-py):**

- `rose-py/rose/cache.py` - Contains Release dataclass, StoredDataFile dataclass, and all filtering/querying functions
- `rose-py/rose/cache.sql` - Database schema with releases table and indexes
- `rose-py/rose/releases.py` - Release manipulation functions including toggle operations

**Configuration & Templates (rose-py):**

- `rose-py/rose/config.py` - VirtualFS configuration for hiding classifiers
- `rose-py/rose/templates.py` - Path template configuration and Jinja2 rendering

**Rule Engine (rose-py):**

- `rose-py/rose/rule_parser.py` - Tag registration for rule matching
- `rose-py/rose/rules.py` - Rule matching and action execution logic

**Virtual Filesystem (rose-vfs):**

- `rose-vfs/rose_vfs/virtualfs.py` - View definitions, path parsing, filtering, and template selection

**CLI (rose-cli):**

- `rose-cli/rose_cli/cli.py` - User-facing commands

**Public API:**

- `rose-py/rose/__init__.py` - Exported functions

**Tests (all packages):**

- `rose-py/rose/releases_test.py` - Release operation tests
- `rose-py/rose/rules_test.py` - Rule engine tests
- `rose-vfs/rose_vfs/virtualfs_test.py` - VirtualFS tests

## Systems Understanding

**1. Storage System (Dual Persistence):**
The "new" status is persisted in two places:

- **TOML files**: Each release directory contains `.rose.{uuid}.toml` with fields like `new = true` and `added_at = "..."`
- **SQLite cache**: The `releases` table has a `new BOOLEAN NOT NULL DEFAULT true` column with an index for efficient querying

Both must stay synchronized. The TOML file is the source of truth on disk, while the cache enables fast queries.

**2. Data Model System:**

- `Release` dataclass: In-memory representation with `new: bool` field (line 203-231 in cache.py)
- `StoredDataFile` dataclass: Represents TOML file contents with `new: bool = True` default (line 318-323 in cache.py)
- `GenreEntry`/`DescriptorEntry`/`LabelEntry`: Track if classifiers contain "only new releases" for VirtualFS filtering (lines 2304-2378 in cache.py)

**3. Business Logic System:**

- `toggle_release_new()`: Primary operation that reads TOML → toggles value → writes TOML → updates cache (releases.py lines 90-115)
- `filter_releases()`/`filter_tracks()`: Query functions with optional `new: bool | None` parameter (cache.py lines 1807-1809, 1827)
- `list_genres()`/`list_descriptors()`/`list_labels()`: Queries that identify classifiers with "only new releases" using LEFT JOIN (cache.py lines 2310-2391)

**4. VirtualFS System:**
The virtual filesystem exposes a "1. Releases - New" view that:

- Parses paths to identify the "New" view (virtualfs.py line 286-290)
- Filters releases/tracks using `Matcher(["new"], Pattern("true", strict=True))` (virtualfs.py line 1120-1162)
- Selects dedicated templates via `path_templates.releases_new` (virtualfs.py line 560)
- Hides genres/descriptors/labels that contain only new releases based on config (virtualfs.py lines 1186, 1195, 1204)

**5. Rule Engine System:**
Rules can match and modify "new" status:

- Tags registered in `rule_parser.py`: `ALL_QUERYABLE_TAGS`, `ALL_TAGS`, `MODIFIABLE_TAGS`, `SINGLE_VALUE_TAGS` (lines 78, 134, 182, 198)
- Matching: `new:true` or `new:false` patterns load from TOML datafile (rules.py line 255-258)
- Actions: `new/replace:true` validates boolean and writes changes (rules.py line 392-402)

**6. Template System:**

- Default template includes `{% if new %}[NEW]{% endif %}` suffix (templates.py line 165)
- Template context exposes `"new": release.new` for Jinja2 (templates.py line 365-368)
- Dedicated `releases_new: PathTemplateTriad` config for the New view (templates.py line 196-197)

## Current Behavior

**For "new" releases:**

1. New releases default to `new=True` when first added to the library
2. Users can toggle via `rose releases toggle-new {release_id_or_path}`
3. The VirtualFS shows a "1. Releases - New" directory listing only new releases
4. Templates render `[NEW]` suffix for new releases by default
5. Rules can match (`new:true`) and modify (`new/replace:false`) the status
6. Extracted singles automatically default to `new=False`
7. Configuration allows hiding genres/descriptors/labels that only contain new releases

**No equivalent exists for favorites** - users cannot mark releases for long-term preference tracking.

## Current Automated Tests

**Release Operations (releases_test.py line 48-72):**

- `test_toggle_release_new()`: Tests toggling between True/False, verifies TOML file and cache updates

**Rule Engine (rules_test.py line 197-223):**

- `test_rules_fields_match_new()`: Tests matching `new:false` and replacing with `true`, verifies rule execution

**VirtualFS (virtualfs_test.py line 503-521):**

- `test_virtual_filesystem_hide_new_release_classifiers()`: Tests hiding genres/descriptors/labels with only new releases based on config

These tests establish patterns we'll replicate for favorites.

# Solution Proposal

We will implement a "favorite" release classification system that exactly parallels the "new" system architecture: adding a boolean `favorite` field to the data model, persisting it in both TOML files and SQLite cache, exposing it through a dedicated VirtualFS view, integrating it into the rule engine, and providing CLI commands for toggling. The favorite status will default to `false` (opt-in by user) unlike "new" which defaults to `true`.

## Behavioral Changes

**New Behaviors:**

1. Each release will have a `favorite: bool` field (default `false`) persisted in `.rose.{uuid}.toml` and the cache database
2. A new "1. Releases - Favorites" view will appear in the VirtualFS showing only favorited releases (all other views shift down by 1)
3. Users can run `rose releases toggle-favorite {release_id_or_path}` to mark/unmark favorites
4. Templates will have access to `{% if favorite %}` conditionals with a default `[FAVORITE]` suffix rendering
5. Rules can match (`favorite:true`/`favorite:false`) and modify (`favorite/replace:true`) favorite status
6. The metadata editor will allow editing the favorite field
7. Extracted singles will default to `favorite=false` (matching "new" behavior)

**Modified Behaviors:**

- `.rose.{uuid}.toml` files will gain a `favorite = false` field
- The `releases` table will gain a `favorite` column with index
- Template contexts will include the `favorite` field

**Unchanged Behaviors:**

- The "new" system continues to work identically
- Existing releases default to `favorite=false` without user intervention
- All other release operations remain unaffected

## Affected Systems

1. **Storage System**: Add `favorite BOOLEAN NOT NULL DEFAULT false` column to `releases` table with index; add `favorite` field to TOML serialization/deserialization; perform silent database migration
2. **Data Model System**: Add `favorite: bool` to `Release` dataclass; add `favorite: bool = False` to `StoredDataFile`
3. **Business Logic System**: Create `toggle_release_favorite()` function; add `favorite: bool | None` parameter to filtering functions; handle extracted singles defaulting to `favorite=false`
4. **VirtualFS System**: Add "Favorites" view (as view #1, shifting others down) with path parsing; add filtering with `Matcher(["favorite"], Pattern("true"))`; add template selection for `releases_favorite`
5. **Rule Engine System**: Register "favorite" in tag lists; add matching logic for `favorite` field from TOML; add action logic with boolean validation
6. **Template System**: Add `releases_favorite: PathTemplateTriad` config with sensible defaults; add default template with `[FAVORITE]` suffix; expose `favorite` in template context
7. **CLI System**: Add `toggle-favorite` command under `rose releases`
8. **Public API**: Export `toggle_release_favorite` function
9. **Metadata Editor**: Add `favorite` field for editing (parallel to `new` field)

## Affected Tests

1. **test_toggle_release_new** → Create parallel `test_toggle_release_favorite`: Test toggling favorite status, verify TOML updates, verify cache updates
2. **test_rules_fields_match_new** → Create parallel `test_rules_fields_match_favorite`: Test matching `favorite:false`, replacing with `true`, verify rule execution
3. **Global test suite**: Run `just test` after implementation to ensure no regressions (especially VirtualFS view ordering changes)

## Relevant Files

Core files requiring changes (in implementation order):

**Phase 1 - Data Layer:**

- `rose-py/rose/cache.sql` - Add `favorite` column, index, FTS entry, view field
- `rose-py/rose/cache.py` - Update all data models and TOML I/O
- Migration script (new file) - Add column to existing databases

**Phase 2 - Business Logic:**

- `rose-py/rose/releases.py` - Implement `toggle_release_favorite()`
- `rose-py/rose/cache.py` - Add filtering parameters

**Phase 3 - Templates:**

- `rose-py/rose/templates.py` - Add `releases_favorite` config and context

**Phase 4 - Rule Engine:**

- `rose-py/rose/rule_parser.py` - Register "favorite" tags
- `rose-py/rose/rules.py` - Add matching and action logic

**Phase 5 - VirtualFS:**

- `rose-vfs/rose_vfs/virtualfs.py` - Add view, parsing, filtering, template selection

**Phase 6 - CLI:**

- `rose-cli/rose_cli/cli.py` - Add `toggle-favorite` command

**Phase 7 - API & Tests:**

- `rose-py/rose/__init__.py` - Export function
- `rose-py/rose/releases_test.py` - Add toggle test
- `rose-py/rose/rules_test.py` - Add rule test

# Open Questions - ANSWERED

1. **Template Formatting**: Use `[FAVORITE]` text suffix (like `[NEW]`)
2. **View Ordering**: "Favorites" appears BEFORE "New" - shift all views down by 1
3. **Metadata Editor**: Yes, `favorite` field is editable in metadata editor
4. **Extracted Singles**: Default to `false`, matching "new" behavior
5. **Migration Messaging**: Silent database migration
6. **Config Defaults**: Provide sensible default `releases_favorite` template
7. **Classifier Hiding**: Skip the "only favorites" classifier hiding feature (not needed for favorites)
