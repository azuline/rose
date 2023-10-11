from pathlib import Path

import pytest

from rose.cache.dataclasses import CachedArtist, CachedRelease, CachedTrack
from rose.cache.read import (
    artist_exists,
    cover_exists,
    genre_exists,
    get_release_files,
    label_exists,
    list_artists,
    list_genres,
    list_labels,
    list_releases,
    release_exists,
    track_exists,
)
from rose.foundation.conf import Config


@pytest.mark.usefixtures("seeded_cache")
def test_list_releases(config: Config) -> None:
    albums = list(list_releases(config))
    assert albums == [
        CachedRelease(
            id="r1",
            source_path=Path(config.music_source_dir / "r1"),
            cover_image_path=None,
            virtual_dirname="r1",
            title="Release 1",
            release_type="album",
            release_year=2023,
            new=True,
            genres=["Deep House", "Techno"],
            labels=["Silk Music"],
            artists=[
                CachedArtist(name="Techno Man", role="main"),
                CachedArtist(name="Bass Man", role="main"),
            ],
        ),
        CachedRelease(
            id="r2",
            source_path=Path(config.music_source_dir / "r2"),
            cover_image_path=Path(config.music_source_dir / "r2" / "cover.jpg"),
            virtual_dirname="r2",
            title="Release 2",
            release_type="album",
            release_year=2021,
            new=False,
            genres=["Classical"],
            labels=["Native State"],
            artists=[
                CachedArtist(name="Violin Woman", role="main"),
                CachedArtist(name="Conductor Woman", role="guest"),
            ],
        ),
    ]

    assert list(list_releases(config, sanitized_artist_filter="Techno Man")) == [
        CachedRelease(
            id="r1",
            source_path=Path(config.music_source_dir / "r1"),
            cover_image_path=None,
            virtual_dirname="r1",
            title="Release 1",
            release_type="album",
            release_year=2023,
            new=True,
            genres=["Deep House", "Techno"],
            labels=["Silk Music"],
            artists=[
                CachedArtist(name="Techno Man", role="main"),
                CachedArtist(name="Bass Man", role="main"),
            ],
        ),
    ]

    assert list(list_releases(config, sanitized_genre_filter="Techno")) == [
        CachedRelease(
            id="r1",
            source_path=Path(config.music_source_dir / "r1"),
            cover_image_path=None,
            virtual_dirname="r1",
            title="Release 1",
            release_type="album",
            release_year=2023,
            new=True,
            genres=["Deep House", "Techno"],
            labels=["Silk Music"],
            artists=[
                CachedArtist(name="Techno Man", role="main"),
                CachedArtist(name="Bass Man", role="main"),
            ],
        ),
    ]

    assert list(list_releases(config, sanitized_label_filter="Silk Music")) == [
        CachedRelease(
            id="r1",
            source_path=Path(config.music_source_dir / "r1"),
            cover_image_path=None,
            virtual_dirname="r1",
            title="Release 1",
            release_type="album",
            release_year=2023,
            new=True,
            genres=["Deep House", "Techno"],
            labels=["Silk Music"],
            artists=[
                CachedArtist(name="Techno Man", role="main"),
                CachedArtist(name="Bass Man", role="main"),
            ],
        ),
    ]


@pytest.mark.usefixtures("seeded_cache")
def test_get_release_files(config: Config) -> None:
    rf = get_release_files(config, "r1")
    assert rf.tracks == [
        CachedTrack(
            id="t1",
            source_path=Path(config.music_source_dir / "r1" / "01.m4a"),
            virtual_filename="01.m4a",
            title="Track 1",
            release_id="r1",
            track_number="01",
            disc_number="01",
            duration_seconds=120,
            artists=[
                CachedArtist(name="Techno Man", role="main"),
                CachedArtist(name="Bass Man", role="main"),
            ],
        ),
        CachedTrack(
            id="t2",
            source_path=Path(config.music_source_dir / "r1" / "02.m4a"),
            virtual_filename="02.m4a",
            title="Track 2",
            release_id="r1",
            track_number="02",
            disc_number="01",
            duration_seconds=240,
            artists=[
                CachedArtist(name="Techno Man", role="main"),
                CachedArtist(name="Bass Man", role="main"),
            ],
        ),
    ]
    assert rf.cover is None

    rf = get_release_files(config, "r2")
    assert rf.cover == config.music_source_dir / "r2" / "cover.jpg"


@pytest.mark.usefixtures("seeded_cache")
def test_list_artists(config: Config) -> None:
    artists = list(list_artists(config))
    assert set(artists) == {"Techno Man", "Bass Man", "Violin Woman", "Conductor Woman"}


@pytest.mark.usefixtures("seeded_cache")
def test_list_genres(config: Config) -> None:
    genres = list(list_genres(config))
    assert set(genres) == {"Techno", "Deep House", "Classical"}


@pytest.mark.usefixtures("seeded_cache")
def test_list_labels(config: Config) -> None:
    labels = list(list_labels(config))
    assert set(labels) == {"Silk Music", "Native State"}


@pytest.mark.usefixtures("seeded_cache")
def test_release_exists(config: Config) -> None:
    assert release_exists(config, "r1")
    assert not release_exists(config, "lalala")


@pytest.mark.usefixtures("seeded_cache")
def test_track_exists(config: Config) -> None:
    assert track_exists(config, "r1", "01.m4a")
    assert not track_exists(config, "lalala", "lalala")
    assert not track_exists(config, "r1", "lalala")


@pytest.mark.usefixtures("seeded_cache")
def test_cover_exists(config: Config) -> None:
    assert cover_exists(config, "r2", "cover.jpg")
    assert not cover_exists(config, "r2", "cover.png")
    assert not cover_exists(config, "r1", "cover.jpg")


@pytest.mark.usefixtures("seeded_cache")
def test_artist_exists(config: Config) -> None:
    assert artist_exists(config, "Bass Man")
    assert not artist_exists(config, "lalala")


@pytest.mark.usefixtures("seeded_cache")
def test_genre_exists(config: Config) -> None:
    assert genre_exists(config, "Deep House")
    assert not genre_exists(config, "lalala")


@pytest.mark.usefixtures("seeded_cache")
def test_label_exists(config: Config) -> None:
    assert label_exists(config, "Silk Music")
    assert not label_exists(config, "Cotton Music")
