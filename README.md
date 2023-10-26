# Rosé

> [!IMPORTANT]
> Rosé is under active development. See [Issue #1](https://github.com/azuline/rose/issues/1)
> for progress updates.

Rosé is a music manager for Unix-based systems. Rosé provides a virtual FUSE
filesystem for managing your music library and various functions for editing
and improving your music library's metadata and tags.

Rosé's core functionality is taking in a directory of music and creating a
virtual filesystem based on the music's tags.

So for example, given the following directory of music files:

```
source/
├── !collages
│   └── Road Trip.toml
├── !playlists
│   └── Shower.toml
├── BLACKPINK - 2016. SQUARE ONE
│   ├── 01. WHISTLE.opus
│   ├── 02. BOOMBAYAH.opus
│   └── cover.jpg
├── BLACKPINK - 2016. SQUARE TWO
│   ├── 01. PLAYING WITH FIRE.opus
│   ├── 02. STAY.opus
│   ├── 03. WHISTLE (acoustic ver.).opus
│   └── cover.jpg
├── LOOΠΔ - 2017. Kim Lip
│   ├── 01. Eclipse.opus
│   ├── 02. Twilight.opus
│   └── cover.jpg
├── LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match
│   ├── 01. ODD.opus
│   ├── 02. Girl Front.opus
│   ├── 03. LOONATIC.opus
│   ├── 04. Chaotic.opus
│   ├── 05. Starlight.opus
│   └── cover.jpg
└── YUZION - 2019. Young Trapper
    ├── 01. Look At Me!!.mp3
    ├── 02. In My Pocket.mp3
    ├── 03. Henzclub.mp3
    ├── 04. Ballin'.mp3
    ├── 05. Jealousy.mp3
    ├── 06. 18.mp3
    ├── 07. Still Love.mp3
    ├── 08. Next Up.mp3
    └── cover.jpg
```

Rosé produces the following virtual filesystem (duplicate information has been
omitted).

```
virtual/
├── 1. Releases/
│   ├── {NEW} LOOΠΔ - 2017. Kim Lip - Single [K-Pop]/
│   │   ├── 01. LOOΠΔ - Eclipse.opus
│   │   ├── 02. LOOΠΔ - Twilight.opus
│   │   └── cover.jpg
│   ├── BLACKPINK - 2016. SQUARE ONE - Single [K-Pop] {YG Entertainment}/
│   │   └── ...
│   ├── BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}/
│   │   └── ...
│   ├── LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}/
│   │   └── ...
│   └── YUZION - 2019. Young Trapper [Hip Hop]/
│       └── ...
├── 2. Releases - New/
│   └── [2023-10-25] {NEW} LOOΠΔ - 2017. Kim Lip - Single [K-Pop]/...
├── 3. Releases - Recently Added/
│   ├── [2023-10-25] {NEW} LOOΠΔ - 2017. Kim Lip - Single [K-Pop]/...
│   ├── [2023-10-01] LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}/...
│   ├── [2022-08-22] BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}/...
│   ├── [2022-08-10] BLACKPINK - 2016. SQUARE ONE - Single [K-Pop] {YG Entertainment}/...
│   └── [2019-09-16] YUZION - 2019. Young Trapper [Hip Hop]/...
├── 4. Artists/
│   ├── BLACKPINK/
│   │   ├── BLACKPINK - 2016. SQUARE ONE - Single [K-Pop] {YG Entertainment}/...
│   │   └── BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}/...
│   ├── LOOΠΔ/
│   │   ├── {NEW} LOOΠΔ - 2017. Kim Lip - Single [K-Pop]
│   │   └── LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}/...
│   ├── LOOΠΔ ODD EYE CIRCLE/
│   │   └── LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}/...
│   └── YUZION/
│       └── YUZION - 2019. Young Trapper [Hip Hop]/...
├── 5. Genres/
│   ├── Hip Hop/
│   │   └── YUZION - 2019. Young Trapper [Hip Hop]/...
│   └── K-Pop/
│       ├── {NEW} LOOΠΔ - 2017. Kim Lip - Single [K-Pop]/...
│       ├── BLACKPINK - 2016. SQUARE ONE - Single [K-Pop] {YG Entertainment}/...
│       ├── BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}/...
│       └── LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}/...
├── 6. Labels/
│   ├── BlockBerry Creative/
│   │   ├── {NEW} LOOΠΔ - 2017. Kim Lip - Single [K-Pop]/...
│   │   └── LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}/...
│   └── YG Entertainment/
│       ├── BLACKPINK - 2016. SQUARE ONE - Single [K-Pop] {YG Entertainment}/...
│       └── BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}/...
├── 7. Collages/
│   └── Road Trip/
│       ├── 1. BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}/...
│       └── 2. LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}/...
└── 8. Playlists/
    └── Shower/
        ├── 1. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus
        ├── 2. YUZION - Jealousy.mp3
        ├── 3. BLACKPINK - PLAYING WITH FIRE.opus
        └── 4. LOOΠΔ - Eclipse.opus
```

Rosé's virtual filesystem organizes your music library by the metadata in the
music tags. In addition to a flat directory of all releases, Rosé creates
additional directories based on Date Added, Artist, Genre, and Label.

Rosé also provides support for creating Collages (collections of releases) and
Playlists (collections of tracks). These are configured as TOML files in the
source directory.

Because the quality of the virtual filesystem depends on the quality of the
tags, Rosé also provides functions for improving the tags of your music
library. Rosé provides an easy text-based interface for manually modifying
metadata, automatic metadata importing from third-party sources, and a rules
system to automatically apply metadata changes based on patterns.

> [!NOTE]
> Rosé modifies the managed audio files, even on first scan. If you do not want
> to modify your audio files, for example because they are seeding in a
> bittorrent client, you should not use Rosé.

_Demo Video TBD_

## Installation

Install Rosé with Nix Flakes. If you do not have Nix Flakes, you can install it
with [this installer](https://github.com/DeterminateSystems/nix-installer).

```bash
$ nix profile install github:azuline/rose#rose
```

In the future, other packaging systems may be considered. However, I strongly
dislike Python's packaging story, hence: Nix.

## Quickstart

After installing Rosé, let's first confirm that we can invoke the tool. Rosé
provides the `rose` CLI tool, which should emit help text when ran.

```bash
$ rose

Usage: rose [OPTIONS] COMMAND [ARGS]...

  A virtual filesystem for music and metadata improvement tooling.

Options:
  -v, --verbose      Emit verbose logging.
  -c, --config PATH  Override the config file location.
  --help             Show this message and exit.

Commands:
  cache     Manage the read cache.
  collages  Manage collages.
  fs        Manage the virtual library.
  releases  Manage releases.
  playlists Manage playlists.
```

Next...

- Mount
- Play!
- Unmount

## Features

TODO

## Requirements

Rosé supports `.mp3`, `.m4a`, `.ogg` (vorbis), `.opus`, and `.flac` audio files.

Rosé also supports JPEG and PNG cover art. The supported cover art file stems
are `cover`, `folder`, and `art`. The supported cover art file extensions are
`.jpg`, `.jpeg`, and `.png`.

## License

Copyright 2023 blissful <blissful@sunsetglow.net>

Licensed under the Apache License, Version 2.0 (the "License"); you may not use
this file except in compliance with the License. You may obtain a copy of the
License at http://www.apache.org/licenses/LICENSE-2.0.

Unless required by applicable law or agreed to in writing, software distributed
under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
CONDITIONS OF ANY KIND, either express or implied. See the License for the
specific language governing permissions and limitations under the License.

## Contributions

TODO
