# Available Commands

This document enumerates the commands available in Rosé's CLI.

First, a quick note on the structure: Rosé primarily organizes commands by the
resource they effect. Most commands are of the structure `rose {resource} {action}`.

- fs/ _(see [Browsing with the Virtual Filesystem](./VIRTUAL_FILESYSTEM.md))_
  - `fs mount`: Mount the virtual filesystem onto the configured `$fuse_mount_dir`.
  - `fs unmount`: Unmount the virtual filesystem by invoking `umount`.
- cache/ _(see [Maintaining the Cache](./CACHE_MAINTENANCE.md))_
  - `cache update`: Scan the source directory and update the read cache with
    any new metadata changes.
  - `cache watch`: Start a watcher that will trigger `cache update` for any
    files and directories that have been modified.
  - `cache unwatch`: Kill the running cache watcher process.
- config/ _(See [Configuration](./CONFIGURATION.md))_
  - `config generate-completion`: Print a shell completion script for Rosé to stdout.
  - `config preview-templates`: Preview your configured path templates with sample
    data.
- releases/ _(see [Managing Releases](./RELEASES.md))_
  - `releases print`: Print a single release's metadata in JSON.
  - `releases print-all`: Print all releases' metadata in JSON, with an
    optional matcher rule to filter out releases.
  - `releases import`: Import a release directory into the managed source
    directory.
  - `releases edit`: Edit a release's metadata as a text file in your
    `$EDITOR`.
  - `releases toggle-new`: Toggle the "new"-ness of a release.
  - `releases delete`: Remove a release from the library and move its source
    files to the trash bin.
  - `releases set-cover`: Set the cover art for a release. Replaces any
    existing cover art.
  - `releases delete-cover`: Set the cover art for a release. Replaces any
    existing cover art.
  - `releases extract-covers`: Extract embedded cover arts in all releases into
    external cover art files.
  - `releases run-rule`: Run one or more metadata actions on all tracks in the
    release.
  - `releases add-metadata-url`: Associate an external metadata URL to the release.
  - `releases search-metadata-urls`: Search for external metadata URLs to
    associate with the release.
  - `releases download-metadata`: Download metadata from associated URLs and
    suggest metadata improvements.
  - `releases create-single`: Create a "phony" single release from a track and
    copy the track into the new release.
- tracks/
  - `tracks print`: Print a single track's metadata in JSON.
  - `tracks print-all`: Print all tracks' metadata in JSON, with an optional
    matcher rule to filter out tracks.
  - `tracks run-rule`: Run one or more metadata actions on a track.
- collages/ _(see [Managing Playlists & Collages](./PLAYLISTS_COLLAGES.md))_
  - `collages print`: Print a single collage's metadata in JSON.
  - `collages print-all`: Print all collages' metadata in JSON.
  - `collages create`: Create a new collage.
  - `collages edit`: Edit the releases in a collage as a text file. Supports
    reordering and removing releases.
  - `collages delete`: Delete a collage. The collage's release list is moved to
    the trash bin.
  - `collages rename`: Rename a collage.
  - `collages add-release`: Add a release to a collage.
  - `collages remove-release`: Remove a release from a collage.
- playlists/ _(see [Managing Playlists & Collages](./PLAYLISTS_COLLAGES.md))_
  - `playlists print`: Print a single playlist's metadata in JSON.
  - `playlists print-all`: Print all playlists' metadata in JSON.
  - `playlists create`: Create a new playlist.
  - `playlists edit`: Edit the tracks in a playlist as a text file. Supports
    reordering and removing tracks.
  - `playlists delete`: Delete a playlist. The playlist's track list is moved to
    the trash bin.
  - `playlists rename`: Rename a playlist.
  - `playlists add-track`: Add a track to a playlist.
  - `playlists remove-track`: Remove a track from a playlist.
  - `playlists set-cover`: Set the cover art for a playlist. Replaces any existing
    cover art.
  - `playlists delete-cover`: Remove the cover art of a playlist.
- rules/ _(see [Improving Your Music Metadata](./METADATA_TOOLS.md))_
  - `rules run`: Run an ad hoc rule in the command line interface. You can also
    easily test rules with the `--dry-run` flag.
  - `rules run-stored`: Run the rules stored in the configuration file.
