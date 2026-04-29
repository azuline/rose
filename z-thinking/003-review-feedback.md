# 003 — Review Feedback on Plan & Milestones

Critical review of `001-PLAN.md` and `002-MILESTONES.md`. The goal is to identify gaps,
incorrect assumptions, and underestimated risks that will cause the migration to fail or
stall if left unaddressed.

---

## Verdict

The plan is structurally sound. The bottom-up dependency-order strategy is correct, the
crate layout is reasonable, and the quality gates are thorough. The milestones are
well-decomposed. However, there are **several concrete omissions and underestimates** that
will cause real trouble during execution.

---

## Critical Flaws (Will Cause Failures)

### 1. `filter_releases` / `filter_tracks` Are Invisible in the Plan

The plan and milestones never mention `filter_releases` or `filter_tracks`
(`cache.py:1770-1982`). These are ~210 lines of **dynamic SQL query building** that
combine artist alias resolution, genre hierarchy expansion, and 8 optional filter
dimensions. They are the primary query interface used by the VFS (`virtualfs.py` calls
them for every `readdir`). Without these, the VFS cannot render filtered views (by artist,
genre, label, descriptor, new, favorite, etc.).

These functions are not simple "read queries" — they construct SQL dynamically with
variable-length `IN (?,?,?)` clauses, join against the genre hierarchy, and resolve artist
aliases transitively. They are the hardest read-path code in the cache module.

**The milestone M-6.3 ("read queries") lists `list_releases`, `list_tracks`, etc. but does
not mention `filter_releases` or `filter_tracks`.** This omission means Phase 11 (VFS)
will hit a wall when it tries to implement filtered directory listings.

**Fix:** Add explicit milestones for `filter_releases` and `filter_tracks` in Phase 6, or
add a dedicated M-6.4.

### 2. The Watcher's Async Debounce Architecture Is Undocumented

The plan describes the watcher as a simple "file watcher via watchdog" (~260 lines, Port:
"watch source dir for changes, trigger update"). The actual watcher (`watcher.py`) is a
**thread + async event loop** architecture:

- A **sync watchdog thread** enqueues filesystem events into a `Queue`.
- An **async event loop** dequeues events, debounces them (coalescing events on the same
  release directory within a time window), and dispatches `update_cache_for_releases`
  calls.
- Debouncing is critical: without it, copying a 20-file album into the source directory
  fires 20+ events, each of which would trigger a full cache update for that release.

The plan says "watcher with `notify`" as if it's a trivial wrapper. The Rust equivalent
needs either `tokio` or a manual debounce loop, plus cross-thread communication (e.g.,
`crossbeam-channel` or `tokio::sync::mpsc`). This is not hard, but it's not a 50-line
wrapper either.

**The milestone M-13.1 doesn't mention debouncing at all.** An agent picking this up will
write a naive watcher that hammers `update_cache` on every individual file event.

**Fix:** M-13.1 must specify: event debouncing with configurable delay, thread-to-async
event channel, and coalescence by release/collage/playlist directory.

### 3. `StoredDataFile` — The `.rose.{uuid}.toml` Format Is Not Mentioned

The `.rose.{uuid}.toml` file is the **on-disk metadata sidecar** for every release
(`cache.py:327-358`). It stores `new`, `favorite`, `rating`, and `added_at`. The plan's
hard constraint #1 says "on-disk audio tag structure must stay the same," but says nothing
about the datafile format.

This format has quirks that must be preserved exactly:
- Uses `-1` to represent `null` rating (TOML lacks null).
- Has migration-like behavior: when reading old datafiles, missing fields get defaults
  (`added_at` defaults to "now" if absent).
- The UUID is extracted from the filename via regex, not from the file content.
- First-time scans must **create** the datafile with a new UUID.

If the Rust version writes these files differently (different key names, different null
encoding, different datetime format), it will corrupt the existing library state for
`new`/`favorite`/`rating` on every release.

**Fix:** Add this as a hard constraint alongside audio tags. Add a golden file test for
`StoredDataFile` serialization roundtrip.

### 4. `dump.py` (288 Lines) Is Missing from the Migration Surface

`dump.py` in `rose-cli/` handles JSON serialization for **all** entity types (releases,
tracks, artists, genres, labels, descriptors, collages, playlists). Every `rose * print`
and `rose * print-all` CLI command delegates to it. The plan's Phase 7 mentions `dump.py`
in passing (line 308) but the milestones don't have a dedicated item for it.

This matters because `dump.py` defines the **JSON output schema** — the external contract
for any tooling that parses `rose` output. Changing it silently breaks scripts.

