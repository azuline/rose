# Using Playlists & Collages

Rosé supports the creation and management of collages (lists of releases) and
playlists (lists of tracks).

As Rosé implements playlists and collages in almost the same way, except that
one tracks releases and the other tracks tracks, we discuss both collages and
playlists together.

# Storage Format

Collages and playlists are stored on-disk in the source directory, in the
`!collages` and `!playlists` directories, respectively. Each collage and
playlist is a single `.toml` file inside their respective directory.

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
description_meta = "BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}"

[[releases]]
uuid = "018b4ff1-acdf-7ff1-bcd6-67757aea0fed"
description_meta = "LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}"
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

$ tree "7. Collages"
1. Collages/
├── Morning/...
└── Road Trip/...

$ mkdir "8. Playlists/Evening"

$ tree "8. Playlists"
2. Playlists/
├── Evening/...
└── Shower/...
```

## Adding a Release/Track

Command line:

_Releases can be added by UUID or virtual directory name. Tracks can only be
added by UUID. This is because the release virtual directory name is globally
unique, while track virtual filenames are not globally unique._

```bash
$ rose collages add-release "Morning" "LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}"
[17:59:38] INFO: Added release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative} to collage Morning
[17:59:38] INFO: Refreshing the read cache for 1 collages
[17:59:38] INFO: Applying cache updates for collage Morning

$ rose collages add-release "Morning" "018b268e-ef68-7180-a01e-19bc3fdf970e"
[17:59:44] INFO: Added release BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment} to collage Morning
[17:59:44] INFO: Refreshing the read cache for 1 collages
[17:59:44] INFO: Applying cache updates for collage Morning

$ rose playlists add-track "Evening" "018b6514-6fb7-7cc6-9d23-8eaf0b1beee8"
[18:02:21] INFO: Added track LOOΠΔ ODD EYE CIRCLE - Chaotic.opus to playlist Evening
[18:02:21] INFO: Refreshing the read cache for 1 playlists
[18:02:21] INFO: Applying cache updates for playlist Evening
```

Virtual filesystem:

_When copying a release directory, there will be errors if `cp` is used. They
are safe to ignore. The error happens because the action of creating the
directory adds the release to the collage. After that point, all files are
already part of the release directory, yet `cp` attempts to copy the files over
too. We will try to fix this later._

```bash
$ cd $fuse_mount_dir

$ cp -r "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}" "7. Collages/Morning/"
cp: cannot create directory '7. Collages/Morning/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}': No such file or directory

$ tree "7. Collages/Morning/"
7. Collages/Morning/
├── 1. BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}/...
└── 2. LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}/...

$ cp "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}/04. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus" "8. Playlists/Evening/"

$ tree "8. Playlists/Evening/"
8. Playlists/Evening/
└── 1. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus
```

## Removing a Release/Track

Command line:

_Releases can be removed by UUID or virtual directory name. Tracks can only be
removed by UUID. This is because the release virtual directory name is globally
unique, while track virtual filenames are not globally unique._

```bash
$ rose collages remove-release "Morning" "LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}"
[18:11:43] INFO: Removed release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative} from collage Morning
[18:11:43] INFO: Refreshing the read cache for 1 collages
[18:11:43] INFO: Applying cache updates for collage Morning

$ rose collages remove-release "Morning" "018b268e-ef68-7180-a01e-19bc3fdf970e"
[18:12:03] INFO: Removed release BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment} from collage Morning
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

$ rmdir "7. Collages/Morning/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}"

$ tree "7. Collages/Morning/"
7. Collages/Morning/
0 directories, 0 files

$ rm "8. Playlists/Evening/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}/04. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus"

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
BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}
LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}
# We will save the following text:
LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}
BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}
# And the logs printed to stderr are:
[18:20:53] INFO: Edited collage Road Trip from EDITOR
[18:20:53] INFO: Refreshing the read cache for 1 collages
[18:20:53] INFO: Applying cache updates for collage Road Trip

$ tree "7. Collages/Road Trip"
7. Collages/Road Trip/
├── 1. LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}/...
└── 2. BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}/...

$ rose playlists edit "Shower"
# Opens the following text in $EDITOR:
LOOΠΔ ODD EYE CIRCLE - Chaotic.opus
YUZION - Jealousy.mp3
BLACKPINK - PLAYING WITH FIRE.opus
LOOΠΔ - Eclipse.opus
# We will save the following text:
BLACKPINK - PLAYING WITH FIRE.opus
LOOΠΔ ODD EYE CIRCLE - Chaotic.opus
# And the logs printed to stderr are:
[18:22:42] INFO: Edited playlist Shower from EDITOR
[18:22:42] INFO: Refreshing the read cache for 1 playlists
[18:22:42] INFO: Applying cache updates for playlist Shower

$ tree "8. Playlists/Shower"
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

$ tree "7. Collages"
7. Collages
└── Road Trip/...

$ rmdir "8. Playlists/Evening"

$ tree "8. Playlists"
8. Playlists
└── Shower/...
```

## Renaming a Collage/Playlist

Command line:

```bash
$ rose collages rename "Road Trip" "Long Flight"
[18:29:08] INFO: Renaming collage Road Trip to Long Flight
[18:29:08] INFO: Refreshing the read cache for 1 collages
[18:29:08] INFO: Applying cache updates for collage Long Flight
[18:29:08] INFO: Evicting cached collages that are not on disk
[18:29:08] INFO: Evicted collage Road Trip from cache

$ tree "7. Collages"
7. Collages/
└── Long Flight/...

$ rose playlists rename "Shower" "Meal Prep"
[18:30:17] INFO: Renaming playlist Shower to Meal Prep
[18:30:17] INFO: Refreshing the read cache for 1 playlists
[18:30:17] INFO: Applying cache updates for playlist Meal Prep
[18:30:17] INFO: Evicting cached playlists that are not on disk
[18:30:17] INFO: Evicted playlist Shower from cache

$ tree "8. Playlists"
8. Playlists/
└── Meal Prep/...
```

Virtual filesystem:

```bash
$ cd $fuse_mount_dir

$ mv "7. Collages/Road Trip/" "7. Collages/Long Flight"

$ tree "7. Collages"
7. Collages
└── Long Flight/...

$ mv "8. Playlists/Shower" "8. Playlsits/Meal Prep"

$ tree "8. Playlists"
8. Playlists
└── Meal Prep/...
```