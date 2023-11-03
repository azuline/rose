import json
import re
import shutil
from pathlib import Path
from typing import Any

import pytest
import tomllib

from conftest import TEST_RELEASE_1
from rose.audiotags import AudioTags
from rose.cache import CachedArtist, CachedRelease, CachedTrack, connect, get_release, update_cache
from rose.config import Config
from rose.releases import (
    ReleaseDoesNotExistError,
    ReleaseEditFailedError,
    create_single_release,
    delete_release,
    delete_release_cover_art,
    dump_releases,
    edit_release,
    resolve_release_ids,
    set_release_cover_art,
    toggle_release_new,
)


def test_delete_release(config: Config) -> None:
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    update_cache(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT id, virtual_dirname FROM releases")
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
        cursor = conn.execute("SELECT virtual_dirname FROM releases")
        assert not cursor.fetchone()["virtual_dirname"].startswith("{NEW} ")

    # Set new.
    toggle_release_new(config, release_id)
    with datafile.open("rb") as fp:
        data = tomllib.load(fp)
        assert data["new"] is True
    with connect(config) as conn:
        cursor = conn.execute("SELECT virtual_dirname FROM releases")
        assert cursor.fetchone()["virtual_dirname"].startswith("{NEW} ")


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
    rdata = get_release(config, release_id)
    assert rdata is not None
    release, tracks = rdata
    assert release == CachedRelease(
        id=release_id,
        source_path=release_path,
        cover_image_path=None,
        added_at=release.added_at,
        datafile_mtime=release.datafile_mtime,
        virtual_dirname="{NEW} BLACKPINK;JISOO - 2222. I Really Love Blackpink - Single [J-Pop;Pop-Rap]",
        title="I Really Love Blackpink",
        releasetype="single",
        year=2222,
        new=True,
        multidisc=False,
        genres=["J-Pop", "Pop-Rap"],
        labels=["YG Entertainment"],
        artists=[
            CachedArtist(name="BLACKPINK", role="main", alias=False),
            CachedArtist(name="JISOO", role="main", alias=False),
        ],
        formatted_artists="BLACKPINK;JISOO",
    )
    assert tracks == [
        CachedTrack(
            id=track_ids[0],
            source_path=release_path / "01.m4a",
            source_mtime=tracks[0].source_mtime,
            virtual_filename="BLACKPINK - I Do Like That.m4a",
            title="I Do Like That",
            release_id=release_id,
            tracknumber="1",
            discnumber="1",
            formatted_release_position="01",
            duration_seconds=2,
            artists=[
                CachedArtist(name="BLACKPINK", role="main", alias=False),
            ],
            formatted_artists="BLACKPINK",
        ),
        CachedTrack(
            id=track_ids[1],
            source_path=release_path / "02.m4a",
            source_mtime=tracks[1].source_mtime,
            virtual_filename="JISOO - All Eyes On Me.m4a",
            title="All Eyes On Me",
            release_id=release_id,
            tracknumber="2",
            discnumber="1",
            formatted_release_position="02",
            duration_seconds=2,
            artists=[
                CachedArtist(name="JISOO", role="main", alias=False),
            ],
            formatted_artists="JISOO",
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

    rdata = get_release(config, release_id)
    assert rdata is not None
    release, tracks = rdata
    assert release == CachedRelease(
        id=release_id,
        source_path=release_path,
        cover_image_path=None,
        added_at=release.added_at,
        datafile_mtime=release.datafile_mtime,
        virtual_dirname="{NEW} BLACKPINK;JISOO - 2222. I Really Love Blackpink - Single [J-Pop;Pop-Rap]",
        title="I Really Love Blackpink",
        releasetype="single",
        year=2222,
        new=True,
        multidisc=False,
        genres=["J-Pop", "Pop-Rap"],
        labels=["YG Entertainment"],
        artists=[
            CachedArtist(name="BLACKPINK", role="main", alias=False),
            CachedArtist(name="JISOO", role="main", alias=False),
        ],
        formatted_artists="BLACKPINK;JISOO",
    )
    assert tracks == [
        CachedTrack(
            id=track_ids[0],
            source_path=release_path / "01.m4a",
            source_mtime=tracks[0].source_mtime,
            virtual_filename="BLACKPINK - I Do Like That.m4a",
            title="I Do Like That",
            release_id=release_id,
            tracknumber="1",
            discnumber="1",
            formatted_release_position="01",
            duration_seconds=2,
            artists=[
                CachedArtist(name="BLACKPINK", role="main", alias=False),
            ],
            formatted_artists="BLACKPINK",
        ),
        CachedTrack(
            id=track_ids[1],
            source_path=release_path / "02.m4a",
            source_mtime=tracks[1].source_mtime,
            virtual_filename="JISOO - All Eyes On Me.m4a",
            title="All Eyes On Me",
            release_id=release_id,
            tracknumber="2",
            discnumber="1",
            formatted_release_position="02",
            duration_seconds=2,
            artists=[
                CachedArtist(name="JISOO", role="main", alias=False),
            ],
            formatted_artists="JISOO",
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
    assert af.album == "Track 2"
    assert af.tracknumber == "1"
    assert af.discnumber == "1"
    assert af.releasetype == "single"
    assert af.albumartists == af.trackartists


@pytest.mark.usefixtures("seeded_cache")
def test_dump_releases(config: Config) -> None:
    assert json.loads(dump_releases(config)) == [
        {
            "added_at": "0000-01-01T00:00:00+00:00",
            "artists": [
                {"name": "Bass Man", "role": "main"},
                {"name": "Techno Man", "role": "main"},
            ],
            "cover_image_path": None,
            "formatted_artists": "Techno Man;Bass Man",
            "genres": ["Deep House", "Techno"],
            "id": "r1",
            "labels": ["Silk Music"],
            "new": False,
            "releasetype": "album",
            "source_path": f"{config.music_source_dir}/r1",
            "title": "Release 1",
            "year": 2023,
        },
        {
            "added_at": "0000-01-01T00:00:00+00:00",
            "artists": [
                {"name": "Conductor Woman", "role": "guest"},
                {"name": "Violin Woman", "role": "main"},
            ],
            "cover_image_path": f"{config.music_source_dir}/r2/cover.jpg",
            "formatted_artists": "Violin Woman feat. Conductor Woman",
            "genres": ["Classical"],
            "id": "r2",
            "labels": ["Native State"],
            "new": False,
            "releasetype": "album",
            "source_path": f"{config.music_source_dir}/r2",
            "title": "Release 2",
            "year": 2021,
        },
        {
            "added_at": "0000-01-01T00:00:00+00:00",
            "artists": [],
            "cover_image_path": None,
            "formatted_artists": "",
            "genres": [],
            "id": "r3",
            "labels": [],
            "new": True,
            "releasetype": "album",
            "source_path": f"{config.music_source_dir}/r3",
            "title": "Release 3",
            "year": 2021,
        },
    ]


def test_resolve_release_ids(config: Config) -> None:
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    update_cache(config)

    with connect(config) as conn:
        cursor = conn.execute("SELECT id, virtual_dirname FROM releases")
        row = cursor.fetchone()
        release_id = row["id"]
        virtual_dirname = row["virtual_dirname"]

    assert resolve_release_ids(config, release_id) == (release_id, virtual_dirname)
    assert resolve_release_ids(config, virtual_dirname) == (release_id, virtual_dirname)
    with pytest.raises(ReleaseDoesNotExistError):
        resolve_release_ids(config, "lalala")
