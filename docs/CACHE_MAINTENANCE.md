# Maintaining the Cache

The read cache is a SQLite database that replicates the metadata in
`music_source_dir`. The read cache exists solely to improve performance: it can
be read from far more performantly than the `music_source_dir` can.

The read cache does not have any state of its own. _All_ of the data in the read
cache is replicated from the `music_source_dir`. Hence, we never write to the
read cache. Instead, all updates write to the source files, which then triggers
the cache update function. The cache update function updates the cache to match
the source directory's state.

> [!NOTE]
> To better understand how the read cache fits into Rosé, we recommend reading
> [Architecture](./ARCHITECTURE.md).

# Cache Drift

Assuming you only modify your music library through Rosé, the cache will always
remain up to date, since Rosé triggers a cache update whenever it is aware of
an update to the source directory.

However, that assumption does not hold. If changes are made directly to the
source directory, and Rosé is not "informed," Rosé's cache will contain the
previous state of the source directory. We call this a _cache drift_.

This is problematic because Rosé may attempt to read files that no longer exist
or display old metadata. Thus, we should inform Rosé whenever a change is made
to the source directory.

# Updating the Cache

A cache update can be performed manually with the `rose cache update` command.
In this command, Rosé will identify any files that changed and update the read
cache accordingly. In other words, this command informs Rosé that something
changed in the source directory.

For performance reasons, the `rose cache update` command only checks files with
a different Last Modified (mtime) from the last cache update. To disable this
behavior and recheck every file, pass the `--force/-f` flag.

It would be annoying if you had to run `rose cache upate` by hand after each
metadata change. Rosé thus automatically updates the cache in response to
changes made _through_ Rosé. Any updates made through the virtual filesystem or
command line automatically trigger a cache update for the changed files. Rosé
will also update the cache when the virtual filesystem is mounted.

However, even with that improvement, you would still need to run `rose cache
update` automatically whenever changes are made directly to the source
directory. Thus, Rosé provides the `rose cache watch` command, which runs a
watcher that listens for file update events in the source directory. This
watcher will trigger a cache update whenever a file in the source directory
changes.

By default, the watcher runs in the foreground. We recommend backgrounding it
with a service manager, such as systemd. See
[Configuration](./CONFIGURATION.md) for sample systemd unit files.

# Cache Resets

When Rosé detects that:

1. Rosé has been upgraded to a new version,
2. The configuration file has changed,
3. Or the cache database schema has changed,

Rosé will delete the read cache and rebuild it from scratch. A full cache
rebuild is fairly performant, though an order of magnitude slower than a cache
scan that results in no changes.

Deleting the read cache does not result in any loss of data, and is a viable
solution if your cache ends up in a bad state (e.g. due to a bug).
