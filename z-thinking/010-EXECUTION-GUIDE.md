# 010 — Execution Guide: Agent Sessions and Handoffs

How to divide the migration across agents to minimize context overhead per session.

---

## The Problem

The milestones in `008-FINAL-PLAN.md` are good for tracking progress but not directly
delegatable one-by-one to stateless agents. An agent doing M-6.3 (cache read queries)
needs to understand the Rust types from M-6.2, the connection logic from M-6.1, the
`Config` type from M-5.1, and the `AudioTags`/`ArtistMapping` types from earlier phases.
If you hand an agent just the milestone text and the Python source, it will either
reinvent incompatible types or waste time reading the entire `rose-core` crate.

**The fix:** Group milestones into **agent sessions** (batches that share natural context),
define a **handoff protocol** (what each session produces for the next), and identify
**parallelism** (which sessions can run concurrently).

---

## Agent Session Definitions

### Session A: Foundations (M-1.1, M-1.2, M-1.3)

**Milestones:** M-1.1 + M-1.2 + M-1.3
**Python to read:** `common.py` (231 lines), `genre_hierarchy.py`
**Depends on:** Nothing — this is the starting point.
**Est. output:** ~500 lines Rust

This session creates the workspace and all foundational types. Every subsequent session
depends on its output.

**Handoff produces:** `z-thinking/handoff-A.md` — documents:
- `Artist` struct shape (fields, derives)
- `ArtistMapping` struct shape and methods (`all()`, `dump()`, `items()`)
- `sanitize_dirname` / `sanitize_filename` signatures
- `RoseError` variants
- `GENRE_HIERARCHY` / `TRANSITIVE_CHILD_GENRES` access patterns
- Any design decisions made (e.g., `HashMap` vs `BTreeMap` for genre maps)

---

### Session B: Audio Tags (M-2.0, M-2.1, M-2.2)

**Milestones:** M-2.0 + M-2.1 + M-2.2
**Python to read:** `audiotags.py` (629 lines)
**Depends on:** Session A (needs `Artist`, `ArtistMapping`, `RoseDate`, common utils)
**Est. output:** ~800 lines Rust + golden files

This is the highest-risk session (data corruption potential). It should be done by a
single agent in one pass because the read and write paths are deeply intertwined and the
artist parse/format protocol must be developed together.

**Handoff produces:** `z-thinking/handoff-B.md` — documents:
- `AudioTags` struct shape (all fields)
- `AudioTags::from_file(path)` signature and error types
- `AudioTags::flush(config)` signature
- `RoseDate` type and `parse`/`Display`
- `parse_artist_string` / `format_artist_string` signatures
- `SUPPORTED_AUDIO_EXTENSIONS`
- Any `lofty` quirks discovered

---

### Session C: Rule Parser (M-3.1)

**Milestones:** M-3.1
**Python to read:** `rule_parser.py` (846 lines), `rule_parser_test.py` (544 lines)
**Depends on:** Session A (only uses types from `common.rs`)
**Est. output:** ~600 lines Rust

**Can run in PARALLEL with Session B** — both depend only on Session A.

Pure parsing logic, zero I/O. Ideal for isolated agent work.

**Handoff produces:** `z-thinking/handoff-C.md` — documents:
- `Matcher`, `Pattern`, `Action`, `Rule` type shapes
- `parse_matcher()`, `parse_action()`, `parse_rule()` signatures
- All action subtypes (Replace, Sed, Split, Add, Delete)
- Tag field name constants

---

### Session D: Templates + Config (M-4.1, M-5.1)

**Milestones:** M-4.1 + M-5.1
**Python to read:** `templates.py` (588 lines), `config.py` (609 lines)
**Depends on:** Sessions A, B (needs `AudioTags` for `get_sample_music`), C (needs
`Rule` for stored rules in config)
**Est. output:** ~900 lines Rust

Templates and config are coupled: `Config` contains `PathTemplateConfig`, and templates
use `Config` for rendering. Do them together.

**Handoff produces:** `z-thinking/handoff-D.md` — documents:
- `Config` struct shape (all fields)
- `Config::load(path)` signature
- `VirtualFSConfig` shape
- `PathTemplateConfig` and template evaluation signatures
- Artist alias resolution API (`artist_aliases_map`, etc.)
- Custom `minijinja` filter names and behavior

---

### Session E: Cache Schema + Types + Reads (M-6.1, M-6.2, M-6.3, M-6.4)

**Milestones:** M-6.1 + M-6.2 + M-6.3 + M-6.4
**Python to read:** `cache.py` (first ~1,900 lines + lines 1770-1982), `cache.sql` (290)
**Depends on:** Sessions A, B, D (needs `Config`, `AudioTags`, common types)
**Est. output:** ~1,500 lines Rust

This is a large session but the milestones within it are tightly coupled — the types,
schema, and read queries are all one unit. Splitting them across agents would force the
second agent to re-learn all the types the first agent defined.

