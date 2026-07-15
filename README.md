# Rosé

Rosé is a music manager for Unix-based systems.

At its core, Rosé is a Python library that provides an opinionated set of functions for managing a music library. On top of that, there are two thin frontends: a CLI and a virtual filesystem. It is not difficult to add a new frontend, but I only have use for these two.

The two current frontends are meant to be combined with a file browser (like nnn) and a media player (like mpv) to form a complete music playing system.

Rosé is a personal pet project and is provided as-is. Bug fix PRs are welcome; new feature PRs are not.

# Requirements

Rosé has the following requirements:

- Music files are present on the computer in a single music directory.
- All releases are immediate children of the music directory root.
- All tracks are part of a release (single-release tracks are permitted).
- All files in the music directory can be safely written to.

> [!NOTE]
> Rosé modifies the managed audio files on the first scan. If you do not want to modify your audio files, for example because they are seeding in a bittorrent client, then you should not use Rosé.

# Design

Rosé is designed in response to my dislike of [beets](https://github.com/beetbox/beets). Key design decisions and features are:

- The audio tags are the single source of truth for metadata. Rosé's internal database is a readonly cache. This ensures that audio files can never desync from their metadata, because they are one and the same.
- Comprehensive yet opinionated tag semantics following What.CD/Redacted and RateYourMusic conventions.
- Support for playlists, collages, ratings, and other music player metadata, as Rosé is meant to be a complete state-keeping backend component of a music system.
- A metadata editing system that supports editing releases as text files and supports queries (rules) for bulk updates.

Rosé alone is not a full-featured music system, and _that's the point_. You should compose Rosé with other great tools to create the music system that works best for you. We recommend pairing Rosé with:

1. A file manager, such as [nnn](https://github.com/jarun/nnn), [mc](https://midnight-commander.org/), or [ranger](https://github.com/ranger/ranger).
2. A media player, such as [mpv](https://mpv.io/).

# Installation

Install Rosé with Nix Flakes. If you do not have Nix Flakes, you can install Nix Flakes with [this installer](https://github.com/DeterminateSystems/nix-installer).

Then, to install the latest release of Rosé, run:

```bash
$ nix profile install github:azuline/rose/release
```

> [!NOTE]
> The master branch tracks the unstable release, whose documentation may be more up-to-date than the latest release's documentation. You can view the latest release's documentation [here](https://github.com/azuline/rose/blob/release/README.md).

Most users should install the latest release version of Rosé. However, if you wish to install the latest unstable version of Rosé, you can do so with the command `nix profile install github:azuline/rose/master`.

# Quickstart

Let's now get Rosé up and running!

Once Rosé is installed, let's first confirm that `rose` exists and is accessible:

```bash
$ rose

Usage: rose [OPTIONS] COMMAND [ARGS]...

  A music manager with a virtual filesystem.

Options:
  -v, --verbose      Emit verbose logging.
  -c, --config PATH  Override the config file location.
  --help             Show this message and exit.

Commands:
  artists      Manage artists.
  cache        Manage the read cache.
  collages     Manage collages.
  config       Utilites for configuring Rosé.
  descriptors  Manage descriptors.
  fs           Manage the virtual filesystem.
  genres       Manage genres.
  labels       Manage labels.
  playlists    Manage playlists.
  releases     Manage releases.
  rules        Run metadata update rules on the entire library.
  tracks       Manage tracks.
  version      Print version.
```

> [!NOTE]
> This quickstart assumes you have a local "source directory" of music releases for Rosé to manage. Each music release must be an immediate child subdirectory of the "source directory."

Great! Next, we'll (1) configure Rosé, (2) mount the virtual filesystem, and finally (3) play music!

1. Rosé requires a configuration file. On Linux, the configuration file is located at `$XDG_CONFIG_HOME/rose/config.toml`, which is typically `~/.config/rose/.config.toml`. On MacOS, the configuration file is located at `~/Library/Preferences/rose/config.toml`.

   Only two configuration options are required:

   ```toml
   # The directory of music to manage.
   # WARNING: The files in this directory WILL be modified by Rosé!
   music_source_dir = "~/.music-source"
   # The mountpoint for the virtual filesystem.
   vfs.mount_dir = "~/music"
   ```

   The full configuration specification is documented in [Configuration](./docs/CONFIGURATION.md).

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

   Rosé emits log lines whenever something significant is occurring. This is expected! The log lines above come from the `rose fs mount` command indexing the `music_source_dir` at startup, in order to populate the read cache.

   The virtual filesystem uses the read cache to determine the available music and its metadata. It's possible for the cache to get out of sync from the source music files. If that happens, the `rose cache update` is guaranteed to resynchronize them. See [Maintaining the Cache](./docs/CACHE_MAINTENANCE.md) for additional documentation on cache updates and synchronization.

   Now that the virtual filesystem is mounted, let's go take a look! Navigate to the configured `vfs.mount_dir`, and you should see your music available in the virtual filesystem!

   ```bash
   $ cd $vfs_mount_dir

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
   'BLACKPINK - 2016. SQUARE ONE - Single'
   'BLACKPINK - 2016. SQUARE TWO - Single'
   'LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP'
   'LOOΠΔ - 2017. Kim Lip - Single [NEW]'
   'NewJeans - 2022. Ditto - Single'
   ```

3. Let's play some music! You should be able to open a music file in your music player of choice.

   Mine is `mpv`:

   ```bash
   $ mpv "1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP/04. LOOΠΔ ODD EYE CIRCLE - Chaotic.opus"
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

And that's it! If desired, you can unmount the virtual filesystem with the `rose fs unmount` command.

# Learn More

For additional documentation, please refer to the following files:

- [Configuration](./docs/CONFIGURATION.md)
- [Available Commands](./docs/AVAILABLE_COMMANDS.md)
- [Browsing the Virtual Filesystem](./docs/VIRTUAL_FILESYSTEM.md)
- [Managing Releases](./docs/RELEASES.md)
- [Managing Playlists & Collages](./docs/PLAYLISTS_COLLAGES.md)
- [Improving Your Music Metadata](./docs/METADATA_TOOLS.md)
- [Maintaining the Cache](./docs/CACHE_MAINTENANCE.md)
- [Directory & Filename Templates](./docs/TEMPLATES.md)
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
