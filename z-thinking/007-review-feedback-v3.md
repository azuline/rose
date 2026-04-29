# 007 — Review Feedback on 006-MILESTONES-v3

Third-round review. All 15 items from `005-review-feedback-v2.md` are correctly addressed.
This review focuses on **bugs in the Python codebase** (which the Rust port must decide
to fix or reproduce), **behaviors the milestones still don't mention**, and **structural
risks** at the integration seams.

---

## Verdict

v3 is close to ready for execution. The cross-cutting decisions are comprehensive, the VFS
decomposition is now adequate, and the cache pipeline is properly sub-divided. The
remaining issues fall into three categories:

1. **Actual bugs in Python** that the Rust port must consciously decide on (fix or repro).
2. **Behaviors milestones underspecify** — not missing, just under-detailed.
3. **One structural concern** about the rules engine scope.

None of these are plan-killers. Most can be fixed by adding a few sentences to existing
milestones.

---

## Bugs in the Python Codebase

The Rust port inherits these. For each, decide: fix in Rust (preferred) or reproduce
faithfully.

### 1. Rules `--ignore` Logic for `new`/`favorite` Is Broken

`rules.py:299-306` — In the ignore-matcher section, when checking `new` and `favorite`
fields, the result is assigned to `match` instead of `skip`:

```python
# Line 302: should be `skip = matches_pattern(...)`, not `match = ...`
if not skip and field == "new":
    ...
    match = matches_pattern(i.pattern, datafile.new)
# Line 306: same bug
if not skip and field == "favorite":
    ...
    match = matches_pattern(i.pattern, datafile.favorite)
```

Compare with line 310 which correctly does `skip = matches_pattern(...)` for `rating`.
This means `--ignore new:true` and `--ignore favorite:true` silently fail — they overwrite
the outer `match` variable instead of setting `skip`, so the track is incorrectly
included/excluded based on the wrong variable.

**Recommendation:** Fix in Rust. This is a clear bug.

### 2. Collage Editor Has a Duplicate-Description Collision

`collages.py:149` — The editor builds a reverse mapping `{description_meta: uuid}`. If two
releases in a collage have identical `description_meta` strings (e.g., two different
editions of the same album with the same date), the dict silently drops one UUID. The user
could then inadvertently remove one release by editing the other.

The playlist editor at `playlists.py:162-169` handles this correctly by appending
`[{uuid}]` discriminators when duplicates are detected.

**Recommendation:** Fix in Rust by applying the same `[uuid]` discriminator pattern from
playlists to collages.

---

## Behaviors the Milestones Under-Specify

### 3. Cache Updates Mutate Source Collage/Playlist Files

`update_cache_for_collages` (`cache.py:1447-1493`) and `update_cache_for_playlists`
(`cache.py:1660-1713`) **write back to source TOML files** during cache updates. This is
not a read-only caching operation. Specifically:

- Marks releases/tracks as `missing = true` when they disappear from the library.
- Removes `missing` when they reappear.
- Appends ` {MISSING}` to `description_meta` strings for missing entries.
- Regenerates `description_meta` from current DB state (so metadata changes propagate).
- Rewrites the entire TOML file if any of the above changed.

M-7.1e says "Port: `update_cache_for_collages`, `update_cache_for_playlists`" but doesn't
mention that these functions **modify source files on disk**. An agent porting this
milestone might assume it's read-only caching and miss the write-back logic entirely.

**Fix:** Add a note to M-7.1e: "These functions write back to collage/playlist TOML files
on disk (missing flags, description_meta updates). This is a read-write operation, not
read-only."

### 4. Rules Engine Modifies `.rose.{uuid}.toml` DataFiles, Not Just Audio Tags

`rules.py:399-440` — The rules engine can modify `new`, `favorite`, and `rating` fields,
which live in the `.rose.{uuid}.toml` sidecar file, not in audio tags. The code:

- Reads the datafile from disk per-directory
- Deduplicates writes (multiple tracks in one release share one datafile, line 557-561)
- Writes the datafile back using `StoredDataFile.serialize()`

M-8.1 says "match tracks via FTS, apply actions" implying only audio tag mutations. An
agent would miss the datafile write path.

**Fix:** Add to M-8.1: "Actions on `new`, `favorite`, `rating` fields modify the
`.rose.{uuid}.toml` datafile, not audio tags. Deduplicate datafile writes per-release
directory."

