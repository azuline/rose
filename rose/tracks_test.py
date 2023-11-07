import json
from pathlib import Path

import pytest

from rose.audiotags import AudioTags
from rose.config import Config
from rose.rule_parser import MetadataAction
from rose.tracks import dump_track, run_actions_on_track


def test_run_action_on_track(config: Config, source_dir: Path) -> None:
    action = MetadataAction.parse("tracktitle::replace:Bop")
    af = AudioTags.from_file(source_dir / "Test Release 2" / "01.m4a")
    assert af.id is not None
    run_actions_on_track(config, af.id, [action])
    af = AudioTags.from_file(source_dir / "Test Release 2" / "01.m4a")
    assert af.title == "Bop"


@pytest.mark.usefixtures("seeded_cache")
def test_dump_track(config: Config) -> None:
    assert json.loads(dump_track(config, "t1")) == {
        "artists": {
            "composer": [],
            "djmixer": [],
            "guest": [],
            "main": [
                {"alias": False, "name": "Techno Man"},
                {"alias": False, "name": "Bass Man"},
            ],
            "producer": [],
            "remixer": [],
        },
        "discnumber": "01",
        "duration_seconds": 120,
        "id": "t1",
        "release_id": "r1",
        "source_path": f"{config.music_source_dir}/r1/01.m4a",
        "title": "Track 1",
        "tracknumber": "01",
    }
