# Rosé

> [!IMPORTANT]
> Rosé is under active development. Not all listed features exist yet. See
> [Milestone v0.4.0](https://github.com/azuline/rose/milestone/1) for progress
> updates.

Rosé is a music manager for Unix-based systems. Rosé provides a virtual FUSE
filesystem for managing your music library and various functions for editing
and improving your music library's metadata and tags.

Rosé manages a _source directory_ of music releases. Given the following source
directory:

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
└── NewJeans - 2022. Ditto
    ├── 01. Ditto.opus
    └── cover.jpg
```

Rosé produces the following virtual filesystem (duplicate information has been
omitted).

```
virtual/
├── 1. Releases/
│   ├── BLACKPINK - 2016. SQUARE ONE - Single [Big Room House;Dance-Pop;K-Pop]/
│   │   ├── 01. BLACKPINK - WHISTLE.opus
│   │   ├── 02. BLACKPINK - BOOMBAYAH.opus
│   │   └── cover.jpg
│   ├── BLACKPINK - 2016. SQUARE TWO - Single [Dance-Pop;K-Pop]/...
│   ├── LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/...
│   ├── NewJeans - 2022. Ditto - Single [Contemporary R&B;K-Pop]/...
│   └── {NEW} LOOΠΔ - 2017. Kim Lip - Single [Contemporary R&B;Dance-Pop;K-Pop]/...
├── 2. Releases - New/
│   └── {NEW} LOOΠΔ - 2017. Kim Lip - Single [Contemporary R&B;Dance-Pop;K-Pop]/...
├── 3. Releases - Recently Added/
│   ├── [2023-10-25] LOOΠΔ - 2017. Kim Lip - Single [Contemporary R&B;Dance-Pop;K-Pop]/...
│   ├── [2023-10-01] LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/...
│   └── [2023-02-28] NewJeans - 2022. Ditto - Single [Contemporary R&B;K-Pop]/...
│   ├── [2022-08-22] BLACKPINK - 2016. SQUARE TWO - Single [Dance-Pop;K-Pop]/...
│   ├── [2022-08-10] BLACKPINK - 2016. SQUARE ONE - Single [Big Room House;Dance-Pop;K-Pop]/...
├── 4. Artists/
│   ├── BLACKPINK/
│   │   ├── BLACKPINK - 2016. SQUARE ONE - Single [Big Room House;Dance-Pop;K-Pop]/...
│   │   └── BLACKPINK - 2016. SQUARE TWO - Single [Dance-Pop;K-Pop]/...
│   ├── LOOΠΔ/
│   │   ├── {NEW} LOOΠΔ - 2017. Kim Lip - Single [Contemporary R&B;Dance-Pop;K-Pop]/...
│   │   └── LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/...
│   ├── LOOΠΔ ODD EYE CIRCLE/
│   │   └── LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/...
│   └── NewJeans/
│       └── NewJeans - 2022. Ditto - Single [Contemporary R&B;K-Pop]/...
├── 5. Genres/
│   ├── Big Room House/
│   │   └── BLACKPINK - 2016. SQUARE ONE - Single [Big Room House;Dance-Pop;K-Pop]/...
│   ├── Contemporary R&B/
│   │   ├── NewJeans - 2022. Ditto - Single [Contemporary R&B;K-Pop]/...
│   │   └── {NEW} LOOΠΔ - 2017. Kim Lip - Single [Contemporary R&B;Dance-Pop;K-Pop]/...
│   ├── Future Bass/
│   │   └── LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/...
│   ├── Dance-Pop/
│   │   ├── BLACKPINK - 2016. SQUARE ONE - Single [Big Room House;Dance-Pop;K-Pop]/...
│   │   ├── BLACKPINK - 2016. SQUARE TWO - Single [Dance-Pop;K-Pop]/...
│   │   ├── LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/...
│   │   └── {NEW} LOOΠΔ - 2017. Kim Lip - Single [Contemporary R&B;Dance-Pop;K-Pop]/...
│   └── K-Pop/
│       ├── BLACKPINK - 2016. SQUARE ONE - Single [Big Room House;Dance-Pop;K-Pop]/...
│       ├── BLACKPINK - 2016. SQUARE TWO - Single [Dance-Pop;K-Pop]/...
│       ├── LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/...
│       └── {NEW} LOOΠΔ - 2017. Kim Lip - Single [Contemporary R&B;Dance-Pop;K-Pop]/...
├── 6. Labels/
│   ├── ADOR/
│   │   └── NewJeans - 2022. Ditto - Single [Contemporary R&B;K-Pop]/...
│   ├── BlockBerry Creative/
│   │   ├── LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/...
│   │   └── {NEW} LOOΠΔ - 2017. Kim Lip - Single [Contemporary R&B;Dance-Pop;K-Pop]/...
│   └── YG Entertainment/
│       ├── BLACKPINK - 2016. SQUARE ONE - Single [Big Room House;Dance-Pop;K-Pop]/...
│       └── BLACKPINK - 2016. SQUARE TWO - Single [Dance-Pop;K-Pop]/...
├── 7. Collages/
│   └── Road Trip/
│       ├── 1. BLACKPINK - 2016. SQUARE TWO - Single [Dance-Pop;K-Pop]/...
│       └── 2. LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/...
└── 8. Playlists/
    └── Shower/
        ├── 1. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus
        ├── 2. NewJeans - Ditto.opus
        ├── 3. BLACKPINK - PLAYING WITH FIRE.opus
        └── 4. LOOΠΔ - Eclipse.opus
```

In addition to a flat directory of all releases, Rosé creates directories based
on Date Added, Artist, Genre, and Label. Rosé also provides a few other
concepts for organizing your music library:

- **Collages:** Collections of releases.
- **Playlists:** Collections of tracks.
- **New release tracking:** Track new unlistened additions to the library.

Rosé's virtual filesystem organizes your music library by the metadata in the
music tags. The quality of the virtual filesystem depends on the quality of the
tags.

Thus, Rosé also provides the following functions to improv the tags of your
music library:

1. A text-based interface for manually modifying metadata,
2. A rules engine for bulk updating metadata,
3. And metadata importing from third-party sources.

The rules engine allows you to pattern match tracks in your music library and
apply tag changes to them. For example:

```bash
$ rose rules run 'trackartist,albumartist:^CHUU$' 'replace:Chuu'

CHUU - 2023. Howl/01. Howl.opus
      trackartist[main]: ['CHUU'] -> ['Chuu']
      albumartist[main]: ['CHUU'] -> ['Chuu']
CHUU - 2023. Howl/02. Underwater.opus
      trackartist[main]: ['CHUU'] -> ['Chuu']
      albumartist[main]: ['CHUU'] -> ['Chuu']
CHUU - 2023. Howl/03. My Palace.opus
      trackartist[main]: ['CHUU'] -> ['Chuu']
      albumartist[main]: ['CHUU'] -> ['Chuu']
CHUU - 2023. Howl/04. Aliens.opus
      trackartist[main]: ['CHUU'] -> ['Chuu']
      albumartist[main]: ['CHUU'] -> ['Chuu']
CHUU - 2023. Howl/05. Hitchhiker.opus
      trackartist[main]: ['CHUU'] -> ['Chuu']
      albumartist[main]: ['CHUU'] -> ['Chuu']
```

_Demo Video TBD_

# Features

Rosé allows you to interact with and script against your music library through
a virtual filesystem and through a CLI. A concise list of the features provided
by the two interfaces is:

- Filter your music by artist, genre, label, and "new"-ness
- Create collages of releases and playlists of tracks
- Group artist aliases together under a primary artist
- Flag and unflag release "new"-ness
- Edit release metadata as a text file
- Run and store rules for (bulk) updating metadata
- Import metadata and cover art from third-party sources
- Extract embedded cover art to an external file
- Create "phony" single releases from any individual track
- Support for `.mp3`, `.m4a`, `.ogg` (vorbis), `.opus`, and `.flac` files
- Support for multiple artist, label, and genre tags.

> [!NOTE]
> Rosé modifies the managed audio files, even on first scan. If you do not want
> to modify your audio files, for example because they are seeding in a
> bittorrent client, you should not use Rosé.

# Is Rosé For You?

Rosé expects users to be comfortable with the shell. Rosé's documentation and
user interface assumes that the reader is familiar with software.

Rosé does not provide a complete music system. The user is expected to
compose their own system, with Rosé as one of the pieces.

Rosé is designed for large music libraries. Smaller libraries do not require
the power that Rosé offers.

Rosé expects all tracks to be part of a release. Rosé also expects that each
release is an immediate subdirectory of the source directory. Rosé will not
work with libraries that are collections of unorganized tracks.

Rosé modifies the files that it manages, as early as the first scan (where it
writes `roseid` tags). Rosé does not maintain a separate database; all changes
are directly applied to the managed files. This is incompatible with files
seeded as torrents.

# Installation

Install Rosé with Nix Flakes. If you do not have Nix Flakes, you can install
Nix Flakes with [this installer](https://github.com/DeterminateSystems/nix-installer).

Then, to install the latest release of Rosé, run:

```bash
$ nix profile install github:azuline/rose/release
```

> [!NOTE]
> The master branch tracks the unstable release, whose documentation may be
> more up-to-date than the latest release's documentation. You can view the
> latest release's documentation [here](https://github.com/azuline/rose/blob/release/README.md).

Most users should install the latest release version of Rosé. However, if you
wish to install the latest unstable version of Rosé, you can do so with the
command `nix profile install github:azuline/rose/master`.

# Quickstart

Let's now get Rosé up and running!

Once Rosé is installed, let's first confirm that `rose` exists and is
accessible:

```bash
$ rose

Usage: rose [OPTIONS] COMMAND [ARGS]...

  A music manager with a virtual filesystem.

Options:
  -v, --verbose      Emit verbose logging.
  -c, --config PATH  Override the config file location.
  --help             Show this message and exit.

Commands:
  cache           Manage the read cache
  collages        Manage collages
  fs              Manage the virtual filesystem
  gen-completion  Generate a shell completion script
  playlists       Manage playlists
  releases        Manage releases
  tracks          Manage tracks
  reload          Reload the configuration of active Rosé processes
  rules           Run metadata update rules on the entire library
```

> [!NOTE]
> This quickstart assumes you have a local "source directory" of music releases
> for Rosé to manage. Each music release must be an immediate child
> subdirectory of the "source directory."

Great! Next, we'll (1) configure Rosé, (2) mount the virtual filesystem, and
finally (3) play music!

1. Rosé requires a configuration file. On Linux, the configuration file is
   located at `$XDG_CONFIG_HOME/rose/config.toml`, which is typically
   `~/.config/rose/.config.toml`. On MacOS, the configuration file is located
   at `~/Library/Preferences/rose/config.toml`.

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
   [15:41:13] INFO: Updating cache for release BLACKPINK - 2016. SQUARE TWO
   [15:41:13] INFO: Updating cache for release BLACKPINK - 2016. SQUARE ONE
   [15:41:13] INFO: Updating cache for release LOOΠΔ - 2017. Kim Lip
   [15:41:13] INFO: Updating cache for release NewJeans - 2022. Ditto
   [15:41:13] INFO: Updating cache for release LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match
   [15:41:13] INFO: Updating cache for collage Road Trip
   [15:41:13] INFO: Updating cache for playlist Shower
   ```

   Rosé emits log lines whenever something significant is occurring. This is
   expected! The log lines above come from the `rose fs mount` command indexing
   the `music_source_dir` at startup, in order to populate the read cache.

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
   'BLACKPINK - 2016. SQUARE ONE - Single [Big Room House;Dance-Pop;K-Pop]'
   'BLACKPINK - 2016. SQUARE TWO - Single [Dance-Pop;K-Pop]'
   'LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]'
   'NewJeans - 2022. Ditto - Single [Contemporary R&B;K-Pop]'
   '{NEW} LOOΠΔ - 2017. Kim Lip - Single [Contemporary R&B;Dance-Pop;K-Pop]'
   ```

3. Let's play some music! You should be able to open a music file in your music
   player of choice.

   Mine is `mpv`:

   ```bash
   $ mpv "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]/04. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus"
    (+) Audio --aid=1 'Chaotic' (opus 2ch 48000Hz)
   File tags:
    Artist: LOOΠΔ ODD EYE CIRCLE
    Album: Mix & Match
    Album_Artist: LOOΠΔ ODD EYE CIRCLE
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

We recommend pairing Rosé with:

1. A file manager, such as [nnn](https://github.com/jarun/nnn),
   [mc](https://midnight-commander.org/), or [ranger](https://github.com/ranger/ranger).
2. A media player, such as [mpv](https://mpv.io/).

You also need not use the complete feature set of Rosé. Everything will
continue to work if you only use the virtual filesystem and ignore the
metatdata tooling, and vice versa.

Rosé's CLI is also designed to make scripting against your library easy.
Operations such as "edit release" and "jump to artist" can be expressed as a
bash one-liner and integrated into your file manager.

See [Shell Scripting](./docs/SHELL_SCRIPTING.md) for additional documentation
on scripting with Rosé.

# Learn More

For additional documentation, please refer to the following files:

- [Configuration](./docs/CONFIGURATION.md)
- [Available Commands](./docs/AVAILABLE_COMMANDS.md)
- [Browsing the Virtual Filesystem](./docs/VIRTUAL_FILESYSTEM.md)
- [Managing Releases](./docs/RELEASES.md)
- [Managing Playlists & Collages](./docs/PLAYLISTS_COLLAGES.md)
- [Managing Your Music Metadata](./docs/METADATA_MANAGEMENT.md)
- [Maintaining the Cache](./docs/CACHE_MAINTENANCE.md)
- [Shell Scripting](./docs/SHELL_SCRIPTING.md)
- [Tagging Conventions](./docs/TAGGING_CONVENTIONS.md)
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
discussion. Rosé is a pet project that I developed for personal use. Rosé is
designed to match my specific needs and constraints, and is never destined to
be widely adopted. Therefore, the feature set will remain focused and small.

Rosé is provided as-is, really!
