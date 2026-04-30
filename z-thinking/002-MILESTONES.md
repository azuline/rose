# 002 — Migration Milestones

Canonical checklist for the Python-to-Rust migration. Each milestone is a self-contained
unit of work. A fresh agent should be able to pick up at any unchecked milestone by reading
this file and the referenced Python source.

**How to use this file:**
- Work top-to-bottom. Dependencies flow downward.
- Mark `[x]` when a milestone is done and its tests pass.
- Each milestone has: what to do, what Python file to read, what to produce, and how to
  verify it's done (the "done-when" condition).
- `z-thinking/001-PLAN.md` has the full rationale and strategy. This file is just the
  execution checklist.

**Hard constraints (always apply):**
1. On-disk audio tag structure must be identical to Python version.
2. Feature power must be equal or superior to Python version.
3. Config/CLI interface can change freely.
4. A PyO3 Python scripting shim is required as a permanent deliverable.

---

## Phase 1: Project Scaffolding

- [ ] **M-1.1** Create Cargo workspace at `rose-rs/` with `rose-core` library crate.
  - Produce: `rose-rs/Cargo.toml` (workspace), `rose-rs/rose-core/Cargo.toml`,
    `rose-rs/rose-core/src/lib.rs`.
  - Add initial dependencies: `serde`, `sha2`, `regex`, `unicode-normalization`.
  - Done-when: `cargo build` succeeds in `rose-rs/`.

- [ ] **M-1.2** Port `common.rs` — foundational types and utilities.
  - Read: `rose-py/rose/common.py` (231 lines).
  - Produce: `rose-rs/rose-core/src/common.rs`.
  - Port: `RoseError` hierarchy, `Artist`, `ArtistMapping` (with `.all()`, `.dump()`,
    `.items()`), `sanitize_dirname`, `sanitize_filename`, `sha256_dataclass`, `flatten`,
    `uniq`, `VERSION` (read from file or const), `ILLEGAL_FS_CHARS_REGEX`.
  - Done-when: `cargo test` passes with tests covering all ported items. Specifically:
    - `sanitize_dirname`: illegal chars replaced, max-length truncation at byte boundary,
      diacritics stripping, NFC normalization, empty input, all-illegal input.
    - `sanitize_filename`: same as dirname + extension preservation.
    - `ArtistMapping::all()`: dedup + ordering across roles.
    - `sha256_dataclass` equivalent: deterministic hashing.

- [ ] **M-1.3** Port `genre_hierarchy.rs` — generated genre data.
  - Read: `rose-py/rose/genre_hierarchy.py` (~24,400 lines, generated).
  - Read: `scripts/rym-genres/` to understand generation.
  - Produce: `rose-rs/rose-core/src/genre_hierarchy.rs` (or a build script that generates
    it, or a static `include!`).
  - Port: `GENRE_HIERARCHY` (child->parents map), `GENRE_HIERARCHY_PARENTS` (parent->all
    transitive children map).
  - Done-when: `cargo test` passes. Test that a known genre returns expected parents,
    and a known parent returns expected children.

---

## Phase 2: Audio Tags

- [ ] **M-2.1** Port `audiotags.rs` — audio tag reading.
  - Read: `rose-py/rose/audiotags.py` (629 lines).
  - Add dependency: `lofty` (or alternative).
  - Produce: `rose-rs/rose-core/src/audiotags.rs`.
  - Port read path: `AudioTags.from_file()` for FLAC, MP3, M4A, OGG Vorbis, OGG Opus.
    All fields: `id`, `release_id`, `title`, `artists` (with role parsing), `date`,
    `originaldate`, `compositiondate`, `album`, `releasetype`, `genre`, `secondary_genre`,
    `descriptor`, `label`, `catalognumber`, `tracknumber`, `tracktotal`, `discnumber`,
    `disctotal`, `duration_seconds`, `edition`, `rating`.
  - Port: `RoseDate` type (year, month, day — all optional except year).
  - Port: `SUPPORTED_AUDIO_EXTENSIONS` constant.
  - Done-when: `cargo test` passes with tests reading all 5 files in `testdata/Tagger/`
    and asserting tag values match the Python output. Generate golden file from Python
    first if needed.

