"""
The cli module defines Rose's CLI interface. It does not have any domain logic of its own. It is
dedicated to parsing, resolving arguments, and delegating to the appropriate module.
"""

import contextlib
import logging
import os
import re
import signal
import subprocess
import uuid
from dataclasses import dataclass
from multiprocessing import Process
from pathlib import Path

import click

from rose import (
    VERSION,
    Action,
    AudioTags,
    Config,
    Matcher,
    Rule,
    UnsupportedFiletypeError,
    add_release_to_collage,
    add_track_to_playlist,
    create_collage,
    create_playlist,
    create_single_release,
    delete_collage,
    delete_playlist,
    delete_playlist_cover_art,
    delete_release,
    delete_release_cover_art,
    edit_collage_in_editor,
    edit_playlist_in_editor,
    edit_release,
    execute_metadata_rule,
    execute_stored_metadata_rules,
    maybe_invalidate_cache_database,
    remove_release_from_collage,
    remove_track_from_playlist,
    rename_collage,
    rename_playlist,
    run_actions_on_release,
    run_actions_on_track,
    set_playlist_cover_art,
    set_release_cover_art,
    toggle_release_new,
    update_cache,
)
from rose_cli.dump import (
    dump_all_artists,
    dump_all_collages,
    dump_all_descriptors,
    dump_all_genres,
    dump_all_labels,
    dump_all_playlists,
    dump_all_releases,
    dump_all_tracks,
    dump_artist,
    dump_collage,
    dump_descriptor,
    dump_genre,
    dump_label,
    dump_playlist,
    dump_release,
    dump_track,
)
from rose_cli.templates import preview_path_templates
from rose_vfs import mount_virtualfs
from rose_watchdog import start_watchdog

logger = logging.getLogger(__name__)

STORED_DATA_FILE_REGEX = re.compile(r"\.rose\.([^.]+)\.toml")


class CliExpectedError(Exception):
    pass


class InvalidReleaseArgError(CliExpectedError):
    pass


class InvalidTrackArgError(CliExpectedError):
    pass


class DaemonAlreadyRunningError(CliExpectedError):
    pass


@dataclass
class Context:
    config: "Config"


@click.group()
@click.option("--verbose", "-v", is_flag=True, help="Emit verbose logging.")
@click.option("--config", "-c", type=click.Path(path_type=Path), help="Override the config file location.")  # fmt: skip
@click.pass_context
def cli(cc: click.Context, verbose: bool, config: Path | None = None) -> None:
    """A music manager with a virtual filesystem."""

    cc.obj = Context(
        config=Config.parse(config_path_override=config),
    )
    if verbose:
        logging.getLogger().setLevel(logging.DEBUG)
    maybe_invalidate_cache_database(cc.obj.config)


@cli.command()
def version() -> None:
    """Print version."""

    click.echo(VERSION)


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

    preview_path_templates(ctx.config)


@cli.group()
def cache() -> None:
    """Manage the read cache."""


@cache.command()
@click.option("--force", "-f", is_flag=True, help="Force re-read all data from disk, even for unchanged files.")  # fmt: skip
@click.pass_obj
def update(ctx: Context, force: bool) -> None:
    """Synchronize the read cache with new changes in the source directory."""

    update_cache(ctx.config, force)


@cache.command()
@click.option("--foreground", "-f", is_flag=True, help="Run the filesystem watcher in the foreground (default: daemon).")  # fmt: skip
@click.pass_obj
def watch(ctx: Context, foreground: bool) -> None:
    """Start a watchdog to auto-update the cache when the source directory changes."""

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


@fs.command()
@click.option("--foreground", "-f", is_flag=True, help="Run the FUSE controller in the foreground (default: daemon).")  # fmt: skip
@click.pass_obj
def mount(ctx: Context, foreground: bool) -> None:
    """Mount the virtual filesystem."""
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
    from rose_vfs import unmount_virtualfs

    unmount_virtualfs(ctx.config)


@cli.group()
def releases() -> None:
    """Manage releases."""


@releases.command(name="print")
@click.argument("release", type=click.Path(), nargs=1)
@click.pass_obj
def print_release(ctx: Context, release: str) -> None:
    """Print a single release (in JSON). Accepts a release's UUID/path."""
    release = parse_release_argument(release)
    click.echo(dump_release(ctx.config, release))


@releases.command(name="print-all")
@click.argument("matcher", type=str, nargs=1, required=False)
@click.pass_obj
def print_all_releases(ctx: Context, matcher: str | None) -> None:
    """Print all releases (in JSON). Accepts an optional rules matcher to filter the releases."""
    parsed_matcher = Matcher.parse(matcher) if matcher else None
    click.echo(dump_all_releases(ctx.config, parsed_matcher))


