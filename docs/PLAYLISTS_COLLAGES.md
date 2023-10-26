# Using Playlists & Collages

Rosé supports the creation and management of collages (lists of releases) and
playlists (lists of tracks).

As Rosé implements playlists and collages in almost the same way, except that
one tracks releases and the other tracks tracks, we discuss both collages and
playlists together.

## Storage Format

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
UUIDs are not meaningful, they also contain a `description_meta` field. The
`description_meta` field is set to the virtual directory/file name. The
`description_meta` field is updated to the latest values during Rosé's cache
update, so that they remain meaningful.

The ordering of the releases/tracks is meaningful: they represent the
ordering of releases/tracks in the collage/playlist.

## Working With Collages/Playlists

However, working with this file directly is quite annoying, so Rosé allows you
to manage collages and playlists via the command line and the virtual
filesystem. In this section, we'll cover the basic operations.

### Creating a Collage/Playlist

Command line:

```bash
$ rose collages create "Morning"
[17:51:22] INFO: Refreshing the read cache for 1 collages
[17:51:22] INFO: Applying cache updates for collage Morning
$ rose playlists create "Evening"
[17:51:47] INFO: Refreshing the read cache for 1 playlists
[17:51:47] INFO: Applying cache updates for playlist Evening
```

Virtual filesystem:

```bash
$ cd $fuse_mount_dir
$ mkdir "7. Collages/Morning"
$ tree "7. Collages"
1. Collages
├── Morning
└── Road Trip
$ mkdir "8. Playlists/Evening"
$ tree "8. Playlists"
2. Playlists
├── Evening
└── Shower
```

### Adding a Release/Track

### Removing a Release/Track

### Reordering Releases/Tracks

### Deleting a Collage/Playlist
