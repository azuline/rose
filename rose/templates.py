"""
The templates module provides the ability to customize paths in the source directory and virtual
filesystem as Jinja templates. Users can specify different templates for different views in the
virtual filesystem.
"""

from __future__ import annotations

import dataclasses
import re
import typing
from copy import deepcopy
from functools import cached_property
from typing import Any

import click
import jinja2

from rose.common import Artist, ArtistMapping, RoseExpectedError

if typing.TYPE_CHECKING:
    from rose.cache import CachedRelease, CachedTrack
    from rose.config import Config

RELEASE_TYPE_FORMATTER = {
    "album": "Album",
    "single": "Single",
    "ep": "EP",
    "compilation": "Compilation",
    "anthology": "Anthology",
    "soundtrack": "Soundtrack",
    "live": "Live",
    "remix": "Remix",
    "djmix": "DJ-Mix",
    "mixtape": "Mixtape",
    "other": "Other",
    "demo": "Demo",
    "unknown": "Unknown",
}


def releasetypefmt(x: str) -> str:
    return RELEASE_TYPE_FORMATTER.get(x, x.title())


def arrayfmt(xs: list[str]) -> str:
    """Format an array as x, y & z."""
    if len(xs) == 0:
        return ""
    if len(xs) == 1:
        return xs[0]
    return ", ".join(xs[:-1]) + " & " + xs[-1]


def artistsarrayfmt(xs: list[Artist]) -> str:
    """Format an array of Artists."""
    return arrayfmt([x.name for x in xs if not x.alias])


def artistsfmt(a: ArtistMapping) -> str:
    """Format a mapping of artists."""

    r = artistsarrayfmt(a.main)
    if a.djmixer:
        r = artistsarrayfmt(a.djmixer) + " pres. " + r
    if a.guest:
        r += " (feat. " + artistsarrayfmt(a.guest) + ")"
    if a.producer:
        r += " (prod. " + artistsarrayfmt(a.producer) + ")"
    if r == "":
        return "Unknown Artists"
    return r


ENVIRONMENT = jinja2.Environment()
ENVIRONMENT.filters["arrayfmt"] = arrayfmt
ENVIRONMENT.filters["artistsarrayfmt"] = artistsarrayfmt
ENVIRONMENT.filters["artistsfmt"] = artistsfmt
ENVIRONMENT.filters["releasetypefmt"] = releasetypefmt


class InvalidPathTemplateError(RoseExpectedError):
    def __init__(self, message: str, key: str):
        super().__init__(message)
        self.key = key


@dataclasses.dataclass
class PathTemplate:
    """
    A wrapper for a template that stores the template as a string and compiles on-demand as a
    derived propery. This grants us serialization of the config.
    """

    text: str

    @cached_property
    def compiled(self) -> jinja2.Template:
        return ENVIRONMENT.from_string(self.text)

    def __hash__(self) -> int:
        return hash(self.text)

    def __getstate__(self) -> dict[str, Any]:
        # We cannot pickle a compiled path template, so remove it from the state before we pickle
        # it. We can cheaply recompute it in the subprocess anyways.
        state = self.__dict__.copy()
        if "compiled" in state:
            del state["compiled"]
        return state


@dataclasses.dataclass
class PathTemplatePair:
    release: PathTemplate
    track: PathTemplate


DEFAULT_RELEASE_TEMPLATE = PathTemplate(
    """
{{ artists | artistsfmt }} -
{% if year %}{{ year }}.{% endif %}
{{ title }}
{% if releasetype == "single" %}- {{ releasetype | releasetypefmt }}{% endif %}
{% if new %}[NEW]{% endif %}
"""
)

DEFAULT_TRACK_TEMPLATE = PathTemplate(
    """
{% if disctotal > 1 %}{{ discnumber.rjust(2, '0') }}-{% endif %}{{ tracknumber.rjust(2, '0') }}.
{{ title }}
{% if artists.guest %}(feat. {{ artists.guest | artistsarrayfmt }}){% endif %}
"""
)

DEFAULT_TEMPLATE_PAIR = PathTemplatePair(
    release=DEFAULT_RELEASE_TEMPLATE,
    track=DEFAULT_TRACK_TEMPLATE,
)


