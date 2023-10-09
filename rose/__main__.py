import click


@click.group()
def cli() -> None:
    """A filesystem-driven music library manager."""


if __name__ == "__main__":
    cli()
