# 008 — Rose: Python-to-Rust Migration — Final Plan (v2)

Self-contained reference for any agent executing the migration. Contains all context,
decisions, constraints, known bugs, and the milestone checklist. No other document needs
to be read.

---

## 1. What Is Rose?

Rose is a music library manager for Unix. It manages metadata for a collection of audio
files organized as releases (directories of tracks) in a single `music_source_dir`.

**Core architecture** (from `docs/ARCHITECTURE.md`):

```
Source Files (audio tags, .rose.{uuid}.toml, collage/playlist TOML)
     |                              ^               ^
     | populates                    | writes         | writes
     v                              |               |
Read Cache (SQLite)  ----reads----> Metadata Tooling (CLI / rules engine)
     |                              
     | reads                        
     v                              
Virtual Filesystem (FUSE)  ---writes---> Source Files
```

- **Source files** are the single source of truth.
- The **SQLite read cache** is derived and can always be rebuilt.
- The **VFS** and **CLI** read from cache, write to source files.
- Unidirectional data flow: no ambiguous conflict resolution.

**Current implementation:** ~11,400 lines of hand-written Python across 4 packages:

| Package | Purpose | Size |
|---------|---------|------|
| `rose-py/rose/` | Core library (types, cache, rules, CRUD) | ~11,400 lines + 24,400 generated |
| `rose-cli/rose_cli/` | CLI via `click` | ~1,400 lines |
| `rose-vfs/rose_vfs/` | FUSE filesystem via `llfuse` | ~2,100 lines |
| `rose-watch/rose_watch/` | File watcher via `watchdog` | ~192 lines |

**Why migrate:** The author states Python is "too unperformant for a virtual filesystem."
The Rust toolchain is already in the Nix dev shell. `.gitignore` already ignores
`rose-rs/target/`.

---

## 2. Constraints

1. **Audio tag structure is frozen.** The on-disk tags (`roseid`, `rosereleaseid`, genre
   encoding with `;` delimiters, artist role encoding, `\\PARENTS:\\` genre delimiter,
   etc.) are the source of truth. Changing them corrupts the library.

2. **`.rose.{uuid}.toml` format is frozen.** Key names (`new`, `favorite`, `rating`,
   `added_at`), the `-1` encoding for null rating, ISO8601 datetime for `added_at`. The
   UUID is in the filename, extracted by regex `^\.rose\.([^.]+)\.toml$`.

3. **Feature power must be equal or superior.** Every capability must exist in Rust. The
   interface (CLI flags, config keys, template syntax) can change freely.

4. **Config format can change.** The sole user will adjust.

5. **A PyO3 Python scripting shim is required** as a permanent deliverable (not a
   migration bridge).

---

## 3. Cross-Cutting Decisions

These apply from the first line of Rust onward.

**Logging:** Use `tracing` crate (structured, async-compatible, leveled).

**Path normalization:** Do NOT normalize paths to NFC for comparison (match Python, which
compares raw OS bytes). `sanitize_dirname`/`sanitize_filename` apply NFC on *output* only
(matching Python). Rationale: Python has NFC normalization commented out in
`_compare_strs()` (`cache.py:2535`). Changing this would alter cache hit/miss behavior.

**Error recovery in parallel code:** `rayon` tasks return `Result`. Errors collected into
a `Vec` and reported after all valid work completes. One corrupt directory never aborts the
entire cache update.

**SQLite connection setup:** Every `rusqlite::Connection` must set on open:
- `PRAGMA journal_mode=WAL`
- `PRAGMA foreign_keys=ON`
- `busy_timeout(15000)` (15 seconds)
- Autocommit mode (no implicit transactions)

(Matches `cache.py:88-94`.)

**SQLite concurrency with rayon:** Each rayon worker opens its own connection. WAL mode
and busy_timeout are required for concurrent access. Workers return data structures; the
main thread applies all SQL writes in a single transaction.

**VFS thread safety:** Python's GIL protects all shared state. Rust has no GIL. All shared
mutable VFS state uses `Arc<RwLock<T>>` or `DashMap`. The full inventory of shared state
(7 structures) is listed in M-11.1.

**Regex flavor:** Rust `regex` crate is mostly compatible with Python `re`, but:
- Replacement syntax: `$1`/`${name}` in Rust vs `\1`/`\g<name>` in Python. Document.
- No lookahead/lookbehind. Use `fancy-regex` if needed.

---

## 4. Internal Dependency Graph (rose-py)

