# 006 — Migration Milestones (v3)

Revised after incorporating all feedback from `005-review-feedback-v2.md`.

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
- **Path normalization:** Do NOT normalize to NFC by default (match Python behavior, which
  compares raw OS bytes). NFC normalization can be added as a future enhancement behind a
  config flag after migration is complete. `sanitize_dirname`/`sanitize_filename` still
  apply NFC on output (matching Python), but path *comparison* uses raw bytes.
- **Error recovery in parallel code:** `rayon` tasks return `Result`. Errors are collected
  into a `Vec` and reported after all valid work completes. Never panic on a single bad
  file/directory.
- **SQLite connection setup:** Every `rusqlite::Connection` must set these PRAGMAs on open:
  `PRAGMA journal_mode=WAL`, `PRAGMA foreign_keys=ON`, `busy_timeout(15000)`. This
  matches `cache.py:88-94`.
- **VFS thread safety:** All shared mutable VFS state uses `Arc<RwLock<T>>` or `DashMap`
  for hot-path maps. The `fuser::Filesystem` impl holds `Arc` references. Python relies
  on the GIL; Rust has no GIL, so explicit synchronization is required. The full inventory
  of shared state is documented in M-11.1.
- **SQLite concurrency with rayon:** Each rayon worker thread opens its own
  `rusqlite::Connection` (not shared). WAL mode and busy_timeout are required for
  concurrent access. The main thread coordinates final writes after workers complete.

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
      diacritics stripping, NFC on output, empty input, all-illegal input.
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
  - **Template language migration (breaking change for custom user templates):**
    The Python defaults use features `minijinja` does not support:
    - `{{ discnumber.rjust(2, '0') }}` — Python `str.rjust()`. Replace with custom
      filter: `{{ discnumber | pad(2) }}` or `{{ discnumber | pad_left(2, "0") }}`.
    - `{{ added_at[:10] }}` — Python slice notation. Replace with custom filter:
      `{{ added_at | truncate(10) }}` or `{{ added_at | take(10) }}`.
    - `{{ originaldate or releasedate or '0000' }}` — Python falsy coalescing. Verify
      `minijinja`'s `or` behavior with empty strings and custom types; may need a
      `coalesce` filter.
  - Implement custom `minijinja` filters as replacements for unsupported Python methods.
  - Update default templates in `PathTemplateConfig` to use the new filter syntax.
  - Done-when: `cargo test` passes. Golden file tests against Python output for all
    template contexts (using the *new* default templates — not the old Python ones).

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
  - Port: **Unknown-key detection** — after consuming all known keys, warn about
    remaining keys to help users catch typos (`config.py:556-570`).
  - Note: Config format can change, but all *knobs* must be present.
  - Done-when: `cargo test` passes. Valid config loads, missing keys error, invalid values
    error, defaults applied, unknown keys warned. Port representative cases from
    `config_test.py` (682 lines).

---

## Phase 6: Cache — Schema, Types, and Reads

