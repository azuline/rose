import logging
from dataclasses import dataclass

import click

from rose.cache.database import migrate_database
from rose.foundation.conf import Config


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
@click.pass_obj
def cache(_: Context) -> None:
    """Manage the cached metadata."""


@cache.command()
def reset() -> None:
    """Reset the cache and empty the database."""


if __name__ == "__main__":
    cli()
