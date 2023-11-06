import shutil
from pathlib import Path

import pytest

from conftest import TEST_TAGGER
from rose.audiotags import (
    AudioTags,
    UnsupportedTagValueTypeError,
    _split_tag,
    format_artist_string,
    parse_artist_string,
)
from rose.common import Artist, ArtistMapping


@pytest.mark.parametrize(
    ("filename", "track_num", "duration"),
    [
        ("track1.flac", "1", 2),
        ("track2.m4a", "2", 2),
        ("track3.mp3", "3", 1),
        ("track4.vorbis.ogg", "4", 1),
        ("track5.opus.ogg", "5", 1),
    ],
)
def test_getters(filename: str, track_num: str, duration: int) -> None:
    af = AudioTags.from_file(TEST_TAGGER / filename)
    assert af.tracknumber == track_num
    assert af.title == f"Track {track_num}"

    assert af.album == "A Cool Album"
    assert af.releasetype == "album"
    assert af.year == 1990
    assert af.discnumber == "1"
    assert af.genre == ["Electronic", "House"]
    assert af.label == ["A Cool Label"]

    assert af.albumartists.main == [Artist("Artist A"), Artist("Artist B")]
    assert af.trackartists == ArtistMapping(
        main=[Artist("Artist GH"), Artist("Artist HI")],
        guest=[Artist("Artist C"), Artist("Artist A")],
        remixer=[Artist("Artist AB"), Artist("Artist BC")],
        producer=[Artist("Artist CD"), Artist("Artist DE")],
        composer=[Artist("Artist EF"), Artist("Artist FG")],
        djmixer=[Artist("Artist IJ"), Artist("Artist JK")],
    )
    assert af.duration_sec == duration


@pytest.mark.parametrize(
    ("filename", "track_num", "duration"),
    [
        ("track1.flac", "1", 2),
        ("track2.m4a", "2", 2),
        ("track3.mp3", "3", 1),
        ("track4.vorbis.ogg", "4", 1),
        ("track5.opus.ogg", "5", 1),
    ],
)
def test_flush(isolated_dir: Path, filename: str, track_num: str, duration: int) -> None:
    """Test the flush by flushing the file, then asserting that all the tags still read properly."""
    fpath = isolated_dir / filename
    shutil.copyfile(TEST_TAGGER / filename, fpath)
    af = AudioTags.from_file(fpath)
    # Inject one special case into here: modify the djmixer artist. This checks that we also clear
    # the original djmixer tag, so that the next read does not contain Artist EF and Artist FG.
    af.trackartists.djmixer = [Artist("New")]
    af.flush()
    af = AudioTags.from_file(fpath)

    assert af.tracknumber == track_num
    assert af.title == f"Track {track_num}"

    assert af.album == "A Cool Album"
    assert af.releasetype == "album"
    assert af.year == 1990
    assert af.discnumber == "1"
    assert af.genre == ["Electronic", "House"]
    assert af.label == ["A Cool Label"]

    assert af.albumartists.main == [Artist("Artist A"), Artist("Artist B")]
    assert af.trackartists == ArtistMapping(
        main=[Artist("Artist GH"), Artist("Artist HI")],
        guest=[Artist("Artist C"), Artist("Artist A")],
        remixer=[Artist("Artist AB"), Artist("Artist BC")],
        producer=[Artist("Artist CD"), Artist("Artist DE")],
        composer=[Artist("Artist EF"), Artist("Artist FG")],
        djmixer=[Artist("New")],
    )
    assert af.duration_sec == duration


@pytest.mark.parametrize(
    "filename",
    [
        "track1.flac",
        "track2.m4a",
        "track3.mp3",
        "track4.vorbis.ogg",
        "track5.opus.ogg",
    ],
)
def test_id_assignment(isolated_dir: Path, filename: str) -> None:
    """Test the read/write for the nonstandard Rose ID tags."""
    fpath = isolated_dir / filename
    shutil.copyfile(TEST_TAGGER / filename, fpath)

    af = AudioTags.from_file(fpath)
    af.id = "ahaha"
    af.release_id = "bahaha"
    af.flush()

    af = AudioTags.from_file(fpath)
    assert af.id == "ahaha"
    assert af.release_id == "bahaha"


@pytest.mark.parametrize(
    "filename",
    ["track1.flac", "track2.m4a", "track3.mp3", "track4.vorbis.ogg", "track5.opus.ogg"],
)
def test_releasetype_normalization(isolated_dir: Path, filename: str) -> None:
    """Test the flush by flushing the file, then asserting that all the tags still read properly."""
    fpath = isolated_dir / filename
    shutil.copyfile(TEST_TAGGER / filename, fpath)

    # Check that release type is read correctly.
    af = AudioTags.from_file(fpath)
    assert af.releasetype == "album"
    # Assert that attempting to flush a stupid value fails.
    af.releasetype = "lalala"
    with pytest.raises(UnsupportedTagValueTypeError):
        af.flush()
    # Flush it anyways...
    af.flush(validate=False)
    # Check that stupid release type is normalized as unknown.
    af = AudioTags.from_file(fpath)
    assert af.releasetype == "unknown"
    # And now assert that the read is case insensitive.
    af.releasetype = "ALBUM"
    af.flush(validate=False)
    af = AudioTags.from_file(fpath)
    assert af.releasetype == "album"


def test_split_tag() -> None:
    assert _split_tag(r"a \\ b") == ["a", "b"]
    assert _split_tag(r"a \ b") == [r"a \ b"]
    assert _split_tag("a;b") == ["a", "b"]
    assert _split_tag("a; b") == ["a", "b"]
    assert _split_tag("a vs. b") == ["a", "b"]
    assert _split_tag("a / b") == ["a", "b"]


def test_parse_artist_string() -> None:
    assert parse_artist_string("A;B feat. C;D") == ArtistMapping(
        main=[Artist("A"), Artist("B")],
        guest=[Artist("C"), Artist("D")],
    )
    assert parse_artist_string("A pres. C;D") == ArtistMapping(
        djmixer=[Artist("A")],
        main=[Artist("C"), Artist("D")],
    )
    assert parse_artist_string("A performed by C;D") == ArtistMapping(
        composer=[Artist("A")],
        main=[Artist("C"), Artist("D")],
    )
    assert parse_artist_string("A pres. B;C feat. D;E") == ArtistMapping(
        djmixer=[Artist("A")],
        main=[Artist("B"), Artist("C")],
        guest=[Artist("D"), Artist("E")],
    )
    # Test the deduplication handling.
    assert parse_artist_string("A pres. B", dj="A") == ArtistMapping(
        djmixer=[Artist("A")],
        main=[Artist("B")],
    )


def test_format_artist_string() -> None:
    assert (
        format_artist_string(
            ArtistMapping(
                main=[Artist("A"), Artist("B")],
                guest=[Artist("C"), Artist("D")],
            )
        )
        == "A;B feat. C;D"
    )
    assert (
        format_artist_string(ArtistMapping(djmixer=[Artist("A")], main=[Artist("C"), Artist("D")]))
        == "A pres. C;D"
    )
    assert (
        format_artist_string(ArtistMapping(composer=[Artist("A")], main=[Artist("C"), Artist("D")]))
        == "A performed by C;D"
    )
    assert (
        format_artist_string(
            ArtistMapping(
                djmixer=[Artist("A")],
                main=[Artist("B"), Artist("C")],
                guest=[Artist("D"), Artist("E")],
            )
        )
        == "A pres. B;C feat. D;E"
    )
