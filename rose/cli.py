"""
The cli module defines Rose's CLI interface. It does not have any domain logic of its own. It is
dedicated to parsing, resolving arguments, and delegating to the appropriate module.
"""

import contextlib
import logging
import os
import signal
import subprocess
from dataclasses import dataclass
from multiprocessing import Process
from pathlib import Path

import click

from rose.common import RoseExpectedError
from rose.config import Config

logger = logging.getLogger(__name__)


class InvalidReleaseArgError(RoseExpectedError):
    pass


class InvalidTrackArgError(RoseExpectedError):
    pass


class DaemonAlreadyRunningError(RoseExpectedError):
    pass


@dataclass
class Context:
    config: Config


# fmt: off
@click.group()
@click.option("--verbose", "-v", is_flag=True, help="Emit verbose logging.")
@click.option("--config", "-c", type=click.Path(path_type=Path), help="Override the config file location.")  
@click.pass_context
# fmt: on
def cli(cc: click.Context, verbose: bool, config: Path | None = None) -> None:
    """A music manager with a virtual filesystem."""
    from rose.cache import maybe_invalidate_cache_database

    cc.obj = Context(
        config=Config.parse(config_path_override=config),
    )
    if verbose:
        logging.getLogger().setLevel(logging.DEBUG)
    maybe_invalidate_cache_database(cc.obj.config)


@cli.group()
def config() -> None:
    """Utilites for configuring Rosé."""


@config.command()
@click.argument("shell", type=click.Choice(["bash", "zsh", "fish"]), nargs=1)
def generate_completion(shell: str) -> None:
    """Generate a shell completion script."""
    os.environ["_ROSE_COMPLETE"] = f"{shell}_source"
    subprocess.run(["rose"], env=os.environ)


@config.command()
@click.pass_obj
def preview_templates(ctx: Context) -> None:
    """Preview the configured path templates with sample data."""
    from rose.templates import preview_path_templates
    preview_path_templates(ctx.config)


@cli.group()
def cache() -> None:
    """Manage the read cache."""


# fmt: off
@cache.command()
@click.option("--force", "-f", is_flag=True, help="Force re-read all data from disk, even for unchanged files.")  
@click.pass_obj
# fmt: on
def update(ctx: Context, force: bool) -> None:
    """Synchronize the read cache with new changes in the source directory."""
    from rose.cache import update_cache
    update_cache(ctx.config, force)


# fmt: off
@cache.command()
@click.option("--foreground", "-f", is_flag=True, help="Run the filesystem watcher in the foreground (default: daemon).")  
@click.pass_obj
# fmt: on
def watch(ctx: Context, foreground: bool) -> None:
    """Start a watchdog to auto-update the cache when the source directory changes."""
    from rose.watcher import start_watchdog
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
    try:
        os.kill(pid, signal.SIGTERM)
        logger.info(f"Killed watchdog at process {pid}")
    except ProcessLookupError:
        logger.info(f"No-Op: Process {pid} not found")
    ctx.config.watchdog_pid_path.unlink()


@cli.group()
def fs() -> None:
    """Manage the virtual filesystem."""


# fmt: off
@fs.command()
@click.option("--foreground", "-f", is_flag=True, help="Run the FUSE controller in the foreground (default: daemon).")  
@click.pass_obj
# fmt: on
def mount(ctx: Context, foreground: bool) -> None:
    """Mount the virtual filesystem."""
    from rose.cache import update_cache
    from rose.virtualfs import mount_virtualfs

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
    """Unmount the virtual filesystem."""
    from rose.virtualfs import unmount_virtualfs
    unmount_virtualfs(ctx.config)


@cli.group()
def releases() -> None:
    """Manage releases."""
    # TODO: extract-covers / add-metadata-url / search-metadata-urls / import


