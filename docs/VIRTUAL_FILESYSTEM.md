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

> [!NOTE]
> The command line commands accept a release's virtual directory name as a
> valid method of identifying a release. The virtual directory name passed to
> those commands should not contain any date or position prefixes.

Rosé also exposes all cover art under the filename `cover.{ext}`, regardless of
the filename in the source directory.

# New Releases

Rose supports flagging releases as "NEW" and making that evident in the virtual
directory name. NEW releases have their virtual directory name prefixed with
`{NEW}`.

By default, Rosé flags releases as new when they are first imported.

NEW-ness has no effects besides prefixing the directory name with `{NEW}` and
adding the release to the `2. Releases - New` top-level directory. NEW-ness is
designed for you, the human operator, to edit manually.

Rosé tracks NEW-ness within a release's `.rose.{uuid}.toml` file. See
[Architecture](./ARCHITECTURE.md) for more information about this file.

# Operations

> [!NOTE]
> Operations on collages and playlists are documented in
> [Using Playlists & Collages](./PLAYLISTS_COLLAGES.md).

The virtual filesystem makes some actions available as filesystem operations.
All actions available in the Virtual Filesystem are also available as a CLI
operation.

All command line commands accept releases in three formats:

1. The release's UUID.
2. The release's virtual directory name, excluding prefixes.
3. The path to the release in the virtual filesystem. The virtual filesystem
   must be mounted for this format to work.

## Toggle Release NEW-ness

Command line:

```bash
$ cd $fuse_mount_dir

$ rose releases toggle-new "LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]"
[21:47:52] INFO: Refreshing the read cache for 1 releases
[21:47:52] INFO: Applying cache updates for release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match
[21:47:52] INFO: Refreshing the read cache for 1 collages
[21:47:52] INFO: Updating release descriptions for Long Flight
[21:47:52] INFO: Applying cache updates for collage Long Flight

$ tree "2. Releases - New/"
2. Releases - New/
├── {NEW} LOOΠΔ - 2017. Kim Lip - Single [Contemporary R&B;Dance-Pop;K-Pop]/...
└── {NEW} LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/...

$ rose releases toggle-new "{NEW} LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]"
[21:49:36] INFO: Refreshing the read cache for 1 releases
[21:49:36] INFO: Applying cache updates for release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match
[21:49:36] INFO: Refreshing the read cache for 1 collages
[21:49:36] INFO: Updating release descriptions for Long Flight
[21:49:36] INFO: Applying cache updates for collage Long Flight

$ tree "2. Releases - New/"
2. Releases - New/
└── {NEW} LOOΠΔ - 2017. Kim Lip - Single [Contemporary R&B;Dance-Pop;K-Pop]/...
```

Virtual filesystem:

```bash
$ cd $fuse_mount_dir

$ mv "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]" "1. Releases/{NEW} LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop]"

$ tree "2. Releases - New/"
2. Releases - New/
├── {NEW} LOOΠΔ - 2017. Kim Lip - Single [Contemporary R&B;Dance-Pop;K-Pop]/...
└── {NEW} LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/...

$ mv "1. Releases/{NEW} LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]" "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop]"

$ tree "2. Releases - New/"
2. Releases - New/
└── {NEW} LOOΠΔ - 2017. Kim Lip - Single [Contemporary R&B;Dance-Pop;K-Pop]/...
```

## Set Release Cover Art

_The filename of the cover art in the virtual filesystem will always appear as
`cover.{ext}`, regardless of the cover art name in the source directory._

Command line:

```bash
$ cd $fuse_mount_dir

$ rose releases set-cover "LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]" ./cover.jpg
[20:43:50] INFO: Set the cover of release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match to cover.jpg
[20:43:50] INFO: Refreshing the read cache for 1 releases
[20:43:50] INFO: Applying cache updates for release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match

$ tree "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/"
1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop]/
├── 01. LOOΠΔ ODD EYE CIRCLE - ODD.opus
├── 02. LOOΠΔ ODD EYE CIRCLE - Girl Front.opus
├── 03. LOOΠΔ ODD EYE CIRCLE - LOONATIC.opus
├── 04. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus
├── 05. LOOΠΔ ODD EYE CIRCLE - Starlight.opus
└── cover.jpg
```