Migration order follows this bottom-up:

```
common, genre_hierarchy           <- zero internal deps
  |
audiotags, rule_parser            <- depend on common
  |
templates                         <- depends on audiotags, common
  |
config                            <- depends on common, rule_parser, templates
  |
cache                             <- depends on audiotags, common, config, genre_hierarchy, templates
  |
rules                             <- depends on audiotags, cache, common, config, rule_parser
  |
releases, tracks                  <- depend on most of the above
  |
collages, playlists               <- depend on cache, releases, tracks
```

---

## 5. External Dependencies

| Python          | Rust Equivalent     | Purpose                              |
|-----------------|---------------------|--------------------------------------|
| mutagen         | lofty               | Audio tag reading/writing            |
| llfuse          | fuser               | FUSE virtual filesystem              |
| click           | clap                | CLI framework                        |
| jinja2          | minijinja           | Path template rendering              |
| tomllib/tomli-w | toml/serde          | TOML config parsing/writing          |
| sqlite3         | rusqlite (bundled)  | SQLite database                      |
| watchdog        | notify              | Filesystem event monitoring          |
| appdirs         | dirs                | XDG directory resolution             |
| send2trash      | trash               | Safe file deletion                   |
| uuid6           | uuid                | UUID v7 generation                   |
| multiprocessing | rayon               | Parallel processing                  |
| hashlib         | sha2                | SHA-256 hashing                      |
| re              | regex / fancy-regex | Regular expressions                  |

---

## 6. Crate Layout

```
rose-rs/
  Cargo.toml              (workspace root)
  rose-core/              (library: all domain logic, pure Rust)
    Cargo.toml
    src/
      lib.rs
      common.rs
      genre_hierarchy.rs
      audiotags.rs
      rule_parser.rs
      templates.rs
      config.rs
      cache.rs
      cache.sql
      rules.rs
      releases.rs
      tracks.rs
      collages.rs
      playlists.rs
  rose-py/                (PyO3 shim: permanent Python wrapper for scripting)
    Cargo.toml
    src/lib.rs
  rose-vfs/               (binary: FUSE virtual filesystem)
    Cargo.toml
    src/main.rs
  rose-cli/               (binary: CLI entry point — may absorb VFS + watcher)
    Cargo.toml
    src/main.rs
```

---

## 7. Known Python Bugs to Fix in Rust

| Bug | Location | Fix |
|-----|----------|-----|
| `--ignore` for `new`/`favorite` assigns to `match` instead of `skip` | `rules.py:302,306` | Use `skip = ...` |
| Collage editor silently drops duplicate `description_meta` entries | `collages.py:149` | Add `[uuid]` discriminator (like playlists do) |
| `FileHandleManager` counter never wraps (`1 % 10_000` precedence) | `virtualfs.py:843` | Use `(n + 1) % 10_000` |
| Stale PID file not cleaned up when watchdog process dies | `cli.py:764` | Delete stale PID file when `os.kill(pid, 0)` fails |

## 8. Known Python Quirks to NOT Reproduce

| Quirk | Location | Why |
|-------|----------|-----|
| MP4 literal `"None"` string handling | `audiotags.py:419-420` | `mutagen`-specific; `lofty` won't produce this |
| Dead code: `parse_collage_argument`/`parse_playlist_argument` | `cli.py:730-753` | Never called |
| `dump_all_artists` N+1 query | `dump.py:154` | Reproduce for now, optimize later |
| Config parsed on every CLI invocation including `rose version` | `cli.py:109-111` | Lazy-load in Rust |

---

## 9. Key Behaviors That Are Easy to Miss

These are non-obvious behaviors that milestones call out but deserve emphasis:

1. **Cache update writes to source files.** `update_cache_for_collages` and
   `update_cache_for_playlists` rewrite TOML files on disk (missing flags,
   `description_meta` regeneration). This is NOT read-only caching.

2. **Rules engine modifies `.rose.{uuid}.toml` datafiles.** Actions on `new`, `favorite`,
   `rating` write to the sidecar file, not audio tags. Writes are deduplicated per-release
   directory.

3. **Parent genre encoding.** When `write_parent_genres` is enabled, genre tags include a
   `\\PARENTS:\\` delimiter separating user genres from auto-populated transitive parents.
   The read path must strip this; the write path must append it.

