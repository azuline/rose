# Browsing the Virtual Filesystem

The virtual filesystem is the primary "User Interface" of Music. It exposes a meaningful music
library organization as the filesystem. Since the filesystem is a foundational API, other programs
can trivially integrate with Music. For example, Music can used with a file manager like
[nnn](https://github.com/jarun/nnn) and a media player like [mpv](https://mpv.io/).

# Mounting & Unmounting

You can mount the virtual filesystem `music fs mount` command. By default, this starts a backgrounded
daemon. You can run the filesystem in the foreground with the `--foreground/-f` flag.

You can unmount the virtual filesystem with the `music fs unmount` command. This command simply calls
`umount` under the hood. Thus, this command is subject to the restrictions of `umount`. Including:
if the virtual filesystem is currently in use, unmounting command will fail.

# Directory Structure

Music has 12 top-level directories, each of which is a different view into the library. They are:

1. `Releases`
2. `Releases - New`
3. `Releases - Favorites`
4. `Releases - Added On`
5. `Releases - Released On`
6. `Artists`
7. `Genres`
8. `Descriptors`
9. `Labels`
10. `Loose Tracks`
11. `Collages`
12. `Playlists`

Most directories should be fairly intuitive. They are numbered in the filesystem to create an
intentional ordering.

## Loose Tracks

The only one that may not be intuitive is Loose Tracks. These are releases with the release type set
to `loosetrack`. Releases with the release type `loosetrack` are excluded from the standard release
views; they are only visible in the Loose Tracks view. This feature is intended to allow storing
random one-off tracks for the sake of playlisting without polluting the releases list with many
small entries.

# Directory and File Names

Music constructs a "virtual" directory name for each release and "virtual" file name for each track.
These filenames are different from the release's filenames in the source directory. Music uses the
source directory's metadata tags to generate the virtual names. Therefore, when the music tags
change, the virtual names auto-update in response.

The directory and file names are configurable. See [Directory & Filename Templates](./TEMPLATES.md)
for details.

Music also exposes all cover art under the filename `cover.{ext}`, regardless of the filename in the
source directory. Music also exposes the `.rose.{uuid}.toml` datafile in the virtual filesystem.

# Hiding Artists, Genres, and Labels

Music supports hiding individual artists, genres, and labels in their view directories (`Artists`,
`Genres`, and `Labels`) with the `vfs.x_blacklist` and `vfs.x_whitelist` configuration parameters.
See [Configuration](./CONFIGURATION.md) for additional documentation on configuring the blacklist or
whitelist.

# Operations

Music allows you to modify the library through the virtual filesystem.

Modifying files in the virtual filesystem is passed through to the underlying file. Other
operations, such as creating files and directories, renaming them, and deleting them translate into
specific Music actions.

See [Managing Releases](./RELEASES.md) and [Managing Playlists & Collages](./PLAYLISTS_COLLAGES.md)
for documentation on the supported virtual filesystem operations.
