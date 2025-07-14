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

from rose.audiotags import RoseDate
from rose.common import Artist, ArtistMapping, RoseExpectedError

if typing.TYPE_CHECKING:
    import jinja2

    from rose.cache import Release, Track
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
    strs = [x.name for x in xs if not x.alias]
    return arrayfmt(strs) if len(strs) <= 3 else f"{strs[0]} et al."


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


# Global variable cache for a lazy initialization. We lazily initialize the Jinja environment to
# improve the CLI startup time.
__environment: jinja2.Environment | None = None


def get_environment() -> jinja2.Environment:
    global __environment
    if __environment:
        return __environment

    import jinja2

    __environment = jinja2.Environment()
    __environment.filters["arrayfmt"] = arrayfmt
    __environment.filters["artistsarrayfmt"] = artistsarrayfmt
    __environment.filters["artistsfmt"] = artistsfmt
    __environment.filters["releasetypefmt"] = releasetypefmt
    __environment.filters["sortorder"] = sortorder
    __environment.filters["lastname"] = lastname
    return __environment


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
        return get_environment().from_string(self.text)

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
class PathTemplateTriad:
    release: PathTemplate
    track: PathTemplate
    all_tracks: PathTemplate


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

DEFAULT_ALL_TRACKS_TEMPLATE = PathTemplate(
    """
{{ trackartists | artistsfmt }} -
{% if releasedate %}{{ releasedate.year }}.{% endif %}
{{ releasetitle }} -
{{ tracktitle }}
"""
)

DEFAULT_TEMPLATE_PAIR = PathTemplateTriad(
    release=DEFAULT_RELEASE_TEMPLATE,
    track=DEFAULT_TRACK_TEMPLATE,
    all_tracks=DEFAULT_ALL_TRACKS_TEMPLATE,
)


