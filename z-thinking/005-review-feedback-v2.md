# 005 — Review Feedback on 004-MILESTONES-v2

Second-round review. All 15 items from `003-review-feedback.md` are correctly addressed in
v2. This review covers **new problems** found through deeper codebase analysis.

---

## Verdict

v2 is a substantial improvement. The hard constraints, cross-cutting decisions, and
milestone decomposition are now adequate for Phases 1-10 (the core library). The remaining
problems cluster around **Phase 11 (VFS)**, **Phase 4 (templates)**, and some
under-documented complexity in the cache update pipeline. The VFS is the highest-risk
phase and needs more decomposition.

---

## Critical Flaws

### 1. VFS Thread Safety Is Not Addressed

The Python VFS runs with `llfuse.main(workers=c.max_proc)` (`virtualfs.py:2081`), meaning
**multiple worker threads** serve FUSE requests concurrently. All shared state is accessed
without synchronization:

- `TTLCache` instances (ghost files, getattr/lookup caches, name generator stores)
- `VirtualNameGenerator` internal dicts (6 bidirectional maps)
- `Sanitizer` mappings
- `INodeMapper` (monotonic counter + bidirectional dict)
- `FileHandleManager` (counter)
- `file_creation_special_ops` state dict
- `update_release_on_fh_close` dict

This works in Python only because of the GIL. Rust has no GIL. The `fuser` crate passes
`&mut self` to some callbacks and `&self` to others, but concurrent requests are still
possible.

Every piece of shared mutable state needs either `Arc<RwLock<...>>`,
`Arc<Mutex<...>>`, or a concurrent data structure (`DashMap`). This is a
**design decision** that affects the entire VFS architecture — it's not something an
agent can figure out incrementally per-milestone.

**Fix:** Add a cross-cutting decision for the VFS: "All shared VFS state uses
`Arc<RwLock<T>>` (or `DashMap` for hot-path maps). `fuser` `Filesystem` impl holds
`Arc` references. Identify the full list of shared state in M-11.1 before writing any
VFS logic."

### 2. `minijinja` Does Not Support Python String Methods or Slice Notation

The default templates use:

```jinja
{{ discnumber.rjust(2, '0') }}    {# Python str.rjust() #}
{{ tracknumber.rjust(2, '0') }}   {# Python str.rjust() #}
{{ added_at[:10] }}               {# Python slice notation #}
{{ originaldate or releasedate or '0000-00-00' }}  {# Python falsy coalescing #}
```

(`templates.py:171`, `templates.py:220`, `templates.py:226`)

`minijinja` is a Jinja2-compatible engine for Rust, but it does **not** support:
- Calling arbitrary methods on values (`.rjust()`)
- Python-style slice notation (`[:10]`)

The `or` coalescing *might* work differently — `minijinja` treats `0` and empty strings
as falsy like Python, but custom types like `RoseDate` may not implement the right traits.

Since the config format is free to change, these templates can be redesigned. But the
milestone must explicitly flag this as a **template language migration** that changes user-
facing defaults, not a transparent port. Users with custom templates in their config will
need to update them.

**Fix:** M-4.1 should:
1. Document which Jinja2 features are used and which `minijinja` doesn't support.
2. Implement custom filters as replacements (`{{ discnumber | pad(2) }}`,
   `{{ added_at | truncate(10) }}`).
3. Note that this is a **breaking change** for users with custom templates.

### 3. The VFS Ghost File System Spans 6 Syscall Handlers

M-11.5 describes ghost files as part of "write operations," but the ghost system is
actually a `VirtualFS`-level concern that intercepts `getattr`, `lookup`, `open`,
`opendir`, `mkdir`, and `rmdir` (`virtualfs.py:1678-1700, 1842-1855, 1950-1959`). It is
not localized to write operations.

The collage addition protocol specifically:
1. `mkdir` inside a collage dir -> create ghost directory, add to
   `in_progress_collage_additions` TTLCache (5s)
