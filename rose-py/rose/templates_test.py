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
