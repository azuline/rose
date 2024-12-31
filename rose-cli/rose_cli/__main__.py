import sys

import click
from rose import RoseExpectedError
from rose.common import initialize_logging
from rose_cli.cli import CliExpectedError, cli


def main() -> None:
    initialize_logging()
    try:
        cli()
    except (RoseExpectedError, CliExpectedError) as e:
        click.secho(f"{e.__class__.__module__}.{e.__class__.__name__}: ", fg="red", nl=False)
        click.secho(str(e))
        sys.exit(1)


if __name__ == "__main__":
    main()
