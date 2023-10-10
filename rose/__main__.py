import logging
from dataclasses import dataclass

import click

from rose.cache.database import migrate_database
from rose.cache.update import update_cache_for_all_releases
from rose.foundation.conf import Config
from rose.virtualfs import start_virtualfs


@dataclass
class Context:
    config: Config


@click.group()
@click.option("--verbose", "-v", is_flag=True)
@click.pass_context
def cli(clickctx: click.Context, verbose: bool) -> None:
    """A filesystem-driven music library manager."""
    clickctx.obj = Context(
        config=Config.read(),
    )

    if verbose:
        logging.getLogger().setLevel(logging.DEBUG)

    # Migrate the database on every command invocation.
    migrate_database(clickctx.obj.config)


@cli.group()
def cache() -> None:
    """Manage the cached metadata."""


@cache.command()
@click.pass_obj
def refresh(c: Context) -> None:
    """Refresh the cached data from disk."""
    update_cache_for_all_releases(c.config)


@cache.command()
@click.pass_obj
def clear(c: Context) -> None:
    """Clear the cache; empty the database."""
    c.config.cache_database_path.unlink()


@cli.group()
def fs() -> None:
    """Manage the virtual library."""


@cli.command()
@click.pass_obj
def mount(c: Context) -> None:
    """Mount the virtual library."""
    start_virtualfs(c.config)


if __name__ == "__main__":
    cli()
