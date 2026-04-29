# 001 — Incremental Python-to-Rust Migration Plan for Rose

## 1. Situation Assessment

**What Rose is:** A ~11,400-line (hand-written) Python music library manager with 4
packages (`rose-py`, `rose-cli`, `rose-vfs`, `rose-watch`), a SQLite read cache, FUSE
virtual filesystem, CLI, and file watcher. The architecture doc
(`docs/ARCHITECTURE.md:179-183`) explicitly states: *"Python was chosen to make this a
quick project, but it's ultimately too unperformant for a virtual filesystem. Bad
choice."* The Nix dev shell already includes the Rust toolchain, and `.gitignore` already
ignores `rose-rs/target/`, confirming Rust migration was already anticipated.

**Why incremental:** A big-bang rewrite creates a months-long gap where nothing is
verifiable. The architecture's clean unidirectional data flow and layered dependency graph
make bottom-up incremental replacement feasible. Each phase produces a working `rose`
binary with more modules in Rust.

**Context-minimizing means:** Each migration unit should be achievable by someone who
understands only the module being migrated and its immediate interface contract — not the
entire codebase. This is accomplished by migrating leaf-ward modules first and keeping
each unit small.

### Constraints (from owner)

1. **Audio tag structure must stay the same.** The on-disk tags (`roseid`,
   `rosereleaseid`, genre/artist/label tag encoding, etc.) are the source of truth for
   the entire music library. Changing them would corrupt the library.
2. **Feature power must be equal or superior.** Every *capability* of the Python version
   must exist in Rust. The interface (CLI flags, config keys, etc.) can change freely.
3. **Config format can change.** The sole user will adjust configs as needed.
4. **Python scripting shim required.** A thin PyO3 wrapper (`rose-py`) must expose
   `rose-core` to Python for ad-hoc scripting. This is NOT a migration bridge — it is a
   permanent, thin final layer built after `rose-core` is complete.

---

## 2. Codebase Summary

### Package Structure

```
rose-py/    core library (~11,400 lines hand-written, ~24,400 lines generated)
rose-cli/   CLI frontend via click (~1,400 lines)
rose-vfs/   FUSE virtual filesystem via llfuse (~2,100 lines)
rose-watch/ File watcher via watchdog (~260 lines)
```

### Internal Dependency Graph (rose-py)

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

### External Dependencies (Python -> Rust equivalents)

| Python          | Rust Equivalent     | Purpose                              |
|-----------------|---------------------|--------------------------------------|
| mutagen         | lofty               | Audio tag reading/writing            |
| llfuse          | fuser               | FUSE virtual filesystem              |
| click           | clap                | CLI framework                        |
| jinja2          | minijinja           | Path template rendering              |
| tomllib/tomli-w | toml/serde          | TOML config parsing/writing          |
| sqlite3         | rusqlite            | SQLite database                      |
| watchdog        | notify              | Filesystem event monitoring          |
| appdirs         | dirs                | XDG directory resolution             |
| send2trash      | trash               | Safe file deletion                   |
| uuid6           | uuid                | UUID v7 generation                   |
| multiprocessing | rayon               | Parallel processing                  |
| hashlib         | sha2                | SHA-256 hashing                      |
| re              | regex               | Regular expressions                  |

### Key Files for Reference

