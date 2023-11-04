import re

import click
import pytest

from rose.rule_parser import (
    AddAction,
    DeleteAction,
    InvalidRuleError,
    MetadataAction,
    MetadataMatcher,
    MetadataRule,
    ReplaceAction,
    RuleSyntaxError,
    SedAction,
    SplitAction,
    take,
)


def test_rule_str() -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="Track"),
        actions=[
            MetadataAction(
                behavior=ReplaceAction(replacement="lalala"),
                tags=["albumartist", "genre"],
            ),
        ],
    )
    assert str(rule) == "matcher=tracktitle:Track action=albumartist,genre::replace:lalala"

    # Test that rules are quoted properly.
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle", "albumartist", "genre"], pattern=":"),
        actions=[
            MetadataAction(
                behavior=SedAction(src=re.compile(r":"), dst="; "),
                tags=["tracktitle", "albumartist", "genre"],
            )
        ],
    )
    assert (
        str(rule)
        == r"matcher='tracktitle,albumartist,genre:\:' action='tracktitle,albumartist,genre::sed:\::; '"
    )

    # Test that custom action matcher is printed properly.
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="Track"),
        actions=[
            MetadataAction(
                behavior=ReplaceAction(replacement="lalala"),
                tags=["genre"],
                pattern="lala",
            ),
        ],
    )
    assert str(rule) == "matcher=tracktitle:Track action=genre:lala::replace:lalala"

    # Test that we print `matched` when action pattern is not null.
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["genre"], pattern="b"),
        actions=[
            MetadataAction(behavior=ReplaceAction(replacement="hi"), tags=["genre"], pattern="h")
        ],
    )
    assert str(rule) == r"matcher=genre:b action=genre:h::replace:hi"


def test_rule_parse_matcher() -> None:
    assert MetadataMatcher.parse("tracktitle:Track") == MetadataMatcher(
        tags=["tracktitle"],
        pattern="Track",
    )
    assert MetadataMatcher.parse("tracktitle,tracknumber:Track") == MetadataMatcher(
        tags=["tracktitle", "tracknumber"],
        pattern="Track",
    )
    assert MetadataMatcher.parse("tracktitle,tracknumber:^Track$") == MetadataMatcher(
        tags=["tracktitle", "tracknumber"],
        pattern="^Track$",
    )
    assert MetadataMatcher.parse(r"tracktitle,tracknumber:Tr\:ck") == MetadataMatcher(
        tags=["tracktitle", "tracknumber"],
        pattern="Tr:ck",
    )
    assert MetadataMatcher.parse(r"tracktitle:") == MetadataMatcher(
        tags=["tracktitle"],
        pattern="",
    )

    @pytest.mark.helper()
    def test_err(rule: str, err: str) -> None:
        with pytest.raises(RuleSyntaxError) as exc:
            MetadataMatcher.parse(rule)
        assert click.unstyle(str(exc.value)) == err

    test_err(
        "tracknumber^Track$",
        """\
Failed to parse matcher, invalid syntax:

    tracknumber^Track$
    ^
    Invalid tag: must be one of {tracktitle, trackartist, tracknumber, discnumber, albumtitle, albumartist, releasetype, year, genre, label}. The next character after a tag must be ':' or ','.
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
                 Found another section after the pattern, but the pattern must be the last section. Perhaps you meant to escape this colon?
""",
    )

    test_err(
        "tracktitle::",
        """\
Failed to parse matcher, invalid syntax:

    tracktitle::
               ^
               Found another section after the pattern, but the pattern must be the last section. Perhaps you meant to escape this colon?
""",
    )