**Handoff produces:** `z-thinking/handoff-E.md` — documents:
- `Release`, `Track`, `Collage`, `Playlist` struct shapes
- `StoredDataFile` shape and serialize/parse signatures
- `connect(config)` signature
- All read function signatures: `get_release`, `list_releases`, `filter_releases`, etc.
- `GenreEntry`, `DescriptorEntry`, `LabelEntry` shapes
- `_get_all_artist_aliases` behavior

---

### Session F: Cache Update Pipeline (M-7.1a through M-7.4)

**Milestones:** M-7.1a + M-7.1b + M-7.1c + M-7.1d + M-7.1e + M-7.2 + M-7.3 + M-7.4
**Python to read:** `cache.py` (lines 490-1349 + FTS/locking sections)
**Depends on:** Session E (needs all cache types and read functions)
**Est. output:** ~1,200 lines Rust

The update pipeline sub-milestones are stages of a single function. An agent needs to
understand the full pipeline to design the data flow. Keep it as one session.

**Handoff produces:** `z-thinking/handoff-F.md` — documents:
- `update_cache(config)` signature and behavior
- `update_cache_for_releases/collages/playlists` signatures
- `update_cache_evict_nonexistent_*` signatures
- `lock(config, name, timeout)` API
- `process_string_for_fts` signature
- Rayon error handling pattern used
- Any SQL-level decisions (batch size, transaction boundaries)

---

### Session G: Rules + Business Logic (M-8.1, M-9.1, M-9.2, M-10.1, M-10.2)

**Milestones:** M-8.1 + M-9.1 + M-9.2 + M-10.1 + M-10.2
**Python to read:** `rules.py` (898), `releases.py` (641), `tracks.py` (67),
`collages.py` (169), `playlists.py` (236)
**Depends on:** Session F (needs cache update functions, read functions, types, locking)
**Est. output:** ~2,000 lines Rust

Rules, releases, tracks, collages, and playlists form a tight cluster — releases uses
rules, collages uses releases. An agent doing rules needs to know the cache read API; an
agent doing releases needs rules. Group them.

This is the largest session by milestone count but the individual modules are small (67-898
lines each) and follow similar patterns (read from cache, modify tags/files, update cache).

**Handoff produces:** `z-thinking/handoff-G.md` — documents:
- `execute_metadata_rule` signature and behavior
- `find_releases_matching_rule` / `find_tracks_matching_rule` signatures
- All CRUD function signatures for releases, tracks, collages, playlists
- `edit_release` / `edit_collage_in_editor` / `edit_playlist_in_editor` behavior notes

---

### Session H: VFS (M-11.1 through M-11.5)

**Milestones:** M-11.1 + M-11.2 + M-11.3 + M-11.4 + M-11.5
**Python to read:** `virtualfs.py` (2,089 lines), `docs/ARCHITECTURE.md` VFS section
**Depends on:** Session G (needs all CRUD functions, cache read/filter, `Config`)
**Est. output:** ~3,000 lines Rust

The VFS is a single tightly-coupled system. The path parser, logical core, FUSE ops, and
ghost protocol all reference each other's types and state. Cannot be split.

**Handoff produces:** `z-thinking/handoff-H.md` — documents:
- Mount/unmount API
- Shared state architecture (which types use `Arc<RwLock>` vs `DashMap`)
- Any `fuser`-specific decisions

---

### Session I: CLI + Watcher (M-12.1 through M-12.11, M-13.1)

**Milestones:** M-12.1 through M-12.11 + M-13.1
**Python to read:** `cli.py` (793), `dump.py` (288), `watcher.py` (192)
**Depends on:** Sessions G (CRUD), H (VFS mount)
**Est. output:** ~1,200 lines Rust

The CLI is a thin layer over `rose-core` — all business logic is already done. The agent
just needs to know what functions exist and wire them to `clap` commands. Group with
watcher since it's also a thin wrapper.

**Handoff produces:** `z-thinking/handoff-I.md` — documents:
- Binary entry points and CLI structure
- JSON output format decisions
- Watcher architecture decisions

---

### Session J: PyO3 Shim (M-14.1, M-14.2, M-14.3)

**Milestones:** M-14.1 + M-14.2 + M-14.3
**Python to read:** (none — just wrapping existing Rust API)
**Depends on:** Session G (needs all `rose-core` types and functions)
**Est. output:** ~500 lines Rust

Mechanical wrapping. Reads only `rose-core`'s public API.

---

### Session K: Finalization (M-15.1 through M-15.6)

**Milestones:** M-15.1 + M-15.2 + M-15.3 + M-15.4 + M-15.5 + M-15.6
**Depends on:** Everything.
**Est. output:** CI config, Nix, docs, cleanup.

---

## Dependency Graph

