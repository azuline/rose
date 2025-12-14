# Milestone 1: Add favorite to database schema and data models

- **Tests:** `test_toggle_release_favorite` (will be written but won't pass until later milestones), existing cache tests should still pass
- **Scope:**
  - `rose-py/rose/cache.sql` - Add `favorite` column, index, FTS entry, view field
  - `rose-py/rose/cache.py` - Update `Release` and `StoredDataFile` dataclasses, TOML serialization/deserialization
- **Relevant Design Details:**
  - Add `favorite BOOLEAN NOT NULL DEFAULT false` column to `releases` table after `new` field
  - Create index `CREATE INDEX releases_favorite ON releases(favorite);` for efficient querying
  - Add `favorite` to `rules_engine_fts` FTS5 table for rule engine matching
  - Add `r.favorite` to `releases_view` SELECT clause
  - Add `favorite: bool` field to `Release` dataclass after `new: bool`
  - Add `favorite: bool = False` field to `StoredDataFile` dataclass (opt-in default)
  - Update TOML deserialization: `diskdata.get("favorite", False)` to default missing fields
  - Update TOML serialization: include `favorite` field in written TOML
  - Schema hash will change, triggering automatic database recreation on next cache operation

# Milestone 2: Implement toggle_release_favorite function

- **Tests:** `test_toggle_release_favorite` should now pass
- **Scope:**
  - `rose-py/rose/releases.py` - Add `toggle_release_favorite()` function
  - `rose-py/rose/__init__.py` - Export the new function
- **Relevant Design Details:**
  - Clone logic from `toggle_release_new()` (releases.py line 90-115)
  - Change field from `new` to `favorite` in all operations
  - Read `.rose.{uuid}.toml` → toggle `data["favorite"]` → write TOML → update cache
  - Log message: `'Toggled "favorite"-ness of release {name} to {status}'`
  - Export function in `__all__` for public API access

# Milestone 3: Add favorite filtering to cache queries

- **Tests:** Extend existing `test_filter_releases` to verify `favorite` parameter works correctly
- **Scope:**
  - `rose-py/rose/cache.py` - Add `favorite` parameter to `filter_releases()` and `filter_tracks()`
- **Relevant Design Details:**
  - Add `favorite: bool | None = None` parameter to `filter_releases()` (line ~1807)
  - Add query clause: `if favorite is not None: query += " AND favorite = ?"; args.append(favorite)`
  - Add `favorite: bool | None = None` parameter to `filter_tracks()` (line ~1905)
  - Add same query clause for track filtering
  - Test property 1: `filter_releases(favorite=True)` returns only favorited releases
  - Test property 2: `filter_releases(favorite=False)` returns only non-favorited releases
  - Test property 3: `filter_releases(favorite=None)` returns all releases

# Milestone 4: Add favorite to templates

- **Tests:** Manual verification that template context includes `favorite` field
- **Scope:**
  - `rose-py/rose/templates.py` - Add `releases_favorite` config, expose `favorite` in context
- **Relevant Design Details:**
  - Add `releases_favorite: PathTemplateTriad` field to `PathTemplateConfig` dataclass (before `releases_new`)
  - Create `DEFAULT_FAVORITE_RELEASE_TEMPLATE` with `[FAVORITE]` suffix (clone from `DEFAULT_RELEASE_TEMPLATE`)
  - In `build_template_context()`, add `"favorite": release.favorite` to returned dict
  - Add config parsing for `[path_templates.releases_favorite]` TOML section with default fallback
  - Both `new` and `favorite` exposed in same context for template flexibility

# Milestone 5: Register favorite in rule engine

- **Tests:** `test_rules_fields_match_favorite` should pass
- **Scope:**
  - `rose-py/rose/rule_parser.py` - Register "favorite" tags
  - `rose-py/rose/rules.py` - Add matching and action logic
- **Relevant Design Details:**
  - Add `"favorite"` to `ALL_QUERYABLE_TAGS` list (after `"new"`)
  - Add `"favorite": ["favorite"]` to `ALL_TAGS` dict
  - Add `"favorite"` to `MODIFIABLE_TAGS` list
  - Add `"favorite"` to `SINGLE_VALUE_TAGS` list
  - Clone matching logic from `new` (rules.py line 255-258): load datafile, match against `datafile.favorite`
  - Clone action logic from `new` (rules.py line 392-402): execute action, validate boolean, update datafile, track changes
  - Test property: `favorite:true` matches favorited releases
  - Test property: `favorite:false` matches non-favorited releases
  - Test property: `favorite/replace:true` sets favorite to true
  - Test property: `favorite/replace:false` sets favorite to false

# Milestone 6: Add Favorites view to VirtualFS

- **Tests:** Extend existing VirtualFS tests to verify Favorites view works correctly
- **Scope:**
  - `rose-vfs/rose_vfs/virtualfs.py` - Add view definition, path parsing, filtering, template selection
- **Relevant Design Details:**
  - Add `"Favorites"` to view Literal type (before "New")
  - Add path parsing for `"1. Releases - Favorites"` (clone "New" logic, insert before "New" block)
  - Parse 1-part: `VirtualPath(view="Favorites")`
  - Parse 2-part: `VirtualPath(view="Favorites", release=parts[1])`
  - Parse 3-part: `VirtualPath(view="Favorites", release=parts[1], file=parts[2])`
  - Add template selection: `elif release_parent.view == "Favorites": template = self._config.path_templates.releases_favorite.release`
  - Update root directory listing: insert `"1. Releases - Favorites"` first, before "1. Releases - New"
  - Add release filtering: `elif p.view == "Favorites": matcher = Matcher(["favorite"], Pattern("true", strict=True))`
  - Add track filtering: `if p.view == "Favorites": matcher = Matcher(["favorite"], Pattern("true", strict=True))`
  - Test property: "1. Releases - Favorites" directory appears in root
  - Test property: Directory contains only favorited releases
  - Test property: Template renders with `[FAVORITE]` suffix
  - Test property: Files are accessible and readable

# Milestone 7: Add CLI command and metadata editor integration

- **Tests:** Manual CLI testing, extend `test_release_edit` if metadata editor integration exists
- **Scope:**
  - `rose-cli/rose_cli/cli.py` - Add `toggle-favorite` command
  - `rose-py/rose/releases.py` - Add metadata editor integration
- **Relevant Design Details:**
  - Clone `toggle_new` command (cli.py after line ~259)
  - Command name: `toggle_favorite`
  - Docstring: `"""Toggle a release's "favorite" status. Accepts a release's UUID/path."""`
  - Call `toggle_release_favorite(ctx.config, release)`
  - In `edit_release()`, add check: `if release_meta.favorite != release.favorite: toggle_release_favorite()`
  - In `make_single_release()`, ensure extracted singles default to `favorite=False` (toggle if parent was favorited)
  - Test: Run `rose releases toggle-favorite {release}` and verify toggle works

# Milestone 8: Final testing and validation

- **Tests:** Run global `just test` to ensure no regressions
- **Scope:** All files
- **Relevant Design Details:**
  - Run `just lint` first to catch obvious issues
  - Run `just test` to execute full test suite
  - Verify all new tests pass: `test_toggle_release_favorite`, `test_rules_fields_match_favorite`
  - Verify extended tests pass: filter tests, VirtualFS tests
  - Verify no existing tests are broken
  - Manual verification: Schema recreation triggers on first cache operation after SQL changes
  - Manual verification: VirtualFS shows "1. Releases - Favorites" before "1. Releases - New"
  - Manual verification: Template renders `[FAVORITE]` suffix correctly

# Open Questions

None - all design questions have been answered and incorporated into milestones.
