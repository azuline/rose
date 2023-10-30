"""
The cli module defines Rose's CLI interface. It does not have any business logic of its own. It is
dedicated to parsing and delegating.

Note that we set multiprocessing's start method to "spawn" for the Virtual Filesystem and Watcher,
but not for other operations. In other operations, we want to have the performance of fork; however,
the Virtual Filesystem and Watcher run subthreads, which cannot fork off.
"""

import logging
import os
import signal
import subprocess
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
from rose.common import RoseError
from rose.config import Config
from rose.playlists import (
    add_track_to_playlist,
    create_playlist,
    delete_playlist,
    dump_playlists,
    edit_playlist_in_editor,
    remove_playlist_cover_art,
    remove_track_from_playlist,
    rename_playlist,
    set_playlist_cover_art,
)
from rose.releases import (
    delete_release,
    dump_releases,
    edit_release,
    remove_release_cover_art,
    set_release_cover_art,
    toggle_release_new,
)
from rose.virtualfs import VirtualPath, mount_virtualfs, unmount_virtualfs
from rose.watcher import start_watchdog

logger = logging.getLogger(__name__)


class DaemonAlreadyRunningError(RoseError):
    pass


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
        config=Config.parse(config_path_override=config),
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


# fmt: off
@cache.command()
@click.option("--foreground", "-f", is_flag=True, help="Run the filesystem watcher in the foreground (default: daemon).")  # noqa: E501
@click.pass_obj
# fmt: on
def watch(ctx: Context, foreground: bool) -> None:
    """Start a watchdog that will auto-refresh the cache on changes in music_source_dir."""
    if not foreground:
        daemonize(pid_path=ctx.config.watchdog_pid_path)

    start_watchdog(ctx.config)


@cache.command()
@click.pass_obj
def unwatch(ctx: Context) -> None:
    """Stop the running watchdog."""
    if not ctx.config.watchdog_pid_path.exists():
        logger.info("No-Op: No known watchdog running")
        exit(1)
    with ctx.config.watchdog_pid_path.open("r") as fp:
        pid = int(fp.read())
    logger.info(f"Killing watchdog at process {pid}")
    try:
        os.kill(pid, signal.SIGTERM)
    except ProcessLookupError:
        logger.info(f"No-Op: Process {pid} not found")
    ctx.config.watchdog_pid_path.unlink()


@cli.group()
def fs() -> None:
    """Manage the virtual library."""


# fmt: off
@fs.command()
@click.option("--foreground", "-f", is_flag=True, help="Run the FUSE controller in the foreground (default: daemon).")  # noqa: E501
@click.pass_obj
# fmt: on
def mount(ctx: Context, foreground: bool) -> None:
    """Mount the virtual library."""
    if not foreground:
        daemonize()

    # Trigger a cache refresh in the background when we first mount the filesystem.
    p = Process(target=update_cache, args=[ctx.config, False])
    debug = logging.getLogger().getEffectiveLevel() == logging.DEBUG
    try:
        p.start()
        mount_virtualfs(ctx.config, debug=debug)
    finally:
        p.join(timeout=1)
    return


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
    """
    Edit a release's metadata in $EDITOR. Accepts a release UUID, virtual directory name, or virtual
    filesystem path.
    """
    release = parse_release_from_potential_path(ctx.config, release)
    edit_release(ctx.config, release)


@releases.command()
@click.argument("release", type=str, nargs=1)
@click.pass_obj
def toggle_new(ctx: Context, release: str) -> None:
    """
    Toggle whether a release is new. Accepts a release UUID, virtual directory name, or virtual
    filesystem path.
    """
    release = parse_release_from_potential_path(ctx.config, release)
    toggle_release_new(ctx.config, release)


@releases.command()
@click.argument("release", type=str, nargs=1)
@click.argument("cover", type=click.Path(path_type=Path), nargs=1)
@click.pass_obj
def set_cover(ctx: Context, release: str, cover: Path) -> None:
    """
    Set the cover art of a release. For the release argument, accepts a release UUID, virtual
    directory name, or virtual filesystem path. For the cover argument, accept a path to the image.
    """
    release = parse_release_from_potential_path(ctx.config, release)
    set_release_cover_art(ctx.config, release, cover)


