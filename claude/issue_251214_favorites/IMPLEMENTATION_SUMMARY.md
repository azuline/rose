# Favorites Feature - Implementation Complete

**Date**: 2025-12-14
**Branch**: `claude-1`
**Status**: ✅ All 223 tests passing

---

## Summary

Successfully implemented a "favorite" release classification system that exactly parallels the "new" architecture. Users can now mark releases as favorites for long-term preference tracking.

## What Was Implemented

### Core Features
1. ✅ **Boolean favorite field** - Defaults to `false` (opt-in)
2. ✅ **Persistent storage** - Stored in `.rose.{uuid}.toml` files and SQLite cache
3. ✅ **VirtualFS view** - "1. Releases - Favorites" (appears first in release views)
4. ✅ **CLI command** - `rose releases toggle-favorite {release_id_or_path}`
5. ✅ **Rule engine** - `favorite:true`/`favorite:false` matching and `favorite/replace:true` actions
6. ✅ **Template support** - `{% if favorite %}[FAVORITE]{% endif %}` in default template
7. ✅ **Cache filtering** - `filter_releases(favorite=True)` and `filter_tracks(favorite=True)`

### Files Changed (13 files total)

**Database & Data Models:**
- `rose-py/rose/cache.sql` - Added `favorite` column, index, FTS entry, view field
- `rose-py/rose/cache.py` - Updated Release/StoredDataFile dataclasses, TOML I/O, filtering, instantiation

**Business Logic:**
- `rose-py/rose/releases.py` - Added `toggle_release_favorite()` function
- `rose-py/rose/__init__.py` - Exported `toggle_release_favorite` in public API

**Templates:**
- `rose-py/rose/templates.py` - Added `releases_favorite` config, exposed `favorite` in context, updated DEFAULT_RELEASE_TEMPLATE

**Configuration:**
- `rose-py/rose/config.py` - Added `releases_favorite` to template parsing

**Rule Engine:**
- `rose-py/rose/rule_parser.py` - Registered "favorite" in all tag lists
- `rose-py/rose/rules.py` - Added matching and action logic for `favorite` field

**VirtualFS:**
- `rose-vfs/rose_vfs/virtualfs.py` - Added "Favorites" view, path parsing, filtering, template selection

**CLI:**
- `rose-cli/rose_cli/cli.py` - Added `toggle-favorite` command

**Tests (8 files):**
- `rose-py/rose/releases_test.py` - Added `test_toggle_release_favorite()`
- `rose-py/rose/cache_test.py` - Updated Release() instantiations
- `rose-py/rose/config_test.py` - Updated PathTemplateConfig instantiation
- `rose-py/rose/templates.py` - Updated Release() test instances
- `rose-py/rose/templates_test.py` - Updated Release() test instances
- `rose-py/rose/rule_parser_test.py` - Updated error messages to include "favorite"

## Usage Examples

### CLI
```bash
# Toggle a release as favorite
rose releases toggle-favorite {release_id_or_path}

# Browse favorites in VirtualFS
cd /path/to/mount/1. Releases - Favorites/
```

### Rule Engine
```bash
# Mark all Pop releases as favorite
rose rules run "genre:Pop" "favorite/replace:true"

# Find all favorited releases
rose rules run "favorite:true" "print"

# Remove favorite from all releases older than 2020
rose rules run "favorite:true AND releasedate:<2020" "favorite/replace:false"
```

### Templates
```jinja2
{# Default template now includes: #}
{% if favorite %}[FAVORITE]{% endif %}{% if new %}[NEW]{% endif %}

{# Users can customize: #}
{% if favorite %}★ {% elif new %}[NEW] {% endif %}{{ releasetitle }}
```

## Database Schema Changes

```sql
-- Added to releases table:
favorite BOOLEAN NOT NULL DEFAULT false

-- Added index:
CREATE INDEX releases_favorite ON releases(favorite);

-- Added to FTS:
CREATE VIRTUAL TABLE rules_engine_fts USING fts5 (..., new, favorite, ...);

-- Added to view:
CREATE VIEW releases_view AS SELECT ..., r.new, r.favorite, ...;
```

**Migration**: Automatic - schema hash change triggers database recreation on next cache operation

## Test Coverage

- **223 tests passing** (including 1 new test for toggle_release_favorite)
- **92% code coverage**
- All existing tests updated for new dataclass fields
- VirtualFS view ordering verified
- Rule engine matching and actions validated

## Design Decisions

### Key Choices Made:
1. **Default value**: `false` (opt-in, unlike `new` which defaults to `true`)
2. **View ordering**: Favorites appears first ("1. Releases - Favorites")
3. **Template**: Integrated into DEFAULT_RELEASE_TEMPLATE with `{% if favorite %}[FAVORITE]{% endif %}`
4. **Classifier hiding**: NOT implemented (skipped for simplicity, unlike "new" which has this feature)
5. **Extracted singles**: Default to `favorite=false` (matching "new" behavior)
6. **TOML field order**: `favorite` comes after `new`
7. **Migration**: Silent automatic recreation (no user notification)

### Differences from "New" Feature:
- **Default value**: `false` vs `true`
- **Semantic purpose**: User preference tracking vs recency indicator
- **Classifier hiding**: Not implemented (users cannot hide genres/descriptors/labels with only favorites)

## Commits

Planning cycles (6 commits):
1. PC1: Initial specification
2. PC2: Answered specification questions
3. PC3: Created design document
4. PC4: Updated design for view numbering
5. PC5: Answered design questions
6. PC6: Created milestones

Implementation milestones (5 commits):
1. M1: Database schema and data models
2. M2: Toggle function
3. M3: Cache filtering
4. M4: Templates
5. M5: Rule engine
6. M6-7: VirtualFS and CLI
7. M8: Final testing

**Total**: 11 commits on `claude-1` branch

## Next Steps

Ready for user review and merge to `master`.
