import sys

import click

from rose.cli import cli
from rose.common import RoseExpectedError


def main() -> None:
    try:
        cli()
    except RoseExpectedError as e:
        click.secho(f"{e.__class__.__module__}.{e.__class__.__name__}: ", fg="red", nl=False)
        click.secho(str(e))
        sys.exit(1)


if __name__ == "__main__":
    main()
