# 004 — Migration Milestones (v2)

Revised after incorporating all feedback from `003-review-feedback.md`.

**How to use this file:**
- Work top-to-bottom. Dependencies flow downward.
- Mark `[x]` when a milestone is done and its tests pass.
- Each milestone has: what to do, what Python file to read, what to produce, and the
  "done-when" condition.
- `001-PLAN.md` has the full rationale. This file is the execution checklist.

**Hard constraints (always apply):**
1. On-disk audio tag structure must be identical to Python version.
2. On-disk `.rose.{uuid}.toml` datafile format must be identical to Python version
   (key names, `-1` for null rating, ISO8601 `added_at`, boolean `new`/`favorite`).
3. Feature power must be equal or superior to Python version.
4. Config/CLI interface can change freely.
5. A PyO3 Python scripting shim is required as a permanent deliverable.

**Cross-cutting decisions (apply from Phase 1 onward):**
- **Logging:** Use `tracing` crate from the start (structured, async-compatible).
- **Path normalization:** Always normalize to NFC before comparison and storage.
- **Error recovery in parallel code:** `rayon` tasks return `Result`. Errors are collected
  and reported after all valid work completes. Never panic on a single bad file.

---

## Phase 1: Project Scaffolding

- [ ] **M-1.1** Create Cargo workspace.
  - Produce: `rose-rs/Cargo.toml` (workspace), `rose-rs/rose-core/Cargo.toml`,
    `rose-rs/rose-core/src/lib.rs`.
  - Add initial deps: `serde`, `sha2`, `regex`, `unicode-normalization`, `tracing`,
    `thiserror`.
  - Done-when: `cargo build` and `cargo clippy` succeed.

- [ ] **M-1.2** Port `common.rs`.
  - Read: `rose-py/rose/common.py` (231 lines).
  - Produce: `rose-rs/rose-core/src/common.rs`.
  - Port: `RoseError` hierarchy (use `thiserror`), `Artist`, `ArtistMapping` (with
    `.all()`, `.dump()`, `.items()`), `sanitize_dirname`, `sanitize_filename`,
    `sha256_dataclass`, `flatten`, `uniq`, `VERSION`, `ILLEGAL_FS_CHARS_REGEX`.
  - Done-when: `cargo test` passes with tests for:
    - `sanitize_dirname`: illegal chars, max-length truncation at byte boundary,
      diacritics stripping, NFC normalization, empty input, all-illegal input.
    - `sanitize_filename`: same + extension preservation, long extension (>6 bytes).
    - `ArtistMapping::all()`: dedup + ordering across roles.
    - `sha256_dataclass`: determinism.
    - `flatten`, `uniq`: basic correctness.

- [ ] **M-1.3** Port `genre_hierarchy.rs`.
  - Read: `rose-py/rose/genre_hierarchy.py`, `scripts/rym-genres/generate.py`.
  - Note: generator already writes `rose-rs/src/genre_hierarchy.json`. Use
    `serde_json::from_str(include_str!(...))` at compile time.
  - Produce: `rose-rs/rose-core/src/genre_hierarchy.rs`.
  - Port: `GENRE_HIERARCHY` (child->parents), `TRANSITIVE_CHILD_GENRES`
    (parent->all transitive children).
  - Done-when: `cargo test` passes. Known genre returns expected parents; known parent
    returns expected children.

---

## Phase 2: Audio Tags

- [ ] **M-2.0** Generate golden files for audio tag verification.
  - Write a Python script that dumps every tag from every file in `testdata/Tagger/` as
    JSON (field-level) and raw bytes (for custom tags `roseid`, `rosereleaseid`).
  - Produce: `rose-rs/testdata/golden/tags_*.json` (one per format).
  - Done-when: Golden files committed. Includes all 5 formats.

