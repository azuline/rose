import shutil
from pathlib import Path

import pytest

from conftest import TEST_TAGGER
from rose.artiststr import ArtistMapping
from rose.audiotags import (
    AudioTags,
    UnsupportedTagValueTypeError,
    _split_tag,
)


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
    assert af.track_number == track_num
    assert af.title == f"Track {track_num}"

    assert af.album == "A Cool Album"
    assert af.release_type == "album"
    assert af.year == 1990
    assert af.disc_number == "1"
    assert af.genre == ["Electronic", "House"]
    assert af.label == ["A Cool Label"]

    assert af.album_artists.main == ["Artist A", "Artist B"]
    assert af.artists == ArtistMapping(
        main=["Artist GH", "Artist HI"],
        guest=["Artist C", "Artist A"],
        remixer=["Artist AB", "Artist BC"],
        producer=["Artist CD", "Artist DE"],
        composer=["Artist EF", "Artist FG"],
        djmixer=["Artist IJ", "Artist JK"],
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
    af.artists.djmixer = ["New"]
    af.flush()
    af = AudioTags.from_file(fpath)

    assert af.track_number == track_num
    assert af.title == f"Track {track_num}"

    assert af.album == "A Cool Album"
    assert af.release_type == "album"
    assert af.year == 1990
    assert af.disc_number == "1"
    assert af.genre == ["Electronic", "House"]
    assert af.label == ["A Cool Label"]

    assert af.album_artists.main == ["Artist A", "Artist B"]
    assert af.artists == ArtistMapping(
        main=["Artist GH", "Artist HI"],
        guest=["Artist C", "Artist A"],
        remixer=["Artist AB", "Artist BC"],
        producer=["Artist CD", "Artist DE"],
        composer=["Artist EF", "Artist FG"],
        djmixer=["New"],
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
def test_release_type_normalization(isolated_dir: Path, filename: str) -> None:
    """Test the flush by flushing the file, then asserting that all the tags still read properly."""
    fpath = isolated_dir / filename
    shutil.copyfile(TEST_TAGGER / filename, fpath)

    # Check that release type is read correctly.
    af = AudioTags.from_file(fpath)
    assert af.release_type == "album"
    # Assert that attempting to flush a stupid value fails.
    af.release_type = "lalala"
    with pytest.raises(UnsupportedTagValueTypeError):
        af.flush()
    # Flush it anyways...
    af.flush(validate=False)
    # Check that stupid release type is normalized as unknown.
    af = AudioTags.from_file(fpath)
    assert af.release_type == "unknown"
    # And now assert that the read is case insensitive.
    af.release_type = "ALBUM"
    af.flush(validate=False)
    af = AudioTags.from_file(fpath)
    assert af.release_type == "album"


def test_split_tag() -> None:
    assert _split_tag(r"a \\ b") == ["a", "b"]
    assert _split_tag(r"a \ b") == [r"a \ b"]
    assert _split_tag("a;b") == ["a", "b"]
    assert _split_tag("a; b") == ["a", "b"]
    assert _split_tag("a vs. b") == ["a", "b"]
    assert _split_tag("a / b") == ["a", "b"]
