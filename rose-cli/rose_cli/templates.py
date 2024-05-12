import click
from rose import (
    Config,
    PathTemplate,
    evaluate_release_template,
    evaluate_track_template,
    get_sample_music,
)


def preview_path_templates(c: Config) -> None:
    # fmt: off
    _preview_release_template(c, "Source Directory - Release", c.path_templates.source.release)
    _preview_track_template(c, "Source Directory - Track", c.path_templates.source.track)
    click.echo()
    _preview_release_template(c, "1. Releases - Release", c.path_templates.releases.release)
    _preview_track_template(c, "1. Releases - Track", c.path_templates.releases.track)
    click.echo()
    _preview_release_template(c, "1. Releases (New) - Release", c.path_templates.releases_new.release)
    _preview_track_template(c, "1. Releases (New) - Track", c.path_templates.releases_new.track)
    click.echo()
    _preview_release_template(c, "1. Releases (Added On) - Release", c.path_templates.releases_added_on.release)
    _preview_track_template(c, "1. Releases (Added On) - Track", c.path_templates.releases_added_on.track)
    click.echo()
    _preview_release_template(c, "1. Releases (Released On) - Release", c.path_templates.releases_added_on.release)
    _preview_track_template(c, "1. Releases (Released On) - Track", c.path_templates.releases_added_on.track)
    click.echo()
    _preview_release_template(c, "2. Artists - Release", c.path_templates.artists.release)
    _preview_track_template(c, "2. Artists - Track", c.path_templates.artists.track)
    click.echo()
    _preview_release_template(c, "3. Genres - Release", c.path_templates.genres.release)
    _preview_track_template(c, "3. Genres - Track", c.path_templates.genres.track)
    click.echo()
    _preview_release_template(c, "4. Descriptors - Release", c.path_templates.genres.release)
    _preview_track_template(c, "4. Descriptors - Track", c.path_templates.genres.track)
    click.echo()
    _preview_release_template(c, "5. Labels - Release", c.path_templates.labels.release)
    _preview_track_template(c, "5. Labels - Track", c.path_templates.labels.track)
    click.echo()
    _preview_release_template(c, "6. Collages - Release", c.path_templates.collages.release)
    _preview_track_template(c, "6. Collages - Track", c.path_templates.collages.track)
    click.echo()
    _preview_track_template(c, "7. Playlists - Track", c.path_templates.playlists)
    # fmt: on


def _preview_release_template(c: Config, label: str, template: PathTemplate) -> None:
    (kimlip, _), (youngforever, _), (debussy, _) = get_sample_music(c)
    click.secho(f"{label}:", dim=True, underline=True)
    click.secho("  Sample 1: ", dim=True, nl=False)
    click.secho(evaluate_release_template(template, kimlip, position="1"))
    click.secho("  Sample 2: ", dim=True, nl=False)
    click.secho(evaluate_release_template(template, youngforever, position="2"))
    click.secho("  Sample 3: ", dim=True, nl=False)
    click.secho(evaluate_release_template(template, debussy, position="3"))


def _preview_track_template(c: Config, label: str, template: PathTemplate) -> None:
    (_, kimlip), (_, bts), (_, debussy) = get_sample_music(c)

    click.secho(f"{label}:", dim=True, underline=True)
    click.secho("  Sample 1: ", dim=True, nl=False)
    click.secho(evaluate_track_template(template, kimlip, position="1"))
    click.secho("  Sample 2: ", dim=True, nl=False)
    click.secho(evaluate_track_template(template, bts, position="2"))
    click.secho("  Sample 3: ", dim=True, nl=False)
    click.secho(evaluate_track_template(template, debussy, position="3"))