- [ ] **M-2.1** Port `audiotags.rs` — tag reading.
  - Read: `rose-py/rose/audiotags.py` (629 lines).
  - Add dep: `lofty` (or alternative if issues arise).
  - Produce: `rose-rs/rose-core/src/audiotags.rs`.
  - Port read path: `AudioTags::from_file()` for FLAC, MP3, M4A, OGG Vorbis, OGG Opus.
    All fields: `id`, `release_id`, `title`, `artists` (with role parsing), `date`,
    `originaldate`, `compositiondate`, `album`, `releasetype`, `genre`, `secondary_genre`,
    `descriptor`, `label`, `catalognumber`, `tracknumber`, `tracktotal`, `discnumber`,
    `disctotal`, `duration_seconds`, `edition`, `rating`.
  - Port: `RoseDate` type (year, month, day — all optional except year).
  - Port: `SUPPORTED_AUDIO_EXTENSIONS`.
  - Port: Artist string parsing (`;`-delimited, role-prefixed).
  - Done-when: `cargo test` passes reading all 5 files in `testdata/Tagger/` and asserting
    against golden files from M-2.0. Custom tags match at byte level.

- [ ] **M-2.2** Port `audiotags.rs` — tag writing.
  - Port write path: `AudioTags::flush()` for all 5 formats. All writable fields.
  - Port: ID assignment (`maybe_set_ids`).
  - Done-when: `cargo test` passes with roundtrip tests (read, modify, write, re-read,
    assert) for all 5 formats. `roseid`/`rosereleaseid` survive roundtrip. ID assignment
    works on files with no existing IDs.

---

## Phase 3: Rule Parser

- [ ] **M-3.1** Port `rule_parser.rs`.
  - Read: `rose-py/rose/rule_parser.py` (846 lines).
  - Produce: `rose-rs/rose-core/src/rule_parser.rs`.
  - Port: `Matcher`, `Pattern`, `Action` (all subtypes: `ReplaceAction`, `SedAction`,
    `SplitAction`, `AddAction`, `DeleteAction`), `Rule`.
  - Port: `parse_matcher()`, `parse_action()`, `parse_rule()`.
  - Port: All tag field name constants and mappings.
  - Done-when: `cargo test` passes. All test cases from `rule_parser_test.py` (544 lines)
    translated to Rust. Add `proptest`: arbitrary strings never panic.

---

## Phase 4: Templates

- [ ] **M-4.1** Port `templates.rs`.
  - Read: `rose-py/rose/templates.py` (588 lines).
  - Add dep: `minijinja`.
  - Produce: `rose-rs/rose-core/src/templates.rs`.
  - Port: `PathTemplate`, `PathContext`, `PathTemplateConfig` (with defaults),
    `evaluate_release_template`, `evaluate_track_template`, `get_sample_music`.
  - Port: All custom Jinja2 filters.
  - Done-when: `cargo test` passes. Golden file tests against Python output for all
    template contexts.

---

## Phase 5: Config

- [ ] **M-5.1** Port `config.rs`.
  - Read: `rose-py/rose/config.py` (609 lines).
  - Add deps: `toml`, `serde`, `dirs`.
  - Produce: `rose-rs/rose-core/src/config.rs`.
  - Port: `Config` struct with all fields, `VirtualFSConfig`, defaults,
    `PathTemplateConfig` integration. Config loading from TOML file.
  - Port: All validation (invalid values, missing keys, etc.).
  - Port: Artist alias resolution (`artist_aliases_map`, `artist_aliases_parents_map`).
  - Note: Config format can change, but all *knobs* must be present.
  - Done-when: `cargo test` passes. Valid config loads, missing keys error, invalid values
    error, defaults applied. Port representative cases from `config_test.py` (682 lines).

---

## Phase 6: Cache — Schema, Types, and Reads

- [ ] **M-6.1** Cache schema bootstrapping.
  - Read: `rose-py/rose/cache.py` (top ~200 lines), `cache.sql` (290 lines).
  - Add dep: `rusqlite` (with `bundled` feature).
  - Produce: `rose-rs/rose-core/src/cache.rs` (initial).
  - Copy: `cache.sql` to `rose-rs/rose-core/src/cache.sql`.
  - Port: Schema creation, `_schema_hash` table, schema hash validation,
    `maybe_invalidate_cache_database`.
  - Done-when: `cargo test` passes. Fresh DB created, schema hash mismatch triggers
    rebuild, version mismatch triggers rebuild.

