# Managing Releases

The virtual filesystem makes some actions available as filesystem operations.
All actions available in the Virtual Filesystem are also available as a CLI
operation.

All command line commands accept releases in three formats:

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

## Remove Release Cover Art

Command line:

```bash
$ cd $fuse_mount_dir

$ rose releases remove-cover "LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]"
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
