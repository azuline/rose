# Rosé

> [!IMPORTANT]
> Rosé is under active development. See [Issue #1](https://github.com/azuline/rose/issues/1)
> for progress updates.

Rosé is a music manager for Unix-based systems. Rosé provides a virtual FUSE
filesystem for managing your music library and various functions for editing
and improving your music library's metadata and tags.

> [!NOTE]
> Rosé modifies the managed audio files. If you do not want to modify your
> audio files, for example because they are seeding in a bittorrent client, you
> should not use Rosé.

TODO: Video

## Installation

Install Rosé with Nix Flakes. If you do not have Nix Flakes, you can install it
with [this installer](https://github.com/DeterminateSystems/nix-installer).

```bash
$ nix profile install github:azuline/rose#rose
```

In the future, other packaging systems may be considered. However, I strongly
dislike Python's packaging story, hence: Nix.

## Quickstart

TODO

## Features

TODO

## Requirements

TODO

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

# OLD

## The Virtual Filesystem

Rosé reads a source directory of releases like this:

```tree
.
├── BLACKPINK - 2016. SQUARE ONE
├── BLACKPINK - 2016. SQUARE TWO
├── LOOΠΔ - 2019. [X X]
├── LOOΠΔ - 2020. [#]
├── LOOΠΔ 1_3 - 2017. Love & Evil
├── LOOΠΔ ODD EYE CIRCLE - 2017. Max & Match
└── YUZION - 2019. Young Trapper
```

And constructs a virtual filesystem from the source directory's audio tags. The
virtual filesystem enables viewing various subcollections of the source
directory based on multiple types of tags as a filesystem.

While music players and music servers enable viewing releases with similar
filters, those filters are only available in a proprietary UI. Rosé provides
this filtering as a filesystem, which is easily composable with other tools and
systems.

The virtual filesystem constructed from the above source directory is:

```tree
.
├── Releases
│   ├── BLACKPINK - 2016. SQUARE ONE - Single [K-Pop] {YG Entertainment}
│   ├── BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}
│   ├── LOOΠΔ 1_3 - 2017. Love & Evil [K-Pop] {BlockBerry Creative}
│   ├── LOOΠΔ - 2019. [X X] [K-Pop] {BlockBerry Creative}
│   ├── LOOΠΔ - 2020. [#] [K-Pop] {BlockBerry Creative}
│   ├── LOOΠΔ ODD EYE CIRCLE - 2017. Max & Match [K-Pop] {BlockBerry Creative}
│   └── YUZION - 2019. Young Trapper [Hip Hop] {No Label}
├── Artists
│   ├── BLACKPINK
│   │   ├── BLACKPINK - 2016. SQUARE ONE - Single [K-Pop] {YG Entertainment}
│   │   └── BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}
│   ├── LOOΠΔ
│   │   ├── LOOΠΔ - 2019. [X X] [K-Pop] {BlockBerry Creative}
│   │   └── LOOΠΔ - 2020. [#] [K-Pop] {BlockBerry Creative}
│   ├── LOOΠΔ 1_3
│   │   └── LOOΠΔ 1_3 - 2017. Love & Evil [K-Pop] {BlockBerry Creative}
│   ├── LOOΠΔ ODD EYE CIRCLE
│   │   └── LOOΠΔ ODD EYE CIRCLE - 2017. Max & Match [K-Pop] {BlockBerry Creative}
│   └── YUZION
│       └── YUZION - 2019. Young Trapper [Hip Hop] {No Label}
├── Genres
│   ├── Hip-Hop
│   │   └── YUZION - 2019. Young Trapper [Hip Hop] {No Label}
│   └── K-Pop
│       ├── BLACKPINK - 2016. SQUARE ONE - Single [K-Pop] {YG Entertainment}
│       ├── BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}
│       ├── LOOΠΔ 1_3 - 2017. Love & Evil [K-Pop] {BlockBerry Creative}
│       ├── LOOΠΔ - 2019. [X X] [K-Pop] {BlockBerry Creative}
│       ├── LOOΠΔ - 2020. [#] [K-Pop] {BlockBerry Creative}
│       └── LOOΠΔ ODD EYE CIRCLE - 2017. Max & Match [K-Pop] {BlockBerry Creative}
└── Labels
    ├── BlockBerry Creative
    │   ├── LOOΠΔ 1_3 - 2017. Love & Evil [K-Pop] {BlockBerry Creative}
    │   ├── LOOΠΔ - 2019. [X X] [K-Pop] {BlockBerry Creative}
    │   ├── LOOΠΔ - 2021. [&] [K-Pop] {BlockBerry Creative}
    │   └── LOOΠΔ ODD EYE CIRCLE - 2017. Max & Match [K-Pop] {BlockBerry Creative}
    ├── No Label
    │   └── YUZION - 2019. Young Trapper [Hip Hop] {No Label}
    └── YG Entertainment
        ├── BLACKPINK - 2016. SQUARE ONE - Single [K-Pop] {YG Entertainment}
        └── BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}
```

## The Metadata Improvement Tooling

Rosé constructs the virtual filesystem from the audio tags. However, audio tags
are frequently missing or incorrect. Thus, Rosé also provides a set of tools to
improve the audio tag metadata.

Note that the metadata manager _modifies_ the source files. If you do not want
to modify the source files, you should `chmod 444` and not use the metadata
manager!

I have yet to write this part of the tool. Please check back later!


# Usage

```
Usage: python -m rose [OPTIONS] COMMAND [ARGS]...

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

## Supported Filetypes

Rosé supports `.mp3`, `.m4a`, `.ogg` (vorbis), `.opus`, and `.flac` audio files.

Rosé also supports JPEG and PNG cover art. The supported cover art file stems
are `cover`, `folder`, and `art`. The supported cover art file extensions are
`.jpg`, `.jpeg`, and `.png`.

## Virtual Filesystem

The virtual filesystem is mounted and unmounted by `rose fs mount` and
`rose fs unmount` respectively.

TODO

- document supported operations

## Metadata Management

TODO

## Systemd Unit Files

TODO; example unit files to schedule Rosé with systemd.