Virtual filesystem:

_The filename of the created file in the release directory must be one of the
valid cover art filenames. The valid cover art filenames are controlled by and
documented in [Configuration](./CONFIGURATION.md)._

```bash
$ cd $fuse_mount_dir

$ mv ~/downloads/cover.jpg "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/cover.jpg"

$ tree "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/"
1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop]/
├── 01. LOOΠΔ ODD EYE CIRCLE - ODD.opus
├── 02. LOOΠΔ ODD EYE CIRCLE - Girl Front.opus
├── 03. LOOΠΔ ODD EYE CIRCLE - LOONATIC.opus
├── 04. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus
├── 05. LOOΠΔ ODD EYE CIRCLE - Starlight.opus
└── cover.jpg
```

## Remove Release Cover Art

Command line:

```bash
$ cd $fuse_mount_dir

$ rose releases remove-cover "LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]"
[02:13:17] INFO: Deleted cover arts of release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match
[02:13:17] INFO: Refreshing the read cache for 1 releases
[02:13:17] INFO: Applying cache updates for release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match

$ tree "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/"
1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop]/
├── 01. LOOΠΔ ODD EYE CIRCLE - ODD.opus
├── 02. LOOΠΔ ODD EYE CIRCLE - Girl Front.opus
├── 03. LOOΠΔ ODD EYE CIRCLE - LOONATIC.opus
├── 04. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus
└── 05. LOOΠΔ ODD EYE CIRCLE - Starlight.opus
```

Virtual filesystem:

```bash
$ cd $fuse_mount_dir

$ rm "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/cover.jpg"

$ tree "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/"
1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop]/
├── 01. LOOΠΔ ODD EYE CIRCLE - ODD.opus
├── 02. LOOΠΔ ODD EYE CIRCLE - Girl Front.opus
├── 03. LOOΠΔ ODD EYE CIRCLE - LOONATIC.opus
├── 04. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus
└── 05. LOOΠΔ ODD EYE CIRCLE - Starlight.opus
```

## Delete a Release

_Deletion will move the release into the trashbin, following the
[freedesktop spec](https://freedesktop.org/wiki/Specifications/trash-spec/).
The release can be restored later if the deletion was accidental._

Command line:

```bash
$ cd $fuse_mount_dir

$ rose releases delete "LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]"
[21:56:25] INFO: Trashed release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]
[21:56:25] INFO: Evicting cached releases that are not on disk
[21:56:25] INFO: Evicted release /home/blissful/demo/source/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match from cache
[21:56:25] INFO: Refreshing the read cache for 1 collages
[21:56:25] INFO: Removing nonexistent release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop] from collage Long Flight
[21:56:25] INFO: Updating release descriptions for Long Flight
[21:56:25] INFO: Applying cache updates for collage Long Flight

$ tree "1. Releases/"
1. Releases/
├── BLACKPINK - 2016. SQUARE ONE - Single [Big Room House;Dance-Pop;K-Pop]/...
├── BLACKPINK - 2016. SQUARE TWO - Single [Dance-Pop;K-Pop]/...
├── NewJeans - 2022. Ditto - Single [Contemporary R&B;K-Pop][K-Pop]/...
└── {NEW} LOOΠΔ - 2017. Kim Lip - Single [Contemporary R&B;Dance-Pop;K-Pop]/...
```

Virtual filesystem:

```bash
$ cd $fuse_mount_dir

$ rmdir "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]"

$ tree "1. Releases/"
1. Releases/
├── BLACKPINK - 2016. SQUARE ONE - Single [Big Room House;Dance-Pop;K-Pop]/...
├── BLACKPINK - 2016. SQUARE TWO - Single [Dance-Pop;K-Pop]/...
├── NewJeans - 2022. Ditto - Single [Contemporary R&B;K-Pop][K-Pop]/...
└── {NEW} LOOΠΔ - 2017. Kim Lip - Single [Contemporary R&B;Dance-Pop;K-Pop]/...
```
