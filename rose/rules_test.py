import re
from dataclasses import asdict
from pathlib import Path
from typing import Any
from unittest.mock import Mock

import pytest

from rose.audiotags import AudioTags
from rose.config import Config
from rose.rule_parser import (
    AddAction,
    DeleteAction,
    MetadataAction,
    MetadataMatcher,
    MetadataRule,
    ReplaceAction,
    SedAction,
    SplitAction,
)
from rose.rules import execute_metadata_rule, execute_stored_metadata_rules


def test_rules_execution_match_substring(config: Config, source_dir: Path) -> None:
    # No match
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="bbb"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="lalala"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"

    # Match
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="rack"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="lalala"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


def test_rules_execution_match_beginnning(config: Config, source_dir: Path) -> None:
    # No match
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="^rack"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="lalala"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"

    # Match
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="^Track"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="lalala"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


def test_rules_execution_match_end(config: Config, source_dir: Path) -> None:
    # No match
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="rack$"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="lalala"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"

    # Match
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="rack 1$"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="lalala"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


def test_rules_execution_match_superstrict(config: Config, source_dir: Path) -> None:
    # No match
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="^Track $"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="lalala"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"

    # Match
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="^Track 1$"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="lalala"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


def test_rules_fields_match_tracktitle(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="Track"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="8"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "8"


def test_rules_fields_match_year(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["year"], pattern="1990"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="8"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.year == 8


def test_rules_fields_match_releasetype(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["releasetype"], pattern="album"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="live"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.release_type == "live"


def test_rules_fields_match_tracknumber(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracknumber"], pattern="1"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="8"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.track_number == "8"


def test_rules_fields_match_discnumber(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["discnumber"], pattern="1"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="8"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.disc_number == "8"


def test_rules_fields_match_albumtitle(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["albumtitle"], pattern="Love Blackpink"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="8"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.album == "8"


def test_rules_fields_match_genre(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["genre"], pattern="K-Pop"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="8"), match_pattern="K-Pop")],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["8", "Pop"]


def test_rules_fields_match_label(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["label"], pattern="Cool"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="8"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.label == ["8"]


def test_rules_fields_match_albumartist(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["albumartist"], pattern="BLACKPINK"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="8"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.album_artists.main == ["8"]


def test_rules_fields_match_trackartist(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["trackartist"], pattern="BLACKPINK"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="8"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.artists.main == ["8"]


def test_action_replace_with_delimiter(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["genre"], pattern="K-Pop"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="Hip-Hop;Rap"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["Hip-Hop", "Rap"]


def test_action_replace_with_delimiters_empty_str(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["genre"], pattern="K-Pop"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="Hip-Hop;;;;"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["Hip-Hop"]


def test_sed_action(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="Track"),
        actions=[MetadataAction(behavior=SedAction(src=re.compile("ack"), dst="ip"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "Trip 1"


def test_sed_no_pattern(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["genre"], pattern="P"),
        actions=[MetadataAction(behavior=SedAction(src=re.compile("^(.*)$"), dst=r"i\1"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["iK-Pop", "iPop"]


def test_split_action(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["label"], pattern="Cool"),
        actions=[MetadataAction(behavior=SplitAction(delimiter="Cool"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.label == ["A", "Label"]


def test_split_action_no_pattern(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["genre"], pattern="K-Pop"),
        actions=[MetadataAction(behavior=SplitAction(delimiter="P"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["K-", "op"]


def test_add_action(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["label"], pattern="Cool"),
        actions=[MetadataAction(behavior=AddAction(value="Even Cooler Label"))],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.label == ["A Cool Label", "Even Cooler Label"]


def test_delete_action(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["genre"], pattern="^Pop"),
        actions=[MetadataAction(behavior=DeleteAction(), match_pattern="^Pop")],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["K-Pop"]


def test_delete_action_no_pattern(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["genre"], pattern="^Pop"),
        actions=[MetadataAction(behavior=DeleteAction())],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == []


def test_preserves_unmatched_multitags(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["genre"], pattern="^Pop$"),
        actions=[
            MetadataAction(behavior=ReplaceAction(replacement="lalala"), match_pattern="^Pop$")
        ],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["K-Pop", "lalala"]


def test_action_on_different_tag(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["label"], pattern="A Cool Label"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="hi"), tags=["genre"])],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["hi"]


def test_chained_action(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["label"], pattern="A Cool Label"),
        actions=[
            MetadataAction(behavior=ReplaceAction(replacement="Jennie"), tags=["label"]),
            MetadataAction(
                behavior=ReplaceAction(replacement="Jisoo"),
                tags=["label"],
                match_pattern="^Jennie$",
            ),
            MetadataAction(
                behavior=ReplaceAction(replacement="Rose"), tags=["label"], match_pattern="nomatch"
            ),
            MetadataAction(
                behavior=ReplaceAction(replacement="haha"),
                tags=["genre"],
            ),
        ],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.label == ["Jisoo"]
    assert af.genre == ["haha"]


@pytest.mark.timeout(2)
def test_confirmation_yes(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="Track"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="lalala"))],
    )

    monkeypatch.setattr("rose.rules.click.confirm", lambda *_, **__: True)
    execute_metadata_rule(config, rule, confirm_yes=True)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


@pytest.mark.timeout(2)
def test_confirmation_no(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="Track"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="lalala"))],
    )

    monkeypatch.setattr("rose.rules.click.confirm", lambda *_, **__: False)
    execute_metadata_rule(config, rule, confirm_yes=True)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"


@pytest.mark.timeout(2)
def test_confirmation_count(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="Track"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="lalala"))],
    )

    monkeypatch.setattr("rose.rules.click.prompt", Mock(side_effect=["no", "8", "6"]))
    # Abort.
    execute_metadata_rule(config, rule, confirm_yes=True, enter_number_to_confirm_above_count=1)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"

    # Success in two arguments.
    execute_metadata_rule(config, rule, confirm_yes=True, enter_number_to_confirm_above_count=1)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


def test_dry_run(config: Config, source_dir: Path) -> None:
    rule = MetadataRule(
        matcher=MetadataMatcher(tags=["tracktitle"], pattern="Track"),
        actions=[MetadataAction(behavior=ReplaceAction(replacement="lalala"))],
    )

    execute_metadata_rule(config, rule, dry_run=True, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"


def test_run_stored_rules(config: Config, source_dir: Path) -> None:
    config = Config(
        **{
            **asdict(config),
            "stored_metadata_rules": [
                MetadataRule(
                    matcher=MetadataMatcher(tags=["tracktitle"], pattern="Track"),
                    actions=[MetadataAction(behavior=ReplaceAction(replacement="lalala"))],
                )
            ],
        },
    )

    execute_stored_metadata_rules(config)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"