@dataclasses.dataclass
class PathTemplateConfig:
    source: PathTemplatePair
    all_releases: PathTemplatePair
    new_releases: PathTemplatePair
    recently_added_releases: PathTemplatePair
    artists: PathTemplatePair
    genres: PathTemplatePair
    labels: PathTemplatePair
    collages: PathTemplatePair
    playlists: PathTemplate

    @classmethod
    def with_defaults(
        cls,
        default_pair: PathTemplatePair = DEFAULT_TEMPLATE_PAIR,
    ) -> PathTemplateConfig:
        return PathTemplateConfig(
            source=deepcopy(default_pair),
            all_releases=deepcopy(default_pair),
            new_releases=deepcopy(default_pair),
            recently_added_releases=PathTemplatePair(
                release=PathTemplate("[{{ added_at[:10] }}] " + default_pair.release.text),
                track=deepcopy(default_pair.track),
            ),
            artists=deepcopy(default_pair),
            genres=deepcopy(default_pair),
            labels=deepcopy(default_pair),
            collages=PathTemplatePair(
                release=PathTemplate("{{ position }}. " + default_pair.release.text),
                track=deepcopy(default_pair.track),
            ),
            playlists=PathTemplate(
                """
{{ position }}.
{{ artists | artistsfmt }} -
{{ title }}
"""
            ),
        )

    def parse(self) -> None:
        """
        Attempt to parse all the templates into Jinja templates (which will be cached on the
        cached properties). This will raise an InvalidPathTemplateError if a template is invalid.
        """
        key = ""
        try:
            key = "source.release"
            _ = self.source.release.compiled
            key = "source.track"
            _ = self.source.track.compiled
            key = "all_releases.release"
            _ = self.all_releases.release.compiled
            key = "all_releases.track"
            _ = self.all_releases.track.compiled
            key = "new_releases.release"
            _ = self.new_releases.release.compiled
            key = "new_releases.track"
            _ = self.new_releases.track.compiled
            key = "recently_added_releases.release"
            _ = self.recently_added_releases.release.compiled
            key = "recently_added_releases.track"
            _ = self.recently_added_releases.track.compiled
            key = "artists.release"
            _ = self.artists.release.compiled
            key = "artists.track"
            _ = self.artists.track.compiled
            key = "genres.release"
            _ = self.genres.release.compiled
            key = "genres.track"
            _ = self.genres.track.compiled
            key = "labels.release"
            _ = self.labels.release.compiled
            key = "labels.track"
            _ = self.labels.track.compiled
            key = "collages.release"
            _ = self.collages.release.compiled
            key = "collages.track"
            _ = self.collages.track.compiled
            key = "playlists"
            _ = self.playlists.compiled
        except jinja2.exceptions.TemplateSyntaxError as e:
            raise InvalidPathTemplateError(f"Failed to compile template: {e}", key=key) from e


def eval_release_template(
    template: PathTemplate,
    release: CachedRelease,
    position: str | None = None,
) -> str:
    return _collapse_spacing(template.compiled.render(**_calc_release_variables(release, position)))


def eval_track_template(
    template: PathTemplate,
    track: CachedTrack,
    position: str | None = None,
) -> str:
    return (
        _collapse_spacing(template.compiled.render(**_calc_track_variables(track, position)))
        + track.source_path.suffix
    )


def _calc_release_variables(release: CachedRelease, position: str | None) -> dict[str, Any]:
    return {
        "added_at": release.added_at,
        "title": release.albumtitle,
        "releasetype": release.releasetype,
        "year": release.year,
        "new": release.new,
        "disctotal": release.disctotal,
        "genres": release.genres,
        "labels": release.labels,
        "artists": release.albumartists,
        "position": position,
    }


def _calc_track_variables(track: CachedTrack, position: str | None) -> dict[str, Any]:
    return {
        "title": track.tracktitle,
        "tracknumber": track.tracknumber,
        "tracktotal": track.tracktotal,
        "discnumber": track.discnumber,
        "disctotal": track.disctotal,
        "duration_seconds": track.duration_seconds,
        "artists": track.trackartists,
        "position": position,
    }


COLLAPSE_SPACING_REGEX = re.compile(r"\s+", flags=re.MULTILINE)


def _collapse_spacing(x: str) -> str:
    # All newlines and multi-spaces are replaced with a single space in the final output.
    return COLLAPSE_SPACING_REGEX.sub(" ", x).strip()


