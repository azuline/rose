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

The release directories and track files in `$music_source_dir` can be renamed
with the `rename_source_files` configuration variable. See
[Configuration](./CONFIGURATION.md) for more details.

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

Note: Rosé supports passing releases and tracks by both their UUIDs and by
path. Paths in the source directory and paths in the virtual directory are both
supported. All views in the virtual directory are supported as well.

## Toggle Release "new"-ness

Command line:

```bash
$ cd $fuse_mount_dir

$ rose releases toggle-new "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP"
[21:47:52] INFO: Updating cache for release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match
[21:47:52] INFO: Updating release descriptions for Long Flight
[21:47:52] INFO: Updating cache for collage Long Flight

$ tree "2. Releases - New/"
2. Releases - New/
├── {NEW} LOOΠΔ - 2017. Kim Lip - Single/...
└── {NEW} LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP/...

$ rose releases toggle-new "1. Releases/{NEW} LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP"
[21:49:36] INFO: Updating cache updates for release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match
[21:49:36] INFO: Updating release descriptions for Long Flight
[21:49:36] INFO: Updating cache for collage Long Flight

$ tree "2. Releases - New/"
2. Releases - New/
└── {NEW} LOOΠΔ - 2017. Kim Lip - Single/...
```

Virtual filesystem:

```bash
$ cd $fuse_mount_dir

$ mv "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP" "1. Releases/{NEW} LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP"

$ tree "2. Releases - New/"
2. Releases - New/
├── {NEW} LOOΠΔ - 2017. Kim Lip - Single/...
└── {NEW} LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP/...

$ mv "1. Releases/{NEW} LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP" "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP"

$ tree "2. Releases - New/"
2. Releases - New/
└── {NEW} LOOΠΔ - 2017. Kim Lip - Single/...
```

## Set Release Cover Art

_The filename of the cover art in the virtual filesystem will always appear as
`cover.{ext}`, regardless of the cover art name in the source directory._

Command line:

```bash
$ cd $fuse_mount_dir

$ rose releases set-cover "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP" ./cover.jpg
[20:43:50] INFO: Set the cover of release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match to cover.jpg
[20:43:50] INFO: Updating cache for release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match

$ tree "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP/"
1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP/
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

$ mv ~/downloads/cover.jpg "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP/cover.jpg"

$ tree "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP/"
1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP/
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

$ rose releases delete-cover "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP"
[02:13:17] INFO: Deleted cover arts of release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match
[02:13:17] INFO: Updating cache for release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match

$ tree "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP/"
1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP/
├── 01. LOOΠΔ ODD EYE CIRCLE - ODD.opus
├── 02. LOOΠΔ ODD EYE CIRCLE - Girl Front.opus
├── 03. LOOΠΔ ODD EYE CIRCLE - LOONATIC.opus
├── 04. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus
└── 05. LOOΠΔ ODD EYE CIRCLE - Starlight.opus
```

Virtual filesystem:

```bash
$ cd $fuse_mount_dir

$ rm "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP/cover.jpg"

$ tree "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP/"
1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP/
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

$ rose releases delete "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP"
[21:56:25] INFO: Trashed release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP
[21:56:25] INFO: Evicted release /home/blissful/demo/source/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match from cache
[21:56:25] INFO: Marking missing release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP as missing in collage Long Flight
[21:56:25] INFO: Updating release descriptions for Long Flight
[21:56:25] INFO: Updating cache for collage Long Flight

$ tree "1. Releases/"
1. Releases/
├── BLACKPINK - 2016. SQUARE ONE - Single/...
├── BLACKPINK - 2016. SQUARE TWO - Single/...
├── NewJeans - 2022. Ditto - Single/...
└── {NEW} LOOΠΔ - 2017. Kim Lip - Single/...
```

Virtual filesystem:

```bash
$ cd $fuse_mount_dir

$ rmdir "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP"

$ tree "1. Releases/"
1. Releases/
├── BLACKPINK - 2016. SQUARE ONE - Single/...
├── BLACKPINK - 2016. SQUARE TWO - Single/...
├── NewJeans - 2022. Ditto - Single/...
└── {NEW} LOOΠΔ - 2017. Kim Lip - Single/...
```

## Edit Release Metadata

See the "Text-Based Release Editing" section in [Improving Your Music Metadata](./METADATA_TOOLS.md).

## Run Rule Engine Action on Release

Rosé allows you to run an action from the rule engine on all tracks in a
release. With this command, you do not need to specify a matcher; this command
auto-matches all tracks in the release.

See [Improving Your Music Metadata](./METADATA_TOOLS.md) for documentation on
the rules engine.

