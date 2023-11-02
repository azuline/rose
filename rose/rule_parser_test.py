import re

import pytest

from rose.rule_parser import (
    DeleteAction,
    MetadataAction,
    MetadataMatcher,
    MetadataRule,
    ReplaceAction,
    RuleSyntaxError,
    SedAction,
    SplitAction,
    parse_action,
    parse_matcher,
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
        actions=[MetadataAction(behavior=SedAction(src=re.compile(r":"), dst="; "))],
    )
    assert str(rule) == r'matcher=tracktitle,albumartist,genre:\: action="sed:\::; "'

    # Test that custom action matcher is printed properly.
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="Track"),
        actions=[
            MetadataAction(
                behavior=ReplaceAction(replacement="lalala"),
                tags=["genre"],
                all=True,
                match_pattern="lala",
            ),
        ],
    )
    assert str(rule) == "matcher=tracktitle:Track action=genre:lala::replace-all:lalala"


def test_rule_parse_matcher() -> None:
    assert parse_matcher("tracktitle:Track") == MetadataMatcher(
        tags=["tracktitle"],
        pattern="Track",
    )
    assert parse_matcher("tracktitle,tracknumber:Track") == MetadataMatcher(
        tags=["tracktitle", "tracknumber"],
        pattern="Track",
    )
    assert parse_matcher("tracktitle,tracknumber:^Track$") == MetadataMatcher(
        tags=["tracktitle", "tracknumber"],
        pattern="^Track$",
    )
    assert parse_matcher(r"tracktitle,tracknumber:Tr\:ck") == MetadataMatcher(
        tags=["tracktitle", "tracknumber"],
        pattern="Tr:ck",
    )
    assert parse_matcher(r"tracktitle:") == MetadataMatcher(
        tags=["tracktitle"],
        pattern="",
    )

    @pytest.mark.helper()
    def test_err(rule: str, err: str) -> None:
        with pytest.raises(RuleSyntaxError) as exc:
            parse_matcher(rule)
        assert str(exc.value) == err

    test_err(
        "tracknumber^Track$",
        """\
Failed to parse matcher, invalid syntax:

    tracknumber^Track$
    ^
    Invalid tag: must be one of {tracktitle, year, tracknumber, discnumber, albumtitle, genre, label, releasetype, trackartist, albumartist}. The next character after a tag must be ':' or ','.
""",  # noqa
    )

    test_err(
        "tracknumber",
        """\
Failed to parse matcher, invalid syntax:

    tracknumber
               ^
               Expected to find ',' or ':', found end of string.
""",  # noqa
    )

    test_err(
        "tracktitle:Tr:ck",
        """\
Failed to parse matcher, invalid syntax:

    tracktitle:Tr:ck
                 ^
                 Found another section after the pattern, but the pattern must be the last section. Perhaps you meant to escape this colon?
""",  # noqa
    )

    test_err(
        "tracktitle::",
        """\
Failed to parse matcher, invalid syntax:

    tracktitle::
               ^
               Found another section after the pattern, but the pattern must be the last section. Perhaps you meant to escape this colon?
""",  # noqa
    )


