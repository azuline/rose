# rose

Rose is a Linux music library manager.

## Configuration

Rose reads its configuration from `${XDG_CONFIG_HOME:-$HOME/.config}/rose/config.toml`.

The configuration parameters, with examples, are:

```toml
# The directory containing the music to manage.
music_source_dir = "~/.music-src"
# The directory to mount the library's virtual filesystem on.
fuse_mount_dir = "~/music"
```