The existing tests use **`syrupy` snapshot testing** (`dump_test.py`) to lock down the
JSON format. The plan's test strategy discusses golden files but never mentions that this
exact pattern already exists as snapshots.

**Fix:** Add a milestone for porting `dump.py` serialization and preserving the JSON
output format. Use the existing syrupy snapshots as golden file references.

---

## Significant Risks (Will Cause Delays)

### 5. `lofty` vs `mutagen` Tag Handling Divergence

The plan acknowledges this risk (Section 8) but understates it. Specific known issues:

- **Multi-value tags:** `mutagen` treats Vorbis comments as inherently multi-value (each
  key can appear multiple times). `lofty` may normalize these differently.
- **ID3v2 custom frames:** Rose uses `TXXX:roseid` and `TXXX:rosereleaseid`. The exact
  frame encoding (UTF-8 vs Latin-1 description field, encoding byte) must match.
- **MP4 freeform atoms:** Rose uses `----:com.apple.iTunes:roseid`. `lofty` may use a
  different atom path or encoding.
- **Opus vs Vorbis:** Both use Vorbis comments but have different container headers.
  `lofty` may expose these as different tag types.

The golden file test strategy is correct, but the plan should budget **at least 2-3 days
of debugging** specifically for tag format discrepancies. This is the single most
data-corruption-prone area.

**Fix:** Before writing any Rust code, write a Python script that dumps every tag from
every format as raw bytes. This becomes the ground truth for the Rust tests. Don't rely on
high-level field comparisons — compare at the byte level for custom tags.

### 6. The Cache Update Executor Is 860 Lines of Stateful Logic

The plan breaks cache into 5 sub-phases (4a-4e), but the **actual bulk** is in
`_update_cache_for_releases_executor` (lines 490-1349), a single 860-line function that:

- Scans directories and identifies release UUIDs from filenames
- Batch-queries existing cached data
- Compares mtimes to skip unchanged files
- Reads audio tags for changed files
- Assigns first-time UUIDs (creates `.rose.{uuid}.toml`)
- Detects "in-progress" directories (anti-race for copies in progress)
- Derives release-level metadata from the first audio file
- Renames source directories if metadata changed
- Accumulates all mutations into batch lists
- Executes batched SQL inserts/updates/deletes
- Syncs FTS index

The plan's sub-phases split by schema/read/write/parallel/locking, but the write path
(M-7.1) encompasses all of the above in one milestone. This is too coarse. An agent facing
860 lines of interleaved scanning/reading/comparing/writing logic in a single function
will either port it monolithically (risky) or need finer decomposition.

**Fix:** Split M-7.1 into at least:
- M-7.1a: Directory scanning and UUID discovery
- M-7.1b: Mtime comparison and change detection
- M-7.1c: Tag reading and release metadata derivation
- M-7.1d: Source directory renaming
- M-7.1e: Batch SQL writes + FTS sync

### 7. VFS Line Estimate Is Too Tight

The plan estimates ~2,000 Rust lines for the VFS. The Python VFS is 2,090 lines with 9
classes. Rust is typically 1.3-1.8x more verbose than Python for equivalent logic
(explicit error handling, lifetime annotations, trait implementations, pattern matching).
The VFS has:

- `VirtualPath` parser: ~230 lines of path parsing with 12+ view types
- `VirtualNameGenerator`: ~315 lines of bidirectional TTL caching
- `RoseLogicalCore`: ~706 lines of domain logic including a state machine for file
  creation
- `VirtualFS`: ~450 lines of FUSE operations with multiple TTL caches
- `INodeMapper`: ~70 lines of bidirectional inode mapping
- `TTLCache`: ~40 lines of generic TTL dict
- `CanShower`: ~60 lines of whitelist/blacklist logic
- `FileHandleManager`: ~36 lines

A realistic Rust estimate is **2,800-3,500 lines**, not 2,000. The `fuser` crate's trait
has more methods to implement than `llfuse`, and Rust error handling inflates every
function.

### 8. No Cross-Platform Path Handling Strategy

The codebase has a commented-out Unicode NFC normalization in `_compare_strs`
(`cache.py:2535`), signaling known path comparison issues. macOS normalizes filenames to
NFD; Linux does not. The `sanitize_dirname`/`sanitize_filename` functions strip diacritics
and apply NFC, but the **comparison** of existing paths against cached paths is fragile.

The plan mentions `unicode-normalization` as a dependency but never discusses the strategy
for path comparison. If the Rust version compares paths byte-for-byte while the Python
version had implicit normalization via `str` comparison, existing libraries with non-ASCII
names will break.

