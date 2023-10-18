import logging
from dataclasses import dataclass
from multiprocessing import Process
from pathlib import Path

import click

from rose.cache import migrate_database, update_cache
from rose.collages import (
    add_release_to_collage,
    delete_release_from_collage,
    dump_collages,
    edit_collage_in_editor,
)
from rose.config import Config
from rose.releases import dump_releases
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
@click.option("--foreground", "-f", is_flag=True, help="Foreground the cache watcher.")
@click.pass_obj
def watch(ctx: Context, foreground: bool) -> None:
    """Start a watchdog that will auto-refresh the cache on changes in music_source_dir."""
    start_watchdog(ctx.config, foreground)


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


@cli.group()
def collages() -> None:
    """Manage collages."""


@collages.command()
@click.argument("collage", type=str, nargs=1)
@click.argument("release", type=str, nargs=1)
@click.pass_obj
def add(ctx: Context, collage: str, release: str) -> None:
    """
    Add a release to a collage. Accepts a collage name and a release's UUID or virtual fs dirname
    (both are accepted).
    """
    add_release_to_collage(ctx.config, collage, release)


@collages.command(name="del")
@click.argument("collage", type=str, nargs=1)
@click.argument("release", type=str, nargs=1)
@click.pass_obj
def del_(ctx: Context, collage: str, release: str) -> None:
    """
    Delete a release from a collage. Accepts a collage name and a release's UUID or virtual fs
    dirname (both are accepted).
    """
    delete_release_from_collage(ctx.config, collage, release)


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


if __name__ == "__main__":
    cli()