- [ ] **M-6.1** Cache schema bootstrapping.
  - Read: `rose-py/rose/cache.py` (top ~200 lines), `cache.sql` (290 lines).
  - Add dep: `rusqlite` (with `bundled` feature — verify FTS5 is included).
  - Produce: `rose-rs/rose-core/src/cache.rs` (initial).
  - Copy: `cache.sql` to `rose-rs/rose-core/src/cache.sql`.
  - Port: Schema creation, `_schema_hash` table, schema hash validation,
    `maybe_invalidate_cache_database`.
  - Port: `connect()` function — every connection must set: `PRAGMA journal_mode=WAL`,
    `PRAGMA foreign_keys=ON`, `busy_timeout(15000)`, autocommit mode.
  - Done-when: `cargo test` passes. Fresh DB created correctly. Schema hash mismatch
    triggers rebuild. Version mismatch triggers rebuild. PRAGMAs verified on connection.
    FTS5 is available (`SELECT * FROM fts5_test` doesn't error on a test table).

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
  - **Concurrency model:** Each rayon worker thread opens its own `rusqlite::Connection`.
    WAL mode + busy_timeout required. Workers return `Vec<ReleaseUpdate>` (or equivalent
    data struct). Main thread applies all SQL writes in a single transaction after workers
    finish.
  - **Error handling:** Tasks return `Result`. Errors collected into a `Vec`. Update
    completes for all valid releases. Errors reported after. One corrupt directory must
    not abort the entire update.
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
  - Port: FTS index sync at end of cache update — this is a 63-line SQL statement that
    JOINs 7 tables (`tracks`, `releases`, `releases_genres`, `releases_secondary_genres`,
    `releases_descriptors`, `releases_labels`, `releases_artists`, `tracks_artists`), uses
    `GROUP_CONCAT` on 6 multi-value columns, and calls the custom `process_string_for_fts`
    function registered via `rusqlite::Connection::create_scalar_function()`.
  - Verify: FTS5 is available in the `rusqlite` bundled build (should be, but test it).
  - Done-when: `cargo test` passes. After cache update, FTS substring queries return
    expected results. Custom function registration works.

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
  - Port: `edit_release` — this is complex, not just "open editor":
    - `MetadataRelease` / `MetadataTrack` intermediary types for TOML serialization.
    - TOML null encoding hacks: `-1` for null rating, `""` for null dates/edition/
      catalognumber (`releases.py:318-327`). Rust serializer must match exactly.
    - Opens `$EDITOR`, waits for exit. Handles: non-zero exit, unchanged file.
    - Per-track per-field dirty checking (15+ fields compared individually). Only dirty
      tracks have tags flushed to disk.
    - Resume-on-failure: if any exception during apply, saves edited TOML to
      `failed-release-edit.{uuid}.toml`. `--resume` reloads it. Regex validation on
      resume file.
    - Dynamic role dispatch: maps artist role strings to `ArtistMapping` fields. Unknown
      role raises `UnknownArtistRoleError`.
  - Port: `create_single_release` (with `--loose-track` flag -> `releasetype=loosetrack`).
  - Port: `find_releases_matching_rule`, `run_actions_on_release`.
  - Done-when: `cargo test` passes. Port cases from `releases_test.py` (511 lines).
    Include: edit roundtrip, edit with null fields, edit resume-on-failure, dirty checking.

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
    non-zero exit, unchanged file, parse errors, `DescriptionMismatchError` (release
    description in file doesn't match actual release title).
  - Done-when: `cargo test` passes. Port cases from `collages_test.py` (172 lines).

- [ ] **M-10.2** Port `playlists.rs`.
  - Read: `rose-py/rose/playlists.py` (236 lines).
  - Produce: `rose-rs/rose-core/src/playlists.rs`.
  - Port: `create_playlist`, `rename_playlist`, `delete_playlist`.
  - Port: `add_track_to_playlist`, `remove_track_from_playlist`.
  - Port: `edit_playlist_in_editor` — same editor pattern as collages, same error cases.
  - Port: `set_playlist_cover_art`, `delete_playlist_cover_art`.
  - Done-when: `cargo test` passes. Port cases from `playlists_test.py` (253 lines).

---

## Phase 11: FUSE Virtual Filesystem

- [ ] **M-11.1** Create `rose-vfs` binary crate and inventory shared state.
  - Add deps: `fuser`, `dashmap` (or use `Arc<RwLock<>>`).
  - Produce: `rose-rs/rose-vfs/Cargo.toml`, `rose-rs/rose-vfs/src/main.rs`.
  - Implement: Mount/unmount lifecycle, signal handling, multi-worker threading.
  - **Document the full inventory of shared mutable state** (Python relies on GIL, Rust
    needs explicit sync). All of the following need `Arc<RwLock<T>>` or `DashMap`:
    - `TTLCache` instances (ghost files, getattr cache, lookup cache)
    - `VirtualNameGenerator` internal dicts (6 bidirectional maps)
    - `Sanitizer` mappings
    - `INodeMapper` (monotonic counter + bidirectional dict)
    - `FileHandleManager` (counter — fix wrapping bug: use `(self.state + 1) % 10_000`
      not `self.state + 1 % 10_000`)
    - `file_creation_special_ops` dict
    - `update_release_on_fh_close` dict
  - Done-when: Empty filesystem mounts and unmounts cleanly with multiple workers.
    Shared state types defined with synchronization primitives.

- [ ] **M-11.2** Port VFS — virtual path parser and directory structure.
  - Read: `rose-vfs/rose_vfs/virtualfs.py` (2,089 lines), `docs/ARCHITECTURE.md`.
  - Port: `VirtualPath` type with all 12 view types: `Root`, `Releases`, `Artists`,
    `Genres`, `Descriptors`, `Labels`, `Loose Tracks`, `Collages`, `Playlists`, `New`,
    `Favorites`, `Added On`, `Released On`.
  - Port: Path parsing (`VirtualPath::parse` from string path).
  - Port: `VirtualNameGenerator` (bidirectional name-to-entity mapping with TTL cache,
    wrapped in appropriate sync primitive).
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

- [ ] **M-11.4** Port VFS — file read operations and inode management.
  - Port: `read` (passthrough to source audio files and cover art), `getattr`, `lookup`.
  - Port: `INodeMapper` (bidirectional inode mapping, sync-wrapped).
  - Port: `FileHandleManager` (with fixed wrapping: `(n + 1) % 10_000`).
  - Port: `TTLCache` (generic TTL dict, sync-wrapped).
  - Port: `update_release_on_fh_close` — on `release()` of a writable file handle,
    trigger `update_cache_for_releases` for the associated release.
  - Port: 12 no-op/minimal FUSE stubs required by `fuser` trait: `forget`, `mknod`,
    `flush`, `setattr`, `getxattr`, `setxattr`, `listxattr`, `removexattr`, `statfs`,
    `ftruncate`, etc.
  - Done-when: Integration test: mount, `ls` directories, read an audio file, verify
    content matches source.

- [ ] **M-11.5** Port VFS — write operations and ghost file protocol.
  - The ghost file system is a 6-step protocol spanning multiple FUSE syscall handlers:
    1. **`mkdir`** inside collage dir -> add to `in_progress_collage_additions` TTLCache
       (5s), call `add_release_to_collage()`.
    2. **`opendir`** on ghost dir -> return success (pretend it's real and empty).
    3. **`getattr`/`lookup`** on files inside ghost dir -> return fake attributes.
    4. **`open`** on files inside ghost dir -> route to `/dev/null`.
    5. **`open`** on `.rose.{uuid}.toml` inside ghost dir -> this is the collage trigger.
    6. After 5s, ghost dir expires from TTLCache.
  - Playlist ghost protocol:
    1. **`write`/`release`** for track copy -> `add_track_to_playlist()`.
    2. Pretend written file exists for 2s after handle release.
    3. After 2s, ghost file expires.
  - Port: `file_creation_special_ops` state machine.
  - Done-when: Integration tests:
    - `cp -r` a release into a collage dir succeeds without error.
    - `cp` a track into a playlist dir succeeds without error.
    - Ghost dir/file expires after timeout (not visible in `ls`).
    - Operations during ghost window behave correctly.

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
  - Note: `dump_all_artists` has an N+1 query problem (per-artist
    `find_releases_matching_rule` in a loop). Reproduce for now; optimize later.
  - Use existing `syrupy` snapshots from `dump_test.ambr` as golden file references.
  - Done-when: `cargo test` passes. JSON output for seeded data matches golden files.

- [ ] **M-12.3** Port CLI — cache commands.
  - Port: `rose cache update [--force]`.
  - Done-when: `rose cache update` populates cache from test library.

- [ ] **M-12.4** Port CLI — release commands.
  - Port: `rose releases print`, `print-all`, `edit`, `toggle-new`, `toggle-favorite`,
    `set-rating`, `delete`, `set-cover`, `delete-cover`, `run-rule`, `create-single`
    (with `--loose-track` flag).
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
  - Note: Rust-in-Nix requires either `crane`, `naersk`, or
    `rustPlatform.buildRustPackage`. The PyO3 crate needs special handling (Python + Rust
    cross-compilation). Budget time for Nix plumbing.
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

## Change Log from v2

| # | Feedback Item | What Changed |
|---|---------------|--------------|
| 1 | VFS thread safety not addressed | Added to **cross-cutting decisions**. **M-11.1** now requires full shared state inventory with sync primitives. |
| 2 | `minijinja` incompatibilities with Python string methods/slices | **M-4.1** rewritten: documents specific incompatibilities, requires custom filter implementations, notes breaking change for user templates. |
| 3 | Ghost file protocol spans 6 syscall handlers | **M-11.5** rewritten with full 6-step collage protocol and 3-step playlist protocol, plus `file_creation_special_ops`. |
| 4 | FTS query is a 7-table JOIN with custom function | **M-7.4** expanded: describes JOIN complexity, custom function registration via `rusqlite`, FTS5 availability verification. |
| 5 | `edit_release` much more complex than described | **M-9.1** expanded: TOML null hacks, per-track dirty checking, resume-on-failure, `MetadataRelease`/`MetadataTrack` types, dynamic role dispatch. |
| 6 | SQLite PRAGMAs not listed | Added to **cross-cutting decisions**. **M-6.1** now lists exact PRAGMAs and requires verification. |
| 7 | Multiprocessing vs multithreading isolation | Added to **cross-cutting decisions**. **M-7.2** specifies per-thread connections and main-thread write coordination. |
| 8 | NFC normalization contradicts Python behavior | **Cross-cutting decision reversed:** raw byte comparison by default (match Python). NFC only on `sanitize_*` output. |
| 9 | `update_release_on_fh_close` missing | Added to **M-11.1** (state inventory) and **M-11.4** (implementation). |
| 10 | `FileHandleManager` wrapping bug | Noted in **M-11.1** and **M-11.4** — fix in Rust: `(n+1) % 10_000`. |
| 11 | 12 FUSE no-op stubs needed | Added to **M-11.4**. |
| 12 | `dump_all_artists` N+1 query | Noted in **M-12.2** — reproduce and mark as tech debt. |
| 13 | Config unknown-key detection | Added to **M-5.1**. |
| 14 | Nix-Rust integration complexity | Added note to **M-15.3** about `crane`/`naersk`/`rustPlatform` and PyO3 cross-compilation. |
| 15 | `--loose-track` flag | Verified present in **M-12.4**, explicitly added to **M-9.1**. |