- [ ] **M-6.2** Cache data types.
  - Port: `Release`, `Track`, `Collage`, `Playlist`, `GenreEntry`, `DescriptorEntry`,
    `LabelEntry` structs. Row deserialization from SQLite views.
  - Port: `StoredDataFile` — the `.rose.{uuid}.toml` sidecar format. Serialization must
    be byte-compatible with Python (key names: `new`, `favorite`, `rating`, `added_at`;
    `-1` for null rating; ISO8601 datetime).
  - Port: `STORED_DATA_FILE_REGEX` for UUID extraction from filename.
  - Port: `make_release_logtext`, `make_track_logtext`.
  - Done-when: Types compile with `Debug`, `Clone`. `StoredDataFile` roundtrip test
    (serialize -> parse -> assert equality). Golden file test: serialize a known
    `StoredDataFile` and compare output to Python.

- [ ] **M-6.3** Cache read queries (simple).
  - Port: `get_release`, `get_track`, `get_collage`, `get_playlist`.
  - Port: `list_releases`, `list_tracks`, `list_collages`, `list_playlists`.
  - Port: `list_artists`, `list_genres`, `list_labels`, `list_descriptors`.
  - Port: `artist_exists`, `genre_exists`, `label_exists`, `descriptor_exists`.
  - Port: `get_collage_releases`, `get_playlist_tracks`.
  - Port: `get_tracks_of_release`, `get_tracks_of_releases`.
  - Port: `release_within_collage`, `track_within_release`, `track_within_playlist`.
  - Done-when: `cargo test` passes. Seed DB (equivalent to `conftest._seed_cache`) and
    test every read function.

- [ ] **M-6.4** Cache filtered queries (`filter_releases`, `filter_tracks`).
  - Read: `rose-py/rose/cache.py:1770-1982`.
  - Port: `filter_releases` — dynamic SQL query builder with 8 filter dimensions
    (release_artist, all_artist, genre, descriptor, label, release_type, new, favorite).
    Includes artist alias resolution via `_get_all_artist_aliases` and genre hierarchy
    expansion via `TRANSITIVE_CHILD_GENRES`.
  - Port: `filter_tracks` — same pattern with track-level filters plus release-level
    join.
  - Done-when: `cargo test` passes. Test each filter dimension individually and in
    combination. Test artist alias resolution. Test genre hierarchy expansion.

---

## Phase 7: Cache — Update Pipeline

- [ ] **M-7.1a** Cache update — directory scanning and UUID discovery.
  - Read: `rose-py/rose/cache.py:490-1349` (the `_update_cache_for_releases_executor`).
  - Port: Scan `music_source_dir` for release directories. For each, extract UUID from
    `.rose.{uuid}.toml` filename via `STORED_DATA_FILE_REGEX`. Handle first-time releases
    (no existing datafile — create one with new UUID).
  - Port: "In-progress" directory detection (anti-race for copies in progress).
  - Done-when: `cargo test` passes. Given a temp dir with release dirs (some with
    datafiles, some without), scanning discovers all releases and assigns UUIDs correctly.

- [ ] **M-7.1b** Cache update — mtime comparison and change detection.
  - Port: For each discovered release, compare mtime of datafile and audio files against
    cached values. Identify releases/tracks that need re-reading.
  - Done-when: `cargo test` passes. Unchanged files skipped. Touched files detected.

- [ ] **M-7.1c** Cache update — tag reading and metadata derivation.
  - Port: For changed releases, read audio tags from all tracks. Derive release-level
    metadata from the "first" audio file (title, artists, genre, etc.).
  - Port: `StoredDataFile` reading (parse `.rose.{uuid}.toml` for new/favorite/rating).
  - Done-when: `cargo test` passes. After reading, release metadata matches expected.

- [ ] **M-7.1d** Cache update — source directory renaming.
  - Port: When `rename_source_files` config is enabled, rename source directory to match
    template. Handle collision avoidance.
  - Done-when: `cargo test` passes. Release dir renamed according to template.