| File                              | Lines | Role                              |
|-----------------------------------|-------|-----------------------------------|
| `rose-py/rose/common.py`         | 231   | Foundation types, errors, utils   |
| `rose-py/rose/audiotags.py`      | 629   | Audio tag read/write (mutagen)    |
| `rose-py/rose/rule_parser.py`    | 846   | DSL parser for rules engine       |
| `rose-py/rose/templates.py`      | 588   | Jinja2 path template evaluation   |
| `rose-py/rose/config.py`         | 609   | TOML config parsing               |
| `rose-py/rose/cache.py`          | 2,545 | SQLite cache, all read queries    |
| `rose-py/rose/cache.sql`         | 290   | Database schema                   |
| `rose-py/rose/rules.py`          | 898   | Rule executor                     |
| `rose-py/rose/releases.py`       | 641   | Release CRUD                      |
| `rose-py/rose/tracks.py`         | ~200  | Track CRUD                        |
| `rose-py/rose/collages.py`       | ~200  | Collage CRUD                      |
| `rose-py/rose/playlists.py`      | ~400  | Playlist CRUD                     |
| `rose-py/rose/__init__.py`       | 286   | Public API facade (140 symbols)   |
| `rose-py/rose/genre_hierarchy.py`| 24,430| Generated genre data              |
| `rose-vfs/rose_vfs/virtualfs.py` | 2,089 | FUSE implementation               |
| `rose-cli/rose_cli/cli.py`       | 793   | CLI commands (click)              |
| `conftest.py`                     | 310   | Shared test fixtures              |

---

## 3. Strategy: Clean Rust Rewrite, Module by Module

Since there are no external API consumers and no Python compatibility requirement, we skip
PyO3 entirely. The approach is a **clean Rust rewrite** done module-by-module in
dependency order. The Python code stays in-tree as a behavioral reference but is never
wired to the Rust code. Each Rust module gets its own Rust test suite that covers all
capabilities of the corresponding Python module.

The Python codebase is the **specification**. We read it, we write Rust, we test the Rust.
When the full Rust binary passes all tests and covers all features, the Python code is
deleted.

### Crate Layout

```
rose-rs/
  Cargo.toml              (workspace root)
  rose-core/              (library: all domain logic, pure Rust, no Python dep)
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
      cache.sql            (copied/shared with Python version)
      rules.rs
      releases.rs
      tracks.rs
      collages.rs
      playlists.rs
  rose-py/                (PyO3 shim: thin Python wrapper over rose-core for scripting)
    Cargo.toml
    src/lib.rs
  rose-vfs/               (binary: FUSE virtual filesystem)
    Cargo.toml
    src/main.rs
  rose-cli/               (binary: CLI entry point)
    Cargo.toml
    src/main.rs
  rose-watch/             (binary or integrated into CLI: file watcher)
    Cargo.toml
    src/main.rs
```

All three binaries depend on `rose-core`. They can also be collapsed into a single binary
with subcommands (the CLI does `rose fs mount`, `rose cache watch`, etc. anyway).

`rose-py` is a **permanent** PyO3 crate that wraps `rose-core` for Python scripting. It
exposes the key types (`Config`, `Release`, `Track`, `Collage`, `Playlist`, `AudioTags`,
etc.) and operations (`update_cache`, `list_releases`, `find_releases_matching_rule`,
etc.) as a Python module. It does not need to mirror the Python API 1:1 — it just needs
to expose enough surface for useful scripting. It is built *after* `rose-core` is
complete (Phase 8), not used as a migration bridge.

---

## 4. Migration Phases

### Phase 1: Workspace + `common` + `genre_hierarchy`

**What:** Set up Cargo workspace. Port `Artist`, `ArtistMapping`, error types,
`sanitize_dirname`/`sanitize_filename`, `sha256_dataclass`, `flatten`, `uniq`, `VERSION`.
Port or code-generate `genre_hierarchy`.

**Why first:** Zero internal dependencies. Establishes project structure, CI, and the
foundational types every other module uses.

**Context needed:** `common.py` (231 lines), `genre_hierarchy.py` (generated data).

**Rust tests to write:**
- `sanitize_dirname` / `sanitize_filename`: Unicode edge cases, max-length truncation,
  illegal char replacement, diacritics stripping. Include the cases from `common_test.py`
  plus additional edge cases (empty string, all-illegal, multi-byte truncation boundary).
- `ArtistMapping`: `.all()` deduplication and ordering, `.dump()` serialization.
- `sha256_dataclass`: determinism, field-order independence.

