import json
import re
import shutil
from pathlib import Path
from typing import Any

import pytest
import tomllib

from conftest import TEST_RELEASE_1
from rose.audiotags import AudioTags
from rose.cache import (
    CachedRelease,
    CachedTrack,
    connect,
    get_release,
    get_tracks_associated_with_release,
    update_cache,
)
from rose.common import Artist, ArtistMapping
from rose.config import Config
from rose.releases import (
    ReleaseEditFailedError,
    create_single_release,
    delete_release,
    delete_release_cover_art,
    dump_release,
    dump_releases,
    edit_release,
    run_actions_on_release,
    set_release_cover_art,
    toggle_release_new,
)
from rose.rule_parser import MetadataAction, MetadataMatcher


def test_delete_release(config: Config) -> None:
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    update_cache(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT id FROM releases")
        release_id = cursor.fetchone()["id"]
    delete_release(config, release_id)
    assert not (config.music_source_dir / TEST_RELEASE_1.name).exists()
    with connect(config) as conn:
        cursor = conn.execute("SELECT COUNT(*) FROM releases")
        assert cursor.fetchone()[0] == 0


def test_toggle_release_new(config: Config) -> None:
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    update_cache(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT id FROM releases")
        release_id = cursor.fetchone()["id"]
    datafile = config.music_source_dir / TEST_RELEASE_1.name / f".rose.{release_id}.toml"

    # Set not new.
    toggle_release_new(config, release_id)
    with datafile.open("rb") as fp:
        data = tomllib.load(fp)
        assert data["new"] is False
    with connect(config) as conn:
        cursor = conn.execute("SELECT new FROM releases")
        assert not cursor.fetchone()["new"]

    # Set new.
    toggle_release_new(config, release_id)
    with datafile.open("rb") as fp:
        data = tomllib.load(fp)
        assert data["new"] is True
    with connect(config) as conn:
        cursor = conn.execute("SELECT new FROM releases")
        assert cursor.fetchone()["new"]


def test_set_release_cover_art(isolated_dir: Path, config: Config) -> None:
    imagepath = isolated_dir / "folder.jpg"
    with imagepath.open("w") as fp:
        fp.write("lalala")

    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    old_image_1 = release_dir / "folder.png"
    old_image_2 = release_dir / "cover.jpeg"
    old_image_1.touch()
    old_image_2.touch()
    update_cache(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT id FROM releases")
        release_id = cursor.fetchone()["id"]

    set_release_cover_art(config, release_id, imagepath)
    cover_image_path = release_dir / "cover.jpg"
    assert cover_image_path.is_file()
    with cover_image_path.open("r") as fp:
        assert fp.read() == "lalala"
    assert not old_image_1.exists()
    assert not old_image_2.exists()
    # Assert no other files were touched.
    assert len(list(release_dir.iterdir())) == 5

    with connect(config) as conn:
        cursor = conn.execute("SELECT cover_image_path FROM releases")
        assert Path(cursor.fetchone()["cover_image_path"]) == cover_image_path


def test_remove_release_cover_art(config: Config) -> None:
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    (release_dir / "folder.png").touch()
    update_cache(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT id FROM releases")
        release_id = cursor.fetchone()["id"]

    delete_release_cover_art(config, release_id)
    assert not (release_dir / "folder.png").exists()
    with connect(config) as conn:
        cursor = conn.execute("SELECT cover_image_path FROM releases")
        assert not cursor.fetchone()["cover_image_path"]


def test_edit_release(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    release_path = source_dir / TEST_RELEASE_1.name
    with connect(config) as conn:
        cursor = conn.execute("SELECT id FROM releases WHERE source_path = ?", (str(release_path),))
        release_id = cursor.fetchone()["id"]
        cursor = conn.execute(
            "SELECT id FROM tracks WHERE release_id = ? ORDER BY tracknumber", (str(release_id),)
        )
        track_ids = [r["id"] for r in cursor]
        assert len(track_ids) == 2

    new_toml = f"""
        title = "I Really Love Blackpink"
        new = false
        releasetype = "single"
        year = 2222
        genres = [
            "J-Pop",
            "Pop-Rap",
        ]
        labels = [
            "YG Entertainment",
        ]
        artists = [
            {{ name = "BLACKPINK", role = "main" }},
            {{ name = "JISOO", role = "main" }},
        ]

        [tracks.{track_ids[0]}]
        discnumber = "1"
        tracknumber = "1"
        title = "I Do Like That"
        artists = [
            {{ name = "BLACKPINK", role = "main" }},
        ]

        [tracks.{track_ids[1]}]
        discnumber = "1"
        tracknumber = "2"
        title = "All Eyes On Me"
        artists = [
            {{ name = "JISOO", role = "main" }},
        ]
    """
    monkeypatch.setattr("rose.collages.click.edit", lambda *_, **__: new_toml)

    edit_release(config, release_id)
    release = get_release(config, release_id)
    assert release is not None
    assert release == CachedRelease(
        id=release_id,
        source_path=release_path,
        cover_image_path=None,
        added_at=release.added_at,
        datafile_mtime=release.datafile_mtime,
        releasetitle="I Really Love Blackpink",
        releasetype="single",
        year=2222,
        new=False,
        disctotal=1,
        genres=["J-Pop", "Pop-Rap"],
        labels=["YG Entertainment"],
        releaseartists=ArtistMapping(main=[Artist("BLACKPINK"), Artist("JISOO")]),
        metahash=release.metahash,
    )
    tracks = get_tracks_associated_with_release(config, release)
    assert tracks == [
        CachedTrack(
            id=track_ids[0],
            source_path=release_path / "01.m4a",
            source_mtime=tracks[0].source_mtime,
            tracktitle="I Do Like That",
            tracknumber="1",
            tracktotal=2,
            discnumber="1",
            disctotal=1,
            duration_seconds=2,
            trackartists=ArtistMapping(main=[Artist("BLACKPINK")]),
            metahash=tracks[0].metahash,
            release=release,
        ),
        CachedTrack(
            id=track_ids[1],
            source_path=release_path / "02.m4a",
            source_mtime=tracks[1].source_mtime,
            tracktitle="All Eyes On Me",
            tracknumber="2",
            tracktotal=2,
            discnumber="1",
            disctotal=1,
            duration_seconds=2,
            trackartists=ArtistMapping(main=[Artist("JISOO")]),
            metahash=tracks[1].metahash,
            release=release,
        ),
    ]


def test_edit_release_failure_and_resume(
    monkeypatch: Any, config: Config, source_dir: Path
) -> None:
    release_path = source_dir / TEST_RELEASE_1.name
    with connect(config) as conn:
        cursor = conn.execute("SELECT id FROM releases WHERE source_path = ?", (str(release_path),))
        release_id = cursor.fetchone()["id"]
        cursor = conn.execute(
            "SELECT id FROM tracks WHERE release_id = ? ORDER BY tracknumber", (str(release_id),)
        )
        track_ids = [r["id"] for r in cursor]
        assert len(track_ids) == 2

    # Notice the bullshit releasetype.
    bad_toml = f"""
        title = "I Really Love Blackpink"
        new = false
        releasetype = "bullshit"
        year = 2222
        genres = [
            "J-Pop",
            "Pop-Rap",
        ]
        labels = [
            "YG Entertainment",
        ]
        artists = [
            {{ name = "BLACKPINK", role = "main" }},
            {{ name = "JISOO", role = "main" }},
        ]

        [tracks.{track_ids[0]}]
        discnumber = "1"
        tracknumber = "1"
        title = "I Do Like That"
        artists = [
            {{ name = "BLACKPINK", role = "main" }},
        ]

        [tracks.{track_ids[1]}]
        discnumber = "1"
        tracknumber = "2"
        title = "All Eyes On Me"
        artists = [
            {{ name = "JISOO", role = "main" }},
        ]
    """
    monkeypatch.setattr("rose.collages.click.edit", lambda *_, **__: bad_toml)

    with pytest.raises(ReleaseEditFailedError) as exc:
        edit_release(config, release_id)
    errmsg = str(exc.value)
    match = re.search(r"--resume ([^ ]+)", errmsg)
    assert match is not None
    resume_file = Path(match[1])

    correct_toml = f"""
        title = "I Really Love Blackpink"
        new = false
        releasetype = "single"
        year = 2222
        genres = [
            "J-Pop",
            "Pop-Rap",
        ]
        labels = [
            "YG Entertainment",
        ]
        artists = [
            {{ name = "BLACKPINK", role = "main" }},
            {{ name = "JISOO", role = "main" }},
        ]

        [tracks.{track_ids[0]}]
        discnumber = "1"
        tracknumber = "1"
        title = "I Do Like That"
        artists = [
            {{ name = "BLACKPINK", role = "main" }},
        ]

        [tracks.{track_ids[1]}]
        discnumber = "1"
        tracknumber = "2"
        title = "All Eyes On Me"
        artists = [
            {{ name = "JISOO", role = "main" }},
        ]
    """

    def editfn(text: str, **_: Any) -> str:
        assert text == bad_toml
        return correct_toml

    monkeypatch.setattr("rose.collages.click.edit", editfn)
    edit_release(config, release_id, resume_file=resume_file)

    # Assert the file got deleted.
    assert not resume_file.exists()

    release = get_release(config, release_id)
    assert release is not None
    assert release == CachedRelease(
        id=release_id,
        source_path=release_path,
        cover_image_path=None,
        added_at=release.added_at,
        datafile_mtime=release.datafile_mtime,
        releasetitle="I Really Love Blackpink",
        releasetype="single",
        year=2222,
        new=False,
        disctotal=1,
        genres=["J-Pop", "Pop-Rap"],
        labels=["YG Entertainment"],
        releaseartists=ArtistMapping(main=[Artist("BLACKPINK"), Artist("JISOO")]),
        metahash=release.metahash,
    )
    tracks = get_tracks_associated_with_release(config, release)
    assert tracks == [
        CachedTrack(
            id=track_ids[0],
            source_path=release_path / "01.m4a",
            source_mtime=tracks[0].source_mtime,
            tracktitle="I Do Like That",
            tracknumber="1",
            tracktotal=2,
            discnumber="1",
            disctotal=1,
            duration_seconds=2,
            trackartists=ArtistMapping(main=[Artist("BLACKPINK")]),
            metahash=tracks[0].metahash,
            release=release,
        ),
        CachedTrack(
            id=track_ids[1],
            source_path=release_path / "02.m4a",
            source_mtime=tracks[1].source_mtime,
            tracktitle="All Eyes On Me",
            tracknumber="2",
            tracktotal=2,
            discnumber="1",
            disctotal=1,
            duration_seconds=2,
            trackartists=ArtistMapping(main=[Artist("JISOO")]),
            metahash=tracks[1].metahash,
            release=release,
        ),
    ]


def test_extract_single_release(config: Config) -> None:
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    cover_art_path = config.music_source_dir / TEST_RELEASE_1.name / "cover.jpg"
    cover_art_path.touch()
    update_cache(config)
    create_single_release(config, config.music_source_dir / TEST_RELEASE_1.name / "02.m4a")
    # Assert nothing happened to the files we "extracted."
    assert (config.music_source_dir / TEST_RELEASE_1.name / "02.m4a").is_file()
    assert cover_art_path.is_file()
    # Assert that we've successfully written/copied our files.
    source_path = config.music_source_dir / "BLACKPINK - 1990. Track 2"
    assert source_path.is_dir()
    assert (source_path / "01. Track 2.m4a").is_file()
    assert (source_path / "cover.jpg").is_file()
    af = AudioTags.from_file(source_path / "01. Track 2.m4a")
    assert af.release == "Track 2"
    assert af.tracknumber == "1"
    assert af.discnumber == "1"
    assert af.releasetype == "single"
    assert af.releaseartists == af.trackartists


def test_extract_single_release_with_trailing_space(config: Config) -> None:
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    af = AudioTags.from_file(release_dir / "02.m4a")
    af.title = "Trailing Space "
    af.flush()
    update_cache(config)
    create_single_release(config, release_dir / "02.m4a")
    # Assert that we've successfully written/copied our files.
    source_path = config.music_source_dir / "BLACKPINK - 1990. Trailing Space"
    assert source_path.is_dir()
    assert (source_path / "01. Trailing Space.m4a").is_file()


@pytest.mark.usefixtures("seeded_cache")
def test_dump_release(config: Config) -> None:
    assert json.loads(dump_release(config, "r1")) == {
        "id": "r1",
        "source_path": f"{config.music_source_dir}/r1",
        "cover_image_path": None,
        "added_at": "0000-01-01T00:00:00+00:00",
        "releasetitle": "Release 1",
        "releasetype": "album",
        "year": 2023,
        "new": False,
        "disctotal": 1,
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
        "tracks": [
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
            },
        ],
    }


@pytest.mark.usefixtures("seeded_cache")
def test_dump_releases(config: Config) -> None:
    assert json.loads(dump_releases(config)) == [
        {
            "id": "r1",
            "source_path": f"{config.music_source_dir}/r1",
            "cover_image_path": None,
            "added_at": "0000-01-01T00:00:00+00:00",
            "releasetitle": "Release 1",
            "releasetype": "album",
            "year": 2023,
            "new": False,
            "disctotal": 1,
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
            "tracks": [
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
                },
            ],
        },
        {
            "id": "r2",
            "source_path": f"{config.music_source_dir}/r2",
            "cover_image_path": f"{config.music_source_dir}/r2/cover.jpg",
            "added_at": "0000-01-01T00:00:00+00:00",
            "releasetitle": "Release 2",
            "releasetype": "album",
            "year": 2021,
            "new": False,
            "disctotal": 1,
            "genres": ["Classical"],
            "labels": ["Native State"],
            "releaseartists": {
                "main": [{"name": "Violin Woman", "alias": False}],
                "guest": [{"name": "Conductor Woman", "alias": False}],
                "remixer": [],
                "producer": [],
                "composer": [],
                "conductor": [],
                "djmixer": [],
            },
            "tracks": [
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
                }
            ],
        },
        {
            "id": "r3",
            "source_path": f"{config.music_source_dir}/r3",
            "cover_image_path": None,
            "added_at": "0000-01-01T00:00:00+00:00",
            "releasetitle": "Release 3",
            "releasetype": "album",
            "year": 2021,
            "new": True,
            "disctotal": 1,
            "genres": [],
            "labels": [],
            "releaseartists": {
                "main": [],
                "guest": [],
                "remixer": [],
                "producer": [],
                "composer": [],
                "conductor": [],
                "djmixer": [],
            },
            "tracks": [
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
                }
            ],
        },
    ]


@pytest.mark.usefixtures("seeded_cache")
def test_dump_releases_matcher(config: Config) -> None:
    matcher = MetadataMatcher.parse("releasetitle:2$")
    assert json.loads(dump_releases(config, matcher)) == [
        {
            "id": "r2",
            "source_path": f"{config.music_source_dir}/r2",
            "cover_image_path": f"{config.music_source_dir}/r2/cover.jpg",
            "added_at": "0000-01-01T00:00:00+00:00",
            "releasetitle": "Release 2",
            "releasetype": "album",
            "year": 2021,
            "new": False,
            "disctotal": 1,
            "genres": ["Classical"],
            "labels": ["Native State"],
            "releaseartists": {
                "main": [{"name": "Violin Woman", "alias": False}],
                "guest": [{"name": "Conductor Woman", "alias": False}],
                "remixer": [],
                "producer": [],
                "composer": [],
                "conductor": [],
                "djmixer": [],
            },
            "tracks": [
                {
                    "trackartists": {
                        "composer": [],
                        "djmixer": [],
                        "guest": [{"name": "Conductor Woman", "alias": False}],
                        "main": [{"name": "Violin Woman", "alias": False}],
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
                }
            ],
        },
    ]


def test_run_action_on_release(config: Config, source_dir: Path) -> None:
    action = MetadataAction.parse("tracktitle::replace:Bop")
    run_actions_on_release(config, "ilovecarly", [action])
    af = AudioTags.from_file(source_dir / "Test Release 2" / "01.m4a")
    assert af.title == "Bop"
