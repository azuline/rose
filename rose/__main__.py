import logging
from dataclasses import dataclass
from multiprocessing import Process
from pathlib import Path

import click

from rose.cache import migrate_database, update_cache_for_all_releases
from rose.config import Config
from rose.virtualfs import mount_virtualfs, unmount_virtualfs


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
    update_cache_for_all_releases(ctx.config, force)


@cli.group()
def fs() -> None:
    """Manage the virtual library."""


@fs.command(context_settings={"ignore_unknown_options": True})
@click.option("--foreground", "-f", is_flag=True, help="Foreground the FUSE controller.")
@click.pass_obj
def mount(ctx: Context, foreground: bool) -> None:
    """Mount the virtual library."""
    # Trigger a cache refresh in the background when we first mount the filesystem.
    p = Process(target=update_cache_for_all_releases, args=[ctx.config, False])
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


if __name__ == "__main__":
    cli()
