"""
The templates module provides the ability to customize paths in the source directory and virtual
filesystem as Jinja templates. Users can specify different templates for different views in the
virtual filesystem.
"""

from __future__ import annotations

import dataclasses
import re
import typing
from collections.abc import Iterable
from copy import deepcopy
from functools import cached_property
from typing import Any

import click
import jinja2

from rose.audiotags import RoseDate
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


def arrayfmt(xs: Iterable[str]) -> str:
    """Format an array as x, y & z."""
    xs = list(xs)
    if len(xs) == 0:
        return ""
    if len(xs) == 1:
        return xs[0]
    return ", ".join(xs[:-1]) + " & " + xs[-1]


def artistsarrayfmt(xs: Iterable[Artist]) -> str:
    """Format an array of Artists."""
    return arrayfmt([x.name for x in xs if not x.alias])


def artistsfmt(a: ArtistMapping, *, omit: list[str] | None = None) -> str:
    """Format a mapping of artists."""
    omit = omit or []

    r = artistsarrayfmt(a.main)
    if a.djmixer and "djmixer" not in omit:
        r = artistsarrayfmt(a.djmixer) + " pres. " + r
    elif a.composer and "composer" not in omit:
        r = artistsarrayfmt(a.composer) + " performed by " + r
    if a.conductor and "conductor" not in omit:
        r += " under " + artistsarrayfmt(a.conductor)
    if a.guest and "guest" not in omit:
        r += " (feat. " + artistsarrayfmt(a.guest) + ")"
    if a.producer and "producer" not in omit:
        r += " (prod. " + artistsarrayfmt(a.producer) + ")"
    if r == "":
        return "Unknown Artists"
    return r


def sortorder(x: str) -> str:
    try:
        first, last = x.rsplit(" ", 1)
        return f"{last}, {first}"
    except ValueError:
        return x


def lastname(x: str) -> str:
    try:
        _, last = x.rsplit(" ", 1)
        return last
    except ValueError:
        return x


ENVIRONMENT = jinja2.Environment()
ENVIRONMENT.filters["arrayfmt"] = arrayfmt
ENVIRONMENT.filters["artistsarrayfmt"] = artistsarrayfmt
ENVIRONMENT.filters["artistsfmt"] = artistsfmt
ENVIRONMENT.filters["releasetypefmt"] = releasetypefmt
ENVIRONMENT.filters["sortorder"] = sortorder
ENVIRONMENT.filters["lastname"] = lastname


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
{{ releaseartists | artistsfmt }} -
{% if releasedate %}{{ releasedate.year }}.{% endif %}
{{ releasetitle }}
{% if releasetype == "single" %}- {{ releasetype | releasetypefmt }}{% endif %}
{% if new %}[NEW]{% endif %}
"""
)

DEFAULT_TRACK_TEMPLATE = PathTemplate(
    """
{% if disctotal > 1 %}{{ discnumber.rjust(2, '0') }}-{% endif %}{{ tracknumber.rjust(2, '0') }}.
{{ tracktitle }}
{% if trackartists.guest %}(feat. {{ trackartists.guest | artistsarrayfmt }}){% endif %}
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
{{ trackartists | artistsfmt }} -
{{ tracktitle }}
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


@dataclasses.dataclass
class PathContext:
    genre: str | None
    artist: str | None
    label: str | None
    collage: str | None
    playlist: str | None


def eval_release_template(
    template: PathTemplate,
    release: CachedRelease,
    context: PathContext | None = None,
    position: str | None = None,
) -> str:
    return _collapse_spacing(
        template.compiled.render(context=context, **_calc_release_variables(release, position))
    )


def eval_track_template(
    template: PathTemplate,
    track: CachedTrack,
    context: PathContext | None = None,
    position: str | None = None,
) -> str:
    return (
        _collapse_spacing(
            template.compiled.render(context=context, **_calc_track_variables(track, position))
        )
        + track.source_path.suffix
    )