### 5. Parent Genre Encoding in Audio Tags

`audiotags.py:489-494` — When `config.write_parent_genres` is enabled, genre tags are
written with a special encoding:

```
Rock;Pop\\PARENTS:\\Alternative;Indie
```

The `\\PARENTS:\\` delimiter separates user-specified genres from auto-populated transitive
parent genres. The read path must parse this and strip the parent section.

M-2.1 and M-2.2 don't mention this encoding at all. If the Rust port doesn't handle it,
genre tags will be corrupted on write or misread.

**Fix:** Add to M-2.1: "Parse `\\PARENTS:\\` delimiter in genre/secondary_genre tags." Add
to M-2.2: "When `write_parent_genres` is enabled, append transitive parent genres after
`\\PARENTS:\\` delimiter."

### 6. Rules Engine Has a 5-Phase Pipeline with Optimization Thresholds

`rules.py:75-134` — The rule execution flow is:

1. FTS fast search (may produce false positives)
2. If >400 results: pre-filter via read cache before reading from disk (line 105)
3. Filter false positives by reading actual audio tags from disk
4. Apply actions in-memory, compute per-track diffs
5. User confirmation — two modes: simple y/n for <=25 tracks, or enter-a-number for >25

M-8.1 says "match tracks via FTS, apply actions." This flattens a 5-phase pipeline into
one sentence. The 400-track cache pre-filter optimization and the dual confirmation UI are
both missing.

**Fix:** Expand M-8.1 to list the 5 phases. The 400-track optimization is important for
large libraries.

### 7. Collage/Playlist Rename Also Renames Adjacent Files

`collages.py:71-77` — `rename_collage` renames not just the TOML file but also any
adjacent files with the same stem (typically cover art):

```python
for f in source_path.parent.iterdir():
    if f.stem == source_path.stem and f != source_path:
        f.rename(f.parent / f"{new_name}{f.suffix}")
```

Same pattern in `playlists.py:75-82`. M-10.1 and M-10.2 don't mention this.

**Fix:** Add to M-10.1 and M-10.2: "Rename also renames adjacent files with matching stem
(cover art)."

### 8. Regex Flavor Compatibility for Sed Actions

`rule_parser.py:670` compiles sed patterns via Python's `re.compile()`. The `SedAction`
stores a compiled regex. `rules.py:714` uses `bhv.src.sub(bhv.dst, strvalue)`.

Python's regex flavor includes features like `\b`, `(?P<name>...)`, lookahead/lookbehind,
and backreferences in replacement strings (`\1`, `\g<name>`). The Rust `regex` crate
supports most of these, but:

- Rust `regex` does **not** support lookahead/lookbehind (use `fancy-regex` if needed).
- Replacement syntax differs: Rust uses `$1` and `${name}` vs Python's `\1` and
  `\g<name>`.

If users have config rules with Python-specific regex syntax, they will break.

**Fix:** Note in M-3.1: "Sed action uses Python `re` regex flavor. Rust `regex` crate is
mostly compatible but replacement syntax differs (`$1` vs `\1`). Document the change. If
lookaround is needed, use `fancy-regex`."

### 9. VFS Sanitizer Has a Circular Dependency

`virtualfs.py:725-753` — The `Sanitizer` class holds a reference to `RoseLogicalCore` and
calls its `readdir` method as a fallback when the name->path cache misses. But
`RoseLogicalCore` also uses the `Sanitizer`. This creates a re-entrant call pattern:

```
lookup(path) -> Sanitizer.unsanitize(name) -> cache miss
  -> RoseLogicalCore.readdir(parent) -> Sanitizer.sanitize(entries)
```

In Rust, this circular reference requires either `Rc<RefCell<>>` (single-threaded) or
`Arc<RwLock<>>` (multi-threaded). With the VFS thread-safety decision already requiring
`Arc<RwLock<>>` for shared state, this isn't a new constraint, but the re-entrancy means
a potential **deadlock** if the readdir fallback tries to acquire a lock the caller already
holds.

**Fix:** Note in M-11.2: "Sanitizer has a readdir fallback that creates a circular call
into RoseLogicalCore. Use care with lock ordering to avoid deadlocks. Consider making the
sanitizer cache non-locking (e.g., `DashMap`) to eliminate the risk."

### 10. Daemonization Is Unix-Only and PID Management Is Fragile