- [ ] **M-7.1e** Cache update — batch SQL writes.
  - Port: Accumulate all mutations, execute batched SQL inserts/updates/deletes.
  - Port: `update_cache_for_collages`, `update_cache_for_playlists`.
  - Port: `update_cache_evict_nonexistent_releases/collages/playlists`.
  - Done-when: `cargo test` passes. Full end-to-end: copy `testdata/` releases into temp
    dir, run `update_cache`, verify DB state matches expected.

- [ ] **M-7.2** Cache update — parallel with `rayon`.
  - Add dep: `rayon`.
  - Port: Sharding logic — when release count > threshold, distribute work across threads.
  - **Error handling:** Tasks return `Result`. Errors collected into a `Vec`. Update
    completes for all valid releases. Errors reported after.
  - Done-when: `cargo test` passes. Same test as M-7.1e but with parallelism. Results
    identical to single-threaded. Also test: one corrupt release doesn't abort others.

- [ ] **M-7.3** Cache locking.
  - Port: Advisory locks via `locks` table. `lock()` function (acquire, check expiry,
    release).
  - Port: `release_lock_name`, `collage_lock_name`, `playlist_lock_name`.
  - Done-when: `cargo test` passes. Lock acquired, blocks second acquire, released,
    expired lock can be taken.

- [ ] **M-7.4** Cache FTS index.
  - Port: `process_string_for_fts` (char-level tokenization with `¬` separator).
  - Port: FTS index sync at end of cache update.
  - Done-when: `cargo test` passes. After cache update, FTS substring queries return
    expected results.

---

## Phase 8: Rules Engine

- [ ] **M-8.1** Port `rules.rs`.
  - Read: `rose-py/rose/rules.py` (898 lines).
  - Produce: `rose-rs/rose-core/src/rules.rs`.
  - Port: `execute_metadata_rule` (match tracks via FTS, apply actions).
  - Port: All action types: replace, sed, split, add, delete.
  - Port: `execute_stored_metadata_rules`.
  - Port: Dry-run mode, confirmation prompting, `--yes` bypass.
  - Port: `TrackTagNotAllowedError`, `InvalidReplacementValueError`.
  - Done-when: `cargo test` passes. All cases from `rules_test.py` (474 lines) ported.
    Each action type tested on each applicable tag field.

---

## Phase 9: Release and Track Operations

- [ ] **M-9.1** Port `releases.rs`.
  - Read: `rose-py/rose/releases.py` (641 lines).
  - Produce: `rose-rs/rose-core/src/releases.rs`.
  - Port: `toggle_release_new`, `toggle_release_favorite`, `set_release_rating`.
  - Port: `delete_release` (uses `send2trash` -> `trash` crate).
  - Port: `set_release_cover_art`, `delete_release_cover_art`.
  - Port: `edit_release` — opens `$EDITOR` with temp file, waits for user, parses
    changes, applies. Handle: editor exits non-zero, file unchanged, parse error.
  - Port: `create_single_release`.
  - Port: `find_releases_matching_rule`, `run_actions_on_release`.
  - Done-when: `cargo test` passes. Port cases from `releases_test.py` (511 lines).

- [ ] **M-9.2** Port `tracks.rs`.
  - Read: `rose-py/rose/tracks.py` (67 lines).
  - Produce: `rose-rs/rose-core/src/tracks.rs`.
  - Port: `find_tracks_matching_rule`, `run_actions_on_track`.
  - Done-when: `cargo test` passes. Port cases from `tracks_test.py` (42 lines) + add
    coverage for all matcher tag fields.

---

## Phase 10: Collage and Playlist Operations

- [ ] **M-10.1** Port `collages.rs`.
  - Read: `rose-py/rose/collages.py` (169 lines).
  - Produce: `rose-rs/rose-core/src/collages.rs`.
  - Port: `create_collage`, `rename_collage`, `delete_collage`.
  - Port: `add_release_to_collage`, `remove_release_from_collage`.
  - Port: `edit_collage_in_editor` — opens `$EDITOR`, parses changes. Handle: editor
    failure, unchanged file, parse errors, release description mismatches.
  - Done-when: `cargo test` passes. Port cases from `collages_test.py` (172 lines).