def test_rule_parse_action() -> None:
    assert parse_action("replace-all:lalala", 1) == MetadataAction(
        behavior=ReplaceAction(replacement="lalala"),
        tags="matched",
        match_pattern=None,
        all=True,
    )
    assert parse_action("genre::replace:lalala", 1) == MetadataAction(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["genre"],
        match_pattern=None,
        all=False,
    )
    assert parse_action("tracknumber,genre::replace:lalala", 1) == MetadataAction(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["tracknumber", "genre"],
        match_pattern=None,
        all=False,
    )
    assert parse_action("genre:lala::replace-all:lalala", 1) == MetadataAction(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["genre"],
        match_pattern="lala",
        all=True,
    )

    assert parse_action("sed:lalala:hahaha", 1) == MetadataAction(
        behavior=SedAction(src=re.compile("lalala"), dst="hahaha"),
        tags="matched",
        match_pattern=None,
        all=False,
    )
    assert parse_action("sed-all:lalala:hahaha", 1) == MetadataAction(
        behavior=SedAction(src=re.compile("lalala"), dst="hahaha"),
        tags="matched",
        match_pattern=None,
        all=True,
    )
    assert parse_action(r"split:\:", 1) == MetadataAction(
        behavior=SplitAction(delimiter=":"),
        tags="matched",
        match_pattern=None,
        all=False,
    )
    assert parse_action(r"split-all:\:", 1) == MetadataAction(
        behavior=SplitAction(delimiter=":"),
        tags="matched",
        match_pattern=None,
        all=True,
    )
    assert parse_action(r"delete:", 1) == MetadataAction(
        behavior=DeleteAction(),
        tags="matched",
        match_pattern=None,
        all=False,
    )
    assert parse_action(r"delete-all:", 1) == MetadataAction(
        behavior=DeleteAction(),
        tags="matched",
        match_pattern=None,
        all=True,
    )

    @pytest.mark.helper()
    def test_err(rule: str, err: str) -> None:
        with pytest.raises(RuleSyntaxError) as exc:
            parse_action(rule, 1)
        assert str(exc.value) == err

    test_err(
        "haha::delete",
        """\
Failed to parse action 1, invalid syntax:

    haha::delete
    ^
    Invalid tag: must be one of matched, {tracktitle, year, tracknumber, discnumber, albumtitle, genre, label, releasetype, trackartist, albumartist}. (And if the value is matched, it must be alone.) The next character after a tag must be ':' or ','.
""",  # noqa
    )

    test_err(
        "tracktitle:haha:delete",
        """\
Failed to parse action 1, invalid syntax:

    tracktitle:haha:delete
    ^
    Invalid action kind: must be one of {replace, replace-all, sed, sed-all, split, split-all, delete, delete-all}. If this is pointing at your pattern, you forgot to put :: (double colons) between the matcher section and the action section.
""",  # noqa
    )

    test_err(
        "tracktitle:haha:sed::hi:bye",
        """\
Failed to parse action 1, invalid syntax:

    tracktitle:haha:sed::hi:bye
                    ^
                    End of the action matcher not found. Please end the matcher with a `::`.
""",  # noqa
    )

    test_err(
        "hahaha",
        """\
Failed to parse action 1, invalid syntax:

    hahaha
    ^
    Invalid action kind: must be one of {replace, replace-all, sed, sed-all, split, split-all, delete, delete-all}.
""",  # noqa
    )

    test_err(
        "replace",
        """\
Failed to parse action 1, invalid syntax:

    replace
           ^
           Replacement not found: must specify a non-empty replacement. Use the delete action to remove a value.
""",  # noqa
    )

    test_err(
        "replace:haha:",
        """\
Failed to parse action 1, invalid syntax:

    replace:haha:
                ^
                Found another section after the replacement, but the replacement must be the last section. Perhaps you meant to escape this colon?
""",  # noqa
    )

    test_err(
        "sed",
        """\
Failed to parse action 1, invalid syntax:

    sed
       ^
       Empty sed pattern found: must specify a non-empty pattern. Example: sed:pattern:replacement
""",  # noqa
    )

    test_err(
        "sed:hihi",
        """\
Failed to parse action 1, invalid syntax:

    sed:hihi
            ^
            Sed replacement not found: must specify a sed replacement section. Example: sed:hihi:replacement.
""",  # noqa
    )

    test_err(
        "sed:hihi:byebye:",
        """\
Failed to parse action 1, invalid syntax:

    sed:hihi:byebye:
                   ^
                   Found another section after the sed replacement, but the sed replacement must be the last section. Perhaps you meant to escape this colon?
""",  # noqa
    )

    test_err(
        "split",
        """\
Failed to parse action 1, invalid syntax:

    split
         ^
         Delimiter not found: must specify a non-empty delimiter to split on.
""",  # noqa
    )

    test_err(
        "split:hi:",
        """\
Failed to parse action 1, invalid syntax:

    split:hi:
            ^
            Found another section after the delimiter, but the delimiter must be the last section. Perhaps you meant to escape this colon?
""",  # noqa
    )

    test_err(
        "split::",
        """\
Failed to parse action 1, invalid syntax:

    split::
          ^
          Delimiter not found: must specify a non-empty delimiter to split on. Perhaps you meant to escape this colon?
""",  # noqa
    )

    test_err(
        "delete:h",
        """\
Failed to parse action 1, invalid syntax:

    delete:h
           ^
           Found another section after the action kind, but the delete action has no parameters. Please remove this section.
""",  # noqa
    )


def test_rule_end_to_end() -> None:
    matcher = "tracktitle:Track"
    action = "delete-all"
    assert f"matcher={matcher} action={action}" == str(MetadataRule.parse(matcher, [action]))

    matcher = "tracktitle:Track"
    action = "genre:lala::replace-all:lalala"
    assert f"matcher={matcher} action={action}" == str(MetadataRule.parse(matcher, [action]))

    matcher = "tracktitle,genre,trackartist:Track"
    action = "tracktitle,genre,trackartist,albumartist::delete-all"
    assert f"matcher={matcher} action={action}" == str(MetadataRule.parse(matcher, [action]))


def test_take() -> None:
    assert take("hello", ":") == ("hello", 5)
    assert take("hello:hi", ":") == ("hello", 6)
    assert take(r"h\:lo:hi", ":") == ("h:lo", 6)
    assert take(r"h:lo::hi", "::") == ("h:lo", 6)
    assert take(r"h\:lo::hi", "::") == ("h:lo", 7)
