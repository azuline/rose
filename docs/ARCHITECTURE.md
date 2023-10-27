# Architecture

Rosé has a simple uni-directional looping architecture.

```mermaid
flowchart BT
    S[Source Files]
    C[Read Cache]
    M[Metadata Tooling]
    V[Virtual Filesystem]
    S -->|Populates| C
    M -->|Reads| C
    M -->|Writes| S
    V -->|Writes| S
    V -->|Reads| C
```

1. The source audio files, playlist files, and collage files are single sources
   of truth. All writes are made directly to the sources files.
2. The read cache is transient and deterministically derived from source
   files. It can always be deleted and fully recreated from source files. It
   updates in response to changes in the source files.
3. The virtual filesystem uses the read cache for performance. All writes made
   via the virtual filesystem are made to the Source Files, which in turn
   refreshes the read cache.
4. The metadata tooling uses the read cache for performance, but always writes
   to the source files directly, which in turn refreshes the read cache.

This architecture takes care to ensure that there is a single source of truth
and uni-directional mutations. This means that Rosé and the source files always
have the same metadata. If the source files change, `rose cache update` is
guaranteed to rebuild the cache such that it fully matches the source files.
And if `rose cache watch` is running, the cache updates should happen
automatically.

This has some nice consequences:

- External tag editing tools do not disrupt Rosé. If an external tool modifies
  the audio tags, Rosé's cache can always update to match the newly modified
  source files.
- Rosé is easily synchronized across machines, for example with a tool like
  Syncthing. As long as the source files are in-sync, Rosé's read cache will
  match.
- An inconsistent state between source files and Rosé is trivially resolved.
  This is different from music managers that retain a separate database,
  because conflict resolution is then ambiguous. Whereas in Rosé, there are no
  conflicts.

# Stable Release & Track Identifiers

Rosé assigns UUIDs to each release and track in order to identify them across
arbitrarily large metadata changes. These UUIDs are persisted to the source
files.

- Each release has a `.rose.{uuid}.toml` file, which preserves release-level
  state, such as `New`. The UUID is in the filename instead of the file
  contents for improved performance: we can collect the UUID via a `readdir`
  call instead of an expensive file read.
- Each track has a custom `roseid` tag. This tag is written to the source audio
  file.

# Read Cache Update

The read cache update is optimized to minimize the number of disk accesses, as
it's a hot path and quite expensive if implemented poorly.

The read cache update first pulls all relevant cached data from SQLite. Stored
on each track is the mtime during the previous cache update. The cache update
checks whether any files have changed via `readdir` and `stat` calls, and only
reads the file if the `mtime` has changed. Throughout the update, we take note
of the changes to apply. At the end of the update, we make a few fat SQL
queries to batch the writes.

The update process is also parallelizable, so we shard workloads across
multiple processes.

# Logging

Logs are written to stderr and to `${XDG_STATE_HOME:-$HOME/.local/state}/rose/rose.log`.
Debug logging can be turned on with the `--verbose/-v` option. Rosé is heavily
instrumented with debug logging.
