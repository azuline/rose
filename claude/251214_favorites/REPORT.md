# "New" Release Implementation Analysis
## Purpose: Planning "Favorite" Feature Parallel Implementation

**Date**: 2025-12-14
**Author**: Claude Code
**Objective**: Document all aspects of "new" release functionality to enable building an identical "favorite" feature

---

## Executive Summary

The "new" release feature is a **user-controlled boolean flag** that:
- Defaults to `true` when releases are first added to the library
- Persists in `.rose.{uuid}.toml` files within each release directory
- Is stored in the SQLite database with an index for performance
- Powers a dedicated "New" view in the virtual filesystem
- Is queryable and modifiable through the rule engine
- Can be toggled via CLI or metadata editor
- Influences template rendering (e.g., `[NEW]` suffix)
- Supports VirtualFS filtering to hide genres/descriptors/labels that contain only new releases

**Key Insight**: The "favorite" feature should follow this exact pattern, requiring parallel changes across 10+ files spanning storage, caching, UI, templates, and rules.

---

## 1. Data Model & Storage

### 1.1 In-Memory Data Structures

**File**: `rose-py/rose/cache.py`

**Primary Release Model** (lines 203-231):
```python
@dataclasses.dataclass(slots=True)
class Release:
    id: str
    source_path: Path
    cover_image_path: Path | None

    # ... many other fields ...

    new: bool  # ← Boolean flag for "new" status

    # ... more fields ...
```

**Disk Storage Model** (lines 318-323):
```python
@dataclasses.dataclass(slots=True)
class StoredDataFile:
    """Represents .rose.{uuid}.toml file contents"""
    new: bool = True  # ← Defaults to True
    added_at: str = dataclasses.field(...)
```

**VirtualFS Filter Models** (lines 2304-2378):
```python
@dataclasses.dataclass(slots=True, frozen=True)
class GenreEntry:
    genre: str
    only_new_releases: bool  # ← Tracks if genre contains only new releases

@dataclasses.dataclass(slots=True, frozen=True)
class DescriptorEntry:
    descriptor: str
    only_new_releases: bool  # ← Same for descriptors

@dataclasses.dataclass(slots=True, frozen=True)
class LabelEntry:
    label: str
    only_new_releases: bool  # ← Same for labels
```

**Pattern for Favorites**:
- Add `favorite: bool` field to `Release` dataclass
- Add `favorite: bool = False` to `StoredDataFile` (default to False since favorites are opt-in)
- Add `only_favorite_releases: bool` to `GenreEntry`, `DescriptorEntry`, `LabelEntry`

---

### 1.2 Database Schema

**File**: `rose-py/rose/cache.sql`

**Releases Table** (lines 8-28):
```sql
CREATE TABLE releases (
    id TEXT PRIMARY KEY NOT NULL,
    source_path TEXT NOT NULL,

    -- ... many other columns ...

    new BOOLEAN NOT NULL DEFAULT true,

    -- ... more columns ...
);

CREATE INDEX releases_new ON releases(new);
```

**Full-Text Search Table** (line 186):
```sql
CREATE VIRTUAL TABLE rules_engine_fts USING fts5 (
    id,
    source_path,
    -- ... many fields ...
    new,
    -- ... more fields ...
    content=releases_view
);
```

**Releases View** (line 243):
```sql
CREATE VIEW releases_view AS
SELECT
    r.id,
    r.source_path,
    -- ... many fields ...
    r.new,
    -- ... more fields ...
FROM releases r;
```

**Pattern for Favorites**:
- Add `favorite BOOLEAN NOT NULL DEFAULT false` column to `releases` table
- Create index: `CREATE INDEX releases_favorite ON releases(favorite);`
- Add `favorite` to `rules_engine_fts` virtual table
- Add `r.favorite` to `releases_view`
- **Important**: Migration script needed to add column to existing databases

---

### 1.3 File Storage Format

**File Naming**: `.rose.{uuid}.toml` (one per release directory)

**Example TOML Content**:
```toml
new = true
added_at = "2025-01-15T14:30:00-05:00"
```

**Reading Logic** (cache.py lines 650-669):
```python
diskdata = tomllib.load(fp)
datafile = StoredDataFile(
    new=diskdata.get("new", True),  # Default to True if missing
    added_at=diskdata["added_at"],
)
release.new = datafile.new
```

**Writing Logic** (cache.py lines 632-645):
```python
stored_release_data = StoredDataFile(
    new=True,
    added_at=datetime.now().astimezone().replace(microsecond=0).isoformat(),
)

# Serialize to TOML
toml_data = {
    "new": stored_release_data.new,
    "added_at": stored_release_data.added_at,
}
```

**Pattern for Favorites**:
- Add `favorite = false` field to TOML files
- Update `StoredDataFile` serialization/deserialization
- Default to `false` when reading missing `favorite` key

---

## 2. Business Logic

### 2.1 Toggle Function

**File**: `rose-py/rose/releases.py`

