# Using Playlists & Collages

Rosé supports the creation and management of collages (lists of releases) and
playlists (lists of tracks).

As Rosé implements playlists and collages in almost the same way, except that
one tracks releases and the other tracks tracks, we discuss both collages and
playlists together.

# Storage Format

Collages and playlists are stored on-disk in the source directory, in the
`!collages` and `!playlists` directories, respectively. Each collage and
playlist is a single `.toml` file.

For example:

```
source/
├── !collages
│   └── Road Trip.toml
└── !playlists
    └── Shower.toml
```

An example of the contents of the `.toml` file are, for a collage:

```toml

[[releases]]
uuid = "018b268e-ef68-7180-a01e-19bc3fdf970e"
description_meta = "BLACKPINK - 2016. SQUARE TWO - Single [Dance-Pop;K-Pop]"

[[releases]]
uuid = "018b4ff1-acdf-7ff1-bcd6-67757aea0fed"
description_meta = "LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]"
```

and for a playlist:

```toml
[[tracks]]
uuid = "018b6514-6fb7-7cc6-9d23-8eaf0b1beee8"
description_meta = "LOOΠΔ ODD EYE CIRCLE - Chaotic.opus"

[[tracks]]
uuid = "018b6514-72e7-7321-832d-1a524dbf1a3b"
description_meta = "BLACKPINK - PLAYING WITH FIRE.opus"
```

These files contain the UUIDs assigned to each release/track by Rosé. Since
UUIDs are not meaningful, the files also contain a `description_meta` field.
The `description_meta` field is set to the virtual directory/file name. The
`description_meta` field is updated to the latest values during Rosé's cache
update, so that they remain meaningful.

The ordering of the releases/tracks is meaningful: they represent the
ordering of releases/tracks in the collage/playlist.

Playlists can also have custom cover art. These are stored as
`{playlist_name}.{image_ext}`. So for example, `Shower.toml`'s cover art would
be located at `Shower.jpg` (or `.png`). The extensions to treat as images are
configurable. See [Configuration](./CONFIGURATION.md).

> [!NOTE]
> When a release or track is deleted from the source directory, Rosé does not
> autoremove that release/track from the collages and playlists that it belongs
> to. Rosé instead flags the release/track as "missing," which prevents it from
> appearing in the virtual filesystem. If the release/track is re-added to the
> source directory, Rosé will remove the missing flag, and the release/track
> will "regain" its lost position in the collage/playlist.
>
> We do this because we do not know if a missing release or a missing track is
> transient or not. For example, a file may be deleted by a tool like syncthing
> only to be readded later.

# Operations

However, working with this file directly is quite annoying, so Rosé allows you
to manage collages and playlists via the command line and the virtual
filesystem. In the rest of this document, we'll demonstrate the basic
operations.

## Creating a Collage/Playlist

Command line:

```bash
$ rose collages create "Morning"
[17:51:22] INFO: Creating collage Morning in source directory
[17:51:22] INFO: Refreshing the read cache for 1 collages
[17:51:22] INFO: Applying cache updates for collage Morning

$ rose playlists create "Evening"
[17:51:47] INFO: Creating playlist Evening in source directory
[17:51:47] INFO: Refreshing the read cache for 1 playlists
[17:51:47] INFO: Applying cache updates for playlist Evening
```

Virtual filesystem:

```bash
$ cd $fuse_mount_dir

$ mkdir "7. Collages/Morning"

$ tree "7. Collages/"
1. Collages/
├── Morning/...
└── Road Trip/...

$ mkdir "8. Playlists/Evening"

$ tree "8. Playlists/"
2. Playlists/
├── Evening/...
└── Shower/...
```

## Adding a Release/Track

Command line:

_Releases can be added by UUID, virtual directory name, or virtual filesystem
path. Tracks can only be added by UUID. This is because the release virtual
directory name is globally unique, while track virtual filenames are not
globally unique._

```bash
$ rose collages add-release "Morning" "LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]"
[17:59:38] INFO: Added release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop] to collage Morning
[17:59:38] INFO: Refreshing the read cache for 1 collages
[17:59:38] INFO: Applying cache updates for collage Morning

$ rose collages add-release "Morning" "018b268e-ef68-7180-a01e-19bc3fdf970e"
[17:59:44] INFO: Added release BLACKPINK - 2016. SQUARE TWO - Single [Dance-Pop;K-Pop] to collage Morning
[17:59:44] INFO: Refreshing the read cache for 1 collages
[17:59:44] INFO: Applying cache updates for collage Morning

$ rose playlists add-track "Evening" "018b6514-6fb7-7cc6-9d23-8eaf0b1beee8"
[18:02:21] INFO: Added track LOOΠΔ ODD EYE CIRCLE - Chaotic.opus to playlist Evening
[18:02:21] INFO: Refreshing the read cache for 1 playlists
[18:02:21] INFO: Applying cache updates for playlist Evening
```

