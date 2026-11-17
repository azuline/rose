# AGENTS.md

Conventions for working in this repository, derived from its history and existing code. Read
`docs/ARCHITECTURE.md` before making a non-trivial change.

## Commands

All tooling runs inside the Nix dev shell.

```bash
nix develop --command make check      # typecheck + test + lintcheck; run before pushing
nix develop --command make lint       # autoformat: ruff format, ruff check --fix, prettier
nix develop --command make snapshot   # regenerate syrupy snapshots after an intended output change
```

The virtual filesystem tests mount a real FUSE filesystem and need `/dev/fuse`.

## Architecture rules

- **Source files are the source of truth.** Audio tags, `.rose.{uuid}.toml` datafiles, and the
  `!collages/*.toml` and `!playlists/*.toml` files hold all state. Every write goes to those files
  and is followed by an `update_cache_for_*` call.
- **The read cache is disposable.** It is deterministically derived from the source files and is
  dropped and rebuilt whenever `cache.sql` or a cache-affecting config field changes. There are no
  migrations. Never store state that exists only in the cache.
- **`rose.cache` owns SQLite.** Other modules use its read functions; the one exception is the
  rules engine's full-text search query.
- **`rose` is a library; `rose_cli`, `rose_vfs`, and `rose_watch` are frontends.** Frontends import
  from `rose`'s `__init__.py` exports. Export a new public function there instead of reaching into a
  submodule.
- **Take the lock around writes.** Writes to a release, collage, or playlist happen inside
  `with lock(c, release_lock_name(...))` (or the collage/playlist equivalent).

## Code conventions

- Functions take `c: Config` as their first argument. Optional arguments are keyword-only.
- Data is modeled as dataclasses. Types are fully annotated; `mypy` runs in strict mode.
- Errors the user can cause subclass `RoseExpectedError` and are printed without a traceback.
  Everything else subclasses `RoseError`. Define the error class at the top of the module that
  raises it.
- Each module opens with a docstring that explains what it encapsulates. Comments explain why, not
  what.
- Logging uses `logger = logging.getLogger(__name__)`. State changes log at `info`; the code is
  heavily instrumented with `debug` logs.
- Ruff with line length 120. Markdown is formatted with prettier.
- A user-visible change updates `docs/` in the same commit.

## Testing philosophy

Tests run the real thing and assert on what the user would observe. The suite drives real audio
files in `testdata/`, a real SQLite cache, a real FUSE mount, and a real watchdog process. It has no
mocking layer, and that is deliberate: this project's bugs live in the interaction between tags,
files, and the cache, which a stub cannot reproduce.

1. **Assert on outcomes, not on calls.** Run the operation, then read the result back: tags via
   `AudioTags.from_file`, the datafile via `tomllib`, the cache via `rose.cache` functions or a
   direct `SELECT`, the mount via plain filesystem calls. Do not assert that a function was called,
   how many times, or in what order.
2. **Never patch Rose's own code.** No `monkeypatch`, `mock.patch`, spies, or fakes on Rose
   functions, methods, or constants. If a behavior can only be observed by patching an internal, it
   is an implementation detail; do not test it.
3. **Stub only what a test cannot drive: the human and the process.** The sole stand-ins in the
   suite are for `click.edit`, `click.confirm`, `click.prompt`, and `os._exit`. For new code, pass
   the interaction in as a parameter (for example `editor_fn`) or keep the prompt in `rose_cli`,
   rather than adding a new `monkeypatch`.
4. **Do not test wiring.** A test must be able to fail because the user-visible behavior broke.
   Whether a code path takes a lock, calls a helper, or skips a read is covered by testing the
   primitive once (`test_locks`) and the end result everywhere else. If the only way to write the
   test is to restate the implementation, delete the test.
5. **Test in the layer that owns the behavior.** Exhaustive cases live next to the library function
   in `rose-py`. The CLI, VFS, and watcher tests are brief walkthroughs that spot-check that the
   frontend reaches the library.
6. **Build state through the shared fixtures.** `config` gives an isolated source directory, cache,
   and mount point. `source_dir` copies `testdata/` in and runs a real cache update; use it for
   anything that writes. `seeded_cache` inserts a fixed set of rows for read-path tests.
   `static_cache` is the same with fixed paths for snapshots. Add a case to `testdata/` or the seed
   before writing a one-off fixture.
7. **Prefer fewer, denser tests.** One test may walk a whole workflow with a comment per step, as
   `virtualfs_test.py` and `watcher_test.py` do. Do not add a test per branch for coverage; coverage
   is reported, not enforced.
8. **Compare whole values.** Assert equality against the full expected dataclass or list, written
   out literally. Use syrupy snapshots only for large JSON output (`dump_test.py`).
9. **Poll, do not sleep.** For asynchronous behavior, loop with `retry_for_sec` until the expected
   state appears. Give timing assertions slack for slow CI runners.

### Mechanics

- Tests are colocated as `*_test.py` beside the module they test. Shared fixtures live in the root
  `conftest.py`.
- Tests are plain functions named `test_<function>_<scenario>` and typed `-> None`. There are no
  test classes.
- Use `@pytest.mark.usefixtures(...)` when a fixture's value is not used in the test body.