**Primary Toggle Function** (lines 90-115):
```python
def toggle_release_new(c: Config, release_id: str) -> None:
    """Toggle a release's "new"-ness."""
    with contextlib.chdir(c.music_source_dir):
        # Step 1: Find the release directory
        release = get_release(c, release_id)

        # Step 2: Read the .rose.{uuid}.toml file
        datafile_path = release.source_path / f".rose.{release.id}.toml"
        datafile = read_datafile(datafile_path)

        # Step 3: Toggle the value
        datafile.new = not datafile.new

        # Step 4: Write back to disk
        write_datafile(datafile_path, datafile)

        # Step 5: Update the cache database
        with connect(c) as conn:
            conn.execute(
                "UPDATE releases SET new = ? WHERE id = ?",
                (datafile.new, release.id),
            )
            conn.commit()
```

**Integration with Metadata Editor** (releases.py lines 421-422):
```python
def edit_release(c: Config, release_id: str) -> None:
    # ... user edits metadata in editor ...
    release_meta = parse_edited_metadata()

    if release_meta.new != release.new:
        toggle_release_new(c, release.id)
```

**Automatic Default for Extracted Singles** (releases.py lines 563-570):
```python
def make_single_release(...):
    # ... extract single from album ...

    # Default extracted singles to NOT new
    toggle_release_new(c, release_id)
```

**Pattern for Favorites**:
- Create `toggle_release_favorite(c: Config, release_id: str) -> None`
- Identical logic: read TOML → toggle → write TOML → update cache
- Integrate into metadata editor (add `favorite` field to editor)
- **Decision Needed**: Should extracted singles default to favorite? (Probably not)

---

### 2.2 Querying by Status

**File**: `rose-py/rose/cache.py`

**Release Filtering** (lines 1807-1809):
```python
def filter_releases(
    c: Config,
    ...,
    new: bool | None = None,
    ...
) -> Iterator[Release]:
    query = "SELECT * FROM releases WHERE ..."
    args: list[Any] = []

    if new is not None:
        query += " AND new = ?"
        args.append(new)

    # Execute query and yield results
```

**Track Filtering** (lines 1827, 1905-1907):
```python
def filter_tracks(
    c: Config,
    ...,
    new: bool | None = None,
    ...
) -> Iterator[Track]:
    # Similar pattern to filter_releases
    if new is not None:
        query += " AND new = ?"
        args.append(new)
```

**Pattern for Favorites**:
- Add `favorite: bool | None = None` parameter to `filter_releases()`
- Add `favorite: bool | None = None` parameter to `filter_tracks()`
- Add query clause: `if favorite is not None: query += " AND favorite = ?"`

---

### 2.3 VirtualFS Filtering: "Only New Releases" Logic

**File**: `rose-py/rose/cache.py`

**Genre Listing** (lines 2310-2327):
```python
def list_genres(c: Config) -> list[GenreEntry]:
    """Return all genres with a flag if they contain only new releases."""
    query = """
        SELECT
            rg.genre,
            MIN(r.id) AS has_non_new_release
        FROM releases_genres rg
        LEFT JOIN releases r
            ON r.id = rg.release_id
            AND NOT r.new  -- Check for any non-new release
        GROUP BY rg.genre
        ORDER BY rg.genre COLLATE NOCASE ASC
    """

    entries = []
    for row in cursor.execute(query):
        entries.append(GenreEntry(
            genre=row["genre"],
            only_new_releases=(row["has_non_new_release"] is None)
        ))
    return entries
```

**Descriptor Listing** (lines 2347-2363):
```python
def list_descriptors(c: Config) -> list[DescriptorEntry]:
    # Identical pattern: LEFT JOIN with `NOT r.new` check
    # Returns only_new_releases flag
```

**Label Listing** (lines 2381-2391):
```python
def list_labels(c: Config) -> list[LabelEntry]:
    # Identical pattern: LEFT JOIN with `NOT r.new` check
    # Returns only_new_releases flag
```

**Pattern for Favorites**:
- Add parallel queries with `NOT r.favorite` instead of `NOT r.new`
- Return `only_favorite_releases` flag in each Entry dataclass
- These are used by VirtualFS to conditionally hide classifiers

---

## 3. Configuration

### 3.1 VirtualFS Configuration

**File**: `rose-py/rose/config.py`

**Config Dataclass** (lines 71-73):
```python
@dataclass
class VirtualFSConfig:
    mount_dir: Path

    # Hiding options for "only new" classifiers
    hide_genres_with_only_new_releases: bool
    hide_descriptors_with_only_new_releases: bool
    hide_labels_with_only_new_releases: bool
```

**Parsing from Config File** (lines 222-269):
```toml
[vfs]
hide_genres_with_only_new_releases = false
hide_descriptors_with_only_new_releases = false
hide_labels_with_only_new_releases = false
```

**Pattern for Favorites**:
- Add three new config fields:
  - `hide_genres_with_only_favorite_releases: bool`
  - `hide_descriptors_with_only_favorite_releases: bool`
  - `hide_labels_with_only_favorite_releases: bool`
- Add to TOML parsing logic with default `false`

---

## 4. Path Templates

### 4.1 Template Configuration

**File**: `rose-py/rose/templates.py`

**Template Config** (lines 196-197):
```python
@dataclasses.dataclass
class PathTemplateConfig:
    releases: PathTemplateTriad       # Standard releases view
    releases_new: PathTemplateTriad   # Dedicated "New" view
    releases_added_on: PathTemplateTriad
    # ... other views ...
```

