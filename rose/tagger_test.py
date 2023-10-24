import shutil
from pathlib import Path

import pytest

from conftest import TEST_TAGGER
from rose.artiststr import Artists
from rose.tagger import AudioFile, _split_tag


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
    tf = AudioFile.from_file(TEST_TAGGER / filename)
    assert tf.track_number == track_num
    assert tf.title == f"Track {track_num}"

    assert tf.album == "A Cool Album"
    assert tf.release_type == "Album"
    assert tf.year == 1990
    assert tf.disc_number == "1"
    assert tf.genre == ["Electronic", "House"]
    assert tf.label == ["A Cool Label"]

    assert tf.album_artists.main == ["Artist A", "Artist B"]
    assert tf.artists == Artists(
        main=["Artist GH", "Artist HI"],
        guest=["Artist C", "Artist A"],
        remixer=["Artist AB", "Artist BC"],
        producer=["Artist CD", "Artist DE"],
        composer=["Artist EF", "Artist FG"],
        djmixer=["Artist IJ", "Artist JK"],
    )
    assert tf.duration_sec == duration


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
    AudioFile.from_file(fpath).flush()
    tf = AudioFile.from_file(fpath)

    assert tf.track_number == track_num
    assert tf.title == f"Track {track_num}"

    assert tf.album == "A Cool Album"
    assert tf.release_type == "Album"
    assert tf.year == 1990
    assert tf.disc_number == "1"
    assert tf.genre == ["Electronic", "House"]
    assert tf.label == ["A Cool Label"]

    assert tf.album_artists.main == ["Artist A", "Artist B"]
    assert tf.artists == Artists(
        main=["Artist GH", "Artist HI"],
        guest=["Artist C", "Artist A"],
        remixer=["Artist AB", "Artist BC"],
        producer=["Artist CD", "Artist DE"],
        composer=["Artist EF", "Artist FG"],
        djmixer=["Artist IJ", "Artist JK"],
    )
    assert tf.duration_sec == duration


def test_split_tag() -> None:
    assert _split_tag(r"a \\ b") == ["a", "b"]
    assert _split_tag(r"a \ b") == [r"a \ b"]
    assert _split_tag("a;b") == ["a", "b"]
    assert _split_tag("a; b") == ["a", "b"]
    assert _split_tag("a vs. b") == ["a", "b"]
    assert _split_tag("a / b") == ["a", "b"]
