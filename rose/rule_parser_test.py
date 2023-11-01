import re

from rose.rule_parser import (
    DeleteAction,
    MetadataRule,
    ReplaceAction,
    ReplaceAllAction,
    SedAction,
    SplitAction,
)


def test_rule_to_str() -> None:
    rule = MetadataRule(
        tags=["tracktitle"],
        matcher="Track",
        action=ReplaceAction(replacement="lalala"),
    )
    assert str(rule) == "tracktitle:Track:replace:lalala"

    rule = MetadataRule(
        tags=["tracktitle", "artist", "genre"],
        matcher=":",
        action=SedAction(src=re.compile(r":"), dst=";"),
    )
    assert str(rule) == r"tracktitle,artist,genre:\::sed:\::;"


def test_rule_parser() -> None:
    # Most basic
    assert MetadataRule.parse_dict(
        {
            "tags": "tracktitle",
            "matcher": "lala",
            "action": {"kind": "replace", "replacement": "hihi"},
        }
    ) == MetadataRule(
        tags=["tracktitle"],
        matcher="lala",
        action=ReplaceAction(replacement="hihi"),
    )

    # Test when tags is a list
    assert MetadataRule.parse_dict(
        {
            "tags": ["tracktitle", "albumtitle"],
            "matcher": "lala",
            "action": {"kind": "replace", "replacement": "hihi"},
        }
    ) == MetadataRule(
        tags=["tracktitle", "albumtitle"],
        matcher="lala",
        action=ReplaceAction(replacement="hihi"),
    )

    # Test replaceall
    assert MetadataRule.parse_dict(
        {
            "tags": "genre",
            "matcher": "lala",
            "action": {"kind": "replaceall", "replacement": ["hihi"]},
        }
    ) == MetadataRule(
        tags=["genre"],
        matcher="lala",
        action=ReplaceAllAction(replacement=["hihi"]),
    )

    # Test sed
    assert MetadataRule.parse_dict(
        {
            "tags": "tracktitle",
            "matcher": "lala",
            "action": {"kind": "sed", "src": "lala", "dst": "haha"},
        }
    ) == MetadataRule(
        tags=["tracktitle"],
        matcher="lala",
        action=SedAction(src=re.compile("lala"), dst="haha"),
    )

    # Test spliton
    assert MetadataRule.parse_dict(
        {
            "tags": "genre",
            "matcher": "lala",
            "action": {"kind": "spliton", "delimiter": "."},
        }
    ) == MetadataRule(
        tags=["genre"],
        matcher="lala",
        action=SplitAction(delimiter="."),
    )

    # Test delete
    assert MetadataRule.parse_dict(
        {
            "tags": "tracktitle",
            "matcher": "lala",
            "action": {"kind": "delete"},
        }
    ) == MetadataRule(
        tags=["tracktitle"],
        matcher="lala",
        action=DeleteAction(),
    )
