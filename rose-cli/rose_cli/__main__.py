import multiprocessing
import sys

import click
from rose import RoseExpectedError, RuleSyntaxError
from rose.common import initialize_logging

from rose_cli.cli import CliExpectedError, cli


def main() -> None:
    multiprocessing.set_start_method("fork")
    initialize_logging(output="stderr")
    try:
        cli()
    except (RoseExpectedError, CliExpectedError) as e:
        click.secho(f"{e.__class__.__module__}.{e.__class__.__name__}: ", fg="red", nl=False)
        # The rule parser exposes a styled variant of its error message for terminal display.
        click.secho(e.str_click if isinstance(e, RuleSyntaxError) else str(e))
        sys.exit(1)


if __name__ == "__main__":
    main()
