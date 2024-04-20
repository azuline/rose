import json
from pathlib import Path

import pytest

from rose.audiotags import AudioTags
from rose.config import Config
from rose.rule_parser import MetadataAction, MetadataMatcher
from rose.tracks import dump_all_tracks, dump_track, run_actions_on_track


def test_run_action_on_track(config: Config, source_dir: Path) -> None:
    action = MetadataAction.parse("tracktitle/replace:Bop")
    af = AudioTags.from_file(source_dir / "Test Release 2" / "01.m4a")
    assert af.id is not None
    run_actions_on_track(config, af.id, [action])
    af = AudioTags.from_file(source_dir / "Test Release 2" / "01.m4a")
    assert af.title == "Bop"


@pytest.mark.usefixtures("seeded_cache")
def test_dump_tracks(config: Config) -> None:
    assert json.loads(dump_all_tracks(config)) == [
        {
            "trackartists": {
                "composer": [],
                "djmixer": [],
                "guest": [],
                "main": [
                    {"alias": False, "name": "Techno Man"},
                    {"alias": False, "name": "Bass Man"},
                ],
                "producer": [],
                "remixer": [],
                "conductor": [],
            },
            "discnumber": "01",
            "disctotal": 1,
            "duration_seconds": 120,
            "id": "t1",
            "source_path": f"{config.music_source_dir}/r1/01.m4a",
            "tracktitle": "Track 1",
            "tracknumber": "01",
            "tracktotal": 2,
            "added_at": "0000-01-01T00:00:00+00:00",
            "release_id": "r1",
            "releasetitle": "Release 1",
            "releasetype": "album",
            "releaseyear": 2023,
            "new": False,
            "genres": ["Techno", "Deep House"],
            "labels": ["Silk Music"],
            "releaseartists": {
                "main": [
                    {"name": "Techno Man", "alias": False},
                    {"name": "Bass Man", "alias": False},
                ],
                "guest": [],
                "remixer": [],
                "conductor": [],
                "producer": [],
                "composer": [],
                "djmixer": [],
            },
        },
        {
            "trackartists": {
                "composer": [],
                "djmixer": [],
                "guest": [],
                "main": [
                    {"alias": False, "name": "Techno Man"},
                    {"alias": False, "name": "Bass Man"},
                ],
                "producer": [],
                "remixer": [],
                "conductor": [],
            },
            "discnumber": "01",
            "disctotal": 1,
            "duration_seconds": 240,
            "id": "t2",
            "source_path": f"{config.music_source_dir}/r1/02.m4a",
            "tracktitle": "Track 2",
            "tracknumber": "02",
            "tracktotal": 2,
            "added_at": "0000-01-01T00:00:00+00:00",
            "release_id": "r1",
            "releasetitle": "Release 1",
            "releasetype": "album",
            "releaseyear": 2023,
            "new": False,
            "genres": ["Techno", "Deep House"],
            "labels": ["Silk Music"],
            "releaseartists": {
                "main": [
                    {"name": "Techno Man", "alias": False},
                    {"name": "Bass Man", "alias": False},
                ],
                "guest": [],
                "remixer": [],
                "conductor": [],
                "producer": [],
                "composer": [],
                "djmixer": [],
            },
        },
        {
            "trackartists": {
                "composer": [],
                "djmixer": [],
                "guest": [{"alias": False, "name": "Conductor Woman"}],
                "main": [{"alias": False, "name": "Violin Woman"}],
                "producer": [],
                "remixer": [],
                "conductor": [],
            },
            "discnumber": "01",
            "disctotal": 1,
            "duration_seconds": 120,
            "id": "t3",
            "source_path": f"{config.music_source_dir}/r2/01.m4a",
            "tracktitle": "Track 1",
            "tracknumber": "01",
            "tracktotal": 1,
            "added_at": "0000-01-01T00:00:00+00:00",
            "release_id": "r2",
            "releasetitle": "Release 2",
            "releasetype": "album",
            "releaseyear": 2021,
            "new": False,
            "genres": ["Classical"],
            "labels": ["Native State"],
            "releaseartists": {
                "main": [{"name": "Violin Woman", "alias": False}],
                "guest": [{"name": "Conductor Woman", "alias": False}],
                "remixer": [],
                "conductor": [],
                "producer": [],
                "composer": [],
                "djmixer": [],
            },
        },
        {
            "trackartists": {
                "composer": [],
                "djmixer": [],
                "guest": [],
                "main": [],
                "producer": [],
                "remixer": [],
                "conductor": [],
            },
            "discnumber": "01",
            "disctotal": 1,
            "duration_seconds": 120,
            "id": "t4",
            "source_path": f"{config.music_source_dir}/r3/01.m4a",
            "tracktitle": "Track 1",
            "tracknumber": "01",
            "tracktotal": 1,
            "added_at": "0000-01-01T00:00:00+00:00",
            "release_id": "r3",
            "releasetitle": "Release 3",
            "releasetype": "album",
            "releaseyear": 2021,
            "new": True,
            "genres": [],
            "labels": [],
            "releaseartists": {
                "main": [],
                "guest": [],
                "remixer": [],
                "conductor": [],
                "producer": [],
                "composer": [],
                "djmixer": [],
            },
        },
    ]