@releases.command(name="print")
@click.argument("release", type=click.Path(), nargs=1)
@click.pass_obj
def print1(ctx: Context, release: str) -> None:
    """Print a single release (in JSON). Accepts a release's UUID/path."""
    from rose.releases import dump_release
    release = parse_release_argument(release)
    click.echo(dump_release(ctx.config, release))


@releases.command(name="print-all")
@click.argument("matcher", type=str, nargs=1, required=False)
@click.pass_obj
def print_all(ctx: Context, matcher: str | None) -> None:
    """Print all releases (in JSON). Accepts an optional rules matcher to filter the releases."""
    from rose.releases import dump_releases
    from rose.rule_parser import MetadataMatcher
    parsed_matcher = MetadataMatcher.parse(matcher) if matcher else None
    click.echo(dump_releases(ctx.config, parsed_matcher))


@releases.command(name="edit")
# fmt: off
@click.argument("release", type=click.Path(), nargs=1)
@click.option("--resume", "-r", type=click.Path(path_type=Path), nargs=1, help="Resume a failed release edit.")
# fmt: on
@click.pass_obj
def edit2(ctx: Context, release: str, resume: Path | None) -> None:
    """Edit a release's metadata in $EDITOR. Accepts a release's UUID/path."""
    from rose.releases import edit_release
    release = parse_release_argument(release)
    edit_release(ctx.config, release, resume_file=resume)


@releases.command()
@click.argument("release", type=click.Path(), nargs=1)
@click.pass_obj
def toggle_new(ctx: Context, release: str) -> None:
    """Toggle a release's "new"-ness. Accepts a release's UUID/path."""
    from rose.releases import toggle_release_new
    release = parse_release_argument(release)
    toggle_release_new(ctx.config, release)


@releases.command(name="delete")
@click.argument("release", type=click.Path(), nargs=1)
@click.pass_obj
def delete3(ctx: Context, release: str) -> None:
    """
    Delete a release from the library. The release is moved to the trash bin, following the
    freedesktop spec. Accepts a release's UUID/path.
    """
    from rose.releases import delete_release
    release = parse_release_argument(release)
    delete_release(ctx.config, release)


@releases.command()
@click.argument("release", type=click.Path(), nargs=1)
@click.argument("cover", type=click.Path(path_type=Path), nargs=1)
@click.pass_obj
def set_cover(ctx: Context, release: str, cover: Path) -> None:
    """Set/replace the cover art of a release. Accepts a release's UUID/path."""
    from rose.releases import set_release_cover_art
    release = parse_release_argument(release)
    set_release_cover_art(ctx.config, release, cover)


@releases.command()
@click.argument("release", type=click.Path(), nargs=1)
@click.pass_obj
def delete_cover(ctx: Context, release: str) -> None:
    """Delete the cover art of a release."""
    from rose.releases import delete_release_cover_art
    release = parse_release_argument(release)
    delete_release_cover_art(ctx.config, release)


@releases.command()
# fmt: off
@click.argument("release", type=click.Path(), nargs=1)
@click.argument("actions", type=str, nargs=-1)
@click.option("--dry-run", "-d", is_flag=True, help="Display intended changes without applying them.") 
@click.option("--yes", "-y", is_flag=True, help="Bypass confirmation prompts.")
# fmt: on
@click.pass_obj
def run_rule(ctx: Context, release: str, actions: list[str], dry_run: bool, yes: bool) -> None:
    """Run rule engine actions on all tracks in a release. Accepts a release's UUID/path."""
    from rose.releases import run_actions_on_release
    from rose.rule_parser import MetadataAction
    release = parse_release_argument(release)
    parsed_actions = [MetadataAction.parse(a) for a in actions]
    run_actions_on_release(
        ctx.config,
        release,
        parsed_actions,
        dry_run=dry_run,
        confirm_yes=not yes,
    )


@releases.command()
@click.argument("track_path", type=click.Path(path_type=Path), nargs=1)
@click.pass_obj
def create_single(ctx: Context, track_path: Path) -> None:
    """
    Create a single release for the given track, and copy the track into it. Only accepts a track
    path.
    """
    from rose.releases import create_single_release
    create_single_release(ctx.config, track_path)


