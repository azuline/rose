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
│   ├── BLACKPINK - 2016. SQUARE ONE - Single [K-Pop] {YG Entertainment}/
│   │   ├── 01. BLACKPINK - WHISTLE.opus
│   │   ├── 02. BLACKPINK - BOOMBAYAH.opus
│   │   └── cover.jpg
│   ├── BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}/...
│   ├── LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}/...
│   ├── YUZION - 2019. Young Trapper [Hip Hop]/...
│   └── {NEW} LOOΠΔ - 2017. Kim Lip - Single [K-Pop]/...
├── 2. Releases - New/
│   └── {NEW} LOOΠΔ - 2017. Kim Lip - Single [K-Pop]/...
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
│   │   ├── {NEW} LOOΠΔ - 2017. Kim Lip - Single [K-Pop]/...
│   │   └── LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}/...
│   ├── LOOΠΔ ODD EYE CIRCLE/
│   │   └── LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}/...
│   └── YUZION/
│       └── YUZION - 2019. Young Trapper [Hip Hop]/...
├── 5. Genres/
│   ├── Hip Hop/
│   │   └── YUZION - 2019. Young Trapper [Hip Hop]/...
│   └── K-Pop/
│       ├── BLACKPINK - 2016. SQUARE ONE - Single [K-Pop] {YG Entertainment}/...
│       ├── BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}/...
│       ├── LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}/...
│       └── {NEW} LOOΠΔ - 2017. Kim Lip - Single [K-Pop]/...
├── 6. Labels/
│   ├── BlockBerry Creative/
│   │   ├── LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}/...
│   │   └── {NEW} LOOΠΔ - 2017. Kim Lip - Single [K-Pop]/...
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

Rosé also provides a few other features designed to keep your music library organized:

- **Collages:** Collections of releases.
- **Playlists:** Collections of tracks.
- **New release tracking:** Track new unlistened additions to the library.

And because the quality of the virtual filesystem depends on the quality of the
tags, Rosé also provides functions for improving the tags of your music
library. Rosé provides an easy text-based interface for manually modifying
metadata, automatic metadata importing from third-party sources, and a rules
engine to automatically apply metadata changes based on patterns.

> [!NOTE]
> Rosé modifies the managed audio files, even on first scan. If you do not want
> to modify your audio files, for example because they are seeding in a
> bittorrent client, you should not use Rosé.

_Demo Video TBD_

# Features

This section contains a concise list of Rosé's features.

- Virtual Filesystem
  - Read audio files and cover art
  - Modify files and cover art
  - Filter releases by album artist, genre, label, and "new"-ness
  - Browse and edit collages and playlists
  - Group artist aliases together
  - Toggle release "new"-ness
  - Whitelist/blacklist entries in the artist, genre, and label views
- Command Line
  - Edit release metadata as a text file
  - Import metadata and cover art from third-party sources
  - Extract embedded cover art to a file
  - Automatically update metadata via patterns and rules
  - Collage and playlist management
  - Toggle release "new"-ness
  - Create "singles" from tracks (even if currently tagged as part of an album)
  - Update the read cache (force update supported)
  - Watch the source directory and auto-update the cache on file modification
  - Dump library metadata as JSON
- Support for `.mp3`, `.m4a`, `.ogg` (vorbis), `.opus`, and `.flac` audio
  files.

And the following are opinionated decisions that might make this tool
unsuitable for you:

- Rosé modifies files in the source directory, even as early as the first
  library scan. All mutations in Rosé are persisted by writing to the source
  directory; Rosé maintains no state of its own outside of the source
  directory. This makes Rosé work very poorly with files being seeded as
  torrents.
- Rosé expects all releases to be immediate child directories of the
  source directory. And Rosé expects that all tracks belong to a "release"
  (meaning an album, single, EP, etc.). This means that loose audio files at
  the top-level of the source directory will be ignored. This makes Rosé work
  very poorly with libraries organized as collections of tracks.

# Installation

