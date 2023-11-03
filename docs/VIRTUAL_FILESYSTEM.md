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
name for each track. Rosé uses the source directory's metadata tags to do so.
When the source directory changes, the virtual names auto-update in response.

The release directory name template is:

```
%NEWNESS% %ALBUM_ARTISTS% - %YEAR%. %ALBUM_TITLE% - %RELEASE_TYPE% [%GENRE%]
```

> [!NOTE]
> The `- %RELEASE_TYPE%` field is omitted when the release is of type `album`,
> `other`, or `unknown`.

The track file name template is:

```
%TRACK_ARTISTS% - %TRACK_TITLE%.%EXTENSION%
```

Depending on the view, the virtual names may have a _prefix_. The prefix is of
the format `%PREFIX%. `. For example, tracks in a release have a position
prefix of `%DISC_NUMBER%-%TRACK_NUMBER%. `. Collages and playlists apply a
position prefix to each release/track in them. The Recently Added Releases view
adds a date prefix to each release.

"New" releases are prefixed with `{NEW}` in the virtual filesystem and appear
in the `2. Releases - New` top-level directory.

> [!NOTE]
> The command line commands accept a release's virtual directory name as a
> valid method of identifying a release. The virtual directory name passed to
> those commands should not contain any date or position prefixes.

Rosé also exposes all cover art under the filename `cover.{ext}`, regardless of
the filename in the source directory.

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