2. `opendir` on the ghost dir -> return success (pretend it's real)
3. `getattr`/`lookup` on files inside ghost dir -> return fake attrs
4. `open` on files inside ghost dir -> route to `/dev/null`
5. `open` on `.rose.{uuid}.toml` inside ghost dir -> trigger
   `add_release_to_collage()`, the actual mutation
6. After 5 seconds, ghost dir expires from TTLCache

This protocol depends on the file manager (Nautilus, Finder, `cp -r`) sending these
syscalls in order. It's a fragile contract with external software.

**Fix:** The ghost file system needs its own milestone (M-11.6 or fold it into M-11.5
with explicit sub-steps) with the full 6-step protocol documented. Testing must cover:
- Ghost dir creation and expiry
- File operations inside ghost dirs
- The `.rose.{uuid}.toml` trigger path
- Behavior when the ghost expires mid-operation

---

## Significant Risks

### 4. `_update_cache_for_releases_executor` FTS Query Is a 7-Table, 21-Column JOIN

M-7.4 says "Port: FTS index sync at end of cache update." The actual FTS population query
(`cache.py:1245-1307`) is a 63-line SQL statement that:

- JOINs `tracks`, `releases`, `releases_genres`, `releases_secondary_genres`,
  `releases_descriptors`, `releases_labels`, `releases_artists`, `tracks_artists`
- Uses `GROUP_CONCAT` with ` ¬ ` delimiter on 6 multi-value columns
- Registers a custom SQLite function `process_string_for_fts` via Python's
  `conn.create_function()` (`cache.py:1244`)
- Groups by `t.id` to collapse the cartesian product

In Rust, `rusqlite::Connection::create_scalar_function()` is the equivalent, but:
- The function must be registered per-connection.
- The FTS5 tokenizer config (`unicode61 remove_diacritics 0 categories '...'
  separators '¬'`) is SQLite-compile-time sensitive. `rusqlite` with `bundled` compiles
  its own SQLite, which should include FTS5, but this needs verification.

**Fix:** M-7.4 should explicitly mention: (a) registering a custom SQLite function via
`rusqlite`, (b) the 7-table JOIN complexity, (c) verifying FTS5 availability in the
bundled SQLite.

### 5. `edit_release` Is Significantly More Complex Than M-9.1 Suggests

The `edit_release` function (`releases.py:363-516`) has:

- **TOML null encoding hacks:** `-1` for null rating, `""` for null dates/edition/catalog
  (`releases.py:318-327`). The Rust serializer must reproduce this exactly or the editor
  roundtrip will break.
- **Per-track, per-field dirty checking:** 15+ fields compared individually per track
  (`releases.py:410-486`). Only dirty tracks get their tags flushed to disk.
- **Resume-on-failure:** If any exception occurs during the edit apply, the edited TOML is
  saved to `failed-release-edit.{uuid}.toml` and the user is told to `--resume`
  (`releases.py:494-513`). On resume, the file is validated by regex and loaded.
- **Dynamic role dispatch:** `getattr(m, a.role.lower())` (`releases.py:255`) to map
  artist role strings to `ArtistMapping` fields. An unknown role raises
  `UnknownArtistRoleError`.

This is not a simple "open editor, apply changes" flow. It's a stateful protocol with
error recovery.

**Fix:** M-9.1 should call out: (a) TOML null encoding must match Python exactly (this
is user-facing), (b) per-track dirty checking, (c) the resume-on-failure mechanism,
(d) the `MetadataRelease`/`MetadataTrack` intermediary types.

### 6. SQLite Connection Semantics Differ Between Python and Rust

The Python `connect()` function (`cache.py:84-98`) sets:
- `isolation_level=None` (autocommit — **no implicit transactions**)
- `timeout=15.0` (15-second busy timeout)
- `PRAGMA foreign_keys=ON` and `PRAGMA journal_mode=WAL` per connection
- `row_factory = sqlite3.Row` for dict-style row access
- No connection pooling (new connection per operation)

`rusqlite` defaults differ:
- By default, `rusqlite` opens in autocommit mode (matching Python).
- WAL mode and foreign keys are **not** set by default — must be set explicitly.
- `rusqlite` has no built-in connection pooling (use `r2d2-sqlite` if needed).
- Row access is positional by default; named access requires different patterns.

If the Rust port forgets `PRAGMA journal_mode=WAL`, concurrent reads during cache updates
will block. If it forgets `PRAGMA foreign_keys=ON`, referential integrity is silently
disabled.

**Fix:** M-6.1 should list the exact PRAGMAs required and note that they must be set on
every connection open.

### 7. Multiprocessing vs. Multithreading Changes the Isolation Model

Python uses `multiprocessing.Pool` — each worker is a **separate process** with its own
SQLite connection. This works well because SQLite handles inter-process concurrency via
file locks.

Rust with `rayon` uses **threads**, not processes. All threads share the same address
space. This changes the concurrency model:
- Multiple threads can share a single `rusqlite::Connection` wrapped in a `Mutex`, but
  SQLite connections are not thread-safe by default.
- Or each thread opens its own connection, but then `WAL` mode and `busy_timeout` become
  critical to avoid `SQLITE_BUSY` errors.
- The `Manager().list()` pattern for cross-process communication becomes unnecessary
  (threads can share data directly), but the batch-write-at-end pattern still needs
  coordination.

**Fix:** M-7.2 should specify: each rayon thread opens its own `rusqlite::Connection`.
The main thread coordinates the final collage/playlist updates. SQLite WAL mode and busy
timeout are required for concurrent access.

### 8. NFC Normalization Cross-Cutting Decision Contradicts Python Behavior

The cross-cutting decision says "Always normalize to NFC before comparison and storage."
But the Python code **does not do this** — `_compare_strs()` (`cache.py:2535-2545`) has
NFC normalization commented out.

If the Rust version enables NFC normalization, it will behave differently from the Python
version for non-ASCII paths. This could cause:
- Different cache hit/miss behavior for accented characters
- Source directory renaming differences
- VFS path lookup failures for libraries with non-ASCII names

This is especially risky because macOS uses NFD and Linux typically uses NFC. The Python
version "works" on both because it doesn't normalize (it compares whatever bytes the OS
gives it). Adding NFC normalization in Rust might actually fix macOS issues but change
Linux behavior for edge cases.