@dataclasses.dataclass
class PathTemplateConfig:
    source: PathTemplateTriad
    releases: PathTemplateTriad
    releases_new: PathTemplateTriad
    releases_added_on: PathTemplateTriad
    releases_released_on: PathTemplateTriad
    artists: PathTemplateTriad
    genres: PathTemplateTriad
    descriptors: PathTemplateTriad
    labels: PathTemplateTriad
    loose_tracks: PathTemplateTriad
    collages: PathTemplateTriad
    playlists: PathTemplate

    @classmethod
    def with_defaults(
        cls,
        default_triad: PathTemplateTriad = DEFAULT_TEMPLATE_PAIR,
    ) -> PathTemplateConfig:
        return PathTemplateConfig(
            source=deepcopy(default_triad),
            releases=deepcopy(default_triad),
            releases_new=deepcopy(default_triad),
            releases_added_on=PathTemplateTriad(
                release=PathTemplate("[{{ added_at[:10] }}] " + default_triad.release.text),
                track=deepcopy(default_triad.track),
                all_tracks=deepcopy(default_triad.all_tracks),
            ),
            releases_released_on=PathTemplateTriad(
                release=PathTemplate(
                    "[{{ originaldate or releasedate or '0000-00-00' }}] " + default_triad.release.text
                ),
                track=deepcopy(default_triad.track),
                all_tracks=deepcopy(default_triad.all_tracks),
            ),
            artists=deepcopy(default_triad),
            genres=deepcopy(default_triad),
            descriptors=deepcopy(default_triad),
            labels=deepcopy(default_triad),
            loose_tracks=deepcopy(default_triad),
            collages=PathTemplateTriad(
                release=PathTemplate("{{ position }}. " + default_triad.release.text),
                track=deepcopy(default_triad.track),
                all_tracks=deepcopy(default_triad.all_tracks),
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
        import jinja2

        key = ""
        try:
            key = "source.release"
            _ = self.source.release.compiled
            key = "source.track"
            _ = self.source.track.compiled
            key = "source.all_tracks"
            _ = self.source.all_tracks.compiled
            key = "releases.release"
            _ = self.releases.release.compiled
            key = "releases.track"
            _ = self.releases.track.compiled
            key = "releases.all_tracks"
            _ = self.releases.all_tracks.compiled
            key = "releases_new.release"
            _ = self.releases_new.release.compiled
            key = "releases_new.track"
            _ = self.releases_new.track.compiled
            key = "releases_new.all_tracks"
            _ = self.releases_new.all_tracks.compiled
            key = "releases_added_on.release"
            _ = self.releases_added_on.release.compiled
            key = "releases_added_on.track"
            _ = self.releases_added_on.track.compiled
            key = "releases_added_on.all_tracks"
            _ = self.releases_added_on.all_tracks.compiled
            key = "releases_released_on.release"
            _ = self.releases_released_on.release.compiled
            key = "releases_released_on.track"
            _ = self.releases_released_on.track.compiled
            key = "releases_released_on.all_tracks"
            _ = self.releases_released_on.all_tracks.compiled
            key = "artists.release"
            _ = self.artists.release.compiled
            key = "artists.track"
            _ = self.artists.track.compiled
            key = "artists.all_tracks"
            _ = self.artists.all_tracks.compiled
            key = "genres.release"
            _ = self.genres.release.compiled
            key = "genres.track"
            _ = self.genres.track.compiled
            key = "genres.all_tracks"
            _ = self.genres.all_tracks.compiled
            key = "descriptors.release"
            _ = self.descriptors.release.compiled
            key = "descriptors.track"
            _ = self.descriptors.track.compiled
            key = "descriptors.all_tracks"
            _ = self.descriptors.all_tracks.compiled
            key = "labels.release"
            _ = self.labels.release.compiled
            key = "labels.track"
            _ = self.labels.track.compiled
            key = "loose_tracks.release"
            _ = self.loose_tracks.release.compiled
            key = "loose_tracks.track"
            _ = self.loose_tracks.track.compiled
            key = "labels.all_tracks"
            _ = self.labels.all_tracks.compiled
            key = "collages.release"
            _ = self.collages.release.compiled
            key = "collages.track"
            _ = self.collages.track.compiled
            key = "collages.all_tracks"
            _ = self.collages.all_tracks.compiled
            key = "playlists"
            _ = self.playlists.compiled
        except jinja2.exceptions.TemplateSyntaxError as e:
            raise InvalidPathTemplateError(f"Failed to compile template: {e}", key=key) from e


@dataclasses.dataclass
class PathContext:
    genre: str | None
    artist: str | None
    label: str | None
    descriptor: str | None
    collage: str | None
    playlist: str | None


def evaluate_release_template(
    template: PathTemplate,
    release: Release,
    context: PathContext | None = None,
    position: str | None = None,
) -> str:
    return _collapse_spacing(template.compiled.render(context=context, **_calc_release_variables(release, position)))


def evaluate_track_template(
    template: PathTemplate,
    track: Track,
    context: PathContext | None = None,
    position: str | None = None,
) -> str:
    return (
        _collapse_spacing(template.compiled.render(context=context, **_calc_track_variables(track, position)))
        + track.source_path.suffix
    )


def _calc_release_variables(release: Release, position: str | None) -> dict[str, Any]:
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


def _calc_track_variables(track: Track, position: str | None) -> dict[str, Any]:
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


def get_sample_music(
    c: Config,
) -> tuple[tuple[Release, Track], tuple[Release, Track], tuple[Release, Track]]:
    from rose.cache import Release, Track

    kimlip_rls = Release(
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
    bts_rls = Release(
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
    debussy_rls = Release(
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
        parent_genres=["Modern Classical"],
        secondary_genres=["Tone Poem"],
        parent_secondary_genres=["Orchestral Music"],
        descriptors=["Orchestral Music"],
        labels=["Deustche Grammophon"],
        releaseartists=ArtistMapping(
            main=[Artist("Cleveland Orchestra")],
            composer=[Artist("Claude Debussy")],
            conductor=[Artist("Pierre Boulez")],
        ),
        metahash="0",
    )

    kimlip_trk = Track(
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
        release=kimlip_rls,
    )
    bts_trk = Track(
        id="018b6021-f1e5-7d4b-b796-440fbbea3b15",
        source_path=c.music_source_dir / "BTS - 2016. Young Forever (花樣年華)" / "02-05. House of Cards.opus",
        source_mtime="999",
        tracktitle="House of Cards",
        tracknumber="5",
        tracktotal=8,
        discnumber="2",
        duration_seconds=226,
        trackartists=ArtistMapping(main=[Artist("BTS")]),
        metahash="0",
        release=bts_rls,
    )
    debussy_trk = Track(
        id="018b6514-6e65-78cc-94a5-fdb17418f090",
        source_path=c.music_source_dir
        / "Debussy - 1907. Images performed by Cleveland Orchestra under Pierre Boulez (1992)"
        / "01. Gigues: Modéré.opus",
        source_mtime="999",
        tracktitle="Gigues: Modéré",
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
        release=debussy_rls,
    )

    return (kimlip_rls, kimlip_trk), (bts_rls, bts_trk), (debussy_rls, debussy_trk)

# TESTS

from copy import deepcopy
from pathlib import Path

from rose.audiotags import RoseDate
from rose.cache import Release, Track
from rose.common import Artist, ArtistMapping
from rose.config import Config
from rose.templates import (
    PathTemplate,
    PathTemplateConfig,
    evaluate_release_template,
    evaluate_track_template,
    get_sample_music,
)

EMPTY_CACHED_RELEASE = Release(
    id="",
    source_path=Path(),
    cover_image_path=None,
    added_at="0000-01-01T00:00:00Z",
    datafile_mtime="999",
    releasetitle="",
    releasetype="unknown",
    releasedate=None,
    originaldate=None,
    compositiondate=None,
    edition=None,
    catalognumber=None,
    new=False,
    disctotal=1,
    genres=[],
    parent_genres=[],
    secondary_genres=[],
    parent_secondary_genres=[],
    descriptors=[],
    labels=[],
    releaseartists=ArtistMapping(),
    metahash="0",
)

EMPTY_CACHED_TRACK = Track(
    id="",
    source_path=Path("hi.m4a"),
    source_mtime="",
    tracktitle="",
    tracknumber="",
    tracktotal=1,
    discnumber="",
    duration_seconds=0,
    trackartists=ArtistMapping(),
    metahash="0",
    release=EMPTY_CACHED_RELEASE,
)


def test_default_templates() -> None:
    templates = PathTemplateConfig.with_defaults()

    release = deepcopy(EMPTY_CACHED_RELEASE)
    release.releasetitle = "Title"
    release.releasedate = RoseDate(2023)
    release.releaseartists = ArtistMapping(
        main=[Artist("A1"), Artist("A2"), Artist("A3")],
        guest=[Artist("BB")],
        producer=[Artist("PP")],
    )
    release.releasetype = "single"
    assert (
        evaluate_release_template(templates.source.release, release)
        == "A1, A2 & A3 (feat. BB) (prod. PP) - 2023. Title - Single"
    )
    assert (
        evaluate_release_template(templates.collages.release, release, position="4")
        == "4. A1, A2 & A3 (feat. BB) (prod. PP) - 2023. Title - Single"
    )

    release = deepcopy(EMPTY_CACHED_RELEASE)
    release.releasetitle = "Title"
    assert evaluate_release_template(templates.source.release, release) == "Unknown Artists - Title"
    assert evaluate_release_template(templates.collages.release, release, position="4") == "4. Unknown Artists - Title"

    track = deepcopy(EMPTY_CACHED_TRACK)
    track.tracknumber = "2"
    track.tracktitle = "Trick"
    assert evaluate_track_template(templates.source.track, track) == "02. Trick.m4a"
    assert evaluate_track_template(templates.playlists, track, position="4") == "4. Unknown Artists - Trick.m4a"

    track = deepcopy(EMPTY_CACHED_TRACK)
    track.release.disctotal = 2
    track.discnumber = "4"
    track.tracknumber = "2"
    track.tracktitle = "Trick"
    track.trackartists = ArtistMapping(
        main=[Artist("Main")],
        guest=[Artist("Hi"), Artist("High"), Artist("Hye")],
    )
    assert evaluate_track_template(templates.source.track, track) == "04-02. Trick (feat. Hi, High & Hye).m4a"
    assert (
        evaluate_track_template(templates.playlists, track, position="4")
        == "4. Main (feat. Hi, High & Hye) - Trick.m4a"
    )


def test_classical(config: Config) -> None:
    """Test a complicated classical template."""

    template = PathTemplate(
        """
        {% if new %}{{ '{N}' }}{% endif %}
        {{ releaseartists.composer | map(attribute='name') | map('sortorder') | arrayfmt }} -
        {% if compositiondate %}{{ compositiondate }}.{% endif %}
        {{ releasetitle }}
        performed by {{ releaseartists | artistsfmt(omit=["composer"]) }}
        {% if releasedate %}({{ releasedate }}){% endif %}
        """
    )

    _, _, (debussy, _) = get_sample_music(config)

    assert (
        evaluate_release_template(template, debussy)
        == "Debussy, Claude - 1907. Images performed by Cleveland Orchestra under Pierre Boulez (1992)"
    )
