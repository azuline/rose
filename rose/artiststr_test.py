from rose.artiststr import ArtistMapping, format_artist_string, parse_artist_string


def test_parse_artist_string() -> None:
    assert parse_artist_string("A;B feat. C;D") == ArtistMapping(
        main=["A", "B"],
        guest=["C", "D"],
    )
    assert parse_artist_string("A pres. C;D") == ArtistMapping(
        djmixer=["A"],
        main=["C", "D"],
    )
    assert parse_artist_string("A performed by C;D") == ArtistMapping(
        composer=["A"],
        main=["C", "D"],
    )
    assert parse_artist_string("A pres. B;C feat. D;E") == ArtistMapping(
        djmixer=["A"],
        main=["B", "C"],
        guest=["D", "E"],
    )
    # Test the deduplication handling.
    assert parse_artist_string("A pres. B", dj="A") == ArtistMapping(
        djmixer=["A"],
        main=["B"],
    )


def test_format_artist_string() -> None:
    assert format_artist_string(ArtistMapping(main=["A", "B"], guest=["C", "D"])) == "A;B feat. C;D"
    assert format_artist_string(ArtistMapping(djmixer=["A"], main=["C", "D"])) == "A pres. C;D"
    assert (
        format_artist_string(ArtistMapping(composer=["A"], main=["C", "D"])) == "A performed by C;D"
    )
    assert (
        format_artist_string(ArtistMapping(djmixer=["A"], main=["B", "C"], guest=["D", "E"]))
        == "A pres. B;C feat. D;E"
    )