def _calc_release_variables(release: CachedRelease, position: str | None) -> dict[str, Any]:
    return {
        "added_at": release.added_at,
        "releasetitle": release.releasetitle,
        "releasetype": release.releasetype,
        "releasedate": release.releasedate,
        "originaldate": release.originaldate,
        "compositiondate": release.compositiondate,
        "edition": release.edition,
        "catalognumber": release.catalognumber,
        "new": release.new,
        "disctotal": release.disctotal,
        "genres": release.genres,
        "parentgenres": release.parent_genres,
        "secondarygenres": release.secondary_genres,
        "parentsecondarygenres": release.parent_secondary_genres,
        "descriptors": release.descriptors,
        "labels": release.labels,
        "releaseartists": release.releaseartists,
        "position": position,
    }


def _calc_track_variables(track: CachedTrack, position: str | None) -> dict[str, Any]:
    return {
        "added_at": track.release.added_at,
        "tracktitle": track.tracktitle,
        "tracknumber": track.tracknumber,
        "tracktotal": track.tracktotal,
        "discnumber": track.discnumber,
        "disctotal": track.release.disctotal,
        "duration_seconds": track.duration_seconds,
        "trackartists": track.trackartists,
        "releasetitle": track.release.releasetitle,
        "releasetype": track.release.releasetype,
        "releasedate": track.release.releasedate,
        "originaldate": track.release.originaldate,
        "compositiondate": track.release.compositiondate,
        "edition": track.release.edition,
        "catalognumber": track.release.catalognumber,
        "new": track.release.new,
        "genres": track.release.genres,
        "parentgenres": track.release.parent_genres,
        "secondarygenres": track.release.secondary_genres,
        "parentsecondarygenres": track.release.parent_secondary_genres,
        "descriptors": track.release.descriptors,
        "labels": track.release.labels,
        "releaseartists": track.release.releaseartists,
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


def _get_preview_releases(c: Config) -> tuple[CachedRelease, CachedRelease, CachedRelease]:
    from rose.cache import CachedRelease

    kimlip = CachedRelease(
        id="018b268e-ff1e-7a0c-9ac8-7bbb282761f2",
        source_path=c.music_source_dir / "LOONA - 2017. Kim Lip",
        cover_image_path=None,
        added_at="2023-04-20:23:45Z",
        datafile_mtime="999",
        releasetitle="Kim Lip",
        releasetype="single",
        releasedate=RoseDate(2017, 5, 23),
        originaldate=RoseDate(2017, 5, 23),
        compositiondate=None,
        edition=None,
        catalognumber="CMCC11088",
        new=True,
        disctotal=1,
        genres=["K-Pop", "Dance-Pop", "Contemporary R&B"],
        parent_genres=["Pop", "R&B"],
        secondary_genres=["Synth Funk", "Synthpop", "Future Bass"],
        parent_secondary_genres=["Funk", "Pop"],
        descriptors=[
            "Female Vocalist",
            "Mellow",
            "Sensual",
            "Ethereal",
            "Love",
            "Lush",
            "Romantic",
            "Warm",
            "Melodic",
            "Passionate",
            "Nocturnal",
            "Summer",
        ],
        labels=["BlockBerryCreative"],
        releaseartists=ArtistMapping(main=[Artist("Kim Lip")]),
        metahash="0",
    )

    youngforever = CachedRelease(
        id="018b6021-f1e5-7d4b-b796-440fbbea3b13",
        source_path=c.music_source_dir / "BTS - 2016. Young Forever (花樣年華)",
        cover_image_path=None,
        added_at="2023-06-09:23:45Z",
        datafile_mtime="999",
        releasetitle="Young Forever (花樣年華)",
        releasetype="album",
        releasedate=RoseDate(2016),
        originaldate=RoseDate(2016),
        compositiondate=None,
        edition="Deluxe",
        catalognumber="L200001238",
        new=False,
        disctotal=2,
        genres=["K-Pop"],
        parent_genres=["Pop"],
        secondary_genres=["Pop Rap", "Electropop"],
        parent_secondary_genres=["Hip Hop", "Electronic"],
        descriptors=[
            "Autumn",
            "Passionate",
            "Melodic",
            "Romantic",
            "Eclectic",
            "Melancholic",
            "Male Vocalist",
            "Sentimental",
            "Uplifting",
            "Breakup",
            "Love",
            "Anthemic",
            "Lush",
            "Bittersweet",
            "Spring",
        ],
        labels=["BIGHIT"],
        releaseartists=ArtistMapping(main=[Artist("BTS")]),
        metahash="0",
    )

    debussy = CachedRelease(
        id="018b268e-de0c-7cb2-8ffa-bcc2083c94e6",
        source_path=c.music_source_dir
        / "Debussy - 1907. Images performed by Cleveland Orchestra under Pierre Boulez (1992)",
        cover_image_path=None,
        added_at="2023-09-06:23:45Z",
        datafile_mtime="999",
        releasetitle="Images",
        releasetype="album",
        releasedate=RoseDate(1992),
        originaldate=RoseDate(1991),
        compositiondate=RoseDate(1907),
        edition=None,
        catalognumber="435-766 2",
        new=False,
        disctotal=2,
        genres=["Impressionism, Orchestral"],
        parent_genres=["Classical"],
        secondary_genres=["Tone Poem"],
        parent_secondary_genres=["Orchestral"],
        descriptors=["Orchestral"],
        labels=["Deustche Grammophon"],
        releaseartists=ArtistMapping(
            main=[Artist("Cleveland Orchestra")],
            composer=[Artist("Claude Debussy")],
            conductor=[Artist("Pierre Boulez")],
        ),
        metahash="0",
    )

    return kimlip, youngforever, debussy


def _preview_release_template(c: Config, label: str, template: PathTemplate) -> None:
    # Import cycle trick :)
    kimlip, youngforever, debussy = _get_preview_releases(c)
    click.secho(f"{label}:", dim=True, underline=True)
    click.secho("  Sample 1: ", dim=True, nl=False)
    click.secho(eval_release_template(template, kimlip, position="1"))
    click.secho("  Sample 2: ", dim=True, nl=False)
    click.secho(eval_release_template(template, youngforever, position="2"))
    click.secho("  Sample 3: ", dim=True, nl=False)
    click.secho(eval_release_template(template, debussy, position="3"))


def _preview_track_template(c: Config, label: str, template: PathTemplate) -> None:
    # Import cycle trick :)
    from rose.cache import CachedTrack

    kimlip, youngforever, debussy = _get_preview_releases(c)

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
        duration_seconds=230,
        trackartists=ArtistMapping(main=[Artist("Kim Lip")]),
        metahash="0",
        release=kimlip,
    )
    click.secho(eval_track_template(template, track, position="1"))

    click.secho("  Sample 2: ", dim=True, nl=False)
    track = CachedTrack(
        id="018b6021-f1e5-7d4b-b796-440fbbea3b15",
        source_path=c.music_source_dir
        / "BTS - 2016. Young Forever (花樣年華)"
        / "02-05. House of Cards.opus",
        source_mtime="999",
        tracktitle="House of Cards",
        tracknumber="5",
        tracktotal=8,
        discnumber="2",
        duration_seconds=226,
        trackartists=ArtistMapping(main=[Artist("BTS")]),
        metahash="0",
        release=youngforever,
    )
    click.secho(eval_track_template(template, track, position="2"))

    click.secho("  Sample 3: ", dim=True, nl=False)
    track = CachedTrack(
        id="018b6514-6e65-78cc-94a5-fdb17418f090",
        source_path=c.music_source_dir
        / "Debussy - 1907. Images performed by Cleveland Orchestra under Pierre Boulez (1992)"
        / "01. Gigues: Modéré.opus",
        source_mtime="999",
        tracktitle="Gigues: Modéré.opus",
        tracknumber="1",
        tracktotal=6,
        discnumber="1",
        duration_seconds=444,
        trackartists=ArtistMapping(
            main=[Artist("Cleveland Orchestra")],
            composer=[Artist("Claude Debussy")],
            conductor=[Artist("Pierre Boulez")],
        ),
        metahash="0",
        release=debussy,
    )
    click.secho(eval_track_template(template, track, position="3"))