@releases.command()
@click.argument("release", type=str, nargs=1)
@click.pass_obj
def remove_cover(ctx: Context, release: str) -> None:
    """
    Remove the cover art of a release. For the release argument, accepts a release UUID, virtual
    directory name, or virtual filesystem path.
    """
    release = parse_release_from_potential_path(ctx.config, release)
    remove_release_cover_art(ctx.config, release)


@releases.command(name="delete")
@click.argument("release", type=str, nargs=1)
@click.pass_obj
def delete3(ctx: Context, release: str) -> None:
    """
    Delete a release. The release is moved to the trash bin, following the freedesktop spec. Accepts
    a release UUID, virtual directory name, or virtual filesystem path.
    """
    release = parse_release_from_potential_path(ctx.config, release)
    delete_release(ctx.config, release)


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
    Add a release to a collage. Accepts a release UUID, virtual directory name, or virtual
    filesystem path.
    """
    release = parse_release_from_potential_path(ctx.config, release)
    add_release_to_collage(ctx.config, collage, release)


@collages.command()
@click.argument("collage", type=str, nargs=1)
@click.argument("release", type=str, nargs=1)
@click.pass_obj
def remove_release(ctx: Context, collage: str, release: str) -> None:
    """
    Remove a release from a collage. Accepts a release UUID, virtual directory name, or virtual
    filesystem path.
    """
    release = parse_release_from_potential_path(ctx.config, release)
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


@playlists.command(name="set-cover")
@click.argument("playlist", type=str, nargs=1)
@click.argument("cover", type=click.Path(path_type=Path), nargs=1)
@click.pass_obj
def set_cover2(ctx: Context, playlist: str, cover: Path) -> None:
    """
    Set the cover art of a playlist. Accepts a playlist name and a path to an image.
    """
    set_playlist_cover_art(ctx.config, playlist, cover)


@playlists.command(name="remove-cover")
@click.argument("playlist", type=str, nargs=1)
@click.pass_obj
def remove_cover2(ctx: Context, playlist: str) -> None:
    """Remove the cover art of a playlist. Accepts a playlist name."""
    remove_playlist_cover_art(ctx.config, playlist)


@cli.command()
@click.argument("shell", type=click.Choice(["bash", "zsh", "fish"]), nargs=1)
def generate_shell_completion(shell: str) -> None:
    """Print a shell completion script."""
    os.environ["_ROSE_COMPLETE"] = f"{shell}_source"
    subprocess.run(["rose"], env=os.environ)


def parse_release_from_potential_path(c: Config, r: str) -> str:
    """
    Support paths from the virtual filesystem as valid releases. By default, we accept virtual
    directory names. This function expands that to accept paths.
    """
    p = Path(r).resolve()
    # Exit early if it's not even a real path lol.
    if not p.is_dir():
        return r
    # And also exit if it's not in the virtual filesystem lol.
    if not str(p).startswith(str(c.fuse_mount_dir)):
        return r

    # Parse the virtual path with the standard function.
    vpath = VirtualPath.parse(Path(str(p).removeprefix(str(c.fuse_mount_dir))))
    # If there is no release, or there is a file, abort lol.
    if not vpath.release or vpath.file:
        return r

    # Otherwise return the parsed release.
    return vpath.release


def daemonize(pid_path: Path | None = None) -> None:
    if pid_path and pid_path.exists():
        # Parse the PID. If it's not a valid integer, just skip and move on.
        try:
            with pid_path.open("r") as fp:
                existing_pid = int(fp.read())
        except ValueError:
            logger.debug(f"Ignoring improperly formatted pid file at {pid_path}")
            pass
        else:
            # Otherwise, Check to see if existing_pid is running. Kill 0 does nothing, but errors if
            # the process doesn't exist.
            try:
                os.kill(existing_pid, 0)
            except OSError:
                logger.debug(f"Ignoring pid file with a pid that isn't running: {existing_pid}")
            else:
                raise DaemonAlreadyRunningError(
                    f"Daemon is already running in process {existing_pid}"
                )

    pid = os.fork()
    if pid == 0:
        # Child process. Detach and keep going!
        os.setsid()
        return
    # Parent process, let's exit now!
    if pid_path:
        with pid_path.open("w") as fp:
            fp.write(str(pid))
    os._exit(0)