@pytest.mark.usefixtures("seeded_cache")
def test_dump_tracks_with_matcher(config: Config) -> None:
    matcher = MetadataMatcher.parse("artist:Techno Man")
    assert json.loads(dump_all_tracks(config, matcher)) == [
        {
            "trackartists": {
                "composer": [],
                "djmixer": [],
                "guest": [],
                "main": [
                    {"alias": False, "name": "Techno Man"},
                    {"alias": False, "name": "Bass Man"},
                ],
                "producer": [],
                "remixer": [],
                "conductor": [],
            },
            "discnumber": "01",
            "disctotal": 1,
            "duration_seconds": 120,
            "id": "t1",
            "source_path": f"{config.music_source_dir}/r1/01.m4a",
            "tracktitle": "Track 1",
            "tracknumber": "01",
            "tracktotal": 2,
            "added_at": "0000-01-01T00:00:00+00:00",
            "release_id": "r1",
            "releasetitle": "Release 1",
            "releasetype": "album",
            "releaseyear": 2023,
            "new": False,
            "genres": ["Techno", "Deep House"],
            "labels": ["Silk Music"],
            "releaseartists": {
                "main": [
                    {"name": "Techno Man", "alias": False},
                    {"name": "Bass Man", "alias": False},
                ],
                "guest": [],
                "remixer": [],
                "conductor": [],
                "producer": [],
                "composer": [],
                "djmixer": [],
            },
        },
        {
            "trackartists": {
                "composer": [],
                "djmixer": [],
                "guest": [],
                "main": [
                    {"alias": False, "name": "Techno Man"},
                    {"alias": False, "name": "Bass Man"},
                ],
                "producer": [],
                "remixer": [],
                "conductor": [],
            },
            "discnumber": "01",
            "disctotal": 1,
            "duration_seconds": 240,
            "id": "t2",
            "source_path": f"{config.music_source_dir}/r1/02.m4a",
            "tracktitle": "Track 2",
            "tracknumber": "02",
            "tracktotal": 2,
            "added_at": "0000-01-01T00:00:00+00:00",
            "release_id": "r1",
            "releasetitle": "Release 1",
            "releasetype": "album",
            "releaseyear": 2023,
            "new": False,
            "genres": ["Techno", "Deep House"],
            "labels": ["Silk Music"],
            "releaseartists": {
                "main": [
                    {"name": "Techno Man", "alias": False},
                    {"name": "Bass Man", "alias": False},
                ],
                "guest": [],
                "remixer": [],
                "conductor": [],
                "producer": [],
                "composer": [],
                "djmixer": [],
            },
        },
    ]


@pytest.mark.usefixtures("seeded_cache")
def test_dump_track(config: Config) -> None:
    assert json.loads(dump_track(config, "t1")) == {
        "trackartists": {
            "composer": [],
            "djmixer": [],
            "guest": [],
            "main": [
                {"alias": False, "name": "Techno Man"},
                {"alias": False, "name": "Bass Man"},
            ],
            "producer": [],
            "remixer": [],
            "conductor": [],
        },
        "discnumber": "01",
        "disctotal": 1,
        "duration_seconds": 120,
        "id": "t1",
        "source_path": f"{config.music_source_dir}/r1/01.m4a",
        "tracktitle": "Track 1",
        "tracknumber": "01",
        "tracktotal": 2,
        "added_at": "0000-01-01T00:00:00+00:00",
        "release_id": "r1",
        "releasetitle": "Release 1",
        "releasetype": "album",
        "releaseyear": 2023,
        "new": False,
        "genres": ["Techno", "Deep House"],
        "labels": ["Silk Music"],
        "releaseartists": {
            "main": [
                {"name": "Techno Man", "alias": False},
                {"name": "Bass Man", "alias": False},
            ],
            "guest": [],
            "remixer": [],
            "producer": [],
            "composer": [],
            "conductor": [],
            "djmixer": [],
        },
    }
