from pathlib import Path

import pytest

from rose.audiotags import AudioTags
from rose.config import Config
from rose.rule_parser import MetadataAction, MetadataMatcher
from rose.tracks import find_tracks_matching_rule, run_actions_on_track


def test_run_action_on_track(config: Config, source_dir: Path) -> None:
    action = MetadataAction.parse("tracktitle/replace:Bop")
    af = AudioTags.from_file(source_dir / "Test Release 2" / "01.m4a")
    assert af.id is not None
    run_actions_on_track(config, af.id, [action])
    af = AudioTags.from_file(source_dir / "Test Release 2" / "01.m4a")
    assert af.tracktitle == "Bop"


@pytest.mark.usefixtures("seeded_cache")
def test_find_matching_tracks(config: Config) -> None:
    results = find_tracks_matching_rule(config, MetadataMatcher.parse("releasetitle:Release 2"))
    assert {r.id for r in results} == {"t3"}
    results = find_tracks_matching_rule(config, MetadataMatcher.parse("tracktitle:Track 2"))
    assert {r.id for r in results} == {"t2"}
    results = find_tracks_matching_rule(config, MetadataMatcher.parse("artist:^Techno Man$"))
    assert {r.id for r in results} == {"t1", "t2"}
    results = find_tracks_matching_rule(config, MetadataMatcher.parse("artist:Techno Man"))
    assert {r.id for r in results} == {"t1", "t2"}
    results = find_tracks_matching_rule(config, MetadataMatcher.parse("genre:^Deep House$"))
    assert {r.id for r in results} == {"t1", "t2"}
    results = find_tracks_matching_rule(config, MetadataMatcher.parse("genre:Deep House"))
    assert {r.id for r in results} == {"t1", "t2"}
    results = find_tracks_matching_rule(config, MetadataMatcher.parse("descriptor:^Wet$"))
    assert {r.id for r in results} == {"t3"}
    results = find_tracks_matching_rule(config, MetadataMatcher.parse("descriptor:Wet"))
    assert {r.id for r in results} == {"t3"}
    results = find_tracks_matching_rule(config, MetadataMatcher.parse("label:^Native State$"))
    assert {r.id for r in results} == {"t3"}
    results = find_tracks_matching_rule(config, MetadataMatcher.parse("label:Native State"))
    assert {r.id for r in results} == {"t3"}