- [ ] **M-2.2** Port `audiotags.rs` — audio tag writing.
  - Port write path: `AudioTags.flush()` for all 5 formats. All writable fields.
  - Port: ID assignment logic (`maybe_set_ids`).
  - Done-when: `cargo test` passes with roundtrip tests — read, modify, write, re-read,
    assert. For all 5 formats. Custom tags (`roseid`, `rosereleaseid`) survive roundtrip.

---

## Phase 3: Rule Parser

- [ ] **M-3.1** Port `rule_parser.rs` — the rule DSL parser.
  - Read: `rose-py/rose/rule_parser.py` (846 lines).
  - Produce: `rose-rs/rose-core/src/rule_parser.rs`.
  - Port: `Matcher`, `Pattern`, `Action` (and all subtypes: `ReplaceAction`, `SedAction`,
    `SplitAction`, `AddAction`, `DeleteAction`), `Rule` types.
  - Port: `parse_matcher()`, `parse_action()`, `parse_rule()` functions.
  - Port: All tag field name constants and their mappings.
  - Done-when: `cargo test` passes. Translate every test case from
    `rose-py/rose/rule_parser_test.py` (544 lines) into Rust tests. Add proptest:
    arbitrary strings never panic, only return `Ok` or `Err`.

---

## Phase 4: Templates

- [ ] **M-4.1** Port `templates.rs` — path template evaluation.
  - Read: `rose-py/rose/templates.py` (588 lines).
  - Add dependency: `minijinja` (or custom evaluator if template language is simple).
  - Produce: `rose-rs/rose-core/src/templates.rs`.
  - Port: `PathTemplate`, `PathContext`, `PathTemplateConfig`, `evaluate_release_template`,
    `evaluate_track_template`, `get_sample_music`.
  - Port: All custom Jinja2 filters used by templates.
  - Done-when: `cargo test` passes. Test all template contexts against golden file output
    from Python. `get_sample_music()` returns a valid sample.

---

## Phase 5: Config

- [ ] **M-5.1** Port `config.rs` — configuration parsing.
  - Read: `rose-py/rose/config.py` (609 lines).
  - Add dependencies: `toml`, `serde`, `dirs`.
  - Produce: `rose-rs/rose-core/src/config.rs`.
  - Port: `Config` struct with all fields, `VirtualFSConfig`, default values,
    `PathTemplateConfig` integration. Config loading from TOML file.
  - Port: All validation logic (invalid values, missing required keys, etc.).
  - Port: Artist alias resolution (`artist_aliases_map`, `artist_aliases_parents_map`).
  - Note: Config format is free to change, but all *knobs* must be present.
  - Done-when: `cargo test` passes. Test: valid config loads, missing keys error, invalid
    values error, defaults applied. Port representative cases from
    `rose-py/rose/config_test.py` (682 lines).

---

## Phase 6: Cache — Schema and Read

- [ ] **M-6.1** Port `cache.rs` — schema bootstrapping.
  - Read: `rose-py/rose/cache.py` (lines 1-~200), `rose-py/rose/cache.sql` (290 lines).
  - Add dependency: `rusqlite` (with `bundled` feature).
  - Produce: `rose-rs/rose-core/src/cache.rs` (initial).
  - Port: Schema creation, `_schema_hash` table, schema hash validation, database
    invalidation/rebuild logic (`maybe_invalidate_cache_database`).
  - Share: Copy `cache.sql` to `rose-rs/rose-core/src/cache.sql`.
  - Done-when: `cargo test` passes. Test: fresh database created correctly, schema hash
    mismatch triggers rebuild, version mismatch triggers rebuild.

- [ ] **M-6.2** Port `cache.rs` — data types.
  - Port: `Release`, `Track`, `Collage`, `Playlist`, `GenreEntry`, `DescriptorEntry`,
    `LabelEntry` structs. Row deserialization from SQLite.
  - Port: `make_release_logtext`, `make_track_logtext` helper functions.
  - Done-when: Types compile and have `Debug`, `Clone`. Serialization from SQLite rows
    tested with hand-inserted data.

- [ ] **M-6.3** Port `cache.rs` — read queries.
  - Port: `get_release`, `get_track`, `get_collage`, `get_playlist`.
  - Port: `list_releases`, `list_tracks`, `list_collages`, `list_playlists`.
  - Port: `list_artists`, `list_genres`, `list_labels`, `list_descriptors`.
  - Port: `artist_exists`, `genre_exists`, `label_exists`, `descriptor_exists`.
  - Port: `get_collage_releases`, `get_playlist_tracks`.
  - Port: `get_tracks_of_release`, `get_tracks_of_releases`.
  - Port: `release_within_collage`, `track_within_release`, `track_within_playlist`.
  - Done-when: `cargo test` passes. Seed a database (equivalent to conftest `_seed_cache`)
    and test every read function returns expected data.

