# Maintaining the Cache

The read cache is a SQLite database that replicates the metadata in `music_source_dir`. The read
cache exists solely to improve performance: it can be read from far more performantly than the
`music_source_dir` can.

The read cache does not have any state of its own. _All_ of the data in the read cache is replicated
from the `music_source_dir`. Hence, we never write directly to the read cache. Instead, we always
write directly to the source files and then trigger the cache update sequence. The cache update
sequence updates the cache to match the source directory's state.

> [!NOTE]
> To better understand how the read cache fits into Music, we recommend reading
> [Architecture](./ARCHITECTURE.md).

# Cache Drift

Assuming you only modify your music library through Music, the cache will always remain up to date,
since Music triggers a cache update whenever it is aware of an update to the source directory.

However, that assumption does not hold. If changes are made directly to the source directory, and
Music is not "informed," Music's cache will contain the previous state of the source directory. We
call this a _cache drift_.

This is problematic because Music may attempt to read files that no longer exist or display old
metadata. Thus, we should inform Music whenever a change is made to the source directory.

# Updating the Cache

A cache update can be performed manually with the `music cache update` command. In this command, Music
will identify any files that changed and update the read cache accordingly. In other words, this
command informs Music that something changed in the source directory.

For performance reasons, the `music cache update` command only checks files with a different Last
Modified (mtime) from the last cache update. To disable this behavior and recheck every file, pass
the `--force/-f` flag.

It would be annoying if you had to run `music cache upate` by hand after each metadata change. Music
thus automatically updates the cache in response to changes made _through_ Music. Any updates made
through the virtual filesystem or command line automatically trigger a cache update for the changed
files. Music will also update the cache when the virtual filesystem is mounted.

However, even with that improvement, if you directly change the source directory with a tool that
isn't Music, the cache will not automatically update in response. If you make all edits through Music,
then this isn't a problem! But for users with other tools that directly edit the source directory,
Music provides the `music cache watch` command, which runs a watcher that listens for file update
events in the source directory. This watcher will trigger a cache update whenever a file in the
source directory changes.

# Cache Resets

When Music detects that:

1. Music has been upgraded to a new version,
2. Cache-relevant configuration options have changed,
3. Or the cache database schema has changed,

Music will delete the read cache and rebuild it from scratch. A full cache rebuild is fairly
performant, though an order of magnitude slower than a cache scan that results in no changes.

Deleting the read cache does not result in any loss of data and is a viable solution if your cache
ends up in a really bad state that `music cache update --force` does not resolve. This should not be
necessary normally, but may occur from a surprise bug.