Virtual filesystem:

```bash
$ cd $fuse_mount_dir

$ cp -r "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]" "7. Collages/Morning/"
cp: cannot create directory '7. Collages/Morning/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]': No such file or directory

$ tree "7. Collages/Morning/"
7. Collages/Morning/
├── 1. BLACKPINK - 2016. SQUARE TWO - Single [Dance-Pop;K-Pop]/...
└── 2. LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/...

$ cp "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/04. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus" "8. Playlists/Evening/"

$ tree "8. Playlists/Evening/"
8. Playlists/Evening/
└── 1. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus
```

## Removing a Release/Track

Command line:

_Releases can be removed by UUID, virtual directory name, or virtual filesystem
path. Tracks can only be removed by UUID. This is because the release virtual
directory name is globally unique, while track virtual filenames are not
globally unique._

```bash
$ rose collages remove-release "Morning" "LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]"
[18:11:43] INFO: Removed release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop] from collage Morning
[18:11:43] INFO: Refreshing the read cache for 1 collages
[18:11:43] INFO: Applying cache updates for collage Morning

$ rose collages remove-release "Morning" "018b268e-ef68-7180-a01e-19bc3fdf970e"
[18:12:03] INFO: Removed release BLACKPINK - 2016. SQUARE TWO - Single [Dance-Pop;K-Pop] from collage Morning
[18:12:03] INFO: Refreshing the read cache for 1 collages
[18:12:03] INFO: Applying cache updates for collage Morning

$ rose playlists remove-track "Evening" "018b6514-6fb7-7cc6-9d23-8eaf0b1beee8"
[18:12:22] INFO: Removed track LOOΠΔ ODD EYE CIRCLE - Chaotic.opus from playlist Evening
[18:12:22] INFO: Refreshing the read cache for 1 playlists
[18:12:22] INFO: Applying cache updates for playlist Evening
```

Virtual filesystem:

```bash
$ cd $fuse_mount_dir

$ rmdir "7. Collages/Morning/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]"

$ tree "7. Collages/Morning/"
7. Collages/Morning/
0 directories, 0 files

$ rm "8. Playlists/Evening/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/04. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus"

$ tree "8. Playlists/Evening/"
8. Playlists/Evening/
0 directories, 0 files
```

## Reordering Releases/Tracks

Reordering releases/tracks is only possible via the command line.

_Releases and tracks can also be removed from the collage or playlist by
deleting their line entry from the text file._

```bash
$ rose collages edit "Road Trip"
# Opens the following text in $EDITOR:
BLACKPINK - 2016. SQUARE TWO - Single [Dance-Pop;K-Pop]
LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]
# We will save the following text:
LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]
BLACKPINK - 2016. SQUARE TWO - Single [Dance-Pop;K-Pop]
# And the logs printed to stderr are:
[18:20:53] INFO: Edited collage Road Trip from EDITOR
[18:20:53] INFO: Refreshing the read cache for 1 collages
[18:20:53] INFO: Applying cache updates for collage Road Trip

$ tree "7. Collages/Road Trip/"
7. Collages/Road Trip/
├── 1. LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/...
└── 2. BLACKPINK - 2016. SQUARE TWO - Single [Dance-Pop;K-Pop]/...

$ rose playlists edit "Shower"
# Opens the following text in $EDITOR:
LOOΠΔ ODD EYE CIRCLE - Chaotic.opus
NewJeans - Ditto.opus
BLACKPINK - PLAYING WITH FIRE.opus
LOOΠΔ - Eclipse.opus
# We will save the following text:
BLACKPINK - PLAYING WITH FIRE.opus
LOOΠΔ ODD EYE CIRCLE - Chaotic.opus
# And the logs printed to stderr are:
[18:22:42] INFO: Edited playlist Shower from EDITOR
[18:22:42] INFO: Refreshing the read cache for 1 playlists
[18:22:42] INFO: Applying cache updates for playlist Shower

$ tree "8. Playlists/Shower/"
8. Playlists/Shower/
├── 1. BLACKPINK - PLAYING WITH FIRE.opus
└── 2. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus
```

## Deleting a Collage/Playlist