---

## Phase 7: Cache — Write and Update

- [ ] **M-7.1** Port `cache.rs` — single-threaded cache update.
  - Read: `rose-py/rose/cache.py` update logic (the bulk of the file).
  - Port: `update_cache` (single-threaded path), `update_cache_for_releases`,
    `update_cache_for_collages`, `update_cache_for_playlists`.
  - Port: Mtime comparison, tag reading, datafile parsing (`.rose.{uuid}.toml`), batch
    SQL writes.
  - Port: `update_cache_evict_nonexistent_releases/collages/playlists`.
  - Done-when: `cargo test` passes. Copy `testdata/` releases into a temp dir, run
    `update_cache`, verify database state matches expected releases/tracks/collages/
    playlists.

- [ ] **M-7.2** Port `cache.rs` — parallel cache update.
  - Add dependency: `rayon`.
  - Port: The multiprocessing sharding logic — when release count > threshold, shard work
    across threads.
  - Done-when: `cargo test` passes. Same test as M-7.1 but with parallelism enabled.
    Results must be identical to single-threaded.

- [ ] **M-7.3** Port `cache.rs` — locking.
  - Port: `lock()` context manager equivalent (advisory locks in the `locks` table).
  - Port: `release_lock_name`, `collage_lock_name`, `playlist_lock_name`.
  - Done-when: `cargo test` passes. Test: lock acquired, lock blocks second acquire,
    lock released, expired lock can be taken.

- [ ] **M-7.4** Port `cache.rs` — FTS index.
  - Port: `process_string_for_fts` function.
  - Port: FTS index sync at end of cache update.
  - Done-when: `cargo test` passes. After cache update, FTS queries return expected
    results. Test substring matching.

---

## Phase 8: Rules Engine

- [ ] **M-8.1** Port `rules.rs` — rule execution.
  - Read: `rose-py/rose/rules.py` (898 lines).
  - Produce: `rose-rs/rose-core/src/rules.rs`.
  - Port: `execute_metadata_rule` (the core: match tracks, apply actions).
  - Port: All action types: replace, sed, split, add, delete.
  - Port: `execute_stored_metadata_rules` (run rules from config).
  - Port: Dry-run mode, confirmation prompting, `--yes` bypass.
  - Port: Tag-field validation (`TrackTagNotAllowedError`, `InvalidReplacementValueError`).
  - Done-when: `cargo test` passes. Port all test cases from `rose-py/rose/rules_test.py`
    (474 lines). Test each action type on each applicable tag field.

---

## Phase 9: Release and Track Operations

- [ ] **M-9.1** Port `releases.rs` — release CRUD.
  - Read: `rose-py/rose/releases.py` (641 lines).
  - Produce: `rose-rs/rose-core/src/releases.rs`.
  - Port: `toggle_release_new`, `toggle_release_favorite`, `set_release_rating`.
  - Port: `delete_release`, `set_release_cover_art`, `delete_release_cover_art`.
  - Port: `edit_release` (open in editor, parse changes, apply).
  - Port: `create_single_release`.
  - Port: `find_releases_matching_rule`, `run_actions_on_release`.
  - Done-when: `cargo test` passes. Port test cases from `rose-py/rose/releases_test.py`
    (511 lines).

- [ ] **M-9.2** Port `tracks.rs` — track operations.
  - Read: `rose-py/rose/tracks.py` (~200 lines).
  - Produce: `rose-rs/rose-core/src/tracks.rs`.
  - Port: `find_tracks_matching_rule`, `run_actions_on_track`.
  - Done-when: `cargo test` passes. Port test cases from `rose-py/rose/tracks_test.py`
    (42 lines) + add additional coverage.

---

## Phase 10: Collage and Playlist Operations

- [ ] **M-10.1** Port `collages.rs` — collage CRUD.
  - Read: `rose-py/rose/collages.py` (~200 lines).
  - Produce: `rose-rs/rose-core/src/collages.rs`.
  - Port: `create_collage`, `rename_collage`, `delete_collage`.
  - Port: `add_release_to_collage`, `remove_release_from_collage`.
  - Port: `edit_collage_in_editor`.
  - Done-when: `cargo test` passes. Port test cases from `rose-py/rose/collages_test.py`
    (172 lines).

