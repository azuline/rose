# Browsing the Virtual Filesystem

The virtual filesystem is the primary "User Interface" of Rosé. It exposes a
meaningful music library organization as the filesystem. Since the filesystem
is a foundational API, other programs can trivially integrate with Rosé. For
example, Rosé can used with a file manager like [nnn](https://github.com/jarun/nnn)
and a media player like [mpv](https://mpv.io/).

# Mounting & Unmounting

You can mount the virtual filesystem `rose fs mount` command. By default, this
starts a backgrounded daemon. You can run the filesystem in the foreground with
the `--foreground/-f` flag.

You can unmount the virtual filesystem with the `rose fs unmount` command. This
command simply calls `umount` under the hood. Thus, this command is subject to
the restrictions of `umount`. Including: if the virtual filesystem is currently
in use, unmounting command will fail.

# Directory Structure

Rosé has 8 top-level directories, each of which is a different view into the
library. They are:

1. `Releases`
2. `Releases - New`
3. `Releases - Recently Added`
4. `Artists`
5. `Genres`
6. `Labels`
7. `Collages`
8. `Playlists`

Each directory should be fairly intuitive. They are numbered in the filesystem
to create an intentional ordering.

# Directory and File Names

Rosé constructs a "virtual" directory name for each release and "virtual" file
name for each track. These filenames are different from the release's filenames
in the source directory. Rosé uses the source directory's metadata tags to
generate the virtual names. Therefore, when the music tags change, the virtual
names auto-update in response.

The directory and file names are configurable. See [Directory & Filename
Templates](./TEMPLATES.md) for details.

Rosé also exposes all cover art under the filename `cover.{ext}`, regardless of
the filename in the source directory. Rosé also exposes the `.rose.{uuid}.toml`
datafile in the virtual filesystem.

# Hiding Artists, Genres, and Labels

Rosé supports hiding individual artists, genres, and labels in their view
directories (`4. Artists`, `5. Genres`, and `6. Labels`) with the
`fuse_x_blacklist` and `fuse_x_whitelist` configuration parameters. See
[Configuration](./CONFIGURATION.md) for additional documentation on configuring
the blacklist or whitelist.

# Operations

Rosé allows you to modify the library through the virtual filesystem.

Modifying files in the virtual filesystem is passed through to the underlying
file. Other operations, such as creating files and directories, renaming them,
and deleting them translate into specific Rosé actions.

See [Managing Releases](./RELEASES.md) and [Managing Playlists & Collages](./PLAYLISTS_COLLAGES.md)
for documentation on the supported virtual filesystem operations.
