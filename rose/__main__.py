import logging
from dataclasses import dataclass
from multiprocessing import Process
from pathlib import Path

import click

from rose.cache import migrate_database, update_cache
from rose.collages import (
    add_release_to_collage,
    create_collage,
    delete_collage,
    dump_collages,
    edit_collage_in_editor,
    remove_release_from_collage,
    rename_collage,
)
from rose.config import Config
from rose.playlists import (
    add_track_to_playlist,
    create_playlist,
    delete_playlist,
    dump_playlists,
    edit_playlist_in_editor,
    remove_track_from_playlist,
    rename_playlist,
)
from rose.releases import dump_releases, edit_release, toggle_release_new
from rose.virtualfs import mount_virtualfs, unmount_virtualfs
from rose.watcher import start_watchdog


@dataclass
class Context:
    config: Config


# fmt: off
@click.group()
@click.option("--verbose", "-v", is_flag=True, help="Emit verbose logging.")
@click.option("--config", "-c", type=click.Path(path_type=Path), help="Override the config file location.")  # noqa: E501
@click.pass_context
# fmt: on
def cli(cc: click.Context, verbose: bool, config: Path | None = None) -> None:
    """A virtual filesystem for music and metadata improvement tooling."""
    cc.obj = Context(
        config=Config.read(config_path_override=config),
    )

    if verbose:
        logging.getLogger().setLevel(logging.DEBUG)

    # Migrate the database on every command invocation.
    migrate_database(cc.obj.config)


@cli.group()
def cache() -> None:
    """Manage the read cache."""


# fmt: off
@cache.command()
@click.option("--force", "-f", is_flag=True, help="Force re-read all data from disk, even for unchanged files.")  # noqa: E501
@click.pass_obj
# fmt: on
def update(ctx: Context, force: bool) -> None:
    """Update the read cache from disk data."""
    update_cache(ctx.config, force)


@cache.command()
@click.pass_obj
def watch(ctx: Context) -> None:
    """Start a watchdog that will auto-refresh the cache on changes in music_source_dir."""
    start_watchdog(ctx.config)


@cli.group()
def fs() -> None:
    """Manage the virtual library."""


@fs.command(context_settings={"ignore_unknown_options": True})
@click.option("--foreground", "-f", is_flag=True, help="Foreground the FUSE controller.")
@click.pass_obj
def mount(ctx: Context, foreground: bool) -> None:
    """Mount the virtual library."""
    # Trigger a cache refresh in the background when we first mount the filesystem.
    p = Process(target=update_cache, args=[ctx.config, False])
    try:
        p.start()
        mount_virtualfs(ctx.config, foreground)
    finally:
        p.join(timeout=1)


@fs.command()
@click.pass_obj
def unmount(ctx: Context) -> None:
    """Unmount the virtual library."""
    unmount_virtualfs(ctx.config)


@cli.group()
def releases() -> None:
    """Manage releases."""


@releases.command(name="print")
@click.pass_obj
def print1(ctx: Context) -> None:
    """Print JSON-encoded releases."""
    print(dump_releases(ctx.config))


@releases.command(name="edit")
@click.argument("release", type=str, nargs=1)
@click.pass_obj
def edit2(ctx: Context, release: str) -> None:
    """Edit a release's metadata in $EDITOR."""
    edit_release(ctx.config, release)


@releases.command()
@click.argument("release", type=str, nargs=1)
@click.pass_obj
def toggle_new(ctx: Context, release: str) -> None:
    """
    Toggle whether a release is new. Accepts a release's UUID or virtual fs dirname (both are
    accepted).
    """
    toggle_release_new(ctx.config, release)


@cli.group()
def collages() -> None:
    """Manage collages."""


@collages.command()
@click.argument("name", type=str, nargs=1)
@click.pass_obj
def create(ctx: Context, name: str) -> None:
    """Create a new collage."""
    create_collage(ctx.config, name)