- [ ] **M-10.2** Port `playlists.rs` — playlist CRUD.
  - Read: `rose-py/rose/playlists.py` (~400 lines).
  - Produce: `rose-rs/rose-core/src/playlists.rs`.
  - Port: `create_playlist`, `rename_playlist`, `delete_playlist`.
  - Port: `add_track_to_playlist`, `remove_track_from_playlist`.
  - Port: `edit_playlist_in_editor`.
  - Port: `set_playlist_cover_art`, `delete_playlist_cover_art`.
  - Done-when: `cargo test` passes. Port test cases from `rose-py/rose/playlists_test.py`
    (253 lines).

---

## Phase 11: FUSE Virtual Filesystem

- [ ] **M-11.1** Create `rose-vfs` binary crate with `fuser` dependency.
  - Produce: `rose-rs/rose-vfs/Cargo.toml`, `rose-rs/rose-vfs/src/main.rs`.
  - Implement: Mount/unmount lifecycle, signal handling.
  - Done-when: Empty filesystem mounts and unmounts cleanly.

- [ ] **M-11.2** Port VFS — logical core (directory structure).
  - Read: `rose-vfs/rose_vfs/virtualfs.py` (2,089 lines), `docs/ARCHITECTURE.md` VFS
    section.
  - Port: `RoseLogicalCore` equivalent — the domain logic layer that maps virtual paths
    to cache queries.
  - Port: All 8 top-level virtual directories (Releases, Releases - New,
    Releases - Recently Added, Artists, Genres, Labels, Collages, Playlists).
  - Done-when: `cargo test` passes. Test: `readdir` at each level returns expected
    entries from seeded cache.

- [ ] **M-11.3** Port VFS — file operations (read, getattr, lookup).
  - Port: `read` (audio files, cover art via passthrough to source), `getattr`, `lookup`.
  - Port: Inode management, path-to-inode mapping, file handle tracking.
  - Done-when: `cargo test` passes. Integration test: mount filesystem, `ls` directories,
    `cat` an audio file, verify content matches source.

- [ ] **M-11.4** Port VFS — write operations (collage/playlist mutation via FS).
  - Port: `mkdir` (add release to collage), `write`/`release` (add track to playlist).
  - Port: Ghost file behavior (fake files for 2-5 seconds post-mutation).
  - Done-when: `cargo test` passes. Integration test: `cp -r` a release into a collage
    dir succeeds without error. `cp` a track into a playlist dir succeeds.

- [ ] **M-11.5** Port VFS — blacklist/whitelist filtering.
  - Port: Artist/genre/descriptor/label whitelist and blacklist from VFS config.
  - Port: `hide_*_with_only_new_releases` options.
  - Done-when: `cargo test` passes. Test: filtered entities do not appear in `readdir`.

---

## Phase 12: CLI

- [ ] **M-12.1** Create `rose-cli` binary crate with `clap` dependency.
  - Produce: `rose-rs/rose-cli/Cargo.toml`, `rose-rs/rose-cli/src/main.rs`.
  - Implement: Top-level command structure, `--verbose` flag, config loading.
  - Implement: `rose version`.
  - Done-when: `cargo build` produces a `rose` binary. `rose version` prints version.

- [ ] **M-12.2** Port CLI — cache commands.
  - Port: `rose cache update [--force]`.
  - Done-when: Running `rose cache update` against a test library populates the cache.

- [ ] **M-12.3** Port CLI — release commands.
  - Port: `rose releases print`, `print-all`, `edit`, `toggle-new`, `toggle-favorite`,
    `set-rating`, `delete`, `set-cover`, `delete-cover`, `run-rule`, `create-single`.
  - Done-when: All release subcommands work. JSON output for `print`/`print-all` is valid.

- [ ] **M-12.4** Port CLI — track commands.
  - Port: `rose tracks print`, `print-all`, `run-rule`.
  - Done-when: All track subcommands work. JSON output valid.

- [ ] **M-12.5** Port CLI — collage commands.
  - Port: `rose collages create`, `rename`, `delete`, `add-release`, `remove-release`,
    `edit`, `print`, `print-all`.
  - Done-when: All collage subcommands work.