@releases.command(name="edit")
@click.argument("release", type=click.Path(), nargs=1)
@click.option("--resume", "-r", type=click.Path(path_type=Path), nargs=1, help="Resume a failed release edit.")  # fmt: skip
@click.pass_obj
def edit_release_cmd(ctx: Context, release: str, resume: Path | None) -> None:
    """Edit a release's metadata in $EDITOR. Accepts a release's UUID/path."""
    release = parse_release_argument(release)
    edit_release(ctx.config, release, resume_file=resume)


@releases.command()
@click.argument("release", type=click.Path(), nargs=1)
@click.pass_obj
def toggle_new(ctx: Context, release: str) -> None:
    """Toggle a release's "new"-ness. Accepts a release's UUID/path."""
    release = parse_release_argument(release)
    toggle_release_new(ctx.config, release)


@releases.command(name="delete")
@click.argument("release", type=click.Path(), nargs=1)
@click.pass_obj
def delete_release_cmd(ctx: Context, release: str) -> None:
    """
    Delete a release from the library. The release is moved to the trash bin, following the
    freedesktop spec. Accepts a release's UUID/path.
    """
    release = parse_release_argument(release)
    delete_release(ctx.config, release)


@releases.command()
@click.argument("release", type=click.Path(), nargs=1)
@click.argument("cover", type=click.Path(path_type=Path), nargs=1)
@click.pass_obj
def set_cover_release(ctx: Context, release: str, cover: Path) -> None:
    """Set/replace the cover art of a release. Accepts a release's UUID/path."""
    release = parse_release_argument(release)
    set_release_cover_art(ctx.config, release, cover)


@releases.command()
@click.argument("release", type=click.Path(), nargs=1)
@click.pass_obj
def delete_cover_release(ctx: Context, release: str) -> None:
    """Delete the cover art of a release."""
    release = parse_release_argument(release)
    delete_release_cover_art(ctx.config, release)


@releases.command()
@click.argument("release", type=click.Path(), nargs=1)
@click.argument("actions", type=str, nargs=-1)
@click.option("--dry-run", "-d", is_flag=True, help="Display intended changes without applying them.")  # fmt: skip
@click.option("--yes", "-y", is_flag=True, help="Bypass confirmation prompts.")
@click.pass_obj
def run_rule(ctx: Context, release: str, actions: list[str], dry_run: bool, yes: bool) -> None:
    """Run rule engine actions on all tracks in a release. Accepts a release's UUID/path."""
    release = parse_release_argument(release)
    parsed_actions = [Action.parse(a) for a in actions]
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
    create_single_release(ctx.config, track_path)


@cli.group()
def tracks() -> None:
    """Manage tracks."""


@tracks.command(name="print")
@click.argument("track", type=click.Path(), nargs=1)
@click.pass_obj
def print_track(ctx: Context, track: str) -> None:
    """Print a single track (in JSON). Accepts a tracks's UUID/path."""
    track = parse_track_argument(track)
    click.echo(dump_track(ctx.config, track))


@tracks.command(name="print-all")
@click.argument("matcher", type=str, nargs=1, required=False)
@click.pass_obj
def print_all_track(ctx: Context, matcher: str | None = None) -> None:
    """Print all tracks (in JSON). Accepts an optional rules matcher to filter the tracks."""
    parsed_matcher = Matcher.parse(matcher) if matcher else None
    click.echo(dump_all_tracks(ctx.config, parsed_matcher))


@tracks.command(name="run-rule")
@click.argument("track", type=click.Path(), nargs=1)
@click.argument("actions", type=str, nargs=-1)
@click.option("--dry-run", "-d", is_flag=True, help="Display intended changes without applying them.")  # fmt: skip
@click.option("--yes", "-y", is_flag=True, help="Bypass confirmation prompts.")
@click.pass_obj
def run_rule_track(ctx: Context, track: str, actions: list[str], dry_run: bool, yes: bool) -> None:
    """Run rule engine actions on a single track. Accepts a track's UUID/path."""
    track = parse_track_argument(track)
    parsed_actions = [Action.parse(a) for a in actions]
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
    create_collage(ctx.config, name)


@collages.command()
@click.argument("old_name", type=str, nargs=1)
@click.argument("new_name", type=str, nargs=1)
@click.pass_obj
def rename(ctx: Context, old_name: str, new_name: str) -> None:
    """Rename a collage."""
    rename_collage(ctx.config, old_name, new_name)


@collages.command()
@click.argument("collage", type=str, nargs=1)
@click.pass_obj
def delete(ctx: Context, collage: str) -> None:
    """Delete a collage."""
    delete_collage(ctx.config, collage)


@collages.command()
@click.argument("collage", type=str, nargs=1)
@click.argument("release", type=str, nargs=1)
@click.pass_obj
def add_release(ctx: Context, collage: str, release: str) -> None:
    """Add a release to a collage. Accepts a collage's name and a release's UUID/path."""
    release = parse_release_argument(release)
    add_release_to_collage(ctx.config, collage, release)