**Fix:** Document the path normalization strategy explicitly. Decide: always normalize to
NFC before comparison, or compare raw bytes and accept platform-dependent behavior.

### 9. No Error Recovery Strategy for Partial Cache Updates

The Python version uses `multiprocessing.Pool` with `error_callback` and
`ExceptionGroup`. If one batch fails, others can still complete, and the user sees all
errors. The plan says "parallel cache update with `rayon`" but doesn't discuss:

- What happens if one release directory is corrupted / unreadable?
- Does the entire update abort, or does it skip and continue?
- How are errors from `rayon` parallel iterators collected and reported?

`rayon`'s default behavior with `par_iter().for_each()` is to **panic on the first
error** (unless you use `try_for_each` or collect results). The Python behavior is
**continue and aggregate errors**. If the Rust version panics on one bad file, it takes
down the entire cache update.

**Fix:** Specify that `rayon` tasks must return `Result`, errors must be collected (not
propagated as panics), and the update must complete for all valid releases before reporting
failures.

---

## Minor Issues

### 10. Line Count Inaccuracies

Several line counts in the plan are wrong:
- `tracks.py`: Plan says ~200, actual is **67**
- `playlists.py`: Plan says ~400, actual is **236**
- `collages.py`: Plan says ~200, actual is **169**
- `rose-watch/`: Plan says ~260, actual is **~196**
- `__init__.py`: Plan says "140 symbols", `__all__` has **123**

These don't affect the strategy, but they inflate complexity estimates for some modules and
could mislead an agent about how much work each phase requires.

### 11. Genre Hierarchy Generator Already Outputs JSON for Rust

`scripts/rym-genres/generate.py:159` already writes `rose-rs/src/genre_hierarchy.json`.
The plan discusses code-generating or `include!`-ing the genre hierarchy in Phase 1 but
doesn't note this existing JSON output. The Rust module can simply
`serde_json::from_str(include_str!("genre_hierarchy.json"))` at compile time, making
M-1.3 trivial.

### 12. No Mention of `added_at` / "Added On" / "Released On" VFS Views

The milestones list 8 top-level virtual directories for the VFS (M-11.2), but the actual
VFS has **at least 12 view types** including "Added On" and "Released On" date-based
views. These require date-bucketing logic that isn't mentioned anywhere in the plan.

### 13. `edit_release` / `edit_collage` / `edit_playlist` Open an Editor

These functions open `$EDITOR` with a temporary file, wait for the user to edit, then
parse changes. The plan lists them as straightforward CRUD, but the editor integration
(temp file creation, parsing the edited text, applying diffs) is non-trivial and has
failure modes (editor exits non-zero, file unchanged, parse error in edited text). M-9.1
and M-10.1 should call this out.

### 14. No Logging Strategy

The Python code uses `logging` extensively with structured messages. The plan doesn't
mention a Rust logging framework (`tracing`, `log`, `env_logger`). Logging is critical for
debugging VFS and watcher issues. Pick a framework in Phase 1 and use it from the start.

### 15. No Benchmarking Plan

The entire motivation for the migration is performance. The plan has no benchmarks to
validate the hypothesis. Before deleting Python code (Phase 9/15), there should be a
measured comparison:
- VFS `readdir` latency (the stated pain point)
- Cache update time for N releases
- Cold start time

Without benchmarks, the migration might complete and still be disappointing if the
bottleneck was I/O-bound (SQLite, disk reads) rather than CPU-bound.

---

## Summary of Required Changes

| # | Severity | Action |
|---|----------|--------|
| 1 | Critical | Add `filter_releases`/`filter_tracks` to milestones |
| 2 | Critical | Document watcher debounce architecture in M-13.1 |
| 3 | Critical | Add `.rose.{uuid}.toml` format as hard constraint |
| 4 | Critical | Add `dump.py` milestone for JSON output schema |
| 5 | High | Budget time for `lofty`/`mutagen` byte-level tag comparison |
| 6 | High | Split M-7.1 into finer sub-milestones |
| 7 | Medium | Revise VFS LoC estimate upward (~3,000) |
| 8 | Medium | Document path normalization strategy (NFC) |
| 9 | Medium | Specify `rayon` error collection strategy |
| 10 | Low | Fix line count inaccuracies |
| 11 | Low | Note existing `genre_hierarchy.json` output |
| 12 | Low | Enumerate all VFS view types including date-based views |
| 13 | Low | Call out editor integration complexity |
| 14 | Low | Pick a logging crate in Phase 1 |
| 15 | Medium | Add pre/post benchmarking to Phase 15 |
