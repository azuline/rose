import re

from rose.rule_parser import (
    DeleteAction,
    MetadataMatcher,
    MetadataRule,
    ReplaceAction,
    ReplaceAllAction,
    SedAction,
    SplitAction,
)


def test_rule_to_str() -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="Track"),
        action=ReplaceAction(replacement="lalala", tags=["artist", "genre"]),
    )
    assert str(rule) == "matcher=tracktitle:Track action=artist,genre:replace:lalala"

    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle", "artist", "genre"], pattern=":"),
        action=SedAction(src=re.compile(r":"), dst="; "),
    )
    assert str(rule) == r'matcher=tracktitle,artist,genre:\: action="sed:\::; "'


def test_rule_parser() -> None:
    # Most basic
    assert MetadataRule.parse_dict(
        {
            "matcher": {"tags": "tracktitle", "pattern": "lala"},
            "action": {"kind": "replace", "replacement": "hihi"},
        }
    ) == MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="lala"),
        action=ReplaceAction(replacement="hihi"),
    )

    # Test when tags is a list
    assert MetadataRule.parse_dict(
        {
            "matcher": {"tags": ["tracktitle", "albumtitle"], "pattern": "lala"},
            "action": {"kind": "replace", "replacement": "hihi"},
        }
    ) == MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle", "albumtitle"], pattern="lala"),
        action=ReplaceAction(replacement="hihi"),
    )

    # Test replaceall
    assert MetadataRule.parse_dict(
        {
            "matcher": {"tags": "genre", "pattern": "lala"},
            "action": {"kind": "replaceall", "replacement": ["hihi"]},
        }
    ) == MetadataRule(
        matcher=MetadataMatcher(tags=["genre"], pattern="lala"),
        action=ReplaceAllAction(replacement=["hihi"]),
    )

    # Test sed
    assert MetadataRule.parse_dict(
        {
            "matcher": {"tags": "tracktitle", "pattern": "lala"},
            "action": {"kind": "sed", "src": "lala", "dst": "haha"},
        }
    ) == MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="lala"),
        action=SedAction(src=re.compile("lala"), dst="haha"),
    )

    # Test spliton
    assert MetadataRule.parse_dict(
        {
            "matcher": {"tags": "genre", "pattern": "lala"},
            "action": {"kind": "spliton", "delimiter": "."},
        }
    ) == MetadataRule(
        matcher=MetadataMatcher(tags=["genre"], pattern="lala"),
        action=SplitAction(delimiter="."),
    )

    # Test delete
    assert MetadataRule.parse_dict(
        {
            "matcher": {"tags": "tracktitle", "pattern": "lala"},
            "action": {"kind": "delete"},
        }
    ) == MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="lala"),
        action=DeleteAction(),
    )
