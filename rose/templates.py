"""
The templates module contains the virtual path templating logic. Rose supports configuring release
directory names and track file names as Jinja templates.

All newlines and multi-spaces are replaced with a single space in the final output.
"""

from __future__ import annotations

import dataclasses
import re
import typing
from functools import cached_property
from typing import Any

import jinja2

from rose.common import Artist, ArtistMapping

if typing.TYPE_CHECKING:
    from rose.cache import CachedRelease, CachedTrack


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


@dataclasses.dataclass
class PathTemplatePair:
    release: PathTemplate
    track: PathTemplate


DEFAULT_RELEASE_TEMPLATE = PathTemplate(
    """
{{ artists | artistsfmt }} -
{% if year %}{{ year }}.{% endif %}
{{ title }}
{% if releasetype == "single" %}- Single{% endif %}
"""
)

DEFAULT_TRACK_TEMPLATE = PathTemplate(
    """
{% if multidisc %}{{ discnumber.rjust(2, '0') }}-{% endif %}{{ tracknumber.rjust(2, '0') }}.
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
    # Source Directory
    source: PathTemplatePair = dataclasses.field(default_factory=lambda: DEFAULT_TEMPLATE_PAIR)
    # 1. Releases
    all_releases: PathTemplatePair = dataclasses.field(
        default_factory=lambda: DEFAULT_TEMPLATE_PAIR
    )
    # 2. Releases - New
    new_releases: PathTemplatePair = dataclasses.field(
        default_factory=lambda: DEFAULT_TEMPLATE_PAIR
    )
    # 3. Releases - Recently Added
    recently_added_releases: PathTemplatePair = dataclasses.field(
        default_factory=lambda: PathTemplatePair(
            release=PathTemplate("[{{ added_at[:10] }}] " + DEFAULT_RELEASE_TEMPLATE.text),
            track=DEFAULT_TRACK_TEMPLATE,
        )
    )
    # 4. Artists
    artists: PathTemplatePair = dataclasses.field(
        default_factory=lambda: PathTemplatePair(
            release=PathTemplate(
                """
{% if year %}{{ year }}.{% else %}0000.{% endif %}
{{ title }}
{% if artists.guest %}(feat. {{ artists.guest | artistsarrayfmt }}){% endif %}
{% if releasetype == "single" %}- Single{% endif %}
"""
            ),
            track=DEFAULT_TRACK_TEMPLATE,
        )
    )
    # 5. Genres
    genres: PathTemplatePair = dataclasses.field(default_factory=lambda: DEFAULT_TEMPLATE_PAIR)
    # 6. Labels
    labels: PathTemplatePair = dataclasses.field(default_factory=lambda: DEFAULT_TEMPLATE_PAIR)
    # 7. Collages
    collages: PathTemplatePair = dataclasses.field(
        default_factory=lambda: PathTemplatePair(
            release=PathTemplate("{{ position }}. " + DEFAULT_RELEASE_TEMPLATE.text),
            track=DEFAULT_TRACK_TEMPLATE,
        )
    )
    # 8. Playlists: track template only.
    playlists: PathTemplate = dataclasses.field(
        default_factory=lambda: PathTemplate(
            """
{{ position }}.
{{ artists | artistsfmt }} -
{{ title }}
"""
        )
    )


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
        "title": release.title,
        "releasetype": release.releasetype,
        "year": release.year,
        "new": release.new,
        "genres": release.genres,
        "labels": release.labels,
        "artists": release.artists,
        "position": position,
    }


def _calc_track_variables(track: CachedTrack, position: str | None) -> dict[str, Any]:
    return {
        "title": track.title,
        "tracknumber": track.tracknumber,
        "discnumber": track.discnumber,
        "duration_seconds": track.duration_seconds,
        "multidisc": track.release_multidisc,
        "artists": track.artists,
        "position": position,
    }


COLLAPSE_SPACING_REGEX = re.compile(r"\s+", flags=re.MULTILINE)


def _collapse_spacing(x: str) -> str:
    return COLLAPSE_SPACING_REGEX.sub(" ", x).strip()
