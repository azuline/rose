# Configuration

Rosé must be configured prior to use. Rosé is configured via a TOML file
located at `${XDG_CONFIG_HOME:-$HOME/.config}/rose/config.toml`.

The configuration parameters, with examples, are:

```toml
# === Required values ===

# The directory containing the music to manage. This source directory WILL be
# modified by Rosé; if you do not want the files in directory to be modified,
# use another tool!
music_source_dir = "~/.music-source"
# The directory to mount the library's virtual filesystem on. This is the
# primary "user interface" of Rosé, and the directory in which you will be able
# to browse your music library.
fuse_mount_dir = "~/music"

# === Optional values ===

# The directory to write the cache to. Defaults to
# `${XDG_CACHE_HOME:-$HOME/.cache}/rose`.
cache_dir = "~/.cache/rose"
# Maximum parallel processes that the cache updater can spawn. Defaults to
# nproc/2. The higher this number is; the more performant the cache update will
# be.
max_proc = 4
# Artist aliases: Releases belonging to an alias will also "belong" to the main
# artist. This option improves the Artist browsing view by showing the aliased
# releases in the main artist's releases list.
artist_aliases = [
  { artist = "Abakus", aliases = ["Cinnamon Chasers"] },
  { artist = "tripleS", aliases = ["EVOLution", "LOVElution", "+(KR)ystal Eyes", "Acid Angel From Asia", "Acid Eyes"] },
]
# Artists, genres, and labels to show in the virtual filesystem navigation. By
# default, all artists, genres, and labels are shown. However, these values can
# be used to filter the listed values to a specific few. This is useful e.g. if
# you only care to browse your favorite genres and labels.
fuse_artists_whitelist = [ "xxx", "yyy" ]
fuse_genres_whitelist = [ "xxx", "yyy" ]
fuse_labels_whitelist = [ "xxx", "yyy" ]
# Artists, genres, and labels to hide from the virtual filesystem navigation.
# These options remove specific entities from the default policy of listing all
# entities. These options are mutually exclusive with the fuse_*_whitelist
# options; if both are specified for a given entity type, the configuration
# will not validate.
fuse_artists_blacklist = [ "xxx" ]
fuse_genres_blacklist = [ "xxx" ]
fuse_labels_blacklist = [ "xxx" ]
```

The `--config/-c` flag overrides the config location.

## Music Source Dir

The `music_source_dir` must be a flat directory of releases, meaning all releases
must be top-level directories inside `music_source_dir`. Each release should also
be a single directory in `music_source_dir`.

Every directory should follow the format: `$music_source_dir/$release_name/**/$track.mp3`.
A release can have arbitrarily many nested subdirectories.

So for example: `$music_source_dir/BLACKPINK - 2016. SQUARE ONE/*.mp3`.

Rosé writes playlist and collage files to the `$music_source_dir/.playlists`
and `$music_source_dir/.collages` directories. Each file is a human-readable
TOML file.