- [ ] **M-12.6** Port CLI — playlist commands.
  - Port: `rose playlists create`, `rename`, `delete`, `add-track`, `remove-track`,
    `edit`, `print`, `print-all`, `set-cover`, `delete-cover`.
  - Done-when: All playlist subcommands work.

- [ ] **M-12.7** Port CLI — browse commands (artists, genres, labels, descriptors).
  - Port: `rose artists print/print-all`, `rose genres print/print-all`,
    `rose labels print/print-all`, `rose descriptors print/print-all`.
  - Done-when: All browse subcommands work. JSON output valid.

- [ ] **M-12.8** Port CLI — rules commands.
  - Port: `rose rules run <matcher> <actions...> [--dry-run] [--yes] [--ignore]`.
  - Port: `rose rules run-stored [--dry-run] [--yes]`.
  - Done-when: Rule commands work end-to-end.

- [ ] **M-12.9** Port CLI — fs commands.
  - Port: `rose fs mount [--foreground]`, `rose fs unmount`.
  - Done-when: `rose fs mount` mounts the VFS, `rose fs unmount` unmounts it.

- [ ] **M-12.10** Port CLI — config and misc commands.
  - Port: `rose config generate-completion <shell>`, `rose config preview-templates`.
  - Done-when: Shell completions generated, template preview works.

---

## Phase 13: File Watcher

- [ ] **M-13.1** Port watcher — file watching with `notify`.
  - Read: `rose-watch/rose_watch/watcher.py` (~130 lines).
  - Integrate into CLI or separate binary: `rose cache watch [--foreground]`,
    `rose cache unwatch`.
  - Port: Watch source dir for changes, trigger `update_cache_for_releases/collages/
    playlists` on file events. Debouncing.
  - Done-when: `cargo test` passes. Integration test: start watcher, create/rename/delete
    a file in source dir, verify cache updates within timeout.

---

## Phase 14: PyO3 Scripting Shim

- [ ] **M-14.1** Create `rose-py` PyO3 crate.
  - Add dependency: `pyo3` with `extension-module` feature.
  - Produce: `rose-rs/rose-py/Cargo.toml`, `rose-rs/rose-py/src/lib.rs`.
  - Expose: `Config` (load from path).
  - Done-when: `import rose` in Python works, `rose.Config.load("/path")` works.

- [ ] **M-14.2** Expose read operations to Python.
  - Expose: `update_cache`, `list_releases`, `list_tracks`, `list_collages`,
    `list_playlists`, `get_release`, `get_track`, `get_collage`, `get_playlist`.
  - Expose: `Release`, `Track`, `Collage`, `Playlist` as Python-readable types.
  - Done-when: Python script can load config, update cache, list releases, inspect fields.

- [ ] **M-14.3** Expose write operations to Python.
  - Expose: `AudioTags` read/write, rule execution, release/track/collage/playlist CRUD.
  - Expose: `find_releases_matching_rule`, `find_tracks_matching_rule`.
  - Done-when: Python script can modify tags, run rules, create/delete collages, etc.

---

## Phase 15: Cleanup and Finalization

- [ ] **M-15.1** Write end-to-end smoke test script.
  - Produce: `rose-rs/tests/e2e_smoke.sh` (or Rust integration test).
  - Exercises every CLI subcommand against a temp library built from `testdata/`.
  - Done-when: Script passes on the Rust binary.

- [ ] **M-15.2** Update Nix build.
  - Update `flake.nix` to build the Rust binary + PyO3 shim.
  - Done-when: `nix build` produces working `rose` binary and `rose` Python package.

- [ ] **M-15.3** Update CI.
  - Update `.github/workflows/build.yaml` for Rust: `cargo test`, `cargo clippy`,
    `cargo fmt --check`, e2e smoke test, nix build.
  - Done-when: CI passes on a clean push.

- [ ] **M-15.4** Delete old Python code.
  - Remove: `rose-py/`, `rose-cli/`, `rose-vfs/`, `rose-watch/`, `conftest.py`,
    `pyproject.toml`, old Makefile targets.
  - Keep: `testdata/`, `docs/`, `scripts/` (if still relevant).
  - Done-when: Repo contains only Rust code (+ testdata/docs/scripts), builds clean.

- [ ] **M-15.5** Update documentation.
  - Update `README.md`, `docs/ARCHITECTURE.md`, and other docs to reflect Rust.
  - Update installation instructions.
  - Done-when: Docs are accurate for the Rust version.