**Triad Definition** (lines 92-96):
```python
@dataclasses.dataclass
class PathTemplateTriad:
    """Templates for release, releasetrack, and collectiontrack views"""
    release: PathTemplate
    releasetrack: PathTemplate
    collectiontrack: PathTemplate
```

**Default Release Template** (lines 160-166):
```python
DEFAULT_RELEASE_TEMPLATE = PathTemplate(
    """
{{ releaseartists | artistsfmt }} -
{% if releasedate %}{{ releasedate.year }}.{% endif %}
{{ releasetitle }}
{% if releasetype == "single" %}- {{ releasetype | releasetypefmt }}{% endif %}
{% if new %}[NEW]{% endif %}
"""
)
```

**Pattern for Favorites**:
- Add `releases_favorite: PathTemplateTriad` to `PathTemplateConfig`
- Create default template with `{% if favorite %}[FAVORITE]{% endif %}` (or user's preferred formatting)
- Update config parser to load `releases_favorite` from TOML

---

### 4.2 Template Context

**File**: `rose-py/rose/templates.py`

**Context Builder** (lines 365-368, 396-398):
```python
def build_template_context(release: Release, ...) -> dict[str, Any]:
    return {
        "new": release.new,
        "releaseartists": release.releaseartists,
        "releasetitle": release.releasetitle,
        # ... all other fields ...
    }
```

**Pattern for Favorites**:
- Add `"favorite": release.favorite` to template context
- Enables `{% if favorite %}` conditionals in Jinja2 templates

---

## 5. Virtual Filesystem

### 5.1 View Definition

**File**: `rose-vfs/rose_vfs/virtualfs.py`

**View Type** (line 184):
```python
view: Literal[
    "Releases",
    "Releases - New Releases",  # ← Dedicated view for new releases
    "Releases - Recently Added",
    "Releases - Released On",
    "Artists",
    "Genres",
    # ... more views ...
]
```

**Simplified in Code As** (line 286):
```python
view: Literal["Releases", "New", "RecentlyAdded", ...]
```

**Pattern for Favorites**:
- Add `"Favorites"` to view Literal types
- Add display name like `"Releases - Favorites"` for user-facing path

---

### 5.2 Path Parsing

**File**: `rose-vfs/rose_vfs/virtualfs.py`

**Path Parser** (lines 286-290):
```python
if parts[0] == "1. Releases - New":
    if len(parts) == 1:
        return VirtualPath(view="New")
    if len(parts) == 2:
        return VirtualPath(view="New", release=parts[1])
    if len(parts) == 3:
        return VirtualPath(view="New", release=parts[1], file=parts[2])
```

**Pattern for Favorites**:
```python
if parts[0] == "1. Releases - Favorites":
    if len(parts) == 1:
        return VirtualPath(view="Favorites")
    if len(parts) == 2:
        return VirtualPath(view="Favorites", release=parts[1])
    if len(parts) == 3:
        return VirtualPath(view="Favorites", release=parts[1], file=parts[2])
```

---

### 5.3 Release Filtering

**File**: `rose-vfs/rose_vfs/virtualfs.py`

**Release List Generation** (lines 1120-1162):
```python
elif p.view == "New":
    # Create matcher that filters to only new releases
    matcher = Matcher(["new"], Pattern("true", strict=True))

    # Query releases with this matcher
    releases = filter_releases(
        self.config,
        ...,
        matcher=matcher,
    )
```

**Track List Generation** (similar pattern):
```python
if p.view == "New":
    matcher = Matcher(["new"], Pattern("true", strict=True))

    tracks = filter_tracks(
        self.config,
        ...,
        matcher=matcher,
    )
```

**Pattern for Favorites**:
```python
elif p.view == "Favorites":
    matcher = Matcher(["favorite"], Pattern("true", strict=True))
    releases = filter_releases(self.config, ..., matcher=matcher)
```

---

### 5.4 Template Selection

**File**: `rose-vfs/rose_vfs/virtualfs.py`

**Template Switching Logic** (lines 449, 560, 581):
```python
elif release_parent.view == "New":
    template = self._config.path_templates.releases_new.release
```

**Pattern for Favorites**:
```python
elif release_parent.view == "Favorites":
    template = self._config.path_templates.releases_favorite.release
```

---

### 5.5 Classifier Hiding

**File**: `rose-vfs/rose_vfs/virtualfs.py`

**Hiding Genres/Descriptors/Labels** (lines 1186, 1195, 1204):
```python
# When listing genres:
for e1 in list_genres(self.config):
    if self.config.vfs.hide_genres_with_only_new_releases and e1.only_new_releases:
        continue
    # ... add genre to VirtualFS ...

# When listing descriptors:
for e2 in list_descriptors(self.config):
    if self.config.vfs.hide_descriptors_with_only_new_releases and e2.only_new_releases:
        continue
    # ... add descriptor to VirtualFS ...

# When listing labels:
for e3 in list_labels(self.config):
    if self.config.vfs.hide_labels_with_only_new_releases and e3.only_new_releases:
        continue
    # ... add label to VirtualFS ...
```

**Track Visibility Check** (line 1062):
```python
if (track := get_track(...)) and (p.view != "New" or track.release.new):
    # Allow track to be visible
```

**Pattern for Favorites**:
- Add checks for `hide_genres_with_only_favorite_releases`
- Add checks for `hide_descriptors_with_only_favorite_releases`
- Add checks for `hide_labels_with_only_favorite_releases`
- Add visibility check: `(p.view != "Favorites" or track.release.favorite)`

---

## 6. Rule Engine Integration

### 6.1 Tag Registration

**File**: `rose-py/rose/rule_parser.py`

**Queryable Tags** (line 78):
```python
ALL_QUERYABLE_TAGS = [
    "tracktitle",
    "albumtitle",
    # ... many tags ...
    "new",
    # ... more tags ...
]
```

**Expandable Tags** (lines 134):
```python
ALL_TAGS: dict[ExpandableTag, list[Tag]] = {
    "albumartist": ["albumartist", "albumartists"],
    # ... many mappings ...
    "new": ["new"],
    # ... more mappings ...
}
```

**Modifiable Tags** (line 182):
```python
MODIFIABLE_TAGS: list[Tag] = [
    "tracktitle",
    "albumtitle",
    # ... many tags ...
    "new",
    # ... more tags ...
]
```

**Single-Value Tags** (line 198):
```python
SINGLE_VALUE_TAGS: list[Tag] = [
    "tracktitle",
    "albumtitle",
    # ... many tags ...
    "new",
    # ... more tags ...
]
```

**Pattern for Favorites**:
- Add `"favorite"` to `ALL_QUERYABLE_TAGS`
- Add `"favorite": ["favorite"]` to `ALL_TAGS`
- Add `"favorite"` to `MODIFIABLE_TAGS`
- Add `"favorite"` to `SINGLE_VALUE_TAGS`

---

### 6.2 Rule Matching

**File**: `rose-py/rose/rules.py`

**Matching Logic** (lines 255-258):
```python
if not match and field == "new":
    if not datafile:
        datafile = _get_release_datafile_of_directory(tags.path.parent)
    match = matches_pattern(matcher.pattern, datafile.new)
```

**Usage Examples**:
```
new:true          # Match releases marked as new
new:false         # Match releases NOT marked as new
```

**Pattern for Favorites**:
```python
if not match and field == "favorite":
    if not datafile:
        datafile = _get_release_datafile_of_directory(tags.path.parent)
    match = matches_pattern(matcher.pattern, datafile.favorite)
```

**Usage Examples**:
```
favorite:true     # Match favorited releases
favorite:false    # Match non-favorited releases
```

---

### 6.3 Rule Actions

**File**: `rose-py/rose/rules.py`

**Action Execution** (lines 392-402):
```python
if field == "new":
    datafile = datafile or open_datafile(tags.path)
    v = execute_single_action(act, datafile.new)

    # Validate boolean conversion
    if v != "true" and v != "false":
        raise InvalidReplacementValueError(
            f"new must be 'true' or 'false', got {v!r}"
        )

    # Apply change
    orig_value = datafile.new
    datafile.new = v == "true"

    # Track change for display
    if orig_value != datafile.new:
        potential_datafile_changes.append(("new", orig_value, datafile.new))
```

**Usage Examples**:
```
new:true/replace:false    # Mark as not new
new:false/replace:true    # Mark as new
```

**Pattern for Favorites**:
```python
if field == "favorite":
    datafile = datafile or open_datafile(tags.path)
    v = execute_single_action(act, datafile.favorite)

    if v != "true" and v != "false":
        raise InvalidReplacementValueError(
            f"favorite must be 'true' or 'false', got {v!r}"
        )

    orig_value = datafile.favorite
    datafile.favorite = v == "true"

    if orig_value != datafile.favorite:
        potential_datafile_changes.append(("favorite", orig_value, datafile.favorite))
```

**Usage Examples**:
```
favorite:false/replace:true    # Mark as favorite
favorite:true/replace:false    # Unmark as favorite
```

---

## 7. CLI Interface

### 7.1 Toggle Command

**File**: `rose-cli/rose_cli/cli.py`

**Command Definition** (lines 253-259):
```python
@releases.command()
@click.argument("release", type=click.Path(), nargs=1)
@click.pass_obj
def toggle_new(ctx: Context, release: str) -> None:
    """Toggle a release's "new"-ness. Accepts a release's UUID/path."""
    release = parse_release_argument(release)
    toggle_release_new(ctx.config, release)
```

**Usage**:
```bash
rose releases toggle-new {release_id_or_path}
```

**Pattern for Favorites**:
```python
@releases.command()
@click.argument("release", type=click.Path(), nargs=1)
@click.pass_obj
def toggle_favorite(ctx: Context, release: str) -> None:
    """Toggle a release's "favorite" status. Accepts a release's UUID/path."""
    release = parse_release_argument(release)
    toggle_release_favorite(ctx.config, release)
```

**Usage**:
```bash
rose releases toggle-favorite {release_id_or_path}
```

---

## 8. Public API

### 8.1 Exported Functions

**File**: `rose-py/rose/__init__.py`

**Exports** (lines 110, 215):
```python
from rose.releases import (
    create_single_release,
    delete_release,
    edit_release,
    run_cache_for_release,
    toggle_release_new,  # ← Exported function
)

__all__ = [
    # ... many exports ...
    "toggle_release_new",
    # ... more exports ...
]
```

**Pattern for Favorites**:
- Add `toggle_release_favorite` to imports
- Add `"toggle_release_favorite"` to `__all__`

---

## 9. Testing

### 9.1 Unit Tests

**File**: `rose-py/rose/releases_test.py`

**Toggle Test** (lines 48-72):
```python
def test_toggle_release_new(config: Config) -> None:
    # Step 1: Create a test release
    release = get_release(config, "r1")
    assert release.new is True

    # Step 2: Toggle to False
    toggle_release_new(config, "r1")
    release = get_release(config, "r1")
    assert release.new is False

    # Step 3: Verify TOML file updated
    datafile_path = release.source_path / f".rose.{release.id}.toml"
    with open(datafile_path, "rb") as fp:
        data = tomllib.load(fp)
        assert data["new"] is False

    # Step 4: Toggle back to True
    toggle_release_new(config, "r1")
    release = get_release(config, "r1")
    assert release.new is True
```

**Pattern for Favorites**:
```python
def test_toggle_release_favorite(config: Config) -> None:
    release = get_release(config, "r1")
    assert release.favorite is False  # Defaults to False

    toggle_release_favorite(config, "r1")
    release = get_release(config, "r1")
    assert release.favorite is True

    # Verify TOML file
    datafile_path = release.source_path / f".rose.{release.id}.toml"
    with open(datafile_path, "rb") as fp:
        data = tomllib.load(fp)
        assert data["favorite"] is True

    # Toggle back
    toggle_release_favorite(config, "r1")
    release = get_release(config, "r1")
    assert release.favorite is False
```

---

### 9.2 Rule Engine Tests

**File**: `rose-py/rose/rules_test.py`

**Rule Test** (lines 197-223):
```python
def test_rules_fields_match_new(config: Config) -> None:
    # Create rule that matches new:false and changes to true
    rule = Rule.parse("new:false", ["replace:true"])

    # Get a release that is currently new=False
    release = get_release(config, "r1")

    # Execute rule
    matcher = rule.matcher
    actions = rule.actions
    # ... apply rule ...

    # Verify new=True after rule execution
    release = get_release(config, "r1")
    assert release.new is True
```

**Pattern for Favorites**:
```python
def test_rules_fields_match_favorite(config: Config) -> None:
    rule = Rule.parse("favorite:false", ["replace:true"])

    release = get_release(config, "r1")
    assert release.favorite is False

    # Execute rule
    # ... apply rule ...

    release = get_release(config, "r1")
    assert release.favorite is True
```

---

### 9.3 VirtualFS Tests

**File**: `rose-vfs/rose_vfs/virtualfs_test.py`

**Hiding Test** (lines 503-521):
```python
def test_virtual_filesystem_hide_new_release_classifiers(config: Config) -> None:
    # Enable hiding in config
    config.vfs.hide_genres_with_only_new_releases = True
    config.vfs.hide_descriptors_with_only_new_releases = True
    config.vfs.hide_labels_with_only_new_releases = True

    # Create releases where some classifiers have only new releases
    # ... setup test data ...

    # Mount VFS and verify hidden classifiers
    vfs = VirtualFS(config)
    genres = list(vfs.list_directory("2. Genres"))

    # Assert that genres with only new releases are hidden
    assert "OnlyNewGenre" not in [g.name for g in genres]
    assert "MixedGenre" in [g.name for g in genres]
```

**Pattern for Favorites**:
```python
def test_virtual_filesystem_hide_favorite_release_classifiers(config: Config) -> None:
    config.vfs.hide_genres_with_only_favorite_releases = True
    config.vfs.hide_descriptors_with_only_favorite_releases = True
    config.vfs.hide_labels_with_only_favorite_releases = True

    # ... setup test data ...

    vfs = VirtualFS(config)
    genres = list(vfs.list_directory("2. Genres"))

    assert "OnlyFavoriteGenre" not in [g.name for g in genres]
    assert "MixedGenre" in [g.name for g in genres]
```

---

## 10. Complete File Change Checklist

This section provides a comprehensive checklist for implementing "favorite" as a parallel to "new".

### Python Files (rose-py)

- [ ] **rose-py/rose/cache.py**
  - [ ] Add `favorite: bool` field to `Release` dataclass (line ~220)
  - [ ] Add `favorite: bool = False` field to `StoredDataFile` dataclass (line ~320)
  - [ ] Add `only_favorite_releases: bool` to `GenreEntry` (line ~2304)
  - [ ] Add `only_favorite_releases: bool` to `DescriptorEntry` (line ~2341)
  - [ ] Add `only_favorite_releases: bool` to `LabelEntry` (line ~2375)
  - [ ] Add `favorite` parameter to `filter_releases()` (line ~1807)
  - [ ] Add `favorite` parameter to `filter_tracks()` (line ~1827)
  - [ ] Duplicate `list_genres()` logic for favorite checking (line ~2310)
  - [ ] Duplicate `list_descriptors()` logic for favorite checking (line ~2347)
  - [ ] Duplicate `list_labels()` logic for favorite checking (line ~2381)
  - [ ] Add `favorite` reading in TOML deserialization (line ~650)
  - [ ] Add `favorite` writing in TOML serialization (line ~632)
  - [ ] Add `release.favorite = datafile.favorite` mapping (line ~665)

- [ ] **rose-py/rose/cache.sql**
  - [ ] Add `favorite BOOLEAN NOT NULL DEFAULT false` column to `releases` table
  - [ ] Add `CREATE INDEX releases_favorite ON releases(favorite);`
  - [ ] Add `favorite` field to `rules_engine_fts` virtual table
  - [ ] Add `r.favorite` to `releases_view` SELECT clause
  - [ ] Create migration script for existing databases

- [ ] **rose-py/rose/releases.py**
  - [ ] Create `toggle_release_favorite(c: Config, release_id: str) -> None` function
  - [ ] Integrate `favorite` into metadata editor (if applicable)
  - [ ] Decide: Should extracted singles default to favorite? (Probably not)

- [ ] **rose-py/rose/config.py**
  - [ ] Add `hide_genres_with_only_favorite_releases: bool` to `VirtualFSConfig`
  - [ ] Add `hide_descriptors_with_only_favorite_releases: bool` to `VirtualFSConfig`
  - [ ] Add `hide_labels_with_only_favorite_releases: bool` to `VirtualFSConfig`
  - [ ] Parse these fields from `[vfs]` section with default `False`

- [ ] **rose-py/rose/templates.py**
  - [ ] Add `releases_favorite: PathTemplateTriad` to `PathTemplateConfig` (line ~196)
  - [ ] Create default template with `{% if favorite %}[FAVORITE]{% endif %}`
  - [ ] Add `"favorite": release.favorite` to `build_template_context()` (line ~365)
  - [ ] Parse `releases_favorite` templates from config TOML

- [ ] **rose-py/rose/rule_parser.py**
  - [ ] Add `"favorite"` to `ALL_QUERYABLE_TAGS` (line ~78)
  - [ ] Add `"favorite": ["favorite"]` to `ALL_TAGS` (line ~134)
  - [ ] Add `"favorite"` to `MODIFIABLE_TAGS` (line ~182)
  - [ ] Add `"favorite"` to `SINGLE_VALUE_TAGS` (line ~198)

- [ ] **rose-py/rose/rules.py**
  - [ ] Add matching logic for `field == "favorite"` (line ~255)
  - [ ] Add action execution for `field == "favorite"` (line ~392)
  - [ ] Include validation: `if v != "true" and v != "false": raise`

- [ ] **rose-py/rose/__init__.py**
  - [ ] Import `toggle_release_favorite`
  - [ ] Add `"toggle_release_favorite"` to `__all__`

### VirtualFS Files (rose-vfs)

- [ ] **rose-vfs/rose_vfs/virtualfs.py**
  - [ ] Add `"Favorites"` to view Literal types (line ~184, ~286)
  - [ ] Add path parser for `"1. Releases - Favorites"` (line ~288)
  - [ ] Add release filtering: `matcher = Matcher(["favorite"], Pattern("true", strict=True))` (line ~1120)
  - [ ] Add track filtering with same matcher (similar to line ~1120)
  - [ ] Add template selection: `elif release_parent.view == "Favorites": template = ...` (line ~560)
  - [ ] Add genre hiding check: `if config.vfs.hide_genres_with_only_favorite_releases and e1.only_favorite_releases` (line ~1186)
  - [ ] Add descriptor hiding check (line ~1195)
  - [ ] Add label hiding check (line ~1204)
  - [ ] Add track visibility check: `(p.view != "Favorites" or track.release.favorite)` (line ~1062)

### CLI Files (rose-cli)

- [ ] **rose-cli/rose_cli/cli.py**
  - [ ] Create `toggle_favorite` command (parallel to line ~253)
  - [ ] Implement command: call `toggle_release_favorite(ctx.config, release)`

### Test Files

- [ ] **rose-py/rose/releases_test.py**
  - [ ] Create `test_toggle_release_favorite()` (parallel to line ~48)
  - [ ] Verify TOML file updates
  - [ ] Test toggling both directions

- [ ] **rose-py/rose/rules_test.py**
  - [ ] Create `test_rules_fields_match_favorite()` (parallel to line ~197)
  - [ ] Test matching `favorite:true` and `favorite:false`
  - [ ] Test actions: `favorite:false/replace:true`

- [ ] **rose-vfs/rose_vfs/virtualfs_test.py**
  - [ ] Create `test_virtual_filesystem_hide_favorite_release_classifiers()` (parallel to line ~503)
  - [ ] Verify favorites view filtering
  - [ ] Test classifier hiding

### Documentation Files

- [ ] **README.md** (if exists)
  - [ ] Document "favorite" feature
  - [ ] Add CLI command examples

- [ ] **Config Documentation**
  - [ ] Document `hide_*_with_only_favorite_releases` options
  - [ ] Document `releases_favorite` templates

- [ ] **Rule Engine Documentation**
  - [ ] Document `favorite:true` and `favorite:false` matchers
  - [ ] Document `favorite/replace:true` actions

---

## 11. Implementation Sequence

Recommended order for implementing the "favorite" feature:

### Phase 1: Data Layer (No user-facing changes)
1. Update `cache.sql` schema
2. Create database migration script
3. Update `cache.py` data models (`Release`, `StoredDataFile`, `*Entry`)
4. Update TOML serialization/deserialization
5. Add tests for data layer

### Phase 2: Business Logic
6. Implement `toggle_release_favorite()` in `releases.py`
7. Add filtering parameters to `filter_releases()` and `filter_tracks()`
8. Add `list_*()` functions for favorite checking
9. Add tests for business logic

### Phase 3: Configuration
10. Add config fields to `config.py`
11. Update config parsing
12. Add tests for config parsing

### Phase 4: Templates
13. Add `releases_favorite` templates to `templates.py`
14. Add `favorite` to template context
15. Create default templates with `[FAVORITE]` suffix
16. Add tests for template rendering

### Phase 5: Rule Engine
17. Register `favorite` tags in `rule_parser.py`
18. Add matching logic in `rules.py`
19. Add action logic in `rules.py`
20. Add tests for rule matching and actions

### Phase 6: VirtualFS
21. Add "Favorites" view to `virtualfs.py`
22. Add path parsing for favorites view
23. Add filtering logic (matcher creation)
24. Add template selection
25. Add classifier hiding logic
26. Add tests for VirtualFS

### Phase 7: CLI
27. Add `toggle-favorite` command to `cli.py`
28. Add tests for CLI command

### Phase 8: Integration & Documentation
29. Export function in `__init__.py`
30. Run full integration tests
31. Update documentation
32. Add migration guide for users

---

## 12. Key Differences & Design Decisions

### Default Values
- **New**: Defaults to `true` (new releases are marked automatically)
- **Favorite**: Should default to `false` (favorites are opt-in by user)

### Naming Conventions
- SQL column: `favorite`
- Python field: `favorite`
- Config fields: `hide_*_with_only_favorite_releases`
- Template config: `releases_favorite`
- VirtualFS view: `"Favorites"`
- Display name: `"1. Releases - Favorites"`
- CLI command: `toggle-favorite`

### User Experience Questions (To Be Decided)

1. **Template Formatting**: How should favorites be displayed?
   - Option A: `[FAVORITE]` suffix (like `[NEW]`)
   - Option B: `★` emoji/symbol prefix
   - Option C: Custom user-defined format
   - **Recommendation**: Allow user customization via templates

2. **View Ordering**: Where should "Favorites" appear in VirtualFS hierarchy?
   - Option A: After "New" (e.g., `"2. Releases - Favorites"`)
   - Option B: Before "New" (e.g., `"1. Releases - Favorites"`)
   - **Recommendation**: User-configurable order

3. **Classifier Hiding**: Should hiding favorite-only classifiers be enabled by default?
   - **Recommendation**: Default to `false` (consistent with "new")

4. **Extracted Singles**: Should singles created from albums inherit favorite status?
   - **Recommendation**: No, extracted singles should default to `favorite=false`

5. **Bulk Operations**: Should there be a bulk favorite/unfavorite command?
   - **Recommendation**: Yes, add later as enhancement (e.g., `rose releases favorite-all {pattern}`)

6. **Metadata Editor**: Should favorite status be editable in the metadata editor?
   - **Recommendation**: Yes, add `favorite` field to editor (parallel to `new`)

---

## 13. Migration Considerations

### Database Migration

Users with existing Rose databases will need a migration script:

```python
def migrate_add_favorite_column(conn: sqlite3.Connection) -> None:
    """Add 'favorite' column to existing releases table."""
    # Check if column exists
    cursor = conn.execute("PRAGMA table_info(releases)")
    columns = [row[1] for row in cursor.fetchall()]

    if "favorite" not in columns:
        # Add column with default value
        conn.execute("ALTER TABLE releases ADD COLUMN favorite BOOLEAN NOT NULL DEFAULT false")

        # Create index
        conn.execute("CREATE INDEX releases_favorite ON releases(favorite)")

        # Update FTS table
        conn.execute("INSERT INTO rules_engine_fts(rules_engine_fts) VALUES('rebuild')")

        conn.commit()
```

### TOML File Migration

Existing `.rose.{uuid}.toml` files don't need migration:
- The TOML parser already handles missing keys with default values
- `favorite` will default to `false` when reading old files
- Files will be updated with `favorite` field on next toggle or write

### Config File Migration

Users should be informed to add new config options:

```toml
[vfs]
hide_genres_with_only_favorite_releases = false
hide_descriptors_with_only_favorite_releases = false
hide_labels_with_only_favorite_releases = false

[path_templates]
releases_favorite.release = """
{{ releaseartists | artistsfmt }} -
{% if releasedate %}{{ releasedate.year }}.{% endif %}
{{ releasetitle }}
{% if releasetype == "single" %}- {{ releasetype | releasetypefmt }}{% endif %}
★
"""
# ... similar for releasetrack and collectiontrack ...
```

---

## 14. Testing Strategy

### Unit Tests (Per Component)
- Data model serialization/deserialization
- Toggle function behavior
- Filtering functions with `favorite` parameter
- Rule matching and actions
- Config parsing
- Template rendering

### Integration Tests
- End-to-end: Toggle → Read TOML → Query database → Verify cache
- VirtualFS: Mount → Browse favorites view → Verify filtering
- Rule engine: Create rule → Execute → Verify favorite status changes

### Edge Cases to Test
- Toggling favorite on non-existent release
- Toggling when TOML file is missing
- Toggling when TOML file is corrupted
- Filtering with both `new=true` and `favorite=true`
- Rule matching with complex patterns (e.g., `favorite:true AND genre:rock`)
- VirtualFS with all hiding options enabled
- Template rendering with missing `favorite` key (backward compatibility)

---

## 15. Performance Considerations

### Database Indexing
- `CREATE INDEX releases_favorite ON releases(favorite)` is **critical**
- Without index, filtering favorites will be slow on large libraries

### Query Optimization
- The `list_*()` functions with `only_favorite_releases` use LEFT JOIN
- These queries are already optimized for "new", same pattern applies

### TOML File I/O
- Toggling favorite requires reading + writing one TOML file
- No performance concerns (similar to "new")

### VirtualFS Caching
- VirtualFS already caches release metadata
- Adding `favorite` field has negligible memory impact

---

## 16. Extensibility

### Future Enhancements

1. **Multiple Tags**: Instead of just `favorite`, support arbitrary tags like `wishlist`, `owned`, `loaned`
   - Would require schema change: `release_tags` table instead of boolean columns
   - Significant architectural change, not recommended for initial implementation

2. **Rating System**: 5-star ratings instead of boolean
   - Could coexist with `favorite` (e.g., `favorite` = 5-star rating)
   - Requires UI changes for input

3. **Date-Based Favorites**: Track when a release was favorited
   - Add `favorited_at` timestamp to `StoredDataFile`
   - Enable "Recently Favorited" view

4. **Smart Favorites**: Auto-favorite based on play count or other metrics
   - Requires play tracking (separate feature)
   - Could use rule engine: `playcount:>100/replace:true` → `favorite/replace:true`

---

## 17. Code Snippets for Reference

### Complete Toggle Function Template

```python
def toggle_release_favorite(c: Config, release_id: str) -> None:
    """Toggle a release's "favorite" status."""
    with contextlib.chdir(c.music_source_dir):
        # Step 1: Resolve release
        release = get_release(c, release_id)

        # Step 2: Read datafile
        datafile_path = release.source_path / f".rose.{release.id}.toml"
        with datafile_path.open("rb") as fp:
            diskdata = tomllib.load(fp)

        datafile = StoredDataFile(
            favorite=diskdata.get("favorite", False),
            new=diskdata.get("new", True),
            added_at=diskdata["added_at"],
        )

        # Step 3: Toggle
        datafile.favorite = not datafile.favorite

        # Step 4: Write to disk
        toml_data = {
            "favorite": datafile.favorite,
            "new": datafile.new,
            "added_at": datafile.added_at,
        }

        with datafile_path.open("w") as fp:
            tomli_w.dump(toml_data, fp)

        # Step 5: Update cache
        with connect(c) as conn:
            conn.execute(
                "UPDATE releases SET favorite = ? WHERE id = ?",
                (datafile.favorite, release.id),
            )
            conn.commit()
```

### Complete Rule Matching Template

```python
if not match and field == "favorite":
    if not datafile:
        datafile = _get_release_datafile_of_directory(tags.path.parent)
    match = matches_pattern(matcher.pattern, datafile.favorite)
```

### Complete Rule Action Template

```python
if field == "favorite":
    datafile = datafile or open_datafile(tags.path)
    v = execute_single_action(act, datafile.favorite)

    if v != "true" and v != "false":
        raise InvalidReplacementValueError(
            f"favorite must be 'true' or 'false', got {v!r}"
        )

    orig_value = datafile.favorite
    datafile.favorite = v == "true"

    if orig_value != datafile.favorite:
        potential_datafile_changes.append(("favorite", orig_value, datafile.favorite))
```

---

## 18. Summary

The "new" release feature is a comprehensive boolean flag system that touches:
- **10+ files** across 3 Python packages (`rose-py`, `rose-vfs`, `rose-cli`)
- **6 major subsystems**: Data models, storage, configuration, templates, VirtualFS, rule engine
- **3 user interfaces**: CLI commands, VirtualFS views, rule engine syntax

Implementing "favorite" as a parallel feature requires:
- **~50-70 discrete code changes** across the codebase
- **Database schema migration** for existing users
- **Comprehensive test coverage** (~10-15 new test functions)
- **Documentation updates** for CLI, config, and rules

The architecture is **well-designed for extensibility**: adding "favorite" follows the exact same patterns as "new", making implementation straightforward but thorough.

**Estimated effort**: 2-3 days for implementation + testing, assuming familiarity with the codebase.

---

## Appendix: Full File Paths Reference

```
rose-py/rose/cache.py              # Core data models, TOML I/O, filtering
rose-py/rose/cache.sql             # Database schema
rose-py/rose/releases.py           # Toggle function, release operations
rose-py/rose/config.py             # Configuration parsing
rose-py/rose/templates.py          # Path templates, Jinja2 rendering
rose-py/rose/rule_parser.py        # Rule engine tag registration
rose-py/rose/rules.py              # Rule matching and action execution
rose-py/rose/__init__.py           # Public API exports

rose-vfs/rose_vfs/virtualfs.py     # Virtual filesystem views and filtering

rose-cli/rose_cli/cli.py           # CLI command definitions

rose-py/rose/releases_test.py      # Toggle and release operation tests
rose-py/rose/rules_test.py         # Rule engine tests
rose-vfs/rose_vfs/virtualfs_test.py  # VirtualFS tests
```

---

**End of Report**