def preview_path_templates(c: Config) -> None:
    # fmt: off
    _preview_release_template(c, "Source Directory - Release", c.path_templates.source.release)
    _preview_track_template(c, "Source Directory - Track", c.path_templates.source.track)
    click.echo()
    _preview_release_template(c, "1. All Releases - Release", c.path_templates.all_releases.release)
    _preview_track_template(c, "1. All Releases - Track", c.path_templates.all_releases.track)
    click.echo()
    _preview_release_template(c, "2. New Releases - Release", c.path_templates.new_releases.release)
    _preview_track_template(c, "2. New Releases - Track", c.path_templates.new_releases.track)
    click.echo()
    _preview_release_template(c, "3. Recently Added Releases - Release", c.path_templates.recently_added_releases.release)
    _preview_track_template(c, "3. Recently Added Releases - Track", c.path_templates.recently_added_releases.track)
    click.echo()
    _preview_release_template(c, "4. Artists - Release", c.path_templates.artists.release)
    _preview_track_template(c, "4. Artists - Track", c.path_templates.artists.track)
    click.echo()
    _preview_release_template(c, "5. Genres - Release", c.path_templates.genres.release)
    _preview_track_template(c, "5. Genres - Track", c.path_templates.genres.track)
    click.echo()
    _preview_release_template(c, "6. Labels - Release", c.path_templates.labels.release)
    _preview_track_template(c, "6. Labels - Track", c.path_templates.labels.track)
    click.echo()
    _preview_release_template(c, "7. Collages - Release", c.path_templates.collages.release)
    _preview_track_template(c, "7. Collages - Track", c.path_templates.collages.track)
    click.echo()
    _preview_track_template(c, "8. Playlists - Track", c.path_templates.playlists)
    # fmt: on


def _get_preview_releases(c: Config) -> tuple[CachedRelease, CachedRelease]:
    from rose.cache import CachedRelease

    kimlip = CachedRelease(
        id="018b268e-ff1e-7a0c-9ac8-7bbb282761f2",
        source_path=c.music_source_dir / "LOONA - 2017. Kim Lip",
        cover_image_path=None,
        added_at="2023-04-20:23:45Z",
        datafile_mtime="999",
        albumtitle="Kim Lip",
        releasetype="single",
        year=2017,
        new=True,
        disctotal=1,
        genres=["K-Pop", "Dance-Pop", "Contemporary R&B"],
        labels=["BlockBerryCreative"],
        albumartists=ArtistMapping(main=[Artist("Kim Lip")]),
        metahash="0",
    )

    youngforever = CachedRelease(
        id="018b6021-f1e5-7d4b-b796-440fbbea3b13",
        source_path=c.music_source_dir / "BTS - 2016. Young Forever (花樣年華)",
        cover_image_path=None,
        added_at="2023-06-09:23:45Z",
        datafile_mtime="999",
        albumtitle="Young Forever (花樣年華)",
        releasetype="album",
        year=2016,
        new=False,
        disctotal=2,
        genres=["K-Pop"],
        labels=["BIGHIT"],
        albumartists=ArtistMapping(main=[Artist("BTS")]),
        metahash="0",
    )

    return kimlip, youngforever


def _preview_release_template(c: Config, label: str, template: PathTemplate) -> None:
    # Import cycle trick :)
    kimlip, youngforever = _get_preview_releases(c)
    click.secho(f"{label}:", dim=True, underline=True)
    click.secho("  Sample 1: ", dim=True, nl=False)
    click.secho(eval_release_template(template, kimlip, "1"))
    click.secho("  Sample 2: ", dim=True, nl=False)
    click.secho(eval_release_template(template, youngforever, "2"))


def _preview_track_template(c: Config, label: str, template: PathTemplate) -> None:
    # Import cycle trick :)
    from rose.cache import CachedTrack

    kimlip, youngforever = _get_preview_releases(c)

    click.secho(f"{label}:", dim=True, underline=True)

    click.secho("  Sample 1: ", dim=True, nl=False)
    track = CachedTrack(
        id="018b268e-ff1e-7a0c-9ac8-7bbb282761f1",
        source_path=c.music_source_dir / "LOONA - 2017. Kim Lip" / "01. Eclipse.opus",
        source_mtime="999",
        tracktitle="Eclipse",
        tracknumber="1",
        tracktotal=2,
        discnumber="1",
        disctotal=1,
        duration_seconds=230,
        trackartists=ArtistMapping(main=[Artist("Kim Lip")]),
        metahash="0",
        release=kimlip,
    )
    click.secho(eval_track_template(template, track, "1"))

    click.secho("  Sample 2: ", dim=True, nl=False)
    track = CachedTrack(
        id="018b6021-f1e5-7d4b-b796-440fbbea3b15",
        source_path=c.music_source_dir
        / "BTS - 2016. Young Forever (花樣年華)"
        / "House of Cards.opus",
        source_mtime="999",
        tracktitle="House of Cards",
        tracknumber="5",
        tracktotal=8,
        discnumber="2",
        disctotal=2,
        duration_seconds=226,
        trackartists=ArtistMapping(main=[Artist("BTS")]),
        metahash="0",
        release=youngforever,
    )
    click.secho(eval_track_template(template, track, "2"))