**Fix:** Change the cross-cutting decision to: "Do NOT normalize to NFC by default (match
Python behavior). Add NFC normalization as a future enhancement after migration is
complete, gated behind a config flag."

---

## Minor Issues

### 9. `update_release_on_fh_close` Mechanism Is Missing from VFS Milestones

`virtualfs.py:896` defines `update_release_on_fh_close: dict[int, str]` — a mapping from
writable file handles to release IDs. When a file opened for writing is `release()`d
(closed), the cache is updated for that release (`virtualfs.py:1561-1564`). This is how
the VFS triggers cache refreshes after external tools modify audio files in-place.

No milestone mentions this.

### 10. `FileHandleManager.next()` Has an Operator Precedence Bug

`virtualfs.py:843`: `self._state + 1 % 10_000` evaluates as `self._state + (1 % 10_000)`
= `self._state + 1`. The file handle counter never wraps. This is a bug in the Python
version. The Rust port should fix it: `(self._state + 1) % 10_000`.

### 11. `VirtualFS` Has 12 No-Op FUSE Stubs

`virtualfs.py:2011-2068` implements `forget`, `mknod`, `flush`, `setattr`, `getxattr`,
`setxattr`, `listxattr`, `removexattr`, `statfs`, `ftruncate` as no-ops or minimal stubs.
The `fuser` trait requires implementing these (or accepting defaults). The milestones
don't mention them.

### 12. `dump_all_artists` Has an N+1 Query Problem

`dump.py:154` calls `find_releases_matching_rule()` per artist in a loop (with a TODO
acknowledging the problem). The Rust port should fix this or at minimum reproduce it and
note it as tech debt.

### 13. Config Unknown-Key Detection

`config.py:556-570` traverses the parsed TOML dict after consuming all known keys and
warns about any remaining keys. This helps users catch typos. The milestone doesn't
mention this UX feature.

### 14. Nix Build Integration for Rust

The entire CI pipeline is Nix-based (`.github/workflows/build.yaml` uses
`nix develop --command make ...`). M-15.3 says "Update `flake.nix` to build Rust binary +
PyO3 shim" but doesn't acknowledge that Rust-in-Nix is non-trivial. Common approaches
(`crane`, `naersk`, `rustPlatform.buildRustPackage`) each have tradeoffs. The PyO3 crate
especially needs careful Nix integration (Python + Rust cross-compilation).

### 15. No Mention of `--loose-track` Flag in `create_single_release`

`releases.py:585` accepts a `loose_track` parameter that sets `releasetype = "loosetrack"`
instead of `"single"`. M-9.1 mentions `create_single_release` but the milestone's CLI
inventory (`001-PLAN.md:332`) does include `--loose-track`. Just verify this isn't lost in
the Rust CLI definition.

---

## Summary of Required Changes

| # | Severity | Action |
|---|----------|--------|
| 1 | Critical | Add VFS thread-safety strategy as cross-cutting decision |
| 2 | Critical | Document `minijinja` incompatibilities, plan custom filters, note breaking change for user templates |
| 3 | Critical | Decompose ghost file system into explicit protocol steps in M-11.5 |
| 4 | High | Expand M-7.4 to describe FTS query complexity and custom function registration |
| 5 | High | Expand M-9.1 to cover TOML null hacks, dirty checking, resume-on-failure |
| 6 | High | List required SQLite PRAGMAs in M-6.1 |
| 7 | High | Specify per-thread SQLite connections for rayon in M-7.2 |
| 8 | High | Reverse NFC normalization decision to match Python behavior |
| 9 | Medium | Add `update_release_on_fh_close` to VFS milestones |
| 10 | Low | Fix `FileHandleManager` wrapping bug in Rust port |
| 11 | Low | Note FUSE no-op stubs needed for `fuser` trait |
| 12 | Low | Note `dump_all_artists` N+1 query for future optimization |
| 13 | Low | Add config unknown-key detection to M-5.1 |
| 14 | Low | Note Nix-Rust integration complexity in M-15.3 |
| 15 | Low | Verify `--loose-track` flag is not lost |
