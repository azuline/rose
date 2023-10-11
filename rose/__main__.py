import logging
from dataclasses import dataclass

import click

from rose.cache.database import migrate_database
from rose.cache.update import update_cache_for_all_releases
from rose.foundation.conf import Config
from rose.virtualfs import mount_virtualfs, unmount_virtualfs


@dataclass
class Context:
    config: Config


@click.group()
@click.option("--verbose", "-v", is_flag=True)
@click.pass_context
def cli(cc: click.Context, verbose: bool) -> None:
    """A filesystem-driven music library manager."""
    cc.obj = Context(
        config=Config.read(),
    )

    if verbose:
        logging.getLogger().setLevel(logging.DEBUG)

    # Migrate the database on every command invocation.
    migrate_database(cc.obj.config)


@cli.group()
def cache() -> None:
    """Manage the cached metadata."""


@cache.command()
@click.pass_obj
def refresh(ctx: Context) -> None:
    """Refresh the cached data from disk."""
    update_cache_for_all_releases(ctx.config)


@cache.command()
@click.pass_obj
def clear(ctx: Context) -> None:
    """Clear the cache; empty the database."""
    ctx.config.cache_database_path.unlink()


@cli.group()
def fs() -> None:
    """Manage the virtual library."""


@fs.command(context_settings={"ignore_unknown_options": True})
@click.argument("mount_args", nargs=-1, type=click.UNPROCESSED)
@click.pass_obj
def mount(ctx: Context, mount_args: list[str]) -> None:
    """Mount the virtual library."""
    mount_virtualfs(ctx.config, mount_args)


@fs.command()
@click.pass_obj
def unmount(ctx: Context) -> None:
    """Unmount the virtual library."""
    unmount_virtualfs(ctx.config)


if __name__ == "__main__":
    cli()