@cli.group()
def tracks() -> None:
    """Manage tracks."""


@tracks.command(name="print")
@click.argument("track", type=click.Path(), nargs=1)
@click.pass_obj
def print4(ctx: Context, track: str) -> None:
    """Print a single track (in JSON). Accepts a tracks's UUID/path."""
    from rose.tracks import dump_track
    track = parse_track_argument(track)
    click.echo(dump_track(ctx.config, track))


@tracks.command(name="print-all")
@click.argument("matcher", type=str, nargs=1, required=False)
@click.pass_obj
def print_all3(ctx: Context, matcher: str | None = None) -> None:
    """Print all tracks (in JSON). Accepts an optional rules matcher to filter the tracks."""
    from rose.rule_parser import MetadataMatcher
    from rose.tracks import dump_tracks
    parsed_matcher = MetadataMatcher.parse(matcher) if matcher else None
    click.echo(dump_tracks(ctx.config, parsed_matcher))


@tracks.command(name="run-rule")
# fmt: off
@click.argument("track", type=click.Path(), nargs=1)
@click.argument("actions", type=str, nargs=-1)
@click.option("--dry-run", "-d", is_flag=True, help="Display intended changes without applying them.") 
@click.option("--yes", "-y", is_flag=True, help="Bypass confirmation prompts.")
# fmt: on
@click.pass_obj
def run_rule2(ctx: Context, track: str, actions: list[str], dry_run: bool, yes: bool) -> None:
    """Run rule engine actions on a single track. Accepts a track's UUID/path."""
    from rose.rule_parser import MetadataAction
    from rose.tracks import run_actions_on_track
    track = parse_track_argument(track)
    parsed_actions = [MetadataAction.parse(a) for a in actions]
    run_actions_on_track(
        ctx.config,
        track,
        parsed_actions,
        dry_run=dry_run,
        confirm_yes=not yes,
    )


@cli.group()
def collages() -> None:
    """Manage collages."""


@collages.command()
@click.argument("name", type=str, nargs=1)
@click.pass_obj
def create(ctx: Context, name: str) -> None:
    """Create a new collage."""
    from rose.collages import create_collage
    create_collage(ctx.config, name)


@collages.command()
@click.argument("old_name", type=str, nargs=1)
@click.argument("new_name", type=str, nargs=1)
@click.pass_obj
def rename(ctx: Context, old_name: str, new_name: str) -> None:
    """Rename a collage."""
    from rose.collages import rename_collage
    rename_collage(ctx.config, old_name, new_name)


@collages.command()
@click.argument("collage", type=str, nargs=1)
@click.pass_obj
def delete(ctx: Context, collage: str) -> None:
    """Delete a collage."""
    from rose.collages import delete_collage
    delete_collage(ctx.config, collage)


@collages.command()
@click.argument("collage", type=str, nargs=1)
@click.argument("release", type=str, nargs=1)
@click.pass_obj
def add_release(ctx: Context, collage: str, release: str) -> None:
    """Add a release to a collage. Accepts a collage's name and a release's UUID/path."""
    from rose.collages import add_release_to_collage
    release = parse_release_argument(release)
    add_release_to_collage(ctx.config, collage, release)


@collages.command()
@click.argument("collage", type=str, nargs=1)
@click.argument("release", type=str, nargs=1)
@click.pass_obj
def remove_release(ctx: Context, collage: str, release: str) -> None:
    """Remove a release from a collage. Accepts a collage's name and a release's UUID/path."""
    from rose.collages import remove_release_from_collage
    release = parse_release_argument(release)
    remove_release_from_collage(ctx.config, collage, release)


@collages.command()
@click.argument("collage", type=str, nargs=1)
@click.pass_obj
def edit(ctx: Context, collage: str) -> None:
    """Edit (reorder/remove releases from) a collage in $EDITOR. Accepts a collage's name."""
    from rose.collages import edit_collage_in_editor
    edit_collage_in_editor(ctx.config, collage)


