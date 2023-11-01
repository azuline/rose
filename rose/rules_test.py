import re
from pathlib import Path
from typing import Any
from unittest.mock import Mock

import pytest

from rose.audiotags import AudioTags
from rose.config import Config
from rose.rule_parser import (
    DeleteAction,
    MetadataRule,
    ReplaceAction,
    ReplaceAllAction,
    SedAction,
    SplitAction,
)
from rose.rules import execute_rule


def test_rules_execution_match_substring(config: Config, source_dir: Path) -> None:
    # No match
    rule = MetadataRule(
        tags=["tracktitle"],
        matcher="bbb",
        action=ReplaceAction(replacement="lalala"),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"

    # Match
    rule = MetadataRule(
        tags=["tracktitle"],
        matcher="rack",
        action=ReplaceAction(replacement="lalala"),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


def test_rules_execution_match_beginnning(config: Config, source_dir: Path) -> None:
    # No match
    rule = MetadataRule(
        tags=["tracktitle"],
        matcher="^rack",
        action=ReplaceAction(replacement="lalala"),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"

    # Match
    rule = MetadataRule(
        tags=["tracktitle"],
        matcher="^Track",
        action=ReplaceAction(replacement="lalala"),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


def test_rules_execution_match_end(config: Config, source_dir: Path) -> None:
    # No match
    rule = MetadataRule(
        tags=["tracktitle"],
        matcher="rack$",
        action=ReplaceAction(replacement="lalala"),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"

    # Match
    rule = MetadataRule(
        tags=["tracktitle"],
        matcher="rack 1$",
        action=ReplaceAction(replacement="lalala"),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


def test_rules_execution_match_superstrict(config: Config, source_dir: Path) -> None:
    # No match
    rule = MetadataRule(
        tags=["tracktitle"],
        matcher="^Track $",
        action=ReplaceAction(replacement="lalala"),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"

    # Match
    rule = MetadataRule(
        tags=["tracktitle"],
        matcher="^Track 1$",
        action=ReplaceAction(replacement="lalala"),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


def test_all_fields_match(config: Config, source_dir: Path) -> None:
    # Test most fields.
    rule = MetadataRule(
        tags=[
            "year",
            "tracktitle",
            "tracknumber",
            "discnumber",
            "albumtitle",
            "genre",
            "label",
            "artist",
        ],
        matcher="",  # Empty string matches everything.
        action=ReplaceAction(replacement="8"),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "8"
    assert af.year == 8
    assert af.track_number == "8"
    assert af.disc_number == "8"
    assert af.album == "8"
    assert af.genre == ["8", "8"]
    assert af.label == ["8"]
    assert af.album_artists.main == ["8"]
    assert af.artists.main == ["8"]

    # And then test release type separately.
    rule = MetadataRule(
        tags=["releasetype"],
        matcher="",  # Empty string matches everything.
        action=ReplaceAction(replacement="live"),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.release_type == "live"


def test_action_replace_all(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        tags=["genre"],
        matcher="K-Pop",
        action=ReplaceAllAction(replacement=["Hip-Hop", "Rap"]),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["Hip-Hop", "Rap"]


def test_sed_action(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        tags=["tracktitle"],
        matcher="Track",
        action=SedAction(src=re.compile("ack"), dst="ip"),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "Trip 1"


def test_split_action(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        tags=["label"],
        matcher="Cool",
        action=SplitAction(delimiter="Cool"),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.label == ["A", "Label"]


def test_delete_action(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        tags=["genre"],
        matcher="^Pop",
        action=DeleteAction(),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["K-Pop"]


def test_preserves_unmatched_multitags(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        tags=["genre"],
        matcher="^Pop$",
        action=ReplaceAction(replacement="lalala"),
    )
    execute_rule(config, rule, False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["K-Pop", "lalala"]


@pytest.mark.timeout(2)
def test_confirmation_yes(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        tags=["tracktitle"],
        matcher="Track",
        action=ReplaceAction(replacement="lalala"),
    )

    monkeypatch.setattr("rose.rules.click.confirm", lambda *_, **__: True)
    execute_rule(config, rule, True)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


@pytest.mark.timeout(2)
def test_confirmation_no(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        tags=["tracktitle"],
        matcher="Track",
        action=ReplaceAction(replacement="lalala"),
    )

    monkeypatch.setattr("rose.rules.click.confirm", lambda *_, **__: False)
    execute_rule(config, rule, True)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"


@pytest.mark.timeout(2)
def test_confirmation_count(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        tags=["tracktitle"],
        matcher="Track",
        action=ReplaceAction(replacement="lalala"),
    )

    monkeypatch.setattr("rose.rules.click.prompt", Mock(side_effect=["no", "8", "6"]))
    # Abort.
    execute_rule(config, rule, True, enter_number_to_confirm_above_count=1)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"

    # Success in two arguments.
    execute_rule(config, rule, True, enter_number_to_confirm_above_count=1)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


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