4. **VFS ghost file protocol.** Adding a release to a collage via `mkdir` triggers a
   6-step protocol spanning `mkdir`, `opendir`, `getattr`, `lookup`, `open`, with a 5s
   TTL ghost directory. Adding a track to a playlist has a 2s ghost file.

5. **VFS 2-hour path TTL.** `VirtualNameGenerator` caches paths for 2 hours so media
   players with cached old paths can still read files after metadata changes.

6. **VFS `update_release_on_fh_close`.** When a writable file handle is released, the
   cache is updated for that release. This is how the VFS triggers refreshes after
   external tools modify files in-place.

7. **Collage/playlist rename also renames adjacent files** with matching stem (cover art).

8. **`edit_release` is a stateful protocol** with TOML null encoding hacks (`-1`, `""`),
   per-track per-field dirty checking, and resume-on-failure (saves to
   `failed-release-edit.{uuid}.toml`).

9. **`_unpack` silently drops mismatched GROUP_CONCAT entries.** Decide: match Python's
   silent behavior or add a warning log.

10. **Template language migration.** `minijinja` doesn't support `.rjust()`, `[:10]`
    slices, or Python method calls. Custom filters needed. This is a breaking change for
    user templates.

11. **Watcher debouncing.** Sync watchdog thread -> channel -> async event loop with 200ms
    dedup and 2s delay for releases. Without debouncing, a 20-file album copy fires 20+
    cache updates.

12. **Sanitizer circular dependency in VFS.** `Sanitizer.unsanitize()` on cache miss calls
    `RoseLogicalCore.readdir()` which calls `Sanitizer.sanitize()`. Use `DashMap` or
    careful lock ordering to avoid deadlock.

13. **Artist string parse/format is a bidirectional protocol.** `parse_artist_string`
    (`audiotags.py:553-608`) destructures a single tag string into roles using keyword
    delimiters: `feat.` (guest), `remixed by` (remixer), `produced by` (producer),
    `pres.` (DJ, reversed order), `performed by` (composer, reversed), `under.`
    (conductor). Parse order matters: `produced by` before `remixed by` before `feat.`.
    `format_artist_string` (`audiotags.py:611-629`) reverses this to compose:
    `composer performed by DJ pres. main under. conductor feat. guest remixed by remixer
    produced by producer`. Only non-alias artists are included. This must roundtrip
    losslessly: `format(parse(s)) == s`. Individual artists within each role are split by
    `TAG_SPLITTER_REGEX` (with spaces around `\\`).

14. **Two different tag splitter regexes.** `audiotags.py:32` defines
    `TAG_SPLITTER_REGEX = re.compile(r"\\\\| / |; ?| vs\. ")` (no spaces around `\\`).
    `audiotags.py:550` **redefines** it as
    `TAG_SPLITTER_REGEX = re.compile(r" \\\\ | / |; ?| vs\. ")` (WITH spaces around `\\`).
    The first is captured by `_split_tag` and `_split_genre_tag` (defined above line 550).
    The second is used by `parse_artist_string` (defined below line 550). So:
    genre/descriptor/label tags split on `\\` (no spaces), artist tags split on ` \\ `
    (with spaces). Both must be ported.

15. **ID3/MP4/Vorbis fallback tag keys.** The read path tries multiple keys for some
    fields: MP3 date reads `TDRC`, then `TYER`, then `TDAT`; release type reads
    `TXXX:RELEASETYPE`, then `TXXX:MusicBrainz Album Type`. MP4 original date tries
    3 atom paths. Vorbis label tries `label`, `organization`, `recordlabel`. The write
    path only writes to the primary key. Test fallback reads.

16. **MP3 TIPL/IPLS paired text frames.** Producer and DJ-mix roles are read from ID3
    paired text frames (`TIPL`/`IPLS`), not standard text frames. These are key-value
    pairs like `[("producer", "Name")]`. On write, these frames are **deleted** and the
    info is folded into the main artist string. Verify `lofty` support for paired frames.

17. **ID3 `TRCK`/`TPOS` combined format.** MP3 stores track/disc numbers as
    `number/total` in a single tag. The read path splits on `/`. The write path writes
    only `tracknumber` (no total), so **tracktotal and disctotal are lost on MP3 write**.
    This is existing behavior to reproduce.

---

## 10. Milestones

### Phase 1: Project Scaffolding