_Deletion will move the collage/playlist into the trashbin, following the
[freedesktop spec](https://freedesktop.org/wiki/Specifications/trash-spec/).
The collage/playlist can be restored later if the deletion was accidental._

Command line:

```bash
$ rose collages delete "Morning"
[18:23:44] INFO: Deleting collage Morning from source directory
[18:23:44] INFO: Evicting cached collages that are not on disk
[18:23:44] INFO: Evicted collage Morning from cache

$ rose playlists create "Evening"
[18:26:38] INFO: Deleting playlist Evening from source directory
[18:26:38] INFO: Evicting cached playlists that are not on disk
[18:26:38] INFO: Evicted playlist Evening from cache
```

Virtual filesystem:

```bash
$ cd $fuse_mount_dir

$ rmdir "7. Collages/Morning"

$ tree "7. Collages/"
7. Collages/
└── Road Trip/...

$ rmdir "8. Playlists/Evening"

$ tree "8. Playlists/"
8. Playlists/
└── Shower/...
```

## Renaming a Collage/Playlist

_Renaming a collage/playlist will also rename "adjacent" files (including
playlist cover art). Adjacent files are files with the same stem as the
collage/playlist, but a different file extension. For example, `Shower.toml`
and `Shower.jpg`._

Command line:

```bash
$ rose collages rename "Road Trip" "Long Flight"
[18:29:08] INFO: Renaming collage Road Trip to Long Flight
[18:29:08] INFO: Refreshing the read cache for 1 collages
[18:29:08] INFO: Applying cache updates for collage Long Flight
[18:29:08] INFO: Evicting cached collages that are not on disk
[18:29:08] INFO: Evicted collage Road Trip from cache

$ tree "7. Collages/"
7. Collages/
└── Long Flight/...

$ rose playlists rename "Shower" "Meal Prep"
[18:30:17] INFO: Renaming playlist Shower to Meal Prep
[18:30:17] INFO: Refreshing the read cache for 1 playlists
[18:30:17] INFO: Applying cache updates for playlist Meal Prep
[18:30:17] INFO: Evicting cached playlists that are not on disk
[18:30:17] INFO: Evicted playlist Shower from cache

$ tree "8. Playlists/"
8. Playlists/
└── Meal Prep/...
```

Virtual filesystem:

```bash
$ cd $fuse_mount_dir

$ mv "7. Collages/Road Trip/" "7. Collages/Long Flight"

$ tree "7. Collages/"
7. Collages/
└── Long Flight/...

$ mv "8. Playlists/Shower" "8. Playlsits/Meal Prep"

$ tree "8. Playlists/"
8. Playlists/
└── Meal Prep/...
```

## Set Playlist Cover Art

_This operation is playlist-only, as collages do not have their own cover art._

_The filename of the cover art in the virtual filesystem will always appear as
`cover.{ext}`, regardless of the cover art name in the source directory._

Command line:

```bash
$ cd $fuse_mount_dir

$ rose playlists set-cover "Shower" ./cover.jpg
[20:51:59] INFO: Set the cover of playlist Shower to cover.jpg
[20:51:59] INFO: Refreshing the read cache for 1 playlists
[20:51:59] INFO: Applying cache updates for playlist Shower

$ tree "8. Playlists/Shower/"
8. Playlists/Shower/
├── 1. BLACKPINK - PLAYING WITH FIRE.opus
├── 2. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus
└── cover.jpg
```

Virtual filesystem:

_The filename of the created file in the release directory must be one of the
valid cover art filenames. The valid cover art filenames are controlled by and
documented in [Configuration](./CONFIGURATION.md)._

```bash
$ cd $fuse_mount_dir

$ mv ~/downloads/cover.jpg "8. Playlists/Shower/cover.jpg"

$ tree "8. Playlists/Shower/"
8. Playlists/Shower/
├── 1. BLACKPINK - PLAYING WITH FIRE.opus
├── 2. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus
└── cover.jpg
```

## Remove Playlist Cover Art

_This operation is playlist-only, as collages do not have their own cover art._

Command line:

```bash
$ cd $fuse_mount_dir

$ rose playlists remove-cover "Shower"
[02:10:34] INFO: Deleted cover arts of playlist Lounge
[02:10:34] INFO: Refreshing the read cache for 1 playlists
[02:10:34] INFO: Applying cache updates for playlist Lounge

$ tree "8. Playlists/Shower/"
8. Playlists/Shower/
├── 1. BLACKPINK - PLAYING WITH FIRE.opus
└── 2. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus
```

Virtual filesystem:

```bash
$ cd $fuse_mount_dir

$ rm "8. Playlists/Shower/cover.jpg"

$ tree "8. Playlists/Shower/"
8. Playlists/Shower/
├── 1. BLACKPINK - PLAYING WITH FIRE.opus
└── 2. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus
```
