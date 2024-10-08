import re

import click
import pytest

from rose.rule_parser import (
    Action,
    AddAction,
    DeleteAction,
    InvalidRuleError,
    Matcher,
    Pattern,
    ReplaceAction,
    Rule,
    RuleSyntaxError,
    SedAction,
    SplitAction,
    take,
)


def test_rule_str() -> None:
    rule = Rule.parse("tracktitle:Track", ["releaseartist,genre/replace:lalala"])
    assert str(rule) == "matcher=tracktitle:Track action=releaseartist,genre/replace:lalala"

    # Test that rules are quoted properly.
    rule = Rule.parse(r"tracktitle,releaseartist,genre::: ", [r"sed::::; "])
    assert (
        str(rule)
        == r"matcher='tracktitle,releaseartist,genre::: ' action='tracktitle,releaseartist,genre::: /sed::::; '"
    )

    # Test that custom action matcher is printed properly.
    rule = Rule.parse("tracktitle:Track", ["genre:lala/replace:lalala"])
    assert str(rule) == "matcher=tracktitle:Track action=genre:lala/replace:lalala"

    # Test that we print `matched` when action pattern is not null.
    rule = Rule.parse("genre:b", ["genre:h/replace:hi"])
    assert str(rule) == r"matcher=genre:b action=genre:h/replace:hi"


def test_rule_parse_matcher() -> None:
    assert Matcher.parse("tracktitle:Track") == Matcher(["tracktitle"], Pattern("Track"))
    assert Matcher.parse("tracktitle,tracknumber:Track") == Matcher(["tracktitle", "tracknumber"], Pattern("Track"))
    assert Matcher.parse(r"tracktitle,tracknumber:Tr::ck") == Matcher(["tracktitle", "tracknumber"], Pattern("Tr:ck"))
    assert Matcher.parse("tracktitle,tracknumber:Track:i") == Matcher(
        ["tracktitle", "tracknumber"], Pattern("Track", case_insensitive=True)
    )
    assert Matcher.parse(r"tracktitle:") == Matcher(["tracktitle"], Pattern(""))

    assert Matcher.parse("tracktitle:^Track") == Matcher(["tracktitle"], Pattern("Track", strict_start=True))
    assert Matcher.parse("tracktitle:Track$") == Matcher(["tracktitle"], Pattern("Track", strict_end=True))
    assert Matcher.parse(r"tracktitle:\^Track") == Matcher(["tracktitle"], Pattern(r"\^Track"))
    assert Matcher.parse(r"tracktitle:Track\$") == Matcher(["tracktitle"], Pattern(r"Track\$"))
    assert Matcher.parse(r"tracktitle:\^Track\$") == Matcher(["tracktitle"], Pattern(r"\^Track\$"))

    def test_err(rule: str, err: str) -> None:
        with pytest.raises(RuleSyntaxError) as exc:
            Matcher.parse(rule)
        assert click.unstyle(str(exc.value)) == err

    test_err(
        "tracknumber^Track$",
        """\
Failed to parse matcher, invalid syntax:

    tracknumber^Track$
    ^
    Invalid tag: must be one of {tracktitle, trackartist, trackartist[main], trackartist[guest], trackartist[remixer], trackartist[producer], trackartist[composer], trackartist[conductor], trackartist[djmixer], tracknumber, tracktotal, discnumber, disctotal, releasetitle, releaseartist, releaseartist[main], releaseartist[guest], releaseartist[remixer], releaseartist[producer], releaseartist[composer], releaseartist[conductor], releaseartist[djmixer], releasetype, releasedate, originaldate, compositiondate, edition, catalognumber, genre, secondarygenre, descriptor, label, new, artist}. The next character after a tag must be ':' or ','.
""",
    )

    test_err(
        "tracknumber",
        """\
Failed to parse matcher, invalid syntax:

    tracknumber
               ^
               Expected to find ',' or ':', found end of string.
""",
    )

    test_err(
        "tracktitle:Tr:ck",
        """\
Failed to parse matcher, invalid syntax:

    tracktitle:Tr:ck
                  ^
                  Unrecognized flag: Please specify one of the supported flags: `i` (case insensitive).
""",
    )

    test_err(
        "tracktitle:hi:i:hihi",
        """\
Failed to parse matcher, invalid syntax:

    tracktitle:hi:i:hihi
                    ^
                    Extra input found after end of matcher. Perhaps you meant to escape this colon?
""",
    )


