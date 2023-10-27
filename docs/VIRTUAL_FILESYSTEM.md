# Browsing the Virtual Filesystem

The virtual filesystem is the "User Interface" of Rosé.

The virtual filesystem exposes a rich and meaningful music library organization
as a standard API, which other systems can easily compose with. Contrast this
with "normal" music players: other programs cannot easily interact with their
music organization patterns.

Therefore, the Virtual Filesystem is designed to be used by other programs. A
powerful combination is to combine the Virtual Filesystem with a file manager
like [nnn](https://github.com/jarun/nnn).

# Mounting & Unmounting

The Virtual Filesystem is mounted with the `rose fs mount` command. By default,
this command starts a backgrounded daemon. You can run the filesystem in the
foreground with the `--foreground/-f` flag.

The Virtual Filesystem is unmounted with the `rose fs unmount` command. This
command is a thin wrapper around `umount`. Note that this is subject to all the
restrictions of `umount`: If the Virtual Filesystem is currently in use, the
command will fail.

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

They should be fairly intuitive. They are numbered in the filesystem for the
sake of ordering.

# Directory and File Names

Rosé constructs a "virtual" directory name for each release and "virtual" file
name for each track. These virtual names are constructed from the music
metadata. When the metadata is updated, the virtual names auto-update in
response.

The release template is:

```
%ALBUM_ARTISTS% - %YEAR%. %ALBUM_TITLE% - %RELEASE_TYPE% [%GENRE%] {%LABEL%}
```

But the `- %RELEASE_TYPE%` field is omitted when the release is of type `album`,
`other`, or `unknown`.

The file template is:

```
%TRACK_ARTISTS% - %TRACK_TITLE%.%EXTENSION%
```

Depending on the view, the virtual names may have a position prefix. The
position prefix is of the format `%POSITION%. `. For example, tracks in a
release have a prefix of `%DISC_NUMBER%-%TRACK_NUMBER%. `. Collages and playlists
also apply a position prefix to each release/track in them.

# New Releases

Rose supports flagging releases as "NEW" and making that evident in the virtual
directory name. NEW releases have their virtual directory name prefixed with
`{NEW}`.

By default, releases are flagged as NEW when first imported into Rosé.

NEW-ness has no effects besides prefixing the directory name with `{NEW}` and
adding the release to the `2. Releases - New` top-level directory. NEW-ness is
designed for you, the human operator, to edit manually.

NEW-ness is tracked within a release's `.rose.{uuid}.toml` file. See
[Architecture](./ARCHITECTURE.md) for more information about this file.

# Operations

> [!NOTE]
> Operations on collages and playlists are documented in
> [Using Playlists & Collages](./PLAYLISTS_COLLAGES.md).

The virtual filesystem makes some actions available as filesystem operations.
All actions available in the Virtual Filesystem are also available as a CLI
operation.

Let's go through them!

## Toggle NEW-ness

Command line:

```bash
$ cd $fuse_mount_dir

$ rose releases toggle-new "LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}"
[21:47:52] INFO: Refreshing the read cache for 1 releases
[21:47:52] INFO: Applying cache updates for release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match
[21:47:52] INFO: Refreshing the read cache for 1 collages
[21:47:52] INFO: Updating release descriptions for Long Flight
[21:47:52] INFO: Applying cache updates for collage Long Flight

$ tree "2. Releases - New/"
2. Releases - New/
├── {NEW} LOOΠΔ - 2017. Kim Lip - Single [K-Pop]/...
└── {NEW} LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}/...

$ rose releases toggle-new "{NEW} LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}"
[21:49:36] INFO: Refreshing the read cache for 1 releases
[21:49:36] INFO: Applying cache updates for release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match
[21:49:36] INFO: Refreshing the read cache for 1 collages
[21:49:36] INFO: Updating release descriptions for Long Flight
[21:49:36] INFO: Applying cache updates for collage Long Flight

$ tree "2. Releases - New/"
2. Releases - New/
└── {NEW} LOOΠΔ - 2017. Kim Lip - Single [K-Pop]/...
```

Virtual filesystem:

```bash
$ cd $fuse_mount_dir

$ mv "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}" "1. Releases/{NEW} LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}"
$ tree "2. Releases - New/"
2. Releases - New/
├── {NEW} LOOΠΔ - 2017. Kim Lip - Single [K-Pop]/...
└── {NEW} LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}/...

$ mv "1. Releases/{NEW} LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}" "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}"
$ tree "2. Releases - New/"
2. Releases - New/
└── {NEW} LOOΠΔ - 2017. Kim Lip - Single [K-Pop]/...
```

## Delete a Release

_Deletion will move the release into the trashbin, following the
[freedesktop spec](https://freedesktop.org/wiki/Specifications/trash-spec/).
The release can be restored later if the deletion was accidental._


Command line:

```bash
$ cd $fuse_mount_dir

$ rose releases delete "LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}"
[21:56:25] INFO: Trashed release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}
[21:56:25] INFO: Evicting cached releases that are not on disk
[21:56:25] INFO: Evicted release /home/blissful/demo/source/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match from cache
[21:56:25] INFO: Refreshing the read cache for 1 collages
[21:56:25] INFO: Removing nonexistent release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative} from collage Long Flight
[21:56:25] INFO: Updating release descriptions for Long Flight
[21:56:25] INFO: Applying cache updates for collage Long Flight

$ tree "1. Releases/"
1. Releases/
├── BLACKPINK - 2016. SQUARE ONE - Single [K-Pop] {YG Entertainment}/...
├── BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}/...
├── YUZION - 2019. Young Trapper [Hip Hop]/...
└── {NEW} LOOΠΔ - 2017. Kim Lip - Single [K-Pop]/...
```

Virtual filesystem:

```bash
$ cd $fuse_mount_dir

$ rmdir "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}"
$ tree "1. Releases/"
1. Releases/
├── BLACKPINK - 2016. SQUARE ONE - Single [K-Pop] {YG Entertainment}/...
├── BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}/...
├── YUZION - 2019. Young Trapper [Hip Hop]/...
└── {NEW} LOOΠΔ - 2017. Kim Lip - Single [K-Pop]/...
```