- [ ] **M-10.2** Port `playlists.rs`.
  - Read: `rose-py/rose/playlists.py` (236 lines).
  - Produce: `rose-rs/rose-core/src/playlists.rs`.
  - Port: `create_playlist`, `rename_playlist`, `delete_playlist`.
  - Port: `add_track_to_playlist`, `remove_track_from_playlist`.
  - Port: `edit_playlist_in_editor` — same editor pattern as collages.
  - Port: `set_playlist_cover_art`, `delete_playlist_cover_art`.
  - Done-when: `cargo test` passes. Port cases from `playlists_test.py` (253 lines).

---

## Phase 11: FUSE Virtual Filesystem

- [ ] **M-11.1** Create `rose-vfs` binary crate.
  - Add dep: `fuser`.
  - Produce: `rose-rs/rose-vfs/Cargo.toml`, `rose-rs/rose-vfs/src/main.rs`.
  - Implement: Mount/unmount lifecycle, signal handling.
  - Done-when: Empty filesystem mounts and unmounts cleanly.

- [ ] **M-11.2** Port VFS — virtual path parser and directory structure.
  - Read: `rose-vfs/rose_vfs/virtualfs.py` (2,089 lines), `docs/ARCHITECTURE.md`.
  - Port: `VirtualPath` type with all 12 view types: `Root`, `Releases`, `Artists`,
    `Genres`, `Descriptors`, `Labels`, `Loose Tracks`, `Collages`, `Playlists`, `New`,
    `Favorites`, `Added On`, `Released On`.
  - Port: Path parsing (`VirtualPath::parse` from string path).
  - Port: `VirtualNameGenerator` (bidirectional name-to-entity mapping with TTL cache).
  - Done-when: `cargo test` passes. Parse all documented path formats. Roundtrip
    parse->render.

- [ ] **M-11.3** Port VFS — `RoseLogicalCore` (domain logic layer).
  - Port: All `readdir` logic — maps virtual paths to cache queries. Uses
    `filter_releases`/`filter_tracks` for filtered views.
  - Port: Date-bucketing for "Added On" and "Released On" views.
  - Port: `CanShower` whitelist/blacklist logic.
  - Port: `hide_*_with_only_new_releases` options.
  - Done-when: `cargo test` passes. `readdir` at each view level returns expected entries
    from seeded cache. Filtered views correct.

- [ ] **M-11.4** Port VFS — file read operations.
  - Port: `read` (passthrough to source audio files and cover art), `getattr`, `lookup`.
  - Port: `INodeMapper` (bidirectional inode mapping).
  - Port: `FileHandleManager`.
  - Port: `TTLCache`.
  - Done-when: Integration test: mount, `ls` directories, read an audio file, verify
    content matches source.

- [ ] **M-11.5** Port VFS — write operations and ghost files.
  - Port: `mkdir` (add release to collage), `write`/`release` (add track to playlist).
  - Port: Ghost file behavior — fake files for 2-5 seconds post-mutation. Collage ghost:
    pretend new dir is empty, redirect writes to `/dev/null` for 5s. Playlist ghost:
    pretend written file exists for 2s after release.
  - Done-when: Integration test: `cp -r` a release into a collage dir succeeds. `cp` a
    track into a playlist dir succeeds.

---

## Phase 12: CLI

- [ ] **M-12.1** Create `rose-cli` binary crate.
  - Add dep: `clap` (derive).
  - Produce: `rose-rs/rose-cli/Cargo.toml`, `rose-rs/rose-cli/src/main.rs`.
  - Implement: Top-level command structure, `--verbose`/`-v` flag (sets `tracing` to
    DEBUG), config loading, error display (expected errors without traceback).
  - Implement: `rose version`.
  - Done-when: `rose version` prints version. `rose --help` shows all command groups.