def test_rule_parse_action() -> None:
    assert Action.parse(
        "replace:lalala",
        matcher=Matcher(tags=["tracktitle"], pattern=Pattern("haha")),
    ) == Action(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["tracktitle"],
        pattern=Pattern("haha"),
    )
    assert Action.parse("genre/replace:lalala") == Action(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["genre"],
        pattern=None,
    )
    assert Action.parse("tracknumber,genre/replace:lalala") == Action(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["tracknumber", "genre"],
        pattern=None,
    )
    assert Action.parse("genre:lala/replace:lalala") == Action(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["genre"],
        pattern=Pattern("lala"),
    )
    assert Action.parse("genre:lala:i/replace:lalala") == Action(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["genre"],
        pattern=Pattern("lala", case_insensitive=True),
    )
    assert Action.parse(
        "matched:^x/replace:lalala",
        matcher=Matcher(tags=["tracktitle"], pattern=Pattern("haha")),
    ) == Action(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["tracktitle"],
        pattern=Pattern("^x"),
    )

    # Test that case insensitivity is inherited from the matcher.
    assert Action.parse(
        "replace:lalala",
        matcher=Matcher(tags=["tracktitle"], pattern=Pattern("haha", case_insensitive=True)),
    ) == Action(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["tracktitle"],
        pattern=Pattern("haha", case_insensitive=True),
    )

    # Test that the action excludes the immutable *total tags.
    assert Action.parse(
        "replace:5",
        matcher=Matcher(
            tags=["tracknumber", "tracktotal", "discnumber", "disctotal"],
            pattern=Pattern("1"),
        ),
    ) == Action(
        behavior=ReplaceAction(replacement="5"),
        tags=["tracknumber", "discnumber"],
        pattern=Pattern("1"),
    )

    assert Action.parse(
        "sed:lalala:hahaha",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    ) == Action(
        behavior=SedAction(src=re.compile("lalala"), dst="hahaha"),
        tags=["genre"],
        pattern=Pattern("haha"),
    )
    assert Action.parse(
        r"split:::",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    ) == Action(
        behavior=SplitAction(delimiter=":"),
        tags=["genre"],
        pattern=Pattern("haha"),
    )
    assert Action.parse(
        r"split:::",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    ) == Action(
        behavior=SplitAction(delimiter=":"),
        tags=["genre"],
        pattern=Pattern("haha"),
    )
    assert Action.parse(
        r"add:cute",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    ) == Action(
        behavior=AddAction(value="cute"),
        tags=["genre"],
        pattern=Pattern("haha"),
    )
    assert Action.parse(
        r"delete:",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    ) == Action(
        behavior=DeleteAction(),
        tags=["genre"],
        pattern=Pattern("haha"),
    )
    assert Action.parse(
        r"delete:",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    ) == Action(
        behavior=DeleteAction(),
        tags=["genre"],
        pattern=Pattern("haha"),
    )

    def test_err(rule: str, err: str, matcher: Matcher | None = None) -> None:
        with pytest.raises(RuleSyntaxError) as exc:
            Action.parse(rule, 1, matcher)
        assert click.unstyle(str(exc.value)) == err

    test_err(
        "tracktitle:hello/:delete",
        """\
Failed to parse action 1, invalid syntax:

    tracktitle:hello/:delete
                     ^
                     Invalid action kind: must be one of {replace, sed, split, add, delete}.
""",
    )

    test_err(
        "haha/delete",
        """\
Failed to parse action 1, invalid syntax:

    haha/delete
    ^
    Invalid tag: must be one of {tracktitle, trackartist, trackartist[main], trackartist[guest], trackartist[remixer], trackartist[producer], trackartist[composer], trackartist[conductor], trackartist[djmixer], tracknumber, discnumber, releasetitle, releaseartist, releaseartist[main], releaseartist[guest], releaseartist[remixer], releaseartist[producer], releaseartist[composer], releaseartist[conductor], releaseartist[djmixer], releasetype, releasedate, originaldate, compositiondate, edition, catalognumber, genre, secondarygenre, descriptor, label, new, artist}. The next character after a tag must be ':' or ','.
""",
    )

    test_err(
        "tracktitler/delete",
        """\
Failed to parse action 1, invalid syntax:

    tracktitler/delete
    ^
    Invalid tag: must be one of {tracktitle, trackartist, trackartist[main], trackartist[guest], trackartist[remixer], trackartist[producer], trackartist[composer], trackartist[conductor], trackartist[djmixer], tracknumber, discnumber, releasetitle, releaseartist, releaseartist[main], releaseartist[guest], releaseartist[remixer], releaseartist[producer], releaseartist[composer], releaseartist[conductor], releaseartist[djmixer], releasetype, releasedate, originaldate, compositiondate, edition, catalognumber, genre, secondarygenre, descriptor, label, new, artist}. The next character after a tag must be ':' or ','.
""",
    )

    test_err(
        "tracktitle:haha:delete",
        """\
Failed to parse action 1, invalid syntax:

    tracktitle:haha:delete
    ^
    Invalid action kind: must be one of {replace, sed, split, add, delete}. If this is pointing at your pattern, you forgot to put a `/` between the matcher section and the action section.
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "tracktitle:haha:sed/hi:bye",
        """\
Failed to parse action 1, invalid syntax:

    tracktitle:haha:sed/hi:bye
                    ^
                    Unrecognized flag: Either you forgot a colon here (to end the matcher), or this is an invalid matcher flag. The only supported flag is `i` (case insensitive).
""",
    )

    test_err(
        "hahaha",
        """\
Failed to parse action 1, invalid syntax:

    hahaha
    ^
    Invalid action kind: must be one of {replace, sed, split, add, delete}.
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "replace",
        """\
Failed to parse action 1, invalid syntax:

    replace
           ^
           Replacement not found: must specify a non-empty replacement. Use the delete action to remove a value.
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )
    test_err(
        "replace:haha:",
        """\
Failed to parse action 1, invalid syntax:

    replace:haha:
                ^
                Found another section after the replacement, but the replacement must be the last section. Perhaps you meant to escape this colon?
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "sed",
        """\
Failed to parse action 1, invalid syntax:

    sed
       ^
       Empty sed pattern found: must specify a non-empty pattern. Example: sed:pattern:replacement
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "sed:hihi",
        """\
Failed to parse action 1, invalid syntax:

    sed:hihi
            ^
            Sed replacement not found: must specify a sed replacement section. Example: sed:hihi:replacement.
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "sed:invalid[",
        """\
Failed to parse action 1, invalid syntax:

    sed:invalid[
        ^
        Failed to compile the sed pattern regex: invalid pattern: unterminated character set at position 7
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "sed:hihi:byebye:",
        """\
Failed to parse action 1, invalid syntax:

    sed:hihi:byebye:
                   ^
                   Found another section after the sed replacement, but the sed replacement must be the last section. Perhaps you meant to escape this colon?
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "split",
        """\
Failed to parse action 1, invalid syntax:

    split
         ^
         Delimiter not found: must specify a non-empty delimiter to split on.
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "split:hi:",
        """\
Failed to parse action 1, invalid syntax:

    split:hi:
            ^
            Found another section after the delimiter, but the delimiter must be the last section. Perhaps you meant to escape this colon?
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "split:",
        """\
Failed to parse action 1, invalid syntax:

    split:
          ^
          Delimiter not found: must specify a non-empty delimiter to split on.
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "add",
        """\
Failed to parse action 1, invalid syntax:

    add
       ^
       Value not found: must specify a non-empty value to add.
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "add:hi:",
        """\
Failed to parse action 1, invalid syntax:

    add:hi:
          ^
          Found another section after the value, but the value must be the last section. Perhaps you meant to escape this colon?
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "add:",
        """\
Failed to parse action 1, invalid syntax:

    add:
        ^
        Value not found: must specify a non-empty value to add.
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "delete:h",
        """\
Failed to parse action 1, invalid syntax:

    delete:h
           ^
           Found another section after the action kind, but the delete action has no parameters. Please remove this section.
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "delete",
        """\
Failed to parse action 1, invalid syntax:

    delete
    ^
    Tags/pattern section not found. Must specify tags to modify, since there is no matcher to default to. Make sure you are formatting your action like {tags}:{pattern}/{kind}:{args} (where `:{pattern}` is optional)
""",
    )

    test_err(
        "tracktotal/replace:1",
        """\
Failed to parse action 1, invalid syntax:

    tracktotal/replace:1
    ^
    Invalid tag: tracktotal is not modifiable.
""",
    )

    test_err(
        "disctotal/replace:1",
        """\
Failed to parse action 1, invalid syntax:

    disctotal/replace:1
    ^
    Invalid tag: disctotal is not modifiable.
""",
    )


@pytest.mark.parametrize(
    ("matcher", "action"),
    [("tracktitle:Track", "delete")],
)
def test_rule_parsing_end_to_end_1(matcher: str, action: str) -> None:
    assert str(Rule.parse(matcher, [action])) == f"matcher={matcher} action={matcher}/{action}"


@pytest.mark.parametrize(
    ("matcher", "action"),
    [
        (r"tracktitle:\^Track", "delete"),
        (r"tracktitle:Track\$", "delete"),
        (r"tracktitle:\^Track\$", "delete"),
    ],
)
def test_rule_parsing_end_to_end_2(matcher: str, action: str) -> None:
    assert str(Rule.parse(matcher, [action])) == f"matcher='{matcher}' action='{matcher}/{action}'"


@pytest.mark.parametrize(
    ("matcher", "action"),
    [
        ("tracktitle:Track", "genre:lala/replace:lalala"),
        ("tracktitle,genre,trackartist:Track", "tracktitle,genre,artist/delete"),
    ],
)
def test_rule_parsing_end_to_end_3(matcher: str, action: str) -> None:
    assert str(Rule.parse(matcher, [action])) == f"matcher={matcher} action={action}"


def test_rule_parsing_multi_value_validation() -> None:
    with pytest.raises(InvalidRuleError) as e:
        Rule.parse("tracktitle:h", ["split:x"])
    assert str(e.value) == "Single valued tags tracktitle cannot be modified by multi-value action split"
    with pytest.raises(InvalidRuleError):
        Rule.parse("tracktitle:h", ["split:x"])
    assert str(e.value) == "Single valued tags tracktitle cannot be modified by multi-value action split"
    with pytest.raises(InvalidRuleError):
        Rule.parse("genre:h", ["tracktitle/split:x"])
    assert str(e.value) == "Single valued tags tracktitle cannot be modified by multi-value action split"
    with pytest.raises(InvalidRuleError):
        Rule.parse("genre:h", ["split:y", "tracktitle/split:x"])
    assert str(e.value) == "Single valued tags tracktitle cannot be modified by multi-value action split"


def test_rule_parsing_defaults() -> None:
    rule = Rule.parse("tracktitle:Track", ["replace:hi"])
    assert rule.actions[0].pattern is not None
    assert rule.actions[0].pattern.needle == "Track"
    rule = Rule.parse("tracktitle:Track", ["tracktitle/replace:hi"])
    assert rule.actions[0].pattern is not None
    assert rule.actions[0].pattern.needle == "Track"
    rule = Rule.parse("tracktitle:Track", ["tracktitle:Lack/replace:hi"])
    assert rule.actions[0].pattern is not None
    assert rule.actions[0].pattern.needle == "Lack"


def test_parser_take() -> None:
    assert take("hello", ":") == ("hello", 5)
    assert take("hello:hi", ":") == ("hello", 6)
    assert take(r"h::lo:hi", ":") == ("h:lo", 6)
    assert take(r"h:://lo:hi", ":") == ("h:/lo", 8)
    assert take(r"h::lo/hi", "/") == ("h:lo", 6)
    assert take(r"h:://lo/hi", "/") == ("h:/lo", 8)