**Estimated size:** ~400 lines Rust

### Phase 2: `audiotags` + `rule_parser`

**What:** Port audio tag reading/writing (using `lofty` crate) and the rule DSL parser.

**Why second:** Both depend only on `common`. `rule_parser` is pure logic (846 lines,
zero I/O). `audiotags` has a well-defined interface over 5 audio formats.

**Context needed:** `audiotags.py` (629 lines), `rule_parser.py` (846 lines).

**Critical constraint:** The tag structure must be identical. The Rust `audiotags` must
read and write the exact same tag names/fields/encoding as the Python version. This means:
- `roseid` and `rosereleaseid` custom tags preserved identically
- Artist role encoding in tags (`;` delimited, role-prefixed) identical
- Genre/label/descriptor tag names identical
- Date parsing identical
- All 5 formats (FLAC, MP3, M4A, OGG Vorbis, OGG Opus) supported

**Rust tests to write:**
- Roundtrip read/write for all 5 formats using files from `testdata/Tagger/`
- Parser: all valid rule syntaxes, all error cases from `rule_parser_test.py`
- Parser: fuzz-style tests (arbitrary strings should return `Err`, never panic)

**Estimated size:** ~1,200 lines Rust

### Phase 3: `templates` + `config`

**What:** Port path template evaluation and TOML config parsing.

**Context needed:** `templates.py` (588 lines), `config.py` (609 lines).

**Notes:**
- Templates can use `minijinja` for Jinja2 compatibility, or a simpler custom evaluator
  if the template language is constrained enough. Need to check which Jinja2 features are
  actually used (filters, conditionals, loops, etc.).
- Config format is free to change. We can use `serde` + `toml` and redesign the config
  struct as idiomatic Rust. Just preserve all *capabilities* (all config knobs).

**Rust tests to write:**
- Template evaluation for all `PathContext` variants
- Config parsing: valid configs, missing keys, invalid values, defaults
- Template + config integration: sample music rendering

**Estimated size:** ~900 lines Rust

### Phase 4: `cache`

**What:** Port the SQLite cache layer. This is the largest and most critical module.

**Context needed:** `cache.py` (2,545 lines), `cache.sql` (290 lines).

**Sub-phases:**
1. **4a:** Schema + bootstrapping. `rusqlite` + the same `cache.sql`. Schema hash
   validation, migration logic.
2. **4b:** Read queries. All `get_*`, `list_*`, `*_exists` functions. The `Release`,
   `Track`, `Collage`, `Playlist` structs.
3. **4c:** Cache update — single-threaded. Read source dir, compare mtimes, read tags,
   write to SQLite.
4. **4d:** Parallel cache update with `rayon`.
5. **4e:** Locking (advisory locks table) + FTS index sync.

**Important:** The SQLite schema (`cache.sql`) can be shared verbatim — it's just SQL.
The `rusqlite` and Python `sqlite3` both wrap the same C library, so schema compatibility
is guaranteed.

**Rust tests to write:**
- Schema bootstrap + rebuild from scratch
- Cache update with test audio files from `testdata/`
- All read queries with seeded data (equivalent to `conftest.py:_seed_cache`)
- Parallel update correctness
- Lock acquisition/release/expiry
- FTS index correctness

**Estimated size:** ~2,500 lines Rust

### Phase 5: `rules` + `releases` + `tracks` + `collages` + `playlists`

**What:** Port business logic — CRUD operations and rule executor.

**Context needed:** Each module + its dependencies (all now in Rust).

**Order:**
1. `rules.rs` — depends on cache, audiotags, rule_parser
2. `releases.rs` + `tracks.rs` — depend on rules
3. `collages.rs` + `playlists.rs` — depend on releases, tracks

**Rust tests to write:**
- Rule execution: replace, sed, split, add, delete on all tag fields
- Release CRUD: create, edit, delete, toggle-new, toggle-favorite, set-rating, cover art
- Track CRUD: matching, action execution
- Collage CRUD: create, rename, delete, add/remove release, edit
- Playlist CRUD: create, rename, delete, add/remove track, edit, cover art

