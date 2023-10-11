from rose.artiststr import Artists, format_artist_string, parse_artist_string


def test_parse_artist_string() -> None:
    assert parse_artist_string("A;B feat. C;D") == Artists(
        main=["A", "B"],
        guest=["C", "D"],
    )
    assert parse_artist_string("A pres. C;D") == Artists(
        djmixer=["A"],
        main=["C", "D"],
    )
    assert parse_artist_string("A performed by C;D") == Artists(
        composer=["A"],
        main=["C", "D"],
    )
    assert parse_artist_string("A pres. B;C feat. D;E") == Artists(
        djmixer=["A"],
        main=["B", "C"],
        guest=["D", "E"],
    )


def test_format_artist_string() -> None:
    assert format_artist_string(Artists(main=["A", "B"], guest=["C", "D"]), []) == "A;B feat. C;D"
    assert format_artist_string(Artists(djmixer=["A"], main=["C", "D"]), []) == "A pres. C;D"
    assert format_artist_string(Artists(composer=["A"], main=["C", "D"]), []) == "C;D"
    assert (
        format_artist_string(Artists(composer=["A"], main=["C", "D"]), ["Classical"])
        == "A performed by C;D"
    )
    assert (
        format_artist_string(Artists(djmixer=["A"], main=["B", "C"], guest=["D", "E"]), [])
        == "A pres. B;C feat. D;E"
    )
