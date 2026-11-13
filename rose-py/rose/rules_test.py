import dataclasses
from pathlib import Path
from typing import Any
from unittest.mock import Mock

import pytest

from rose import rules
from rose.audiotags import AudioTags, RoseDate
from rose.cache import (
    get_release,
    list_releases,
    list_tracks,
    update_cache,
)
from rose.common import Artist
from rose.config import Config
from rose.rule_parser import Matcher, Rule
from rose.rules import (
    FastSearchResult,
    TrackTagNotAllowedError,
    execute_metadata_rule,
    execute_stored_metadata_rules,
    fast_search_for_matching_releases,
    fast_search_for_matching_tracks,
    filter_release_false_positives_using_read_cache,
    filter_track_false_positives_using_read_cache,
    filter_track_noops_using_read_cache,
)


def test_rules_execution_match_substring(config: Config, source_dir: Path) -> None:
    # No match
    rule = Rule.parse("tracktitle:bbb", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle != "lalala"

    # Match
    rule = Rule.parse("tracktitle:rack", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle == "lalala"


def test_rules_execution_match_beginnning(config: Config, source_dir: Path) -> None:
    # No match
    rule = Rule.parse("tracktitle:^rack", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle != "lalala"

    # Match
    rule = Rule.parse("tracktitle:^Track", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle == "lalala"


def test_rules_execution_match_end(config: Config, source_dir: Path) -> None:
    # No match
    rule = Rule.parse("tracktitle:rack$", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle != "lalala"

    # Match
    rule = Rule.parse("tracktitle:rack 1$", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle == "lalala"


def test_rules_execution_match_superstrict(config: Config, source_dir: Path) -> None:
    # No match
    rule = Rule.parse("tracktitle:^Track $", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle != "lalala"

    # Match
    rule = Rule.parse("tracktitle:^Track 1$", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle == "lalala"


def test_rules_execution_match_escaped_superstrict(config: Config, source_dir: Path) -> None:
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    af.tracktitle = "hi^Test$bye"
    af.flush(config)
    update_cache(config)

    # No match
    rule = Rule.parse("tracktitle:^Test$", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle != "lalala"

    # Match
    rule = Rule.parse(r"tracktitle:\^Test\$", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle == "lalala"


def test_rules_execution_match_case_insensitive(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("tracktitle:tRaCk:i", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle == "lalala"


def test_rules_fields_match_tracktitle(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("tracktitle:Track", ["replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle == "8"


def test_rules_fields_match_releasedate(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("releasedate:1990", ["replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.releasedate == RoseDate(8)


def test_rules_fields_match_releasetype(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("releasetype:album", ["replace:live"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.releasetype == "live"


def test_rules_fields_match_tracknumber(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("tracknumber:1", ["replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracknumber == "8"


def test_rules_fields_match_tracktotal(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("tracktotal:2", ["tracktitle/replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle == "8"


def test_rules_fields_match_discnumber(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("discnumber:1", ["replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.discnumber == "8"


def test_rules_fields_match_disctotal(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("disctotal:1", ["tracktitle/replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle == "8"


def test_rules_fields_match_releasetitle(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("releasetitle:Love Blackpink", ["replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.releasetitle == "8"


def test_rules_fields_match_genre(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("genre:K-Pop", ["replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["8", "Pop"]


def test_rules_fields_match_label(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("label:Cool", ["replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.label == ["8"]


def test_rules_fields_match_releaseartist(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("releaseartist:BLACKPINK", ["replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.releaseartists.main == [Artist("8")]


def test_rules_fields_match_trackartist(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("trackartist:BLACKPINK", ["replace:8"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.trackartists.main == [Artist("8")]


@pytest.mark.usefixtures("source_dir")
def test_rules_fields_match_new(config: Config) -> None:
    rule = Rule.parse("new:false", ["replace:true"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    release = get_release(config, "ilovecarly")
    assert release
    assert release.new
    release = get_release(config, "ilovenewjeans")
    assert release
    assert release.new

    rule = Rule.parse("new:true", ["replace:false"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    release = get_release(config, "ilovecarly")
    assert release
    assert not release.new
    release = get_release(config, "ilovenewjeans")
    assert release
    assert not release.new

    rule = Rule.parse("releasetitle:Carly", ["new/replace:true"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    release = get_release(config, "ilovecarly")
    assert release
    assert release.new
    release = get_release(config, "ilovenewjeans")
    assert release
    assert not release.new


@pytest.mark.usefixtures("source_dir")
def test_rules_fields_match_favorite(config: Config) -> None:
    rule = Rule.parse("favorite:false", ["replace:true"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    release = get_release(config, "ilovecarly")
    assert release
    assert release.favorite
    release = get_release(config, "ilovenewjeans")
    assert release
    assert release.favorite

    rule = Rule.parse("favorite:true", ["replace:false"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    release = get_release(config, "ilovecarly")
    assert release
    assert not release.favorite
    release = get_release(config, "ilovenewjeans")
    assert release
    assert not release.favorite

    rule = Rule.parse("releasetitle:Carly", ["favorite/replace:true"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    release = get_release(config, "ilovecarly")
    assert release
    assert release.favorite
    release = get_release(config, "ilovenewjeans")
    assert release
    assert not release.favorite


def test_match_backslash(config: Config, source_dir: Path) -> None:
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    af.tracktitle = r"X \\ Y"
    af.flush(config)
    update_cache(config)

    rule = Rule.parse(r"tracktitle: \\ ", [r"sed: \\\\ : // "])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle == "X / Y"


def test_action_replace_with_delimiter(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("genre:K-Pop", ["replace:Hip-Hop;Rap"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["Hip-Hop", "Rap", "Pop"]


def test_action_replace_with_delimiters_empty_str(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("genre:K-Pop", ["matched:/replace:Hip-Hop;;;;"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["Hip-Hop"]


def test_sed_action(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("tracktitle:Track", ["sed:ack:ip"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle == "Trip 1"


def test_sed_no_pattern(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("genre:P", [r"matched:/sed:^(.*)$:i\1"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["iK-Pop", "iPop"]


def test_split_action(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("label:Cool", ["split:Cool"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.label == ["A", "Label"]


def test_split_action_no_pattern(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("genre:K-Pop", ["matched:/split:P"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["K-", "op"]


def test_add_action(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("label:Cool", ["add:Even Cooler Label"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.label == ["A Cool Label", "Even Cooler Label"]


def test_delete_action(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("genre:^Pop$", ["delete"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["K-Pop"]


def test_delete_action_no_pattern(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("genre:^Pop$", ["matched:/delete"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == []


def test_preserves_unmatched_multitags(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("genre:^Pop$", ["replace:lalala"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["K-Pop", "lalala"]


def test_action_on_different_tag(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("label:A Cool Label", ["genre/replace:hi"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["hi"]


def test_action_no_pattern(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("genre:K-Pop", ["matched:/sed:P:B"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.genre == ["K-Bop", "Bop"]


def test_copy_action_single_to_single(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("tracktitle:Track", ["edition/copy:releasetitle"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.edition == "I Love Blackpink"


def test_copy_action_single_to_multi(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("tracktitle:Track", ["label/copy:tracktitle"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.label == ["Track 1"]


def test_copy_action_multi_to_multi_overwrites(config: Config, source_dir: Path) -> None:
    # The destination list is overwritten wholesale (label was ["A Cool Label"]).
    rule = Rule.parse("tracktitle:Track", ["label/copy:genre"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.label == ["K-Pop", "Pop"]


def test_copy_action_into_empty_multi(config: Config, source_dir: Path) -> None:
    # descriptor starts empty; copy must still populate it.
    rule = Rule.parse("tracktitle:Track", ["descriptor/copy:genre"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.descriptor == ["K-Pop", "Pop"]


def test_copy_action_with_sed_single(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("tracktitle:Track", ["edition/copy:releasetitle:sed:Blackpink:BLACKPINK"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.edition == "I Love BLACKPINK"


def test_copy_action_with_sed_multi(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("tracktitle:Track", ["descriptor/copy:genre:sed:Pop:Rock"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.descriptor == ["K-Rock", "Rock"]


def test_copy_action_pattern_filters_source(config: Config, source_dir: Path) -> None:
    # Only source values matching the action pattern are copied.
    rule = Rule.parse("tracktitle:Track", ["descriptor:^Pop$/copy:genre"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.descriptor == ["Pop"]


def test_copy_action_from_datafile_source(config: Config, source_dir: Path) -> None:
    # New releases default to new=true; copying reads the datafile value.
    rule = Rule.parse("tracktitle:Track", ["edition/copy:new"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.edition == "true"


def test_chained_action(config: Config, source_dir: Path) -> None:
    rule = Rule.parse(
        "label:A Cool Label",
        [
            "replace:Jennie",
            "label:^Jennie$/replace:Jisoo",
            "label:nomatch/replace:Rose",
            "genre/replace:haha",
        ],
    )
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.label == ["Jisoo"]
    assert af.genre == ["haha"]


@pytest.mark.timeout(2)
def test_confirmation_yes(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    rule = Rule.parse("tracktitle:Track", ["replace:lalala"])
    monkeypatch.setattr("rose.rules.click.confirm", lambda *_, **__: True)
    execute_metadata_rule(config, rule, confirm_yes=True)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle == "lalala"


@pytest.mark.timeout(2)
def test_confirmation_no(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    rule = Rule.parse("tracktitle:Track", ["replace:lalala"])
    monkeypatch.setattr("rose.rules.click.confirm", lambda *_, **__: False)
    execute_metadata_rule(config, rule, confirm_yes=True)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle != "lalala"


@pytest.mark.timeout(2)
def test_confirmation_count(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    rule = Rule.parse("tracktitle:Track", ["replace:lalala"])
    monkeypatch.setattr("rose.rules.click.prompt", Mock(side_effect=["no", "8", "6"]))
    # Abort.
    execute_metadata_rule(config, rule, confirm_yes=True, enter_number_to_confirm_above_count=1)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle != "lalala"

    # Success in two arguments.
    execute_metadata_rule(config, rule, confirm_yes=True, enter_number_to_confirm_above_count=1)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle == "lalala"


def test_dry_run(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("tracktitle:Track", ["replace:lalala"])
    execute_metadata_rule(config, rule, dry_run=True, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle != "lalala"


def test_run_stored_rules(config: Config, source_dir: Path) -> None:
    config = dataclasses.replace(
        config,
        stored_metadata_rules=[Rule.parse("tracktitle:Track", ["replace:lalala"])],
    )

    execute_stored_metadata_rules(config)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle == "lalala"


@pytest.mark.usefixtures("seeded_cache")
def test_fast_search_for_matching_releases(config: Config) -> None:
    results = fast_search_for_matching_releases(config, Matcher.parse("releaseartist:Techno Man"))
    assert results == [FastSearchResult(id="r1", path=config.music_source_dir / "r1")]


@pytest.mark.usefixtures("seeded_cache")
def test_fast_search_for_matching_releases_invalid_tag(config: Config) -> None:
    with pytest.raises(TrackTagNotAllowedError):
        fast_search_for_matching_releases(config, Matcher.parse("tracktitle:x"))
    with pytest.raises(TrackTagNotAllowedError):
        fast_search_for_matching_releases(config, Matcher.parse("trackartist:x"))
    # But allow artist tag:
    fast_search_for_matching_releases(config, Matcher.parse("artist:x"))


@pytest.mark.usefixtures("seeded_cache")
def test_filter_release_false_positives_with_read_cache(config: Config) -> None:
    matcher = Matcher.parse("releaseartist:^Man")
    fsresults = fast_search_for_matching_releases(config, matcher)
    assert len(fsresults) == 2
    cacheresults = list_releases(config, [r.id for r in fsresults])
    assert len(cacheresults) == 2
    filteredresults = filter_release_false_positives_using_read_cache(matcher, cacheresults)
    assert not filteredresults


@pytest.mark.usefixtures("seeded_cache")
def test_filter_track_false_positives_with_read_cache(config: Config) -> None:
    matcher = Matcher.parse("trackartist:^Man")
    fsresults = fast_search_for_matching_tracks(config, matcher)
    assert len(fsresults) == 3
    tracks = list_tracks(config, [r.id for r in fsresults])
    assert len(tracks) == 3
    filteredresults = filter_track_false_positives_using_read_cache(matcher, tracks)
    assert not filteredresults


def test_ignore_values(config: Config, source_dir: Path) -> None:
    rule = Rule.parse("tracktitle:rack", ["replace:lalala"], ["tracktitle:^Track 1$"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.tracktitle == "Track 1"


def test_artist_matcher_on_trackartist_only(config: Config, source_dir: Path) -> None:
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    af.trackartists.main = [Artist("BIGBANG & 2NE1")]
    af.releaseartists.main = [Artist("BIGBANG"), Artist("2NE1")]
    af.flush(config)
    update_cache(config)
    rule = Rule.parse("artist: & ", ["split: & "])
    execute_metadata_rule(config, rule, confirm_yes=False)
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    assert af.trackartists.main == [Artist("BIGBANG"), Artist("2NE1")]


def _noop_filter(config: Config, source_dir: Path, matcher: str, actions: list[str]) -> list[str]:
    rule = Rule.parse(matcher, actions)
    tracks = filter_track_noops_using_read_cache(rule.actions, list_tracks(config))
    return [str(t.source_path.relative_to(source_dir)) for t in tracks]


def test_filter_track_noops_with_read_cache(config: Config, source_dir: Path) -> None:
    all_tracks = [f"Test Release {r}/0{t}.m4a" for r in [1, 2, 3] for t in [1, 2]]
    r1 = ["Test Release 1/01.m4a", "Test Release 1/02.m4a"]
    r2 = ["Test Release 2/01.m4a", "Test Release 2/02.m4a"]
    r3 = ["Test Release 3/01.m4a", "Test Release 3/02.m4a"]

    # Single-value tags.
    assert _noop_filter(config, source_dir, "label:Cool", ["tracktitle/replace:Track 1"]) == [
        "Test Release 1/02.m4a",
        "Test Release 2/02.m4a",
        "Test Release 3/02.m4a",
    ]
    assert _noop_filter(config, source_dir, "label:Cool", ["releasetitle:Carly/sed:Carly:Carly"]) == []
    assert _noop_filter(config, source_dir, "label:Cool", ["releasetitle:Carly/sed:Carly:Rae"]) == r2
    assert _noop_filter(config, source_dir, "label:Cool", ["releasedate/replace:1990-02-05"]) == []
    assert _noop_filter(config, source_dir, "label:Cool", ["releasedate/replace:1990"]) == all_tracks
    assert _noop_filter(config, source_dir, "label:Cool", ["releasetype/replace:album"]) == []
    assert _noop_filter(config, source_dir, "label:Cool", ["edition/delete"]) == []
    # Multi-value tags.
    assert _noop_filter(config, source_dir, "label:Cool", ["genre:^Dream Pop$/delete"]) == r2
    assert _noop_filter(config, source_dir, "label:Cool", ["genre:^R&B$/replace:Rhythm;Blues"]) == r3
    assert _noop_filter(config, source_dir, "label:Cool", ["genre:^Shoegaze$/replace:Dream Pop"]) == []
    assert _noop_filter(config, source_dir, "label:Cool", ["trackartist[main]/add:BLACKPINK"]) == [*r2, *r3]
    assert _noop_filter(config, source_dir, "label:Cool", ["trackartist/split: & "]) == []
    assert _noop_filter(config, source_dir, "label:Cool", ["trackartist:^BLACKPINK$/replace:BP"]) == r1
    # Datafile tags.
    assert _noop_filter(config, source_dir, "label:Cool", ["new/replace:false"]) == r1
    assert _noop_filter(config, source_dir, "label:Cool", ["rating/delete"]) == []
    assert _noop_filter(config, source_dir, "label:Cool", ["rating/replace:5"]) == all_tracks
    # Copy actions.
    assert _noop_filter(config, source_dir, "label:Cool", ["tracktitle/copy:tracktitle"]) == []
    assert _noop_filter(config, source_dir, "label:Cool", ["trackartist[main]/copy:trackartist[main]"]) == []
    assert _noop_filter(config, source_dir, "label:Cool", ["descriptor/copy:genre"]) == all_tracks
    # If any action in a chain changes a tag, the track is kept.
    assert _noop_filter(config, source_dir, "label:Cool", ["genre:^Dream Pop$/delete", "new/replace:false"]) == [
        *r1,
        *r2,
    ]


def test_filter_track_noops_with_read_cache_keeps_undeterminable(config: Config, source_dir: Path) -> None:
    all_tracks = [f"Test Release {r}/0{t}.m4a" for r in [1, 2, 3] for t in [1, 2]]
    # The cache normalizes track and disc numbers, so they cannot be compared against the tags.
    assert _noop_filter(config, source_dir, "label:Cool", ["discnumber/replace:1"]) == all_tracks
    assert _noop_filter(config, source_dir, "label:Cool", ["tracktitle/copy:tracknumber"]) == all_tracks
    # Invalid values are kept so that the executor raises on them.
    assert _noop_filter(config, source_dir, "label:Cool", ["new/replace:lalala"]) == all_tracks
    assert _noop_filter(config, source_dir, "label:Cool", ["rating/replace:lalala"]) == all_tracks
    # A missing title is cached as a default value.
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    af.tracktitle = None
    af.flush(config)
    update_cache(config)
    assert _noop_filter(config, source_dir, "label:Cool", ["tracktitle/replace:Unknown Title"]) == all_tracks


def test_filter_track_noops_keeps_tags_deduplicated_by_cache(config: Config, source_dir: Path) -> None:
    all_tracks = [f"Test Release {r}/0{t}.m4a" for r in [1, 2, 3] for t in [1, 2]]
    r2 = ["Test Release 2/01.m4a", "Test Release 2/02.m4a"]
    # The cache drops duplicate values, so it cannot prove that an action which rewrites one of
    # these tags leaves the on-disk values unchanged.
    assert _noop_filter(config, source_dir, "label:Cool", ["genre/add:K-Pop"]) == all_tracks
    assert _noop_filter(config, source_dir, "label:Cool", ["genre:^Dream Pop$/replace:Dream Pop"]) == r2
    assert _noop_filter(config, source_dir, "label:Cool", ["label/copy:label"]) == all_tracks
    # An action whose pattern matches none of the values returns them untouched, duplicates included.
    assert _noop_filter(config, source_dir, "label:Cool", ["genre:^Shoegaze$/replace:K-Pop"]) == []


def test_rules_execution_deduplicates_tags_hidden_by_cache(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    monkeypatch.setattr(rules, "READ_CACHE_FILTER_THRESHOLD", 0)
    path = source_dir / "Test Release 1" / "01.m4a"
    af = AudioTags.from_file(path)
    af.genre = ["K-Pop", "K-Pop", "Pop"]
    af.flush(config)
    assert AudioTags.from_file(path).genre == ["K-Pop", "K-Pop", "Pop"]
    update_cache(config)

    # The cached genres already contain K-Pop, yet the write must still deduplicate the tag.
    execute_metadata_rule(config, Rule.parse("label:Cool", ["genre/add:K-Pop"]), confirm_yes=False)
    assert AudioTags.from_file(path).genre == ["K-Pop", "Pop"]


def test_filter_track_noops_ignores_artist_aliases(config: Config, source_dir: Path) -> None:
    config = dataclasses.replace(
        config,
        artist_aliases_map={"BLACKPINK": ["JISOO"]},
        artist_aliases_parents_map={"JISOO": ["BLACKPINK"]},
    )
    af = AudioTags.from_file(source_dir / "Test Release 1" / "01.m4a")
    af.trackartists.main = [Artist("JISOO")]
    af.flush(config)
    update_cache(config)
    # BLACKPINK is only present as an alias of JISOO on this track, so adding it is a change.
    assert "Test Release 1/01.m4a" in _noop_filter(
        config, source_dir, "label:Cool", ["trackartist[main]/add:BLACKPINK"]
    )
    assert "Test Release 1/02.m4a" not in _noop_filter(
        config, source_dir, "label:Cool", ["trackartist[main]/add:BLACKPINK"]
    )


def test_rules_execution_skips_reading_noop_tracks(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    monkeypatch.setattr(rules, "READ_CACHE_FILTER_THRESHOLD", 0)
    # The paths whose tags the rule engine reads from disk.
    read_paths: list[Path] = []
    tag_filter = rules.filter_track_false_positives_using_tags

    def spy(matcher: Matcher, fsrs: list[FastSearchResult], ignore: list[Matcher]) -> list[AudioTags]:
        read_paths.extend(x.path for x in fsrs)
        return tag_filter(matcher, fsrs, ignore)

    monkeypatch.setattr(rules, "filter_track_false_positives_using_tags", spy)

    rule = Rule.parse("label:Cool", ["genre:^Dream Pop$/replace:Shoegaze"])
    execute_metadata_rule(config, rule, confirm_yes=False)
    assert read_paths == [source_dir / "Test Release 2" / "01.m4a", source_dir / "Test Release 2" / "02.m4a"]
    af = AudioTags.from_file(source_dir / "Test Release 2" / "01.m4a")
    assert af.genre == ["Pop", "Shoegaze"]

    # The rule is now a no-op for every track, so no track is read from disk.
    read_paths.clear()
    execute_metadata_rule(config, rule, confirm_yes=False)
    assert read_paths == []
