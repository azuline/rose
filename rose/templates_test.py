from copy import deepcopy
from pathlib import Path

from rose.cache import CachedArtist, CachedRelease, CachedTrack
from rose.templates import PathTemplateConfig, eval_release_template, eval_track_template

EMPTY_CACHED_RELEASE = CachedRelease(
    id="",
    source_path=Path(),
    cover_image_path=None,
    added_at="0000-01-01T00:00:00Z",
    datafile_mtime="999",
    title="",
    releasetype="unknown",
    year=None,
    new=False,
    multidisc=False,
    genres=[],
    labels=[],
    artists=[],
    formatted_artists="",
)

EMPTY_CACHED_TRACK = CachedTrack(
    id="",
    source_path=Path("hi.m4a"),
    source_mtime="",
    title="",
    release_id="",
    tracknumber="",
    discnumber="",
    duration_seconds=0,
    artists=[],
    formatted_artists="",
    release_multidisc=False,
)


def test_default_templates() -> None:
    templates = PathTemplateConfig()

    release = deepcopy(EMPTY_CACHED_RELEASE)
    release.title = "Title"
    release.year = 2023
    release.artists = [
        CachedArtist(name="A1", role="main"),
        CachedArtist(name="A2", role="main"),
        CachedArtist(name="A3", role="main"),
        CachedArtist(name="BB", role="guest"),
        CachedArtist(name="PP", role="producer"),
    ]
    release.releasetype = "single"
    assert (
        eval_release_template(templates.source.release, release)
        == "A1, A2 & A3 (feat. BB) (prod. PP) - 2023. Title - Single"
    )
    assert (
        eval_release_template(templates.artists.release, release)
        == "2023. Title (feat. BB) - Single"
    )
    assert (
        eval_release_template(templates.collages.release, release, "4")
        == "4. A1, A2 & A3 (feat. BB) (prod. PP) - 2023. Title - Single"
    )

    release = deepcopy(EMPTY_CACHED_RELEASE)
    release.title = "Title"
    assert eval_release_template(templates.source.release, release) == "Unknown Artists - Title"
    assert eval_release_template(templates.artists.release, release) == "0000. Title"
    assert (
        eval_release_template(templates.collages.release, release, "4")
        == "4. Unknown Artists - Title"
    )

    track = deepcopy(EMPTY_CACHED_TRACK)
    track.tracknumber = "2"
    track.title = "Trick"
    assert eval_track_template(templates.source.track, track) == "02. Trick.m4a"
    assert eval_track_template(templates.playlists, track, "4") == "4. Unknown Artists - Trick.m4a"

    track = deepcopy(EMPTY_CACHED_TRACK)
    track.release_multidisc = True
    track.discnumber = "4"
    track.tracknumber = "2"
    track.title = "Trick"
    track.artists = [
        CachedArtist(name="Main", role="main"),
        CachedArtist(name="Hi", role="guest"),
        CachedArtist(name="High", role="guest"),
        CachedArtist(name="Hye", role="guest"),
    ]
    assert (
        eval_track_template(templates.source.track, track)
        == "04-02. Trick (feat. Hi, High & Hye).m4a"
    )
    assert (
        eval_track_template(templates.playlists, track, "4")
        == "4. Main (feat. Hi, High & Hye) - Trick.m4a"
    )