**Estimated size:** ~2,000 lines Rust

### Phase 6: VFS (`rose-vfs`)

**What:** FUSE virtual filesystem using `fuser` crate. This is the primary performance
payoff of the migration.

**Context needed:** `virtualfs.py` (2,089 lines), `rose-core` API.

**Rust tests to write:**
- Mount/unmount lifecycle
- Directory listing for all virtual paths (releases, artists, genres, labels, collages,
  playlists, new, recently added)
- File read (audio files, cover art)
- File write (add release to collage, add track to playlist)
- Ghost file behavior
- Blacklist/whitelist filtering

**Estimated size:** ~2,000 lines Rust

### Phase 7: CLI + Watcher

**What:** CLI with `clap`, watcher with `notify`. Can be one binary with subcommands.

**Context needed:** `cli.py` (793 lines), `watcher.py` (~130 lines), `dump.py`,
`rose_cli/templates.py`.

**Capability inventory (every CLI subcommand that must exist):**

```
rose version
rose config generate-completion <shell>
rose config preview-templates
rose cache update [--force]
rose cache watch [--foreground]
rose cache unwatch
rose fs mount [--foreground]
rose fs unmount
rose releases print <release>
rose releases print-all [matcher]
rose releases edit <release> [--resume]
rose releases toggle-new <release>
rose releases toggle-favorite <release>
rose releases set-rating <release> <rating> [--clear]
rose releases delete <release>
rose releases set-cover <release> <cover>
rose releases delete-cover <release>
rose releases run-rule <release> <actions...> [--dry-run] [--yes]
rose releases create-single <track_path> [--loose-track]
rose tracks print <track>
rose tracks print-all [matcher]
rose tracks run-rule <track> <actions...> [--dry-run] [--yes]
rose collages create <name>
rose collages rename <old> <new>
rose collages delete <collage>
rose collages add-release <collage> <release>
rose collages remove-release <collage> <release>
rose collages edit <collage>
rose collages print <collage>
rose collages print-all
rose playlists create / rename / delete / add-track / remove-track / edit
rose playlists print / print-all / set-cover / delete-cover
rose artists print <artist> / print-all
rose genres print <genre> / print-all
rose labels print <label> / print-all
rose descriptors print <descriptor> / print-all
rose rules run <matcher> <actions...> [--dry-run] [--yes] [--ignore]
rose rules run-stored [--dry-run] [--yes]
```

**Rust tests to write:**
- Integration tests exercising every subcommand
- JSON output format verification for `print` / `print-all` commands
- Watcher: file create/rename/delete triggers cache update

**Estimated size:** ~1,000 lines Rust

### Phase 8: Python Scripting Shim (`rose-py` via PyO3)

**What:** Build the permanent `rose-py` PyO3 crate that wraps `rose-core` for Python
scripting. This is a thin layer — it does not contain logic, just type conversions and
function wrappers.

**Exposed surface (minimum viable):**
- `Config` — load from path
- `update_cache`
- `Release`, `Track`, `Collage`, `Playlist` — read-only data types
- `list_releases`, `list_tracks`, `list_collages`, `list_playlists`
- `get_release`, `get_track`, `get_collage`, `get_playlist`
- `find_releases_matching_rule`, `find_tracks_matching_rule`
- `AudioTags` — read/write
- Rule execution functions
- Release/Track/Collage/Playlist CRUD operations

The shim is intentionally minimal and can grow surface over time as scripting needs arise.

**Estimated size:** ~500 lines Rust (PyO3 boilerplate)

### Phase 9: Cleanup

- Delete old Python source (`rose-py/`, `rose-cli/`, `rose-vfs/`, `rose-watch/`)
- Update `flake.nix` to build the Rust binary + the PyO3 shim
- Update CI to `cargo test` + `cargo clippy` + `cargo fmt` + shim smoke test
- Update docs
- Final deliverables: single `rose` binary + `rose` Python package (PyO3)

