# Managing Releases

# Release & Track Identifiers

Rosé assigns a UUID to each release and track in order to identify them across
cache updates. The UUIDs are also used to track membership in collages and
playlists.

These UUIDs are persisted to the source files on first scan:

- A `.rose.{uuid}.toml` file is created in each release directory. This file
  also stores release state. The release UUID is also written to the
  nonstandard `rosereleaseid` audio tag in each track.
- The track UUID is written to each track's nonstandard `roseid` audio tag.

# Storage Format

The `.rose.{uuid}.toml` file stores release state that is not suitable to be
written to the audio tags. The format of this file is:

```toml
# Release "new"-ness.
new = false
# The timestamp that the release was "added" to the library. Rosé uses the
# timestamp when the `.rose.{uuid}.toml` file was created, as that is
# equivalent to the first time Rosé scanned the release.
added_at = 2018-10-01 00:00:00-04:00
```

# "New" Releases

Rosé supports flagging releases as "new." "New"-ness has no effects besides
prefixing the release's virtual filesystem name with `{NEW}` and adding the
release to the `2. Releases - New` top-level directory.

On first import, releases are flagged as new by default. "New"-ness can be
toggled manually afterwards. This feature is designed to allow you to
distinguish between music you've listened to and music you're planning on
listening to.

# Operations

Rosé allows you to manage releases via the command line and the virtual
filesystem. In the rest of this document, we'll demonstrate the supported
operations.

Note: All command line commands accept releases in three formats:

1. The release's UUID.
2. The release's virtual directory name, excluding prefixes.
3. The path to the release in the virtual filesystem. The virtual filesystem
   must be mounted for this format to work.

## Toggle Release "new"-ness

Command line:

```bash
$ cd $fuse_mount_dir

$ rose releases toggle-new "LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]"
[21:47:52] INFO: Updating cache for release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match
[21:47:52] INFO: Updating release descriptions for Long Flight
[21:47:52] INFO: Updating cache for collage Long Flight

$ tree "2. Releases - New/"
2. Releases - New/
├── {NEW} LOOΠΔ - 2017. Kim Lip - Single [Contemporary R&B;Dance-Pop;K-Pop]/...
└── {NEW} LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/...

$ rose releases toggle-new "{NEW} LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]"
[21:49:36] INFO: Updating cache updates for release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match
[21:49:36] INFO: Updating release descriptions for Long Flight
[21:49:36] INFO: Updating cache for collage Long Flight

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
[20:43:50] INFO: Updating cache for release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match

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

## Delete Release Cover Art

Command line:

```bash
$ cd $fuse_mount_dir

$ rose releases delete-cover "LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]"
[02:13:17] INFO: Deleted cover arts of release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match
[02:13:17] INFO: Updating cache for release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match

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
[21:56:25] INFO: Evicted release /home/blissful/demo/source/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match from cache
[21:56:25] INFO: Marking missing release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop] as missing in collage Long Flight
[21:56:25] INFO: Updating release descriptions for Long Flight
[21:56:25] INFO: Updating cache for collage Long Flight

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

## Edit Release Metadata

Editing a release's metadata is only possible via the command line.

See the "Text-Based Release Editing" section in [Improving Your Music Metadata](./METADATA_TOOLS.md)
for documentation on this operation.

## Create "Phony" Single Release

TODO