Install Rosé with Nix Flakes. If you do not have Nix Flakes, you can install
Nix Flakes with [this installer](https://github.com/DeterminateSystems/nix-installer).

Then, to install Rosé, run:

```bash
$ nix profile install github:azuline/rose#rose
```

# Quickstart

Let's now get Rosé up and running!

Once Rosé is installed, let's first confirm that `rose` exists and is
accessible:

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

Great! Next, we'll (1) configure Rosé, (2) mount the virtual filesystem, and
finally (3) play music!

1. Rosé requires a configuration file. The configuration file is located at
   `$XDG_CONFIG_HOME/rose/config.toml`. If you've not done anything weird with
   your system, it should be located at `~/.config/rose/config.toml`.

   Only two configuration options are required:

   ```toml
   # The directory of music to manage.
   # WARNING: The files in this directory WILL be modified by Rosé!
   music_source_dir = "~/.music-source"
   # The mountpoint for the virtual filesystem.
   fuse_mount_dir = "~/music"
   ```

   The full configuration specification is documented in
   [Configuration](./docs/CONFIGURATION.md).

2. Now let's mount the virtual filesystem:

   ```bash
   $ rose fs mount
   [15:41:13] INFO: Refreshing the read cache for 5 releases
   [15:41:13] INFO: Applying cache updates for release BLACKPINK - 2016. SQUARE TWO
   [15:41:13] INFO: Applying cache updates for release BLACKPINK - 2016. SQUARE ONE
   [15:41:13] INFO: Applying cache updates for release LOOΠΔ - 2017. Kim Lip
   [15:41:13] INFO: Applying cache updates for release YUZION - 2019. Young Trapper
   [15:41:13] INFO: Applying cache updates for release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match
   [15:41:13] INFO: Evicting cached releases that are not on disk
   [15:41:13] INFO: Refreshing the read cache for 1 collages
   [15:41:13] INFO: Applying cache updates for collage Road Trip
   [15:41:13] INFO: Evicting cached collages that are not on disk
   [15:41:13] INFO: Refreshing the read cache for 1 playlists
   [15:41:13] INFO: Applying cache updates for playlist Shower
   [15:41:13] INFO: Evicting cached playlists that are not on disk
   ```

   Expect to see log lines emitted to stderr on filesystem mount. This is
   because, at startup, the `rose fs mount` command indexes the
   `music_source_dir` to populate the read cache.

   The virtual filesystem uses the read cache to determine the available music
   and its metadata. It's possible for the cache to get out of sync from the
   source music files. If that happens, the `rose cache update` is guaranteed to
   resynchronize them. See [Maintaining the Cache](./docs/CACHE_MAINTENANCE.md)
   for additional documentation on cache updates and synchronization.

   Now that the virtual filesystem is mounted, let's go take a look! Navigate
   to the configured `fuse_mount_dir`, and you should see your music available
   in the virtual filesystem!

   ```bash
   $ cd $fuse_mount_dir

   $ ls -1
   '1. Releases'
   '2. Releases - New'
   '3. Releases - Recently Added'
   '4. Artists'
   '5. Genres'
   '6. Labels'
   '7. Collages'
   '8. Playlists'

   $ ls -1 "1. Releases/"
   'BLACKPINK - 2016. SQUARE ONE - Single [K-Pop] {YG Entertainment}'
   'BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}'
   'LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}'
   'YUZION - 2019. Young Trapper [Hip Hop]'
   '{NEW} LOOΠΔ - 2017. Kim Lip - Single [K-Pop]'
   ```

3. Let's play some music! You should be able to open a music file in your music
   player of choice.

   Mine is `mpv`:

   ```bash
   $ mpv "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}/04. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus"
    (+) Audio --aid=1 'Chaotic' (opus 2ch 48000Hz)
   File tags:
    Artist: LOOΠΔ ODD EYE CIRCLE
    Album: Mix & Match
    Album_Artist: LOOΠΔ ODD EYE CIRCLE
    Comment: Cat #: WMED0709
    Date: 2017
    Genre: K-Pop
    Title: Chaotic
    Track: 4
   AO: [pipewire] 48000Hz stereo 2ch floatp
   ```

And that's it! If desired, you can unmount the virtual filesystem with the
`rose fs unmount` command.

# Recommended Usage

Rosé alone is not a full-featured music system, and _that's the point_. You
should compose Rosé with other great tools to create the music system that
works best for you.

We recommend using Rosé with:

1. A file manager, such as [nnn](https://github.com/jarun/nnn),
   [mc](https://midnight-commander.org/), and [ranger](https://github.com/ranger/ranger).
2. A media player, such as [mpv](https://mpv.io/).

You also need not use the complete feature set of Rosé. Everything will
continue to work if you only use the virtual filesystem and ignore the
metatdata tooling, and vice versa.

# Learn More

For additional documentation, please read the following files:

- [Configuration](./docs/CONFIGURATION.md)
- [Browsing with the Virtual Filesystem](./docs/VIRTUAL_FILESYSTEM.md)
- [Managing Your Music Metadata](./docs/METADATA_MANAGEMENT.md)
- [Using Playlists & Collages](./docs/PLAYLISTS_COLLAGES.md)
- [Maintaining the Cache](./docs/CACHE_MAINTENANCE.md)
- [Architecture](./docs/ARCHITECTURE.md)

# License

```
Copyright 2023 blissful <blissful@sunsetglow.net>

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

   http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

# Contributions

Bug fixes are happily accepted!

However, please do not open a pull request for a new feature without prior
discussion.

Rosé is a pet project that I developed for personal use. Rosé is designed to
match my specific needs and constraints, and is never destined to be widely
adopted. Therefore, I will lean towards keeping the feature set focused and
small, and will not add too many features over the lifetime of the project.

Rosé is provided as-is: I may not maintain it in the future.