@collages.command(name="print")
@click.argument("collage", type=str, nargs=1)
@click.pass_obj
def print2(ctx: Context, collage: str) -> None:
    """Print a collage (in JSON). Accepts a collage's name."""
    from rose.collages import dump_collage
    click.echo(dump_collage(ctx.config, collage))


@collages.command(name="print-all")
@click.pass_obj
def print_all1(ctx: Context) -> None:
    """Print all collages (in JSON)."""
    from rose.collages import dump_collages
    click.echo(dump_collages(ctx.config))


@cli.group()
def playlists() -> None:
    """Manage playlists."""


@playlists.command(name="create")
@click.argument("name", type=str, nargs=1)
@click.pass_obj
def create2(ctx: Context, name: str) -> None:
    """Create a new playlist."""
    from rose.playlists import create_playlist
    create_playlist(ctx.config, name)


@playlists.command(name="rename")
@click.argument("old_name", type=str, nargs=1)
@click.argument("new_name", type=str, nargs=1)
@click.pass_obj
def rename2(ctx: Context, old_name: str, new_name: str) -> None:
    """Rename a playlist. Accepts a playlist's name."""
    from rose.playlists import rename_playlist
    rename_playlist(ctx.config, old_name, new_name)


@playlists.command(name="delete")
@click.argument("playlist", type=str, nargs=1)
@click.pass_obj
def delete2(ctx: Context, playlist: str) -> None:
    """Delete a playlist. Accepts a playlist's name."""
    from rose.playlists import delete_playlist
    delete_playlist(ctx.config, playlist)


@playlists.command()
@click.argument("playlist", type=str, nargs=1)
@click.argument("track", type=str, nargs=1)
@click.pass_obj
def add_track(ctx: Context, playlist: str, track: str) -> None:
    """Add a track to a playlist. Accepts a playlist name and a track's UUID/path."""
    from rose.playlists import add_track_to_playlist
    track = parse_track_argument(track)
    add_track_to_playlist(ctx.config, playlist, track)


@playlists.command()
@click.argument("playlist", type=str, nargs=1)
@click.argument("track", type=str, nargs=1)
@click.pass_obj
def remove_track(ctx: Context, playlist: str, track: str) -> None:
    """Remove a track from a playlist. Accepts a playlist name and a track's UUID/path."""
    from rose.playlists import remove_track_from_playlist
    track = parse_track_argument(track)
    remove_track_from_playlist(ctx.config, playlist, track)


@playlists.command(name="edit")
@click.argument("playlist", type=str, nargs=1)
@click.pass_obj
def edit3(ctx: Context, playlist: str) -> None:
    """
    Edit a playlist in $EDITOR. Reorder lines to update the ordering of tracks. Delete lines to
    delete tracks from the playlist.
    """
    from rose.playlists import edit_playlist_in_editor
    edit_playlist_in_editor(ctx.config, playlist)


@playlists.command(name="print")
@click.argument("playlist", type=str, nargs=1)
@click.pass_obj
def print3(ctx: Context, playlist: str) -> None:
    """Print a playlist (in JSON). Accepts a playlist's name."""
    from rose.playlists import dump_playlist
    click.echo(dump_playlist(ctx.config, playlist))


@playlists.command(name="print-all")
@click.pass_obj
def print_all2(ctx: Context) -> None:
    """Print all playlists (in JSON)."""
    from rose.playlists import dump_playlists
    click.echo(dump_playlists(ctx.config))


@playlists.command(name="set-cover")
@click.argument("playlist", type=str, nargs=1)
@click.argument("cover", type=click.Path(path_type=Path), nargs=1)
@click.pass_obj
def set_cover2(ctx: Context, playlist: str, cover: Path) -> None:
    """
    Set the cover art of a playlist. Accepts a playlist name and a path to an image.
    """
    from rose.playlists import set_playlist_cover_art
    set_playlist_cover_art(ctx.config, playlist, cover)


