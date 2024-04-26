from copy import deepcopy
from pathlib import Path

import click
from click.testing import CliRunner

from rose.cache import CachedRelease, CachedTrack
from rose.common import Artist, ArtistMapping
from rose.config import Config
from rose.templates import (
    PathTemplate,
    PathTemplateConfig,
    _get_preview_releases,
    eval_release_template,
    eval_track_template,
    preview_path_templates,
)

EMPTY_CACHED_RELEASE = CachedRelease(
    id="",
    source_path=Path(),
    cover_image_path=None,
    added_at="0000-01-01T00:00:00Z",
    datafile_mtime="999",
    releasetitle="",
    releasetype="unknown",
    releaseyear=None,
    originalyear=None,
    compositionyear=None,
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

EMPTY_CACHED_TRACK = CachedTrack(
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
    release.releaseyear = 2023
    release.releaseartists = ArtistMapping(
        main=[Artist("A1"), Artist("A2"), Artist("A3")],
        guest=[Artist("BB")],
        producer=[Artist("PP")],
    )
    release.releasetype = "single"
    assert (
        eval_release_template(templates.source.release, release)
        == "A1, A2 & A3 (feat. BB) (prod. PP) - 2023. Title - Single"
    )
    assert (
        eval_release_template(templates.collages.release, release, position="4")
        == "4. A1, A2 & A3 (feat. BB) (prod. PP) - 2023. Title - Single"
    )

    release = deepcopy(EMPTY_CACHED_RELEASE)
    release.releasetitle = "Title"
    assert eval_release_template(templates.source.release, release) == "Unknown Artists - Title"
    assert (
        eval_release_template(templates.collages.release, release, position="4")
        == "4. Unknown Artists - Title"
    )

    track = deepcopy(EMPTY_CACHED_TRACK)
    track.tracknumber = "2"
    track.tracktitle = "Trick"
    assert eval_track_template(templates.source.track, track) == "02. Trick.m4a"
    assert (
        eval_track_template(templates.playlists, track, position="4")
        == "4. Unknown Artists - Trick.m4a"
    )

    track = deepcopy(EMPTY_CACHED_TRACK)
    track.release.disctotal = 2
    track.discnumber = "4"
    track.tracknumber = "2"
    track.tracktitle = "Trick"
    track.trackartists = ArtistMapping(
        main=[Artist("Main")],
        guest=[Artist("Hi"), Artist("High"), Artist("Hye")],
    )
    assert (
        eval_track_template(templates.source.track, track)
        == "04-02. Trick (feat. Hi, High & Hye).m4a"
    )
    assert (
        eval_track_template(templates.playlists, track, position="4")
        == "4. Main (feat. Hi, High & Hye) - Trick.m4a"
    )


def test_preview_templates(config: Config) -> None:
    runner = CliRunner()
    with runner.isolated_filesystem(), runner.isolation() as out_streams:
        preview_path_templates(config)
        out_streams[0].seek(0)
        output = click.unstyle(out_streams[0].read().decode())

    assert (
        output
        == """\
Source Directory - Release:
  Sample 1: Kim Lip - 2017. Kim Lip - Single [NEW]
  Sample 2: BTS - 2016. Young Forever (花樣年華)
  Sample 3: Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - 1992. Images
Source Directory - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus
  Sample 3: 01-01. Gigues: Modéré.opus.opus

1. All Releases - Release:
  Sample 1: Kim Lip - 2017. Kim Lip - Single [NEW]
  Sample 2: BTS - 2016. Young Forever (花樣年華)
  Sample 3: Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - 1992. Images
1. All Releases - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus
  Sample 3: 01-01. Gigues: Modéré.opus.opus

2. New Releases - Release:
  Sample 1: Kim Lip - 2017. Kim Lip - Single [NEW]
  Sample 2: BTS - 2016. Young Forever (花樣年華)
  Sample 3: Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - 1992. Images
2. New Releases - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus
  Sample 3: 01-01. Gigues: Modéré.opus.opus

3. Recently Added Releases - Release:
  Sample 1: [2023-04-20] Kim Lip - 2017. Kim Lip - Single [NEW]
  Sample 2: [2023-06-09] BTS - 2016. Young Forever (花樣年華)
  Sample 3: [2023-09-06] Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - 1992. Images
3. Recently Added Releases - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus
  Sample 3: 01-01. Gigues: Modéré.opus.opus

4. Artists - Release:
  Sample 1: Kim Lip - 2017. Kim Lip - Single [NEW]
  Sample 2: BTS - 2016. Young Forever (花樣年華)
  Sample 3: Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - 1992. Images
4. Artists - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus
  Sample 3: 01-01. Gigues: Modéré.opus.opus

5. Genres - Release:
  Sample 1: Kim Lip - 2017. Kim Lip - Single [NEW]
  Sample 2: BTS - 2016. Young Forever (花樣年華)
  Sample 3: Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - 1992. Images
5. Genres - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus
  Sample 3: 01-01. Gigues: Modéré.opus.opus

6. Labels - Release:
  Sample 1: Kim Lip - 2017. Kim Lip - Single [NEW]
  Sample 2: BTS - 2016. Young Forever (花樣年華)
  Sample 3: Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - 1992. Images
6. Labels - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus
  Sample 3: 01-01. Gigues: Modéré.opus.opus

7. Collages - Release:
  Sample 1: 1. Kim Lip - 2017. Kim Lip - Single [NEW]
  Sample 2: 2. BTS - 2016. Young Forever (花樣年華)
  Sample 3: 3. Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - 1992. Images
7. Collages - Track:
  Sample 1: 01. Eclipse.opus
  Sample 2: 02-05. House of Cards.opus
  Sample 3: 01-01. Gigues: Modéré.opus.opus

8. Playlists - Track:
  Sample 1: 1. Kim Lip - Eclipse.opus
  Sample 2: 2. BTS - House of Cards.opus
  Sample 3: 3. Claude Debussy performed by Cleveland Orchestra under Pierre Boulez - Gigues: Modéré.opus.opus
"""
    )


def test_classical(config: Config) -> None:
    """Test a complicated classical template."""

    template = PathTemplate(
        """
        {% if new %}{{ '{N}' }}{% endif %}
        {{ releaseartists.composer | map(attribute='name') | map('sortorder') | arrayfmt }} -
        {% if compositionyear %}{{ compositionyear }}.{% endif %}
        {{ releasetitle }}
        performed by {{ releaseartists | artistsfmt(omit=["composer"]) }}
        {% if releaseyear %}({{ releaseyear }}){% endif %}
        """
    )

    _, _, debussy = _get_preview_releases(config)
    assert (
        eval_release_template(template, debussy)
        == "Debussy, Claude - 1907. Images performed by Cleveland Orchestra under Pierre Boulez (1992)"
    )