`cli.py:756-785` — The `daemonize` function uses `os.fork()` (Unix-only). The watchdog
uses a PID file at `{cache_dir}/watchdog.pid`. Issues:

- Stale PID files: if the process dies without cleanup, the PID file persists. The code
  checks `os.kill(pid, 0)` but **doesn't delete the stale file** if the process is dead
  (line 764 is just `pass`).
- No signal handler: `SIGTERM` kills the watchdog immediately without cleanup.
- VFS mount calls `daemonize()` without a PID file, so there's no way to track it.

In Rust, use `fork()` from `nix` crate, or consider a simpler approach (systemd socket
activation, or just `--foreground` only). The stale PID issue should be fixed.

**Fix:** Note in M-13.1 and M-12.10: "Daemonization requires `nix::unistd::fork()` or
equivalent. Fix stale PID file handling. Install a SIGTERM handler for graceful shutdown."

---

## Minor Items

### 11. VFS 2-Hour Path TTL Is a Compatibility Feature

`virtualfs.py:416-421` — The `VirtualNameGenerator` caches release/track virtual paths
for 2 hours. This means after a metadata change, the **old** virtual path remains valid
alongside the new one for 2 hours. This is intentional: media players that have cached
the old path can still read files. M-11.2 mentions the TTL cache but doesn't explain the
**why** — an agent might change the TTL thinking it's an arbitrary choice.

### 12. `_unpack` Silently Drops Mismatched Entries

`cache.py:2511-2526` — `_unpack` uses `zip(..., strict=False)` to pair artist names with
roles from GROUP_CONCAT results. If the counts mismatch (data corruption), entries are
silently dropped. The Rust port should decide: panic (catch corruption early) or match
Python's silent behavior.

### 13. Config Parse Runs on Every CLI Invocation

`cli.py:109-111` — `Config.parse()` and `maybe_invalidate_cache_database()` run even for
`rose version`. In the Rust port, consider lazy-loading config only for commands that need
it.

### 14. Dead Code: `parse_collage_argument` / `parse_playlist_argument`

`cli.py:730-753` — Defined but never called. Don't port these.

### 15. MP4 Literal `"None"` String Handling

`audiotags.py:419-420` — When reading MP4 track/disc numbers, the code handles the
literal string `"None"` (from how mutagen serializes missing values). This is a
mutagen-specific quirk that `lofty` won't have. Don't reproduce this; just handle `None`
normally in Rust.

---

## Summary of Required Changes

| # | Severity | Action |
|---|----------|--------|
| 1 | Medium | Decide: fix `rules.py` ignore bug for `new`/`favorite` in Rust port |
| 2 | Low | Decide: fix collage editor duplicate-description collision in Rust port |
| 3 | High | Add note to M-7.1e: collage/playlist cache updates write to source TOML files |
| 4 | High | Add note to M-8.1: rules engine modifies datafiles, not just audio tags |
| 5 | High | Add parent genre `\\PARENTS:\\` encoding to M-2.1 and M-2.2 |
| 6 | Medium | Expand M-8.1 with 5-phase pipeline and 400-track threshold |
| 7 | Low | Add adjacent file rename note to M-10.1 and M-10.2 |
| 8 | Medium | Note regex flavor difference in M-3.1 (replacement syntax `$1` vs `\1`) |
| 9 | Medium | Note Sanitizer circular dependency and deadlock risk in M-11.2 |
| 10 | Low | Note daemonization constraints and PID fix in M-13.1 and M-12.10 |
| 11 | Low | Explain 2-hour path TTL rationale in M-11.2 |
| 12 | Low | Decide: silent drop vs panic for mismatched GROUP_CONCAT in M-6.2 |
| 13 | Low | Consider lazy config loading in M-12.1 |
| 14 | Low | Don't port dead code (`parse_collage_argument`, `parse_playlist_argument`) |
| 15 | Low | Don't reproduce MP4 `"None"` string handling; use native `lofty` behavior |

---

## Overall Assessment

After three rounds, the milestones document is solid. The critical architectural decisions
(thread safety, SQLite semantics, path normalization, error recovery) are all covered. The
remaining issues are **detail-level**: missing notes about write-back behaviors, an
undocumented tag encoding, and Python bugs to decide on. None of these require structural
changes to the plan.

**The plan is ready to execute.** Incorporate the high-severity items (3, 4, 5) into the
milestone text and the rest can be handled as the implementing agent encounters them.
