import dataclasses
from pathlib import Path
from typing import Any
from unittest.mock import Mock

import pytest

from rose.audiotags import AudioTags
from rose.cache import (
    list_releases,
    list_tracks,
    update_cache,
)
from rose.common import Artist
from rose.config import Config
from rose.rule_parser import MetadataMatcher, MetadataRule
from rose.rules import (
    FastSearchResult,
    TrackTagNotAllowedError,
    execute_metadata_rule,
    execute_stored_metadata_rules,
    fast_search_for_matching_releases,
    fast_search_for_matching_tracks,
    filter_release_false_positives_using_read_cache,
    filter_track_false_positives_using_read_cache,
)


def test_rules_execution_match_substring(config: Config, source_dir: Path) -> None:
    # No match
    rule = MetadataRule.parse("tracktitle:bbb", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"

    # Match
    rule = MetadataRule.parse("tracktitle:rack", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


def test_rules_execution_match_beginnning(config: Config, source_dir: Path) -> None:
    # No match
    rule = MetadataRule.parse("tracktitle:^rack", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"

    # Match
    rule = MetadataRule.parse("tracktitle:^Track", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


def test_rules_execution_match_end(config: Config, source_dir: Path) -> None:
    # No match
    rule = MetadataRule.parse("tracktitle:rack$", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"

    # Match
    rule = MetadataRule.parse("tracktitle:rack 1$", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


def test_rules_execution_match_superstrict(config: Config, source_dir: Path) -> None:
    # No match
    rule = MetadataRule.parse("tracktitle:^Track $", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"

    # Match
    rule = MetadataRule.parse("tracktitle:^Track 1$", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


def test_rules_execution_match_case_insensitive(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("tracktitle:tRaCk:i", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


def test_rules_fields_match_tracktitle(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("tracktitle:Track", ["replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "8"


def test_rules_fields_match_year(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("year:1990", ["replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.year == 8


def test_rules_fields_match_releasetype(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("releasetype:release", ["replace:live"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.releasetype == "live"


def test_rules_fields_match_tracknumber(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("tracknumber:1", ["replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracknumber == "8"


def test_rules_fields_match_tracktotal(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("tracktotal:2", ["tracktitle::replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "8"


def test_rules_fields_match_discnumber(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("discnumber:1", ["replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.discnumber == "8"


def test_rules_fields_match_disctotal(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("disctotal:1", ["tracktitle::replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "8"


def test_rules_fields_match_releasetitle(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("releasetitle:Love Blackpink", ["replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.release == "8"


def test_rules_fields_match_genre(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("genre:K-Pop", ["replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["8", "Pop"]


def test_rules_fields_match_label(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("label:Cool", ["replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.label == ["8"]


def test_rules_fields_match_releaseartist(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("releaseartist:BLACKPINK", ["replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.releaseartists.main == [Artist("8")]


def test_rules_fields_match_trackartist(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("trackartist:BLACKPINK", ["replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.trackartists.main == [Artist("8")]


def test_match_backslash(config: Config, source_dir: Path) -> None:
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    af.title = r"X \\ Y"
    af.flush()
    update_cache(config)

    rule = MetadataRule.parse(r"tracktitle: \\\\ ", [r"sed: \\\\\\\\ : / "])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "X / Y"


def test_action_replace_with_delimiter(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("genre:K-Pop", ["replace:Hip-Hop;Rap"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["Hip-Hop", "Rap", "Pop"]


def test_action_replace_with_delimiters_empty_str(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("genre:K-Pop", ["matched:::replace:Hip-Hop;;;;"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["Hip-Hop"]


def test_sed_action(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("tracktitle:Track", ["sed:ack:ip"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "Trip 1"


def test_sed_no_pattern(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("genre:P", [r"matched:::sed:^(.*)$:i\\1"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["iK-Pop", "iPop"]


def test_split_action(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("label:Cool", ["split:Cool"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.label == ["A", "Label"]


def test_split_action_no_pattern(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("genre:K-Pop", ["matched:::split:P"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["K-", "op"]


def test_add_action(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("label:Cool", ["add:Even Cooler Label"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.label == ["A Cool Label", "Even Cooler Label"]


def test_delete_action(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("genre:^Pop$", ["delete"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["K-Pop"]


def test_delete_action_no_pattern(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("genre:^Pop$", ["matched:::delete"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == []


def test_preserves_unmatched_multitags(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("genre:^Pop$", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["K-Pop", "lalala"]


def test_action_on_different_tag(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("label:A Cool Label", ["genre::replace:hi"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["hi"]


def test_action_no_pattern(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("genre:K-Pop", ["matched:::sed:P:B"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["K-Bop", "Bop"]


def test_chained_action(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse(
        "label:A Cool Label",
        [
            "replace:Jennie",
            "label:^Jennie$::replace:Jisoo",
            "label:nomatch::replace:Rose",
            "genre::replace:haha",
        ],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.label == ["Jisoo"]
    assert af.genre == ["haha"]


@pytest.mark.timeout(2)
def test_confirmation_yes(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("tracktitle:Track", ["replace:lalala"])
    monkeypatch.setattr("rose.rules.click.confirm", lambda *_, **__: True)
    execute_metadata_rule(config, rule, confirm_yes=True)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


@pytest.mark.timeout(2)
def test_confirmation_no(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("tracktitle:Track", ["replace:lalala"])
    monkeypatch.setattr("rose.rules.click.confirm", lambda *_, **__: False)
    execute_metadata_rule(config, rule, confirm_yes=True)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"


@pytest.mark.timeout(2)
def test_confirmation_count(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("tracktitle:Track", ["replace:lalala"])
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
    rule = MetadataRule.parse("tracktitle:Track", ["replace:lalala"])
    execute_metadata_rule(config, rule, dry_run=True, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title != "lalala"


def test_run_stored_rules(config: Config, source_dir: Path) -> None:
    config = dataclasses.replace(
        config,
        stored_metadata_rules=[MetadataRule.parse("tracktitle:Track", ["replace:lalala"])],
    )

    execute_stored_metadata_rules(config)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "lalala"


@pytest.mark.usefixtures("seeded_cache")
def test_fast_search_for_matching_releases(config: Config) -> None:
    results = fast_search_for_matching_releases(
        config, MetadataMatcher.parse("releaseartist:Techno Man")
    )
    assert results == [FastSearchResult(id="r1", path=config.music_source_dir / "r1")]


@pytest.mark.usefixtures("seeded_cache")
def test_fast_search_for_matching_releases_invalid_tag(config: Config) -> None:
    with pytest.raises(TrackTagNotAllowedError):
        fast_search_for_matching_releases(config, MetadataMatcher.parse("tracktitle:x"))
    with pytest.raises(TrackTagNotAllowedError):
        fast_search_for_matching_releases(config, MetadataMatcher.parse("trackartist:x"))
    # But allow artist tag:
    fast_search_for_matching_releases(config, MetadataMatcher.parse("artist:x"))


@pytest.mark.usefixtures("seeded_cache")
def test_filter_release_false_positives_with_read_cache(config: Config) -> None:
    matcher = MetadataMatcher.parse("releaseartist:^Man")
    fsresults = fast_search_for_matching_releases(config, matcher)
    assert len(fsresults) == 2
    cacheresults = list_releases(config, [r.id for r in fsresults])
    assert len(cacheresults) == 2
    filteredresults = filter_release_false_positives_using_read_cache(matcher, cacheresults)
    assert not filteredresults


@pytest.mark.usefixtures("seeded_cache")
def test_filter_track_false_positives_with_read_cache(config: Config) -> None:
    matcher = MetadataMatcher.parse("trackartist:^Man")
    fsresults = fast_search_for_matching_tracks(config, matcher)
    assert len(fsresults) == 3
    tracks = list_tracks(config, [r.id for r in fsresults])
    assert len(tracks) == 3
    filteredresults = filter_track_false_positives_using_read_cache(matcher, tracks)
    assert not filteredresults


def test_ignore_values(config: Config, source_dir: Path) -> None:
    rule = MetadataRule.parse("tracktitle:rack", ["replace:lalala"], ["tracktitle:^Track 1$"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.title == "Track 1"
