import base64
import shutil
from dataclasses import replace
from pathlib import Path
from typing import no_type_check

import mutagen
import mutagen.flac
import mutagen.id3
import mutagen.mp4
import pytest

from conftest import TEST_TAGGER
from rose.audiotags import (
    AudioTags,
    RoseDate,
    UnsupportedTagValueTypeError,
    _split_tag,
    format_artist_string,
    parse_artist_string,
    read_embedded_arts,
    remove_embedded_arts,
)
from rose.common import Artist, ArtistMapping
from rose.config import Config


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
    assert af.releasetitle == "A Cool Album"
    assert af.releasetype == "album"
    assert af.releasedate == RoseDate(1990, 2, 5)
    assert af.originaldate == RoseDate(1990)
    assert af.compositiondate == RoseDate(1984)
    assert af.genre == ["Electronic", "House"]
    assert af.secondarygenre == ["Minimal", "Ambient"]
    assert af.descriptor == ["Lush", "Warm"]
    assert af.label == ["A Cool Label"]
    assert af.catalognumber == "DN-420"
    assert af.edition == "Japan"
    assert af.releaseartists.main == [Artist("Artist A"), Artist("Artist B")]

    assert af.tracknumber == track_num
    assert af.tracktotal == 5
    assert af.discnumber == "1"
    assert af.disctotal == 1

    assert af.tracktitle == f"Track {track_num}"
    assert af.trackartists == ArtistMapping(
        main=[Artist("Artist A"), Artist("Artist B")],
        guest=[Artist("Artist C"), Artist("Artist D")],
        remixer=[Artist("Artist AB"), Artist("Artist BC")],
        producer=[Artist("Artist CD"), Artist("Artist DE")],
        composer=[Artist("Artist EF"), Artist("Artist FG")],
        conductor=[Artist("Artist GH"), Artist("Artist HI")],
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
def test_flush(config: Config, isolated_dir: Path, filename: str, track_num: str, duration: int) -> None:
    """Test the flush by flushing the file, then asserting that all the tags still read properly."""
    fpath = isolated_dir / filename
    shutil.copyfile(TEST_TAGGER / filename, fpath)
    af = AudioTags.from_file(fpath)
    # Inject one special case into here: modify the djmixer artist. This checks that we also clear
    # the original djmixer tag, so that the next read does not contain Artist EF and Artist FG.
    af.trackartists.djmixer = [Artist("New")]
    # Also test date writing.
    af.originaldate = RoseDate(1990, 4, 20)
    af.flush(config)
    af = AudioTags.from_file(fpath)

    assert af.releasetitle == "A Cool Album"
    assert af.releasetype == "album"
    assert af.releasedate == RoseDate(1990, 2, 5)
    assert af.originaldate == RoseDate(1990, 4, 20)
    assert af.compositiondate == RoseDate(1984)
    assert af.genre == ["Electronic", "House"]
    assert af.secondarygenre == ["Minimal", "Ambient"]
    assert af.descriptor == ["Lush", "Warm"]
    assert af.label == ["A Cool Label"]
    assert af.catalognumber == "DN-420"
    assert af.edition == "Japan"
    assert af.releaseartists.main == [Artist("Artist A"), Artist("Artist B")]

    assert af.tracknumber == track_num
    assert af.discnumber == "1"

    assert af.tracktitle == f"Track {track_num}"
    assert af.trackartists == ArtistMapping(
        main=[Artist("Artist A"), Artist("Artist B")],
        guest=[Artist("Artist C"), Artist("Artist D")],
        remixer=[Artist("Artist AB"), Artist("Artist BC")],
        producer=[Artist("Artist CD"), Artist("Artist DE")],
        composer=[Artist("Artist EF"), Artist("Artist FG")],
        conductor=[Artist("Artist GH"), Artist("Artist HI")],
        djmixer=[Artist("New")],
    )
    assert af.duration_sec == duration


def test_write_parent_genres(config: Config, isolated_dir: Path) -> None:
    config = replace(config, write_parent_genres=True)

    filename = "track1.flac"
    fpath = isolated_dir / filename
    shutil.copyfile(TEST_TAGGER / filename, fpath)
    af = AudioTags.from_file(fpath)
    # Inject one special case into here: modify the djmixer artist. This checks that we also clear
    # the original djmixer tag, so that the next read does not contain Artist EF and Artist FG.
    af.trackartists.djmixer = [Artist("New")]
    # Also test date writing.
    af.originaldate = RoseDate(1990, 4, 20)
    af.flush(config)

    # Check that parents show up in raw tags (or not if there are no parents).
    mf = mutagen.File(fpath)  # type: ignore
    assert mf is not None
    assert mf.tags["genre"] == ["Electronic;House\\\\PARENTS:\\\\Dance;Electronic Dance Music"]
    assert mf.tags["secondarygenre"] == ["Minimal;Ambient"]

    af = AudioTags.from_file(fpath)
    assert af.genre == ["Electronic", "House"]
    assert af.secondarygenre == ["Minimal", "Ambient"]


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
def test_id_assignment(config: Config, isolated_dir: Path, filename: str) -> None:
    """Test the read/write for the nonstandard Rose ID tags."""
    fpath = isolated_dir / filename
    shutil.copyfile(TEST_TAGGER / filename, fpath)

    af = AudioTags.from_file(fpath)
    af.id = "ahaha"
    af.release_id = "bahaha"
    af.flush(config)

    af = AudioTags.from_file(fpath)
    assert af.id == "ahaha"
    assert af.release_id == "bahaha"


@pytest.mark.parametrize(
    "filename",
    ["track1.flac", "track2.m4a", "track3.mp3", "track4.vorbis.ogg", "track5.opus.ogg"],
)
def test_releasetype_normalization(config: Config, isolated_dir: Path, filename: str) -> None:
    """Test the flush by flushing the file, then asserting that all the tags still read properly."""
    fpath = isolated_dir / filename
    shutil.copyfile(TEST_TAGGER / filename, fpath)

    # Check that release type is read correctly.
    af = AudioTags.from_file(fpath)
    assert af.releasetype == "album"
    # Assert that attempting to flush a stupid value fails.
    af.releasetype = "lalala"
    with pytest.raises(UnsupportedTagValueTypeError):
        af.flush(config)
    # Flush it anyways...
    af.flush(config, validate=False)
    # Check that stupid release type is normalized as unknown.
    af = AudioTags.from_file(fpath)
    assert af.releasetype == "unknown"
    # And now assert that the read is case insensitive.
    af.releasetype = "ALBUM"
    af.flush(config, validate=False)
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
    assert format_artist_string(ArtistMapping(djmixer=[Artist("A")], main=[Artist("C"), Artist("D")])) == "A pres. C;D"
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


@no_type_check
def embed_art(path: Path, data: bytes, picture_type: int = 3) -> None:
    """Embed an image into an audio file, in addition to the images that are already embedded."""
    m = mutagen.File(path)
    assert m is not None
    if path.suffix == ".mp3":
        desc = str(len(m.tags.getall("APIC")))
        m.tags.add(mutagen.id3.APIC(encoding=3, mime="image/jpeg", type=picture_type, desc=desc, data=data))
    elif path.suffix == ".m4a":
        m.tags["covr"] = [*m.tags.get("covr", []), mutagen.mp4.MP4Cover(data)]
    else:
        pic = mutagen.flac.Picture()
        pic.type = picture_type
        pic.mime = "image/jpeg"
        pic.data = data
        if path.suffix == ".flac":
            m.add_picture(pic)
        else:
            encoded = base64.b64encode(pic.write()).decode("ascii")
            m.tags["metadata_block_picture"] = [*m.tags.get("metadata_block_picture", []), encoded]
    m.save()


@pytest.mark.parametrize(
    "filename",
    ["track1.flac", "track2.m4a", "track3.mp3", "track4.vorbis.ogg", "track5.opus.ogg"],
)
def test_embedded_arts(isolated_dir: Path, filename: str) -> None:
    fpath = isolated_dir / filename
    shutil.copyfile(TEST_TAGGER / filename, fpath)
    # Some of the test files come with an embedded image.
    remove_embedded_arts(fpath, read_embedded_arts(fpath))
    assert read_embedded_arts(fpath) == []
    assert not remove_embedded_arts(fpath, [b"front"])

    # MP4 has no picture types: the first image is the cover.
    first, second = (b"front", b"back") if filename.endswith(".m4a") else (b"back", b"front")
    embed_art(fpath, first, picture_type=3 if first == b"front" else 4)
    embed_art(fpath, second, picture_type=3 if second == b"front" else 4)
    assert read_embedded_arts(fpath) == [b"front", b"back"]

    assert not remove_embedded_arts(fpath, [b"other"])
    assert remove_embedded_arts(fpath, [b"front"])
    assert read_embedded_arts(fpath) == [b"back"]
    assert remove_embedded_arts(fpath, [b"back"])
    assert read_embedded_arts(fpath) == []
    # The other tags are untouched.
    assert AudioTags.from_file(fpath).releasetitle == "A Cool Album"