@playlists.command(name="delete-cover")
@click.argument("playlist", type=str, nargs=1)
@click.pass_obj
def delete_cover2(ctx: Context, playlist: str) -> None:
    """Delete the cover art of a playlist. Accepts a playlist name."""
    from rose.playlists import delete_playlist_cover_art
    delete_playlist_cover_art(ctx.config, playlist)


@cli.group()
def rules() -> None:
    """Run metadata update rules on the entire library."""


@rules.command()
# fmt: off
@click.argument("matcher", type=str, nargs=1)
@click.argument("actions", type=str, nargs=-1)
@click.option("--dry-run", "-d", is_flag=True, help="Display intended changes without applying them.") 
@click.option("--yes", "-y", is_flag=True, help="Bypass confirmation prompts.")
# fmt: on
@click.pass_obj
def run(ctx: Context, matcher: str, actions: list[str], dry_run: bool, yes: bool) -> None:
    """Run an ad hoc rule."""
    from rose.rule_parser import MetadataRule
    from rose.rules import execute_metadata_rule
    if not actions:
        logger.info("No-Op: No actions passed")
        return
    rule = MetadataRule.parse(matcher, actions)
    execute_metadata_rule(ctx.config, rule, dry_run=dry_run, confirm_yes=not yes)


@rules.command()
# fmt: off
@click.option("--dry-run", "-d", is_flag=True, help="Display intended changes without applying them.")
@click.option("--yes", "-y", is_flag=True, help="Bypass confirmation prompts.")
# fmt: on
@click.pass_obj
def run_stored(ctx: Context, dry_run: bool, yes: bool) -> None:
    """Run the rules stored in the config."""
    from rose.rules import execute_stored_metadata_rules
    execute_stored_metadata_rules(ctx.config, dry_run=dry_run, confirm_yes=not yes)


def parse_release_argument(r: str) -> str:
    """Takes in a release argument and normalizes it to the release ID."""
    from rose.cache import STORED_DATA_FILE_REGEX
    from rose.common import valid_uuid
    if valid_uuid(r):
        logger.debug(f"Treating release argument {r} as UUID")
        return r
    # We treat cases (2) and (3) the same way: look for a .rose.{uuid}.toml file. Parse the release
    # ID from that file.
    with contextlib.suppress(FileNotFoundError, NotADirectoryError):
        p = Path(r).resolve()
        for f in p.iterdir():
            if m := STORED_DATA_FILE_REGEX.match(f.name):
                logger.debug(f"Parsed release ID {m[1]} from release argument {r}")
                return m[1]
    raise InvalidReleaseArgError(
        f"""\
{r} is not a valid release argument.

Release arguments must be one of:

  1. The release UUID
  2. The path of the source directory of a release
  3. The path of the release in the virtual filesystem (from any view)

{r} is not recognized as any of the above.
"""
    )


def parse_track_argument(t: str) -> str:
    """Takes in a track argument and normalizes it to the track ID."""
    from rose.audiotags import AudioTags, UnsupportedFiletypeError
    from rose.common import valid_uuid
    if valid_uuid(t):
        logger.debug(f"Treating track argument {t} as UUID")
        return t
    # We treat cases (2) and (3) the same way: crack the track file open and parse the ID from the
    # tags.
    with contextlib.suppress(FileNotFoundError, UnsupportedFiletypeError):
        af = AudioTags.from_file(Path(t))
        if af.id is not None:
            return af.id
    raise InvalidTrackArgError(
        f"""\
{t} is not a valid track argument.

Track arguments must be one of:

  1. The track UUID
  2. The path of the track in the source directory
  3. The path of the track in the virtual filesystem (from any view)

{t} is not recognized as any of the above.
"""
    )


def daemonize(pid_path: Path | None = None) -> None:
    """Forks into a background daemon and exits the foreground process."""
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
