from pathlib import Path

from rose.audiotags import AudioTags
from rose.config import Config
from rose.rule_parser import MetadataAction
from rose.tracks import run_actions_on_track


def test_run_action_on_track(config: Config, source_dir: Path) -> None:
    action = MetadataAction.parse("tracktitle::replace:Bop")
    af = AudioTags.from_file(source_dir / "Test Release 2" / "01.m4a")
    assert af.id is not None
    run_actions_on_track(config, af.id, [action])
    af = AudioTags.from_file(source_dir / "Test Release 2" / "01.m4a")
    assert af.title == "Bop"