@collages.command()
@click.argument("collage", type=str, nargs=1)
@click.argument("release", type=str, nargs=1)
@click.pass_obj
def remove_release(ctx: Context, collage: str, release: str) -> None:
    """Remove a release from a collage. Accepts a collage's name and a release's UUID/path."""
    release = parse_release_argument(release)
    remove_release_from_collage(ctx.config, collage, release)


@collages.command()
@click.argument("collage", type=str, nargs=1)
@click.pass_obj
def edit(ctx: Context, collage: str) -> None:
    """Edit (reorder/remove releases from) a collage in $EDITOR. Accepts a collage's name."""
    edit_collage_in_editor(ctx.config, collage)


@collages.command(name="print")
@click.argument("collage", type=str, nargs=1)
@click.pass_obj
def print_collage(ctx: Context, collage: str) -> None:
    """Print a collage (in JSON). Accepts a collage's name."""
    click.echo(dump_collage(ctx.config, collage))


@collages.command(name="print-all")
@click.pass_obj
def print_all_collages(ctx: Context) -> None:
    """Print all collages (in JSON)."""
    click.echo(dump_all_collages(ctx.config))


@cli.group()
def playlists() -> None:
    """Manage playlists."""


@playlists.command(name="create")
@click.argument("name", type=str, nargs=1)
@click.pass_obj
def create_playlist_cmd(ctx: Context, name: str) -> None:
    """Create a new playlist."""
    create_playlist(ctx.config, name)


@playlists.command(name="rename")
@click.argument("old_name", type=str, nargs=1)
@click.argument("new_name", type=str, nargs=1)
@click.pass_obj
def rename_playlist_cmd(ctx: Context, old_name: str, new_name: str) -> None:
    """Rename a playlist. Accepts a playlist's name."""
    rename_playlist(ctx.config, old_name, new_name)


@playlists.command(name="delete")
@click.argument("playlist", type=str, nargs=1)
@click.pass_obj
def delete_playlist_cmd(ctx: Context, playlist: str) -> None:
    """Delete a playlist. Accepts a playlist's name."""
    delete_playlist(ctx.config, playlist)


@playlists.command()
@click.argument("playlist", type=str, nargs=1)
@click.argument("track", type=str, nargs=1)
@click.pass_obj
def add_track(ctx: Context, playlist: str, track: str) -> None:
    """Add a track to a playlist. Accepts a playlist name and a track's UUID/path."""
    track = parse_track_argument(track)
    add_track_to_playlist(ctx.config, playlist, track)


@playlists.command()
@click.argument("playlist", type=str, nargs=1)
@click.argument("track", type=str, nargs=1)
@click.pass_obj
def remove_track(ctx: Context, playlist: str, track: str) -> None:
    """Remove a track from a playlist. Accepts a playlist name and a track's UUID/path."""
    track = parse_track_argument(track)
    remove_track_from_playlist(ctx.config, playlist, track)


@playlists.command(name="edit")
@click.argument("playlist", type=str, nargs=1)
@click.pass_obj
def edit_playlist(ctx: Context, playlist: str) -> None:
    """
    Edit a playlist in $EDITOR. Reorder lines to update the ordering of tracks. Delete lines to
    delete tracks from the playlist.
    """
    edit_playlist_in_editor(ctx.config, playlist)


@playlists.command(name="print")
@click.argument("playlist", type=str, nargs=1)
@click.pass_obj
def print_playlist(ctx: Context, playlist: str) -> None:
    """Print a playlist (in JSON). Accepts a playlist's name."""
    click.echo(dump_playlist(ctx.config, playlist))


@playlists.command(name="print-all")
@click.pass_obj
def print_all_playlists(ctx: Context) -> None:
    """Print all playlists (in JSON)."""
    click.echo(dump_all_playlists(ctx.config))


@playlists.command(name="set-cover")
@click.argument("playlist", type=str, nargs=1)
@click.argument("cover", type=click.Path(path_type=Path), nargs=1)
@click.pass_obj
def set_cover_playlist(ctx: Context, playlist: str, cover: Path) -> None:
    """
    Set the cover art of a playlist. Accepts a playlist name and a path to an image.
    """
    set_playlist_cover_art(ctx.config, playlist, cover)


@playlists.command(name="delete-cover")
@click.argument("playlist", type=str, nargs=1)
@click.pass_obj
def delete_cover_playlist(ctx: Context, playlist: str) -> None:
    """Delete the cover art of a playlist. Accepts a playlist name."""
    delete_playlist_cover_art(ctx.config, playlist)


@cli.group()
def artists() -> None:
    """Manage artists."""