@collages.command()
@click.argument("old_name", type=str, nargs=1)
@click.argument("new_name", type=str, nargs=1)
@click.pass_obj
def rename(ctx: Context, old_name: str, new_name: str) -> None:
    """Rename a collage."""
    rename_collage(ctx.config, old_name, new_name)


@collages.command()
@click.argument("name", type=str, nargs=1)
@click.pass_obj
def delete(ctx: Context, name: str) -> None:
    """Delete a collage."""
    delete_collage(ctx.config, name)


@collages.command()
@click.argument("collage", type=str, nargs=1)
@click.argument("release", type=str, nargs=1)
@click.pass_obj
def add_release(ctx: Context, collage: str, release: str) -> None:
    """
    Add a release to a collage. Accepts a collage name and a release's UUID or virtual fs dirname
    (both are accepted).
    """
    add_release_to_collage(ctx.config, collage, release)


@collages.command()
@click.argument("collage", type=str, nargs=1)
@click.argument("release", type=str, nargs=1)
@click.pass_obj
def remove_release(ctx: Context, collage: str, release: str) -> None:
    """
    Remove a release from a collage. Accepts a collage name and a release's UUID or virtual fs
    dirname (both are accepted).
    """
    remove_release_from_collage(ctx.config, collage, release)


@collages.command()
@click.argument("collage", type=str, nargs=1)
@click.pass_obj
def edit(ctx: Context, collage: str) -> None:
    """
    Edit a collage in $EDITOR. Reorder lines to update the ordering of releases. Delete lines to
    delete releases from the collage.
    """
    edit_collage_in_editor(ctx.config, collage)


@collages.command(name="print")
@click.pass_obj
def print2(ctx: Context) -> None:
    """Print JSON-encoded collages."""
    print(dump_collages(ctx.config))


@cli.group()
def playlists() -> None:
    """Manage playlists."""


@playlists.command(name="create")
@click.argument("name", type=str, nargs=1)
@click.pass_obj
def create2(ctx: Context, name: str) -> None:
    """Create a new playlist."""
    create_playlist(ctx.config, name)


@playlists.command(name="rename")
@click.argument("old_name", type=str, nargs=1)
@click.argument("new_name", type=str, nargs=1)
@click.pass_obj
def rename2(ctx: Context, old_name: str, new_name: str) -> None:
    """Rename a playlist."""
    rename_playlist(ctx.config, old_name, new_name)


@playlists.command(name="delete")
@click.argument("name", type=str, nargs=1)
@click.pass_obj
def delete2(ctx: Context, name: str) -> None:
    """Delete a playlist."""
    delete_playlist(ctx.config, name)


@playlists.command()
@click.argument("playlist", type=str, nargs=1)
@click.argument("track", type=str, nargs=1)
@click.pass_obj
def add_track(ctx: Context, playlist: str, track: str) -> None:
    """Add a track to a playlist. Accepts a playlist name and a track's UUID."""
    add_track_to_playlist(ctx.config, playlist, track)


@playlists.command()
@click.argument("playlist", type=str, nargs=1)
@click.argument("track", type=str, nargs=1)
@click.pass_obj
def remove_track(ctx: Context, playlist: str, track: str) -> None:
    """Remove a track from a playlist. Accepts a playlist name and a track's UUID."""
    remove_track_from_playlist(ctx.config, playlist, track)


@playlists.command(name="edit")
@click.argument("playlist", type=str, nargs=1)
@click.pass_obj
def edit3(ctx: Context, playlist: str) -> None:
    """
    Edit a playlist in $EDITOR. Reorder lines to update the ordering of tracks. Delete lines to
    delete tracks from the playlist.
    """
    edit_playlist_in_editor(ctx.config, playlist)


@playlists.command(name="print")
@click.pass_obj
def print3(ctx: Context) -> None:
    """Print JSON-encoded playlists."""
    print(dump_playlists(ctx.config))


if __name__ == "__main__":
    cli()
