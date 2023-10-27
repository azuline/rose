# Maintaining the Cache

The read cache is a SQLite database that replicates the metadata in
`music_source_dir`. The read cache exists to improve performance: it can be
read from far more performantly than the `music_source_dir` can.

The read cache is never written to directly, outside of the `update_cache_*`
functions, which re-read the source files and write their metadata into the
cache. All mutations in the system occur directly to the source files, not to
the read cache.

# Cache Drift

So what's the problem?

The read cache has the possibility of _drifting_ from the source directory. For
example, let's say that I move files around in the source directory, modify
tags directly, or even delete a release. After those actions, the read cache
will still reflect the previous state, and thus we say that the cache has
_drifted_.

This is problematic because some operations in the virtual filesystem will
begin to fail. For example, if a file is moved in the source directory, and the
virtual filesystem then attempts to read from its previous path, it will hit a
FileNotFound (ENOENT) error.

Thus, after changes to the source directory, we need to update the cache so
that it _synchronizes_ with the source directory. Note that this
synchronization is entirely _one-way_: the source files update the read cache.
The opposite direction never occurs: the read cache never updates the source
files.

# Updating the Cache

The cache can be updated with the command `rose cache update`. By default, this
command only checks files which have changed since the last cache update. It
uses the mtime (last modified) field for this purpose. However, sometimes we
want to refresh the cache regardless of mtimes. In that case, we can run `rose
cache update --force`.

It would be pretty annoying if you had to run this command by hand after each
metadata update. So Rosé will automatically run this command whenever an update
happens _through_ Rosé. That means:

- If a file is modified in the virtual filesystem, a cache update is
  triggered when the file handle closes.
- If a release is modified by the command line, a cache update is triggered at
  the end of the command.

Rosé will also update the cache on virtual filesystem mount.

# Updating on External Changes

However, there is one class of updates that this does not update in response
to, and that is updates made by external tools directly to the source
directory. If another system updates your source directory directly, the read
cache will drift.

To update in response to those external changes, you can run `rose cache
watch`. This command starts a watcher that triggers a cache update whenever a
file changes in the source directory. `rose cache watch` runs in the
foreground, so it is recommended that you manage it with a service manager like
systemd. See [Configuration](./CONFIGURATION.md) for example systemd unit
files.

# Cache Resets

When Rosé detects that:

1. Rosé has been upgraded to a new version,
2. The configuration file has changed,
3. Or the cache database schema has changed,

Rosé will delete the read cache and rebuild it from scratch.

A full cache rebuild is fairly performant, though an order of magnitude slower
than a cache scan that results in no changes.

Deleting the read cache is a supported operation, and it is a viable solution
in case your cache ends up in a bad state (e.g. due to a bug or other issue).

Since Rosé stores all of its state in the source directory, and merely
replicates that state into the cache, deleting the cache will never result in a
loss of data.
