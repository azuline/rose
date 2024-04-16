import sys

import click

from rose_cli.cli import CliExpectedError, cli


def main() -> None:
    from rose import RoseExpectedError

    try:
        cli()
    except (RoseExpectedError, CliExpectedError) as e:
        click.secho(f"{e.__class__.__module__}.{e.__class__.__name__}: ", fg="red", nl=False)
        click.secho(str(e))
        sys.exit(1)


if __name__ == "__main__":
    main()