- [ ] **M-1.1** Create Cargo workspace.
  - Produce: `rose-rs/Cargo.toml` (workspace), `rose-rs/rose-core/Cargo.toml`,
    `rose-rs/rose-core/src/lib.rs`.
  - Deps: `serde`, `sha2`, `regex`, `unicode-normalization`, `tracing`, `thiserror`.
  - Done-when: `cargo build` and `cargo clippy` succeed.

- [ ] **M-1.2** Port `common.rs`.
  - Read: `rose-py/rose/common.py` (231 lines).
  - Port: `RoseError` hierarchy (`thiserror`), `Artist`, `ArtistMapping` (`.all()`,
    `.dump()`, `.items()`), `sanitize_dirname`, `sanitize_filename`, `sha256_dataclass`,
    `flatten`, `uniq`, `VERSION`, `ILLEGAL_FS_CHARS_REGEX`.
  - Tests: illegal chars, max-length truncation at byte boundary, diacritics, NFC output,
    empty input, extension preservation, `ArtistMapping::all()` dedup, hash determinism.
  - Done-when: `cargo test` passes.

- [ ] **M-1.3** Port `genre_hierarchy.rs`.
  - Read: `rose-py/rose/genre_hierarchy.py`, `scripts/rym-genres/generate.py`.
  - Note: generator already writes `rose-rs/src/genre_hierarchy.json`.
  - Port: `GENRE_HIERARCHY` (child->parents), `TRANSITIVE_CHILD_GENRES`
    (parent->transitive children). Use `serde_json::from_str(include_str!(...))`.
  - Done-when: `cargo test` passes.

---

### Phase 2: Audio Tags

- [ ] **M-2.0** Generate golden files for tag verification.
  - Python script dumps every tag from `testdata/Tagger/` as JSON + raw bytes for custom
    tags. One golden file per format.
  - Produce: `rose-rs/testdata/golden/tags_*.json`.
  - Done-when: Golden files committed for all 5 formats.

