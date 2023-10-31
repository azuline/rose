from pathlib import Path

from rose.audiotags import AudioTags
from rose.config import Config
from rose.rules import ReplaceAction, UpdateRule, execute_rule


def test_rules_execution_match_substring(config: Config, source_dir: Path) -> None:
    # No match
    rule = UpdateRule(
        tags=["tracktitle"],
        matcher="bbb",
        action=ReplaceAction(replacement="lalala"),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"

    # Match
    rule = UpdateRule(
        tags=["tracktitle"],
        matcher="rack",
        action=ReplaceAction(replacement="lalala"),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


def test_rules_execution_match_beginnning(config: Config, source_dir: Path) -> None:
    # No match
    rule = UpdateRule(
        tags=["tracktitle"],
        matcher="^rack",
        action=ReplaceAction(replacement="lalala"),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"

    # Match
    rule = UpdateRule(
        tags=["tracktitle"],
        matcher="^Track",
        action=ReplaceAction(replacement="lalala"),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


def test_rules_execution_match_end(config: Config, source_dir: Path) -> None:
    # No match
    rule = UpdateRule(
        tags=["tracktitle"],
        matcher="rack$",
        action=ReplaceAction(replacement="lalala"),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"

    # Match
    rule = UpdateRule(
        tags=["tracktitle"],
        matcher="rack 1$",
        action=ReplaceAction(replacement="lalala"),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"
    raise AssertionError()