- [ ] **M-12.2** Port CLI — JSON serialization (`dump` module).
  - Read: `rose-cli/rose_cli/dump.py` (288 lines).
  - Port: `release_to_json`, `track_to_json`, `_partition_releases_by_role`.
  - Port: All `dump_*` functions (release, all_releases, track, all_tracks, artist,
    all_artists, genre, all_genres, label, all_labels, descriptor, all_descriptors,
    collage, all_collages, playlist, all_playlists).
  - Use existing `syrupy` snapshots from `dump_test.ambr` as golden file references.
  - Done-when: `cargo test` passes. JSON output for seeded data matches golden files.

- [ ] **M-12.3** Port CLI — cache commands.
  - Port: `rose cache update [--force]`.
  - Done-when: `rose cache update` populates cache from test library.

- [ ] **M-12.4** Port CLI — release commands.
  - Port: `rose releases print`, `print-all`, `edit`, `toggle-new`, `toggle-favorite`,
    `set-rating`, `delete`, `set-cover`, `delete-cover`, `run-rule`, `create-single`.
  - Done-when: All release subcommands work. JSON output valid.

- [ ] **M-12.5** Port CLI — track commands.
  - Port: `rose tracks print`, `print-all`, `run-rule`.
  - Done-when: All track subcommands work.

- [ ] **M-12.6** Port CLI — collage commands.
  - Port: `rose collages create`, `rename`, `delete`, `add-release`, `remove-release`,
    `edit`, `print`, `print-all`.
  - Done-when: All collage subcommands work.

- [ ] **M-12.7** Port CLI — playlist commands.
  - Port: `rose playlists create`, `rename`, `delete`, `add-track`, `remove-track`,
    `edit`, `print`, `print-all`, `set-cover`, `delete-cover`.
  - Done-when: All playlist subcommands work.

- [ ] **M-12.8** Port CLI — browse commands.
  - Port: `rose artists print/print-all`, `rose genres print/print-all`,
    `rose labels print/print-all`, `rose descriptors print/print-all`.
  - Done-when: All browse subcommands work. JSON output valid.

- [ ] **M-12.9** Port CLI — rules commands.
  - Port: `rose rules run <matcher> <actions...> [--dry-run] [--yes] [--ignore]`.
  - Port: `rose rules run-stored [--dry-run] [--yes]`.
  - Done-when: Rule commands work end-to-end.

- [ ] **M-12.10** Port CLI — fs commands.
  - Port: `rose fs mount [--foreground]`, `rose fs unmount`.
  - Done-when: `rose fs mount` mounts VFS, `rose fs unmount` unmounts.

- [ ] **M-12.11** Port CLI — config and misc.
  - Port: `rose config generate-completion <shell>` (clap built-in).
  - Port: `rose config preview-templates`.
  - Done-when: Shell completions generated. Template preview works.

---

## Phase 13: File Watcher

- [ ] **M-13.1** Port watcher with debouncing.
  - Read: `rose-watch/rose_watch/watcher.py` (192 lines).
  - Add dep: `notify`, `tokio` (or use `crossbeam-channel` + manual event loop).
  - Integrate into CLI: `rose cache watch [--foreground]`, `rose cache unwatch`.
  - Architecture (must match Python):
    - A **sync `notify` watcher thread** pushes filesystem events into a channel.
    - An **async event loop** (or polling loop) consumes events, debounces them
      (coalesce events on the same release/collage/playlist within a time window),
      and dispatches the appropriate `update_cache_for_*` call.
    - Debounce window: ~200ms for dedup, ~2s delay for release events (to let
      multi-file copies complete before processing).
  - Port: Event classification — parse path to determine if it's a release, collage,
    or playlist event. Map to appropriate cache update function.
  - Port: Event types: created, deleted, modified, moved. "Moved" triggers both update
    and evict.
  - Done-when: Integration test: start watcher, create/rename/delete files in source dir,
    verify cache updates within timeout. Verify debouncing: rapid events on same release
    trigger only one update.

---

## Phase 14: PyO3 Scripting Shim

- [ ] **M-14.1** Create `rose-py` PyO3 crate.
  - Add dep: `pyo3` with `extension-module` feature.
  - Produce: `rose-rs/rose-py/Cargo.toml`, `rose-rs/rose-py/src/lib.rs`.
  - Expose: `Config` (load from path).
  - Done-when: `import rose` in Python works, `rose.Config.load(path)` works.