---

## 5. Addressing Test Noncompliance

"Test noncompliance" = Rust code that has tests but the tests don't actually verify the
behavior matches what the Python version does, letting bugs slip through.

### Method 1: Test Against Real Files from `testdata/`

The `testdata/` directory contains real audio files with known tags, real collage/playlist
TOML files, and cover art. Rust tests must use these same fixtures and assert the same
extracted values. This is the most direct way to verify behavioral equivalence for the
critical audio tag path.

Concretely: for each of the 5 audio formats in `testdata/Tagger/`, the Rust test reads
the file and asserts every tag field matches the expected values (which we extract from
the Python tests or by running the Python code once and recording the output).

### Method 2: Golden File Tests

Before starting each module's Rust port, run the Python code and capture its output as
golden files (JSON, text, or SQLite dumps). The Rust tests then assert against these
golden files.

Priority targets:
- **audiotags:** JSON dump of all tags for each test audio file
- **cache:** SQLite dump after `update_cache` on `testdata/`
- **templates:** Rendered paths for all template contexts
- **rule_parser:** Serialized AST for every test case in `rule_parser_test.py`
- **CLI:** JSON output of every `print` / `print-all` command

Golden files are checked into the repo under `testdata/golden/` or
`rose-rs/testdata/golden/`.

### Method 3: Property-Based Testing (Rust)

Use `proptest` or `quickcheck` crate for Rust property tests:

```rust
proptest! {
    #[test]
    fn sanitize_dirname_never_exceeds_max_bytes(s in "\\PC{0,300}") {
        let result = sanitize_dirname(&config, &s, true);
        assert!(result.as_bytes().len() <= 240);
        assert!(!ILLEGAL_FS_CHARS_REGEX.is_match(&result));
    }

    #[test]
    fn rule_parser_never_panics(s in "\\PC{0,500}") {
        let _ = parse_rule(&s);  // may return Err, must not panic
    }
}
```

### Method 4: Coverage-Gated Module Sign-Off

Before considering a Rust module "done," run `cargo tarpaulin` (or `llvm-cov`) and
require >= 90% line coverage. Modules below threshold get more tests before moving on.

---

## 6. Addressing Feature Incompleteness

"Feature incompleteness" = the Rust version is missing a capability that existed in
Python. A forgotten CLI subcommand, an unimplemented rule action type, a missing config
knob, etc.

### Method 1: Feature Inventory (The Canonical Checklist)

Before starting the Rust rewrite of each module, extract a **feature inventory** from the
Python source. This is a flat list of every capability, not every function. Examples:

```
## audiotags features
- Read tags from FLAC files
- Read tags from MP3 files (ID3v2)
- Read tags from M4A files (MP4 atoms)
- Read tags from OGG Vorbis files
- Read tags from OGG Opus files
- Write tags to all 5 formats
- Read/write custom `roseid` tag
- Read/write custom `rosereleaseid` tag
- Parse artist strings with role delimiters
- Parse date with year-only, year-month, and year-month-day formats
- Handle missing/empty tags gracefully
- Normalize tag value types (list-of-string vs string)
```

Each item gets a checkbox. The module is not done until all boxes are checked. The
inventory is stored in `z-thinking/` as a numbered document for each phase.

### Method 2: CLI Command Parity Matrix

The CLI is the user-facing surface. A simple matrix:

| Command | Python | Rust | Tested |
|---------|--------|------|--------|
| `rose cache update` | Y | | |
| `rose cache update --force` | Y | | |
| `rose releases print <id>` | Y | | |
| ... | | | |

This matrix lives in `z-thinking/` and is updated as commands are implemented. It is the
single source of truth for "are we done."

### Method 3: End-to-End Smoke Test Script

A shell script that exercises the full lifecycle against a real (small) music library:

```bash
#!/usr/bin/env bash
set -euo pipefail
# Uses testdata/ to create a temporary library and exercises every command.
# Exit 0 = all features work. Exit non-zero = something is missing.
```

This runs in CI. It's the final gate: if the smoke test passes, the Rust binary is
feature-complete.

---

## 7. Quality Gates

Every module is considered done when:

| Gate | Tool | Purpose |
|------|------|---------|
| Rust unit/integration tests pass | `cargo test` | Correctness |
| Rust lints pass | `cargo clippy -- -D warnings` | Code quality |
| Format check passes | `cargo fmt --check` | Style |
| Coverage >= 90% | `cargo tarpaulin` / `llvm-cov` | Test thoroughness |
| Feature inventory fully checked | `z-thinking/` doc | Completeness |
| Golden file tests pass (where applicable) | `cargo test` | Behavioral equivalence |
| Property tests pass | `proptest` | Edge case robustness |

At the end (Phase 8), the additional gate:

| Gate | Tool | Purpose |
|------|------|---------|
| CLI parity matrix 100% | `z-thinking/` doc | No missing commands |
| E2E smoke test passes | Shell script in CI | Full lifecycle works |
| Nix build succeeds | `nix build` | Packaging works |

---

## 8. Risk Mitigation

| Risk | Mitigation |
|------|------------|
| `lofty` tag handling differs from `mutagen` | Golden file tests for all 5 formats. Run Python once, record expected tag values, assert in Rust. If `lofty` can't handle a format, fall back to `symphonia` or `id3`/`metaflac` crates. |
| `minijinja` differs from Jinja2 | Golden file tests for all template evaluations. If incompatible, use a simpler custom template engine — the template language used in Rose is limited. |
| SQLite schema compatibility | Share the same `cache.sql` file. Both `rusqlite` and Python `sqlite3` use the same C library. |
| FUSE semantics differ | The ghost-file behavior and inode management are well-documented in `docs/ARCHITECTURE.md`. Integration tests with real FUSE mounts are the gate. |
| Migration stalls | Each phase is independently useful. Phases 1-5 produce a Rust library that can be tested standalone. Phases 6-7 can be done in any order. The Python version remains functional throughout. |

---

## 9. Estimated Size

| Phase | Modules | Est. Rust LoC | Complexity |
|-------|---------|---------------|------------|
| 1 | `common`, `genre_hierarchy` | ~400 | Low |
| 2 | `audiotags`, `rule_parser` | ~1,200 | Medium |
| 3 | `templates`, `config` | ~900 | Medium |
| 4 | `cache` (sub-phases a-e) | ~2,500 | High |
| 5 | `rules`, `releases`, `tracks`, `collages`, `playlists` | ~2,000 | Medium |
| 6 | `rose-vfs` | ~2,000 | High |
| 7 | `rose-cli`, `rose-watch` | ~1,000 | Low |
| 8 | PyO3 scripting shim (`rose-py`) | ~500 | Low |
| 9 | Cleanup | -net | Low |
| **Total** | | **~10,500** | |

---

## 10. State Tracking

- [ ] Phase 1: Workspace + `common` + `genre_hierarchy`
- [ ] Phase 2: `audiotags` + `rule_parser`
- [ ] Phase 3: `templates` + `config`
- [ ] Phase 4a: Cache schema + bootstrapping
- [ ] Phase 4b: Cache read queries
- [ ] Phase 4c: Cache update (single-threaded)
- [ ] Phase 4d: Cache update (parallel, rayon)
- [ ] Phase 4e: Cache locking + FTS
- [ ] Phase 5: Business logic (`rules`, `releases`, `tracks`, `collages`, `playlists`)
- [ ] Phase 6: FUSE virtual filesystem
- [ ] Phase 7: CLI + watcher
- [ ] Phase 8: PyO3 scripting shim (`rose-py`)
- [ ] Phase 9: Cleanup + delete Python