```bash
$ rose releases run-rule '{NEW} The Strokes - 2001. Is This It' 'genre::add:Indie Rock'
The Strokes - 2001. Is This It/01. Is This It.opus
      genre: [] -> ['Indie Rock']
The Strokes - 2001. Is This It/02. The Modern Age.opus
      genre: [] -> ['Indie Rock']
The Strokes - 2001. Is This It/03. Soma.opus
      genre: [] -> ['Indie Rock']
The Strokes - 2001. Is This It/04. Barely Legal.opus
      genre: [] -> ['Indie Rock']
The Strokes - 2001. Is This It/05. Someday.opus
      genre: [] -> ['Indie Rock']
The Strokes - 2001. Is This It/06. Alone, Together.opus
      genre: [] -> ['Indie Rock']
The Strokes - 2001. Is This It/07. Last Nite.opus
      genre: [] -> ['Indie Rock']
The Strokes - 2001. Is This It/08. Hard to Explain.opus
      genre: [] -> ['Indie Rock']
The Strokes - 2001. Is This It/09. When It Started.opus
      genre: [] -> ['Indie Rock']
The Strokes - 2001. Is This It/10. Trying Your Luck.opus
      genre: [] -> ['Indie Rock']
The Strokes - 2001. Is This It/11. Take It or Leave It.opus
      genre: [] -> ['Indie Rock']

Write changes to 11 tracks? [Y/n] y

[16:26:42] INFO: Writing tag changes for actions genre::add
[16:26:42] INFO: Wrote tag changes to The Strokes - 2001. Is This It/01. Is This It.opus
[16:26:42] INFO: Wrote tag changes to The Strokes - 2001. Is This It/02. The Modern Age.opus
[16:26:42] INFO: Wrote tag changes to The Strokes - 2001. Is This It/03. Soma.opus
[16:26:42] INFO: Wrote tag changes to The Strokes - 2001. Is This It/04. Barely Legal.opus
[16:26:42] INFO: Wrote tag changes to The Strokes - 2001. Is This It/05. Someday.opus
[16:26:42] INFO: Wrote tag changes to The Strokes - 2001. Is This It/06. Alone, Together.opus
[16:26:42] INFO: Wrote tag changes to The Strokes - 2001. Is This It/07. Last Nite.opus
[16:26:42] INFO: Wrote tag changes to The Strokes - 2001. Is This It/08. Hard to Explain.opus
[16:26:42] INFO: Wrote tag changes to The Strokes - 2001. Is This It/09. When It Started.opus
[16:26:42] INFO: Wrote tag changes to The Strokes - 2001. Is This It/10. Trying Your Luck.opus
[16:26:42] INFO: Wrote tag changes to The Strokes - 2001. Is This It/11. Take It or Leave It.opus

Applied tag changes to 11 tracks!
```

## Run Rule Engine Action on Track

Similar to how you can run a rule engine action on a release, Rosé also allows
you to run an action on a single track.

```bash
$ rose tracks run-rule '018b6514-6fb7-7cc6-9d23-8eaf0b1beee8' 'tracktitle::replace:Certified Banger'
LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match/04. Chaotic.opus
      title: Chaotic -> Certified Banger

Write changes to 1 tracks? [Y/n] y

[16:40:16] INFO: Writing tag changes for actions tracktitle::replace:Certified Banger
[16:40:16] INFO: Wrote tag changes to LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match/04. Chaotic.opus

Applied tag changes to 11 tracks!
```

## Create "Phony" Single Release

Let's say that you did not enjoy a release, and want to delete it from your
library, but you did enjoy one standout track and wish to keep only that track.

Rosé allows you to create a new, "phony," single release for that track alone,
so that you can get rid of the release while keeping the track(s) you liked.

To demonstrate:

```bash
$ cd $fuse_mount_dir

$ rose releases create-single "1. Releases/ITZY - 2022. CHECKMATE/01.\ SNEAKERS.opus"
[12:16:06] INFO: Created phony single release ITZY - 2022. SNEAKERS
[12:16:06] INFO: Updating cache for release ITZY - 2022. SNEAKERS
[12:16:06] INFO: Toggled "new"-ness of release /home/blissful/.music-source/ITZY - 2022. SNEAKERS to False
[12:16:06] INFO: Updating cache for release ITZY - 2022. SNEAKERS

$ tree "1. Releases/ITZY - 2022. SNEAKERS - Single/"
1. Releases/ITZY - 2022. SNEAKERS - Single/
├── 01. ITZY - SNEAKERS.opus
└── cover.jpg
```

The original track is unmodified by the command: the new release contains a
copy of the previous track with some modified tags. If cover art is present in
the directory of the given track, that cover art is also copied to the new
single release.

The new single release's tags are modified from the original track, like so:

```
albumtitle = $tracktitle
releasetype = "single"
albumartists = $trackartists
tracknumber = 1
discnumber = 1
```
