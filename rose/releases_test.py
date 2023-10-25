import shutil
from pathlib import Path
from typing import Any

import pytest
import tomllib

from conftest import TEST_RELEASE_1
from rose.cache import CachedArtist, CachedRelease, CachedTrack, connect, get_release, update_cache
from rose.config import Config
from rose.releases import (
    ReleaseDoesNotExistError,
    delete_release,
    dump_releases,
    edit_release,
    resolve_release_ids,
    toggle_release_new,
)


def test_dump_releases(config: Config) -> None:
    assert dump_releases(config) == "[]"


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
        cursor = conn.execute("SELECT id, virtual_dirname FROM releases")
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


def test_edit_release(monkeypatch: Any, config: Config, source_dir: Path) -> None:
    release_path = source_dir / TEST_RELEASE_1.name
    with connect(config) as conn:
        cursor = conn.execute("SELECT id FROM releases WHERE source_path = ?", (str(release_path),))
        release_id = cursor.fetchone()["id"]
        cursor = conn.execute(
            "SELECT id FROM tracks WHERE release_id = ? ORDER BY track_number", (str(release_id),)
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
        disc_number = "1"
        track_number = "1"
        title = "I Do Like That"
        artists = [
            {{ name = "BLACKPINK", role = "main" }},
        ]

        [tracks.{track_ids[1]}]
        disc_number = "1"
        track_number = "2"
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
        virtual_dirname="{NEW} BLACKPINK;JISOO - 2222. I Really Love Blackpink - Single [J-Pop;Pop-Rap] {YG Entertainment}",  # noqa: E501
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
            virtual_filename="01. I Do Like That.m4a",
            title="I Do Like That",
            release_id=release_id,
            track_number="1",
            disc_number="1",
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
            virtual_filename="02. All Eyes On Me.m4a",
            title="All Eyes On Me",
            release_id=release_id,
            track_number="2",
            disc_number="1",
            duration_seconds=2,
            artists=[
                CachedArtist(name="JISOO", role="main", alias=False),
            ],
            formatted_artists="JISOO",
        ),
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