@artists.command(name="print")
@click.argument("artist", type=str, nargs=1)
@click.pass_obj
def print_artist(ctx: Context, artist: str) -> None:
    """Print a artist (in JSON). Accepts a artist's name."""
    click.echo(dump_artist(ctx.config, artist))


@artists.command(name="print-all")
@click.pass_obj
def print_all_artists(ctx: Context) -> None:
    """Print all artists (in JSON)."""
    click.echo(dump_all_artists(ctx.config))


@cli.group()
def genres() -> None:
    """Manage genres."""


@genres.command(name="print")
@click.argument("genre", type=str, nargs=1)
@click.pass_obj
def print_genre(ctx: Context, genre: str) -> None:
    """Print a genre (in JSON). Accepts a genre's name."""
    click.echo(dump_genre(ctx.config, genre))


@genres.command(name="print-all")
@click.pass_obj
def print_all_genres(ctx: Context) -> None:
    """Print all genres (in JSON)."""
    click.echo(dump_all_genres(ctx.config))


@cli.group()
def labels() -> None:
    """Manage labels."""


@labels.command(name="print")
@click.argument("label", type=str, nargs=1)
@click.pass_obj
def print_label(ctx: Context, label: str) -> None:
    """Print a label (in JSON). Accepts a label's name."""
    click.echo(dump_label(ctx.config, label))


@labels.command(name="print-all")
@click.pass_obj
def print_all_labels(ctx: Context) -> None:
    """Print all labels (in JSON)."""
    click.echo(dump_all_labels(ctx.config))


@cli.group()
def descriptors() -> None:
    """Manage descriptors."""


@descriptors.command(name="print")
@click.argument("descriptor", type=str, nargs=1)
@click.pass_obj
def print_descriptor(ctx: Context, descriptor: str) -> None:
    """Print a descriptor (in JSON). Accepts a descriptor's name."""
    click.echo(dump_descriptor(ctx.config, descriptor))


@descriptors.command(name="print-all")
@click.pass_obj
def print_all_descriptors(ctx: Context) -> None:
    """Print all descriptors (in JSON)."""
    click.echo(dump_all_descriptors(ctx.config))


@cli.group()
def rules() -> None:
    """Run metadata update rules on the entire library."""


@rules.command()
@click.argument("matcher", type=str, nargs=1)
@click.argument("actions", type=str, nargs=-1)
@click.option("--dry-run", "-d", is_flag=True, help="Display intended changes without applying them.")  # fmt: skip
@click.option("--yes", "-y", is_flag=True, help="Bypass confirmation prompts.")
@click.option("--ignore", "-i", type=str, multiple=True, help="Ignore tracks matching this matcher.")  # fmt: skip
@click.pass_obj
def run(
    ctx: Context,
    matcher: str,
    actions: list[str],
    dry_run: bool,
    yes: bool,
    ignore: list[str],
) -> None:
    """Run an ad hoc rule."""
    if not actions:
        logger.info("No-Op: No actions passed")
        return
    rule = Rule.parse(matcher, actions, ignore)
    execute_metadata_rule(ctx.config, rule, dry_run=dry_run, confirm_yes=not yes)


@rules.command()
@click.option("--dry-run", "-d", is_flag=True, help="Display intended changes without applying them.")  # fmt: skip
@click.option("--yes", "-y", is_flag=True, help="Bypass confirmation prompts.")
@click.pass_obj
def run_stored(ctx: Context, dry_run: bool, yes: bool) -> None:
    """Run the rules stored in the config."""
    execute_stored_metadata_rules(ctx.config, dry_run=dry_run, confirm_yes=not yes)


def parse_release_argument(r: str) -> str:
    """Takes in a release argument and normalizes it to the release ID."""
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


def parse_collage_argument(c: str) -> str:
    """Takes in a collage argument and normalizes it to the collage ID."""
    path = Path(c).resolve()
    # Handle the case where the argument is a path to a directory.
    if path.exists() and path.is_dir():
        return path.name
    # Then handle the case where the argument is a path to the TOML file.
    if path.exists() and path.is_file() and path.parent.name == "!collages":
        return path.stem
    # Then handle the case where the argument is a collage name (aka everything else).
    return c


def parse_playlist_argument(p: str) -> str:
    """Takes in a collage argument and normalizes it to the collage ID."""
    path = Path(p).resolve()
    # Handle the case where the argument is a path to a directory.
    if path.exists() and path.is_dir():
        return path.name
    # Then handle the case where the argument is a path to the TOML file.
    if path.exists() and path.is_file() and path.parent.name == "!playlists":
        return path.stem
    # Then handle the case where the argument is a collage name (aka everything else).
    return p


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


def valid_uuid(x: str) -> bool:
    try:
        uuid.UUID(x)
        return True
    except ValueError:
        return False