- [ ] **M-14.2** Expose read operations to Python.
  - Expose: `update_cache`, `list_releases`, `list_tracks`, `list_collages`,
    `list_playlists`, `get_release`, `get_track`, `get_collage`, `get_playlist`.
  - Expose: `Release`, `Track`, `Collage`, `Playlist` as Python-readable types with
    attribute access.
  - Done-when: Python script can load config, update cache, list releases, inspect fields.

- [ ] **M-14.3** Expose write operations to Python.
  - Expose: `AudioTags` read/write, rule execution, release/track/collage/playlist CRUD.
  - Expose: `find_releases_matching_rule`, `find_tracks_matching_rule`.
  - Done-when: Python script can modify tags, run rules, CRUD collages/playlists.

---

## Phase 15: Finalization

- [ ] **M-15.1** Write end-to-end smoke test.
  - Produce: `rose-rs/tests/e2e_smoke.sh` or Rust integration test.
  - Exercises every CLI subcommand against a temp library from `testdata/`.
  - Done-when: Script passes on the Rust binary.

- [ ] **M-15.2** Benchmark: Python vs Rust.
  - Measure and record:
    - VFS `readdir` latency (the stated pain point)
    - `cache update` time for N releases
    - Cold start time (first `cache update` from empty)
  - Produce: `z-thinking/benchmark-results.md`.
  - Done-when: Results documented. Rust is measurably faster.

- [ ] **M-15.3** Update Nix build.
  - Update `flake.nix` to build Rust binary + PyO3 shim.
  - Done-when: `nix build` produces working `rose` binary and `rose` Python package.

- [ ] **M-15.4** Update CI.
  - Update `.github/workflows/build.yaml`: `cargo test`, `cargo clippy -- -D warnings`,
    `cargo fmt --check`, e2e smoke test, nix build.
  - Done-when: CI passes on clean push.

- [ ] **M-15.5** Delete old Python code.
  - Remove: `rose-py/`, `rose-cli/`, `rose-vfs/`, `rose-watch/`, `conftest.py`,
    `pyproject.toml`, old Makefile targets.
  - Keep: `testdata/`, `docs/`, `scripts/`.
  - Done-when: Repo contains only Rust + testdata/docs/scripts. Builds clean.

- [ ] **M-15.6** Update documentation.
  - Update `README.md`, `docs/ARCHITECTURE.md`, installation instructions.
  - Done-when: Docs accurate for Rust version.

---

## Change Log from v1

| # | Feedback Item | What Changed |
|---|---------------|--------------|
| 1 | `filter_releases`/`filter_tracks` missing | Added **M-6.4** as dedicated milestone |
| 2 | Watcher debounce undocumented | Rewrote **M-13.1** with full debounce architecture |
| 3 | `.rose.{uuid}.toml` format not constrained | Added as **hard constraint #2**; added to **M-6.2** |
| 4 | `dump.py` missing from milestones | Added **M-12.2** for JSON serialization |
| 5 | `lofty`/`mutagen` divergence risk | Added **M-2.0** (golden file generation before any Rust) |
| 6 | M-7.1 too coarse | Split into **M-7.1a through M-7.1e** |
| 7 | VFS LoC estimate too tight | Acknowledged (estimate is guidance, not a gate) |
| 8 | No path normalization strategy | Added to **cross-cutting decisions** (NFC always) |
| 9 | No `rayon` error strategy | Added to **cross-cutting decisions** + **M-7.2** |
| 10 | Line count inaccuracies | Fixed throughout (tracks=67, playlists=236, etc.) |
| 11 | Genre hierarchy JSON exists | Noted in **M-1.3** |
| 12 | Missing VFS view types | **M-11.2** now lists all 12 views including Added On, Released On |
| 13 | Editor integration complexity | Called out in **M-9.1**, **M-10.1**, **M-10.2** |
| 14 | No logging strategy | Added `tracing` to **cross-cutting decisions** and **M-1.1** |
| 15 | No benchmarking | Added **M-15.2** |