def test_rule_parse_action() -> None:
    assert MetadataAction.parse(
        "replace:lalala",
        1,
        MetadataMatcher(tags=["tracktitle"], pattern="haha"),
    ) == MetadataAction(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["tracktitle"],
        pattern="haha",
    )
    assert MetadataAction.parse("genre::replace:lalala", 1) == MetadataAction(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["genre"],
        pattern=None,
    )
    assert MetadataAction.parse("tracknumber,genre::replace:lalala", 1) == MetadataAction(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["tracknumber", "genre"],
        pattern=None,
    )
    assert MetadataAction.parse("genre:lala::replace:lalala", 1) == MetadataAction(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["genre"],
        pattern="lala",
    )
    assert MetadataAction.parse(
        "matched:^x::replace:lalala",
        1,
        MetadataMatcher(tags=["tracktitle"], pattern="haha"),
    ) == MetadataAction(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["tracktitle"],
        pattern="^x",
    )

    assert MetadataAction.parse(
        "sed:lalala:hahaha",
        1,
        MetadataMatcher(tags=["genre"], pattern="haha"),
    ) == MetadataAction(
        behavior=SedAction(src=re.compile("lalala"), dst="hahaha"),
        tags=["genre"],
        pattern="haha",
    )
    assert MetadataAction.parse(
        r"split:\:",
        1,
        MetadataMatcher(tags=["genre"], pattern="haha"),
    ) == MetadataAction(
        behavior=SplitAction(delimiter=":"),
        tags=["genre"],
        pattern="haha",
    )
    assert MetadataAction.parse(
        r"split:\:",
        1,
        MetadataMatcher(tags=["genre"], pattern="haha"),
    ) == MetadataAction(
        behavior=SplitAction(delimiter=":"),
        tags=["genre"],
        pattern="haha",
    )
    assert MetadataAction.parse(
        r"add:cute",
        1,
        MetadataMatcher(tags=["genre"], pattern="haha"),
    ) == MetadataAction(
        behavior=AddAction(value="cute"),
        tags=["genre"],
        pattern="haha",
    )
    assert MetadataAction.parse(
        r"delete:",
        1,
        MetadataMatcher(tags=["genre"], pattern="haha"),
    ) == MetadataAction(
        behavior=DeleteAction(),
        tags=["genre"],
        pattern="haha",
    )
    assert MetadataAction.parse(
        r"delete:",
        1,
        MetadataMatcher(tags=["genre"], pattern="haha"),
    ) == MetadataAction(
        behavior=DeleteAction(),
        tags=["genre"],
        pattern="haha",
    )

    @pytest.mark.helper()
    def test_err(rule: str, err: str, matcher: MetadataMatcher | None = None) -> None:
        with pytest.raises(RuleSyntaxError) as exc:
            MetadataAction.parse(rule, 1, matcher)
        assert click.unstyle(str(exc.value)) == err

    test_err(
        "haha::delete",
        """\
Failed to parse action 1, invalid syntax:

    haha::delete
    ^
    Invalid tag: must be one of {tracktitle, trackartist, tracknumber, discnumber, albumtitle, albumartist, releasetype, year, genre, label}. The next character after a tag must be ':' or ','.
""",
    )

    test_err(
        "tracktitler::delete",
        """\
Failed to parse action 1, invalid syntax:

    tracktitler::delete
    ^
    Invalid tag: must be one of {tracktitle, trackartist, tracknumber, discnumber, albumtitle, albumartist, releasetype, year, genre, label}. The next character after a tag must be ':' or ','.
""",
    )

    test_err(
        "tracktitle:haha:delete",
        """\
Failed to parse action 1, invalid syntax:

    tracktitle:haha:delete
    ^
    Invalid action kind: must be one of {replace, sed, split, add, delete}. If this is pointing at your pattern, you forgot to put :: (double colons) between the matcher section and the action section.
""",
        matcher=MetadataMatcher(tags=["genre"], pattern="haha"),
    )

    test_err(
        "tracktitle:haha:sed::hi:bye",
        """\
Failed to parse action 1, invalid syntax:

    tracktitle:haha:sed::hi:bye
                    ^
                    End of the action matcher not found. Please end the matcher with a `::`.
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
        matcher=MetadataMatcher(tags=["genre"], pattern="haha"),
    )

    test_err(
        "replace",
        """\
Failed to parse action 1, invalid syntax:

    replace
           ^
           Replacement not found: must specify a non-empty replacement. Use the delete action to remove a value.
""",
        matcher=MetadataMatcher(tags=["genre"], pattern="haha"),
    )
    test_err(
        "replace:haha:",
        """\
Failed to parse action 1, invalid syntax:

    replace:haha:
                ^
                Found another section after the replacement, but the replacement must be the last section. Perhaps you meant to escape this colon?
""",
        matcher=MetadataMatcher(tags=["genre"], pattern="haha"),
    )

    test_err(
        "sed",
        """\
Failed to parse action 1, invalid syntax:

    sed
       ^
       Empty sed pattern found: must specify a non-empty pattern. Example: sed:pattern:replacement
""",
        matcher=MetadataMatcher(tags=["genre"], pattern="haha"),
    )

    test_err(
        "sed:hihi",
        """\
Failed to parse action 1, invalid syntax:

    sed:hihi
            ^
            Sed replacement not found: must specify a sed replacement section. Example: sed:hihi:replacement.
""",
        matcher=MetadataMatcher(tags=["genre"], pattern="haha"),
    )

    test_err(
        "sed:invalid[",
        """\
Failed to parse action 1, invalid syntax:

    sed:invalid[
        ^
        Failed to compile the sed pattern regex: invalid pattern: unterminated character set at position 7
""",
        matcher=MetadataMatcher(tags=["genre"], pattern="haha"),
    )

    test_err(
        "sed:hihi:byebye:",
        """\
Failed to parse action 1, invalid syntax:

    sed:hihi:byebye:
                   ^
                   Found another section after the sed replacement, but the sed replacement must be the last section. Perhaps you meant to escape this colon?
""",
        matcher=MetadataMatcher(tags=["genre"], pattern="haha"),
    )

    test_err(
        "split",
        """\
Failed to parse action 1, invalid syntax:

    split
         ^
         Delimiter not found: must specify a non-empty delimiter to split on.
""",
        matcher=MetadataMatcher(tags=["genre"], pattern="haha"),
    )

    test_err(
        "split:hi:",
        """\
Failed to parse action 1, invalid syntax:

    split:hi:
            ^
            Found another section after the delimiter, but the delimiter must be the last section. Perhaps you meant to escape this colon?
""",
        matcher=MetadataMatcher(tags=["genre"], pattern="haha"),
    )

    test_err(
        "split::",
        """\
Failed to parse action 1, invalid syntax:

    split::
          ^
          Delimiter not found: must specify a non-empty delimiter to split on. Perhaps you meant to escape this colon?
""",
        matcher=MetadataMatcher(tags=["genre"], pattern="haha"),
    )

    test_err(
        "add",
        """\
Failed to parse action 1, invalid syntax:

    add
       ^
       Value not found: must specify a non-empty value to add.
""",
        matcher=MetadataMatcher(tags=["genre"], pattern="haha"),
    )

    test_err(
        "add:hi:",
        """\
Failed to parse action 1, invalid syntax:

    add:hi:
          ^
          Found another section after the value, but the value must be the last section. Perhaps you meant to escape this colon?
""",
        matcher=MetadataMatcher(tags=["genre"], pattern="haha"),
    )

    test_err(
        "add::",
        """\
Failed to parse action 1, invalid syntax:

    add::
        ^
        Value not found: must specify a non-empty value to add. Perhaps you meant to escape this colon?
""",
        matcher=MetadataMatcher(tags=["genre"], pattern="haha"),
    )

    test_err(
        "delete:h",
        """\
Failed to parse action 1, invalid syntax:

    delete:h
           ^
           Found another section after the action kind, but the delete action has no parameters. Please remove this section.
""",
        matcher=MetadataMatcher(tags=["genre"], pattern="haha"),
    )

    test_err(
        "delete",
        """\
Failed to parse action 1, invalid syntax:

    delete
    ^
    Tags/pattern section not found. Must specify tags to modify, since there is no matcher to default to. Make sure you are formatting your action like {tags}:{pattern}::{kind}:{args} (where `:{pattern}` is optional)
""",
    )


def test_rule_parsing_end_to_end() -> None:
    matcher = "tracktitle:Track"
    action = "delete"
    assert f"matcher={matcher} action=tracktitle:Track::{action}" == str(
        MetadataRule.parse(matcher, [action])
    )

    matcher = "tracktitle:Track"
    action = "genre:lala::replace:lalala"
    assert f"matcher={matcher} action={action}" == str(MetadataRule.parse(matcher, [action]))

    matcher = "tracktitle,genre,trackartist:Track"
    action = "tracktitle,genre,trackartist,albumartist::delete"
    assert f"matcher={matcher} action={action}" == str(MetadataRule.parse(matcher, [action]))

    matcher = "tracktitle:Track"
    action = "delete"
    assert f"matcher={matcher} action=tracktitle:Track::{action}" == str(
        MetadataRule.parse(matcher, [action])
    )


def test_rule_parsing_multi_value_validation() -> None:
    with pytest.raises(InvalidRuleError) as e:
        MetadataRule.parse("tracktitle:h", ["split:x"])
    assert (
        str(e.value)
        == "Single valued tags tracktitle cannot be modified by multi-value action split"
    )
    with pytest.raises(InvalidRuleError):
        MetadataRule.parse("tracktitle:h", ["split:x"])
    assert (
        str(e.value)
        == "Single valued tags tracktitle cannot be modified by multi-value action split"
    )
    with pytest.raises(InvalidRuleError):
        MetadataRule.parse("genre:h", ["tracktitle::split:x"])
    assert (
        str(e.value)
        == "Single valued tags tracktitle cannot be modified by multi-value action split"
    )
    with pytest.raises(InvalidRuleError):
        MetadataRule.parse("genre:h", ["split:y", "tracktitle::split:x"])
    assert (
        str(e.value)
        == "Single valued tags tracktitle cannot be modified by multi-value action split"
    )


def test_rule_parsing_defaults() -> None:
    rule = MetadataRule.parse("tracktitle:Track", ["replace:hi"])
    assert rule.actions[0].pattern == "Track"
    rule = MetadataRule.parse("tracktitle:Track", ["tracktitle::replace:hi"])
    assert rule.actions[0].pattern == "Track"
    rule = MetadataRule.parse("tracktitle:Track", ["tracktitle:Lack::replace:hi"])
    assert rule.actions[0].pattern == "Lack"


def test_parser_take() -> None:
    assert take("hello", ":") == ("hello", 5)
    assert take("hello:hi", ":") == ("hello", 6)
    assert take(r"h\:lo:hi", ":") == ("h:lo", 6)
    assert take(r"h:lo::hi", "::") == ("h:lo", 6)
    assert take(r"h\:lo::hi", "::") == ("h:lo", 7)
