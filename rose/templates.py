"""
The templates module contains the virtual path templating logic. Rose supports configuring release
directory names and track file names as Jinja templates.

All newlines and multi-spaces are replaced with a single space in the final output.
"""

from __future__ import annotations

import dataclasses
import re
import typing
from copy import deepcopy
from functools import cached_property
from typing import Any

import jinja2

from rose.common import Artist, ArtistMapping, RoseExpectedError

if typing.TYPE_CHECKING:
    from rose.cache import CachedRelease, CachedTrack

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
