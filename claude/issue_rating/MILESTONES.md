# Rating Feature — Milestones

## M1: DB Schema + Data Models

**Scope**: `cache.sql`, `cache.py` (StoredDataFile, Release, cached_release_from_view)

**Changes**:
- Add `rating INTEGER` column to `releases` table in cache.sql
- Add `CREATE INDEX releases_rating ON releases(rating)` in cache.sql
- Add `rating` to `rules_engine_fts` FTS5 table in cache.sql
- Add `r.rating` to `releases_view` in cache.sql
- Add `rating: int | None = None` to `StoredDataFile` dataclass
- Add `rating: int | None` to `Release` dataclass
- Add `rating=row["rating"]` to `cached_release_from_view()`
- Update cache read logic to read `rating` from TOML
- Update SQL INSERT/UPDATE for releases to include `rating`
- Update FTS sync to include `rating`
- Update any `Release(...)` constructor calls (e.g. sample music) with `rating=None`

**Tests**: Existing cache tests should still pass. DB schema hash will change, triggering full rebuild.

## M2: set_release_rating() Function + Public API

**Scope**: `releases.py`, `__init__.py`

**Changes**:
- New `set_release_rating(c: Config, release_id: str, rating: int | None) -> None` function
- Validates rating is 1-100 or None
- Reads TOML, sets/removes `rating` key, writes back, refreshes cache
- Export from `__init__.py`

**Tests**: Manual verification; function follows same pattern as `toggle_release_favorite()`.

## M3: Templates — rating exposed to Jinja context

**Scope**: `templates.py`

**Changes**:
- Add `"rating": release.rating` to `_calc_release_variables()`
- Add `"rating": track.release.rating` to `_calc_track_variables()`
- Update sample release objects in `get_sample_music()` if they construct Release directly

**Tests**: Template tests should pass with new variable available.

## M4: Rule Engine — tag registration, matching, action

**Scope**: `rule_parser.py`, `rules.py`

**Changes**:
- Add `"rating": ["rating"]` to `ALL_TAGS`
- Add `"rating"` to `MODIFIABLE_TAGS`
- Add `rating` to `_get_release_datafile_of_directory()` StoredDataFile construction
- Add `rating` field handling in `execute_metadata_actions()` — validate integer 1-100 or empty
- Add rating matching in track-level and release-level matching functions

**Tests**: Rule engine tests should pass.

## M5: CLI Command + Metadata Editor

**Scope**: `cli.py`, `releases.py` (MetadataRelease, edit_release)

**Changes**:
- Add `rating: int | None` to `MetadataRelease` dataclass
- Update `MetadataRelease.from_cache()`, `.serialize()`, `.from_toml()`
- Update `edit_release()` to detect and apply rating changes
- Import `set_release_rating` in cli.py
- Add `set-rating` CLI command: `rose releases set-rating <release> <rating>`
- Support `--clear` flag to remove rating

**Tests**: Manual CLI testing.

## M6: JSON Dump Integration

**Scope**: `dump.py`

**Changes**:
- Add `"rating": r.rating` to `release_to_json()`
- Add `"rating": t.release.rating` to `track_to_json()` (in the with_release_info block)

**Tests**: Verify dump output includes rating.

## M7: Final Validation

**Scope**: Full test suite

**Steps**:
- Run `just lint` — all checks pass
- Run `just test` — all tests pass
- Manual verification of CLI commands
