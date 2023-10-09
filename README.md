# rose

_In Progress_

Rose is a Linux music library manager.

## Configuration

Rose reads its configuration from `${XDG_CONFIG_HOME:-$HOME/.config}/rose/config.toml`.

The configuration parameters, with examples, are:

```toml
# The directory containing the music to manage.
music_source_dir = "~/.music-src"
# The directory to mount the library's virtual filesystem on.
fuse_mount_dir = "~/music"
# The directory to write the cache to. Defaults to `${XDG_CACHE_HOME:-$HOME/.cache}/rose`.
cache_dir = "~/.cache/rose"
```

## Library Conventions & Expectations

### Directory Structure

`$music_source_dir/albums/track.ogg`

### Supported Extensions

### Tag Structure

WIP

artist1;artist2 feat. artist3

BNF TODO

# Architecture

todo

- db is read cache, not source of truth
- filetags and files are source of truth