- [ ] **M-2.1** Port `audiotags.rs` — reading.
  - Read: `rose-py/rose/audiotags.py` (629 lines). Dep: `lofty`.
  - Port: `AudioTags::from_file()` for FLAC, MP3, M4A, OGG Vorbis, OGG Opus. All fields.
  - Port: `RoseDate`, `SUPPORTED_AUDIO_EXTENSIONS`.
  - Port: `\\PARENTS:\\` delimiter parsing in genre/secondary_genre tags — strip parent
    section on read (`_split_genre_tag`, `audiotags.py:480-486`).
  - Port: **Two different `TAG_SPLITTER_REGEX` patterns** (see Key Behaviors #14):
    - Genre/descriptor/label splitting: `\\\\| / |; ?| vs\. ` (no spaces around `\\`).
    - Artist splitting: ` \\\\ | / |; ?| vs\. ` (WITH spaces around `\\`).
  - Port: `parse_artist_string` — bidirectional keyword-delimiter protocol (see Key
    Behaviors #13). 6 delimiters: `feat.`, `remixed by`, `produced by`, `pres.`,
    `performed by`, `under.`. Parse order matters. Must roundtrip with
    `format_artist_string`. Dedicated unit tests for all 6 delimiters, combinations,
    and roundtrip.
  - Port: `format_artist_string` — reverse of parse. Compose order: composer
    `performed by` DJ `pres.` main `under.` conductor `feat.` guest `remixed by`
    remixer `produced by` producer. Only non-alias artists.
  - Port: **Fallback tag keys** (see Key Behaviors #15): MP3 date reads `TDRC`/`TYER`/
    `TDAT`; release type reads `TXXX:RELEASETYPE`/`TXXX:MusicBrainz Album Type`; MP4
    original date tries 3 atom paths; Vorbis label tries `label`/`organization`/
    `recordlabel`. Write path uses only the primary key.
  - Port: **TIPL/IPLS paired text frames** for MP3 (see Key Behaviors #16): producer and
    DJ-mix roles read from paired frames. Verify `lofty` support.
  - Port: **ID3 `TRCK`/`TPOS` combined format** (see Key Behaviors #17): split
    `number/total` on read.
  - Done-when: `cargo test` against golden files. Custom tags match at byte level.
    Artist parse/format roundtrip tests. Fallback key tests.

- [ ] **M-2.2** Port `audiotags.rs` — writing.
  - Port: `AudioTags::flush()` for all 5 formats. `maybe_set_ids`.
  - Port: `\\PARENTS:\\` genre encoding on write when `write_parent_genres` enabled.
  - Port: MP3 `TRCK` writes tracknumber only (no total) — tracktotal is lost. Match
    existing behavior.
  - Port: MP3 TIPL/IPLS frames are **deleted** on write; producer/DJ info folded into
    main artist string via `format_artist_string`.
  - Done-when: Roundtrip tests pass for all formats. IDs survive roundtrip. Artist
    parse->format->reparse roundtrips losslessly.

---

### Phase 3: Rule Parser

- [ ] **M-3.1** Port `rule_parser.rs`.
  - Read: `rose-py/rose/rule_parser.py` (846 lines).
  - Port: `Matcher`, `Pattern`, `Action` (Replace/Sed/Split/Add/Delete), `Rule`.
  - Port: All parsers and tag field mappings.
  - Note: Sed action regex uses Python `re` flavor. Rust `regex` replacement syntax
    differs (`$1` vs `\1`). Document. Use `fancy-regex` if lookaround needed.
  - Done-when: All 544 lines of `rule_parser_test.py` cases ported. `proptest`: arbitrary
    strings never panic.

---

### Phase 4: Templates

- [ ] **M-4.1** Port `templates.rs`.
  - Read: `rose-py/rose/templates.py` (588 lines). Dep: `minijinja`.
  - Port: `PathTemplate`, `PathContext`, `PathTemplateConfig`, `evaluate_release_template`,
    `evaluate_track_template`, `get_sample_music`.
  - **Breaking change:** `minijinja` doesn't support `.rjust()`, `[:10]` slices, or
    Python method calls. Implement custom filters:
    - `{{ discnumber | pad(2) }}` replaces `{{ discnumber.rjust(2, '0') }}`
    - `{{ added_at | truncate(10) }}` replaces `{{ added_at[:10] }}`
    - Verify `or` coalescing works correctly for empty strings / custom types.
  - Update default templates in `PathTemplateConfig` to use new syntax.
  - Done-when: `cargo test` with golden files for all template contexts (new defaults).

---

### Phase 5: Config

- [ ] **M-5.1** Port `config.rs`.
  - Read: `rose-py/rose/config.py` (609 lines). Deps: `toml`, `serde`, `dirs`.
  - Port: `Config`, `VirtualFSConfig`, defaults, `PathTemplateConfig` integration, TOML
    loading, all validation, artist alias resolution.
  - Port: Unknown-key detection (warn on unrecognized config keys).
  - Done-when: `cargo test`. Representative cases from `config_test.py` (682 lines).

---

### Phase 6: Cache — Schema, Types, and Reads

- [ ] **M-6.1** Cache schema bootstrapping.
  - Read: `cache.py` (top ~200 lines), `cache.sql` (290 lines).
  - Dep: `rusqlite` (bundled, with FTS5).
  - Port: Schema creation, `_schema_hash`, `maybe_invalidate_cache_database`.
  - Port: `connect()` — set PRAGMAs (WAL, foreign_keys, busy_timeout). Verify FTS5
    availability.
  - Done-when: `cargo test`. Fresh DB, hash mismatch rebuild, PRAGMAs verified.

- [ ] **M-6.2** Cache data types.
  - Port: `Release`, `Track`, `Collage`, `Playlist`, `GenreEntry`, `DescriptorEntry`,
    `LabelEntry`. Row deserialization from SQLite views.
  - Port: `StoredDataFile` — serialize/parse byte-compatible with Python. Golden file test.
  - Port: `STORED_DATA_FILE_REGEX`, `make_release_logtext`, `make_track_logtext`.
  - Decide: `_unpack` GROUP_CONCAT mismatch — silent drop (match Python) or warn.
  - Done-when: `cargo test`. `StoredDataFile` roundtrip. Types compile with Debug/Clone.

- [ ] **M-6.3** Cache read queries (simple).
  - Port: All `get_*`, `list_*`, `*_exists`, `get_*_of_*`, `*_within_*` functions.
  - Done-when: `cargo test` with seeded DB (equivalent to `conftest._seed_cache`).

- [ ] **M-6.4** Cache filtered queries.
  - Read: `cache.py:1770-1982`.
  - Port: `filter_releases` (8 filter dimensions, dynamic SQL, artist alias resolution,
    genre hierarchy expansion). `filter_tracks` (same + track-level joins).
  - Done-when: `cargo test`. Each filter dimension individually and in combination.

---

### Phase 7: Cache — Update Pipeline

- [ ] **M-7.1a** Directory scanning and UUID discovery.
  - Read: `cache.py:490-1349`.
  - Port: Scan source dir, extract UUID from `.rose.{uuid}.toml`, create datafile for
    new releases, detect in-progress directories.
  - Done-when: `cargo test`.

- [ ] **M-7.1b** Mtime comparison and change detection.
  - Port: Compare mtimes, identify changed releases/tracks.
  - Done-when: `cargo test`. Unchanged skipped, touched detected.

- [ ] **M-7.1c** Tag reading and metadata derivation.
  - Port: Read audio tags, derive release metadata from first track, parse
    `StoredDataFile`.
  - Done-when: `cargo test`.

- [ ] **M-7.1d** Source directory renaming.
  - Port: Rename source dir to match template when `rename_source_files` enabled.
    Collision avoidance.
  - Done-when: `cargo test`.

- [ ] **M-7.1e** Batch SQL writes.
  - Port: Batched inserts/updates/deletes.
  - Port: `update_cache_for_collages`, `update_cache_for_playlists` — **these write to
    source TOML files on disk** (missing flags, `description_meta` regeneration). Not
    read-only.
  - Port: `update_cache_evict_nonexistent_releases/collages/playlists`.
  - Done-when: Full end-to-end `cargo test` with `testdata/`.

- [ ] **M-7.2** Parallel cache update with `rayon`.
  - Per-thread connections. Workers return data; main thread writes. Errors collected.
  - Done-when: Results identical to single-threaded. Corrupt dir doesn't abort others.

- [ ] **M-7.3** Cache locking.
  - Port: Advisory locks (`locks` table). Acquire/release/expiry.
  - Done-when: `cargo test`.

- [ ] **M-7.4** Cache FTS index.
  - Port: `process_string_for_fts` (char-level `¬`-separated tokenization).
  - Port: FTS sync — 63-line 7-table JOIN with custom function registered via
    `rusqlite::Connection::create_scalar_function()`.
  - Done-when: FTS substring queries return expected results.

---

### Phase 8: Rules Engine

- [ ] **M-8.1** Port `rules.rs`.
  - Read: `rose-py/rose/rules.py` (898 lines).
  - Port the 5-phase pipeline:
    1. FTS fast search (may produce false positives)
    2. If >400 results: pre-filter via cache before reading from disk
    3. Filter false positives by reading actual audio tags
    4. Apply actions in-memory, compute per-track diffs
    5. Confirmation UI: simple y/n for <=25 tracks, enter-a-number for >25
  - Port: All action types (replace, sed, split, add, delete).
  - Port: `execute_stored_metadata_rules`.
  - Port: Dry-run, `--yes` bypass.
  - **Actions on `new`/`favorite`/`rating` modify `.rose.{uuid}.toml`, not audio tags.**
    Deduplicate datafile writes per-release directory.
  - **Fix bug:** `--ignore` for `new`/`favorite` — use `skip`, not `match`
    (`rules.py:302,306`).
  - Done-when: All 474 lines of `rules_test.py` ported.

---

### Phase 9: Releases and Tracks

- [ ] **M-9.1** Port `releases.rs`.
  - Read: `rose-py/rose/releases.py` (641 lines).
  - Port: `toggle_release_new/favorite`, `set_release_rating`, `delete_release` (`trash`
    crate), cover art set/delete.
  - Port: `edit_release` — complex stateful protocol:
    - `MetadataRelease`/`MetadataTrack` intermediary types.
    - TOML null hacks: `-1` for null rating, `""` for null dates/edition/catalognumber.
    - Opens `$EDITOR`, handles non-zero exit and unchanged file.
    - Per-track per-field dirty checking (15+ fields). Only dirty tracks flushed.
    - Resume-on-failure: saves to `failed-release-edit.{uuid}.toml`, `--resume` reloads.
    - Dynamic role dispatch for artist mapping.
  - Port: `create_single_release` (with `--loose-track` -> `releasetype=loosetrack`).
  - Port: `find_releases_matching_rule`, `run_actions_on_release`.
  - Done-when: `cargo test`. Port cases from `releases_test.py` (511 lines).

- [ ] **M-9.2** Port `tracks.rs`.
  - Read: `rose-py/rose/tracks.py` (67 lines).
  - Port: `find_tracks_matching_rule`, `run_actions_on_track`.
  - Done-when: `cargo test`.

---

### Phase 10: Collages and Playlists

- [ ] **M-10.1** Port `collages.rs`.
  - Read: `rose-py/rose/collages.py` (169 lines).
  - Port: Create/rename/delete. Add/remove release. Edit in editor.
  - Rename also renames adjacent files with matching stem (cover art).
  - **Fix bug:** Duplicate `description_meta` collision — add `[uuid]` discriminator
    (match playlist behavior).
  - Done-when: `cargo test`. Port `collages_test.py` (172 lines).

- [ ] **M-10.2** Port `playlists.rs`.
  - Read: `rose-py/rose/playlists.py` (236 lines).
  - Port: Create/rename/delete. Add/remove track. Edit in editor. Cover art set/delete.
  - Rename also renames adjacent files with matching stem.
  - Done-when: `cargo test`. Port `playlists_test.py` (253 lines).

---

### Phase 11: FUSE Virtual Filesystem

- [ ] **M-11.1** Create `rose-vfs` crate and inventory shared state.
  - Deps: `fuser`, `dashmap`.
  - Shared mutable state needing `Arc<RwLock<T>>` or `DashMap`:
    - `TTLCache` instances (ghost files, getattr cache, lookup cache)
    - `VirtualNameGenerator` (6 bidirectional maps, 2-hour TTL for media player compat)
    - `Sanitizer` mappings
    - `INodeMapper` (counter + bidict; fix wrapping: `(n+1) % 10_000`)
    - `FileHandleManager` (counter; fix wrapping)
    - `file_creation_special_ops` dict
    - `update_release_on_fh_close` dict
  - Done-when: Empty FS mounts/unmounts with multiple workers. Types defined.

- [ ] **M-11.2** Port VFS — path parser and name generation.
  - 12 view types: Root, Releases, Artists, Genres, Descriptors, Labels, Loose Tracks,
    Collages, Playlists, New, Favorites, Added On, Released On.
  - Port: `VirtualPath` parsing, `VirtualNameGenerator`.
  - Note: Sanitizer has a circular readdir fallback into `RoseLogicalCore`. Use `DashMap`
    or careful lock ordering to avoid deadlock.
  - Done-when: `cargo test`. Path parse roundtrip for all view types.

- [ ] **M-11.3** Port VFS — `RoseLogicalCore` (domain logic).
  - Port: All `readdir` logic. Uses `filter_releases`/`filter_tracks`.
  - Port: Date-bucketing for Added On / Released On.
  - Port: `CanShower` whitelist/blacklist. `hide_*_with_only_new_releases`.
  - Done-when: `cargo test`.

- [ ] **M-11.4** Port VFS — file read operations and inode management.
  - Port: `read`, `getattr`, `lookup`. `INodeMapper`, `FileHandleManager`, `TTLCache`.
  - Port: `update_release_on_fh_close` (cache refresh on writable handle release).
  - Port: 12 no-op FUSE stubs (`forget`, `mknod`, `flush`, `setattr`, `getxattr`,
    `setxattr`, `listxattr`, `removexattr`, `statfs`, `ftruncate`, etc.).
  - Done-when: Integration test — mount, ls, read audio file.

- [ ] **M-11.5** Port VFS — write operations and ghost file protocol.
  - Collage ghost (6-step, 5s TTL):
    1. `mkdir` -> add to `in_progress_collage_additions`, call `add_release_to_collage()`
    2. `opendir` on ghost -> return success (empty)
    3. `getattr`/`lookup` inside ghost -> fake attrs
    4. `open` inside ghost -> route to `/dev/null`
    5. `open` on `.rose.{uuid}.toml` -> collage trigger
    6. Ghost expires after 5s
  - Playlist ghost (2s TTL): `write`/`release` -> `add_track_to_playlist()`, pretend file
    exists for 2s.
  - Port: `file_creation_special_ops` state machine.
  - Done-when: `cp -r` into collage and `cp` into playlist both succeed.

---

### Phase 12: CLI

- [ ] **M-12.1** Create `rose-cli` crate.
  - Dep: `clap` (derive). `--verbose`/`-v` sets tracing to DEBUG.
  - Lazy config loading (don't parse for `rose version`).
  - Done-when: `rose version` works, `rose --help` shows all groups.

- [ ] **M-12.2** Port JSON serialization (`dump` module).
  - Read: `dump.py` (288 lines).
  - Port: All `dump_*` functions, `release_to_json`, `track_to_json`,
    `_partition_releases_by_role`.
  - Use existing `dump_test.ambr` snapshots as golden references.
  - Done-when: JSON output matches golden files.

- [ ] **M-12.3** Port cache commands. (`rose cache update [--force]`)

- [ ] **M-12.4** Port release commands. (print, print-all, edit, toggle-new,
  toggle-favorite, set-rating, delete, set-cover, delete-cover, run-rule,
  create-single with --loose-track)

- [ ] **M-12.5** Port track commands. (print, print-all, run-rule)

- [ ] **M-12.6** Port collage commands. (create, rename, delete, add-release,
  remove-release, edit, print, print-all)

- [ ] **M-12.7** Port playlist commands. (create, rename, delete, add-track, remove-track,
  edit, print, print-all, set-cover, delete-cover)

- [ ] **M-12.8** Port browse commands. (artists, genres, labels, descriptors: print,
  print-all)

- [ ] **M-12.9** Port rules commands. (`rose rules run`, `rose rules run-stored` with
  --dry-run, --yes, --ignore)

- [ ] **M-12.10** Port fs commands. (`rose fs mount [--foreground]`, `rose fs unmount`)
  - Daemonization: use `nix::unistd::fork()` or equivalent. Fix stale PID handling.
    Install SIGTERM handler for graceful shutdown.

- [ ] **M-12.11** Port config/misc. (generate-completion, preview-templates)

---

### Phase 13: File Watcher

- [ ] **M-13.1** Port watcher with debouncing.
  - Read: `watcher.py` (192 lines). Deps: `notify`, `tokio` or `crossbeam-channel`.
  - Architecture:
    - Sync `notify` thread pushes events into channel.
    - Async consumer debounces: 200ms dedup window, 2s delay for release events.
    - Event classification: parse path -> release / collage / playlist.
    - Event types: created/deleted/modified/moved. "Moved" = update + evict.
  - Daemonization: fix stale PID, SIGTERM handler.
  - Done-when: Integration test. Debouncing verified.

---

### Phase 14: PyO3 Scripting Shim

- [ ] **M-14.1** Create `rose-py` PyO3 crate. Expose `Config`.
- [ ] **M-14.2** Expose read operations (cache, list, get, types).
- [ ] **M-14.3** Expose write operations (tags, rules, CRUD).

---

### Phase 15: Finalization

- [ ] **M-15.1** End-to-end smoke test (every CLI subcommand).
- [ ] **M-15.2** Benchmark Python vs Rust (VFS readdir, cache update, cold start).
- [ ] **M-15.3** Update Nix build (`crane`/`naersk`/`rustPlatform` + PyO3 cross-compile).
- [ ] **M-15.4** Update CI (cargo test/clippy/fmt, e2e, nix build).
- [ ] **M-15.5** Delete old Python code.
- [ ] **M-15.6** Update documentation.

---

## 11. Key Python Files for Reference

| File | Lines | What to Read It For |
|------|-------|---------------------|
| `rose-py/rose/common.py` | 231 | Types, errors, sanitization |
| `rose-py/rose/audiotags.py` | 629 | Tag read/write, format handling |
| `rose-py/rose/rule_parser.py` | 846 | DSL grammar, all parse functions |
| `rose-py/rose/templates.py` | 588 | Template evaluation, Jinja2 filters |
| `rose-py/rose/config.py` | 609 | Config struct, validation, aliases |
| `rose-py/rose/cache.py` | 2,545 | Schema, cache update, queries, FTS |
| `rose-py/rose/cache.sql` | 290 | SQLite schema (shared verbatim) |
| `rose-py/rose/rules.py` | 898 | Rule execution pipeline |
| `rose-py/rose/releases.py` | 641 | Release CRUD, edit protocol |
| `rose-py/rose/tracks.py` | 67 | Track matching and actions |
| `rose-py/rose/collages.py` | 169 | Collage CRUD, editor |
| `rose-py/rose/playlists.py` | 236 | Playlist CRUD, editor, cover art |
| `rose-py/rose/__init__.py` | 286 | Public API surface (123 symbols) |
| `rose-vfs/rose_vfs/virtualfs.py` | 2,089 | VFS: paths, logic, FUSE ops, ghosts |
| `rose-cli/rose_cli/cli.py` | 793 | CLI commands, daemonization |
| `rose-cli/rose_cli/dump.py` | 288 | JSON serialization for all entities |
| `rose-watch/rose_watch/watcher.py` | 192 | Debounced file watcher |
| `conftest.py` | 310 | Test fixtures, `_seed_cache` |
| `testdata/` | — | Real audio files, collages, playlists |