```
Session A (foundations)
  |          \
  |           \
Session B      Session C        <- B and C can run in PARALLEL
(audio tags)   (rule parser)
  |           /
  |          /
Session D (templates + config)
  |
Session E (cache reads)
  |
Session F (cache update)
  |
Session G (rules + CRUD)
  |         \
  |          \
Session H    Session J        <- H and J can run in PARALLEL
(VFS)        (PyO3 shim)       (J only needs G, not H)
  |          /
  |         /
Session I (CLI + watcher)     <- needs both H (VFS mount) and G (CRUD)
  |
Session K (finalization)
```

## Parallelism Opportunities

| Round | Sessions | Can Parallelize? |
|-------|----------|-----------------|
| 1 | A | No — must go first |
| 2 | B, C | **Yes — run simultaneously** |
| 3 | D | No — needs B and C |
| 4 | E | No — needs D |
| 5 | F | No — needs E |
| 6 | G | No — needs F |
| 7 | H, J | **Yes — run simultaneously** |
| 8 | I | No — needs H |
| 9 | K | No — last |

**Critical path:** A -> B -> D -> E -> F -> G -> H -> I -> K (9 sequential rounds)
**With parallelism:** Saves ~1 round (C parallel with B, J parallel with H).

The critical path runs through the cache (E, F), which is the largest module. There's no
way to parallelize around it — everything downstream depends on it.

---

## Handoff Protocol

After each session, the executing agent must produce a handoff file:

```
z-thinking/handoff-{LETTER}.md
```

**Required content:**
1. **Public API summary** — every `pub fn` and `pub struct` with its signature. Not the
   full code, just the interface. Example:
   ```rust
   // rose-core/src/audiotags.rs
   pub struct AudioTags { /* 20 fields - see source */ }
   pub fn AudioTags::from_file(p: &Path) -> Result<AudioTags, RoseError>
   pub fn AudioTags::flush(&self, c: &Config) -> Result<(), RoseError>
   pub fn parse_artist_string(main: Option<&str>, ...) -> ArtistMapping
   pub fn format_artist_string(m: &ArtistMapping) -> String
   ```

2. **Design decisions** — anything non-obvious that a later agent needs to know:
   - "Used `BTreeMap` for genre hierarchy for deterministic iteration"
   - "AudioTags stores paths as `PathBuf`, not `&Path`"
   - "`lofty` doesn't support TIPL, so we read IPLS only with a fallback"

3. **Known issues** — anything deferred or imperfect:
   - "Vorbis comment multi-value handling untested for edge case X"

**What the next agent reads:**
- `008-FINAL-PLAN.md` (sections 1-9 for context + the specific milestone text)
- All prior `handoff-*.md` files for sessions it depends on
- The Python source file(s) listed in the milestone
- The existing Rust code it needs to call (guided by the handoff API summary)

This means an agent doing Session F reads: `008-FINAL-PLAN.md`, `handoff-A.md` through
`handoff-E.md`, `cache.py`, and the existing Rust files in `rose-core/src/`. Total context
is bounded: ~5 handoff files (~1-2 pages each) + 1 Python file + existing Rust code.

---

## Agent Prompt Template

When delegating a session to an agent, use this prompt structure:

```
You are porting a Python music library manager (Rose) to Rust.

## Your session: Session {LETTER} — {NAME}
## Milestones: {list}

Read the following files for full context:
1. z-thinking/008-FINAL-PLAN.md (sections 1-9 for constraints and decisions)
2. {list of handoff files from dependencies}
3. {list of Python source files}
4. {list of existing Rust files to understand the API you're building on}

Your job:
- Implement the milestones listed above in Rust
- Run `cargo test` and `cargo clippy` after each milestone
- When done, produce z-thinking/handoff-{LETTER}.md documenting your public API,
  design decisions, and known issues
- Mark completed milestones as [x] in 008-FINAL-PLAN.md

Do not modify any Rust files from prior sessions unless fixing a bug you discover.
```

---

## Summary

| Session | Milestones | Est. LoC | Depends On | Parallel With |
|---------|-----------|----------|------------|---------------|
| A | M-1.1, M-1.2, M-1.3 | ~500 | — | — |
| B | M-2.0, M-2.1, M-2.2 | ~800 | A | C |
| C | M-3.1 | ~600 | A | B |
| D | M-4.1, M-5.1 | ~900 | A, B, C | — |
| E | M-6.1–M-6.4 | ~1,500 | A, B, D | — |
| F | M-7.1a–M-7.4 | ~1,200 | E | — |
| G | M-8.1, M-9.1–M-9.2, M-10.1–M-10.2 | ~2,000 | F | — |
| H | M-11.1–M-11.5 | ~3,000 | G | J |
| I | M-12.1–M-12.11, M-13.1 | ~1,200 | G, H | — |
| J | M-14.1–M-14.3 | ~500 | G | H |
| K | M-15.1–M-15.6 | — | All | — |
| **Total** | **52 milestones** | **~12,200** | | |
