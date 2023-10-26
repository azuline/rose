# Maintaining the Cache

For performance, Rosé stores a copy of every source file's metadata in a SQLite
read cache. The read cache does not accept writes; thus it can always be fully
recreated from the source files. It can be freely deleted and recreated without
consequence.

The cache can be updated with the command `rose cache update`. By default, the
cache updater will only recheck files that have changed since the last run. To
override this behavior and always re-read file tags, run `rose cache update
--force`.

By default, the cache is updated on `rose fs mount` and when files are changed
through the virtual filesystem. However, if the `music_source_dir` is changed
directly, Rosé does not automatically update the cache, which can lead to cache
drifts.

You can solve this problem by running `rose cache watch`. This starts a watcher
that triggers a cache update whenever a source file changes. This can be useful
if you synchronize your music library between two computers, or use another
tool to directly modify the `music_source_dir`.
