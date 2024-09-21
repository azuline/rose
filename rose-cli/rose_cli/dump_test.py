import json
from typing import Any

import pytest
from rose import Config, Matcher

from rose_cli.dump import (
    dump_all_artists,
    dump_all_collages,
    dump_all_descriptors,
    dump_all_genres,
    dump_all_labels,
    dump_all_playlists,
    dump_all_releases,
    dump_all_tracks,
    dump_artist,
    dump_collage,
    dump_descriptor,
    dump_genre,
    dump_label,
    dump_playlist,
    dump_release,
    dump_track,
)


@pytest.mark.usefixtures("static_cache")
def test_dump_release(config: Config, snapshot: Any) -> None:
    snapshot.assert_match(json.loads(dump_release(config, "r1")))


@pytest.mark.usefixtures("static_cache")
def test_dump_releases(config: Config, snapshot: Any) -> None:
    snapshot.assert_match(json.loads(dump_all_releases(config)))


@pytest.mark.usefixtures("static_cache")
def test_dump_releases_matcher(config: Config, snapshot: Any) -> None:
    matcher = Matcher.parse("releasetitle:2$")
    snapshot.assert_match(json.loads(dump_all_releases(config, matcher)))


@pytest.mark.usefixtures("static_cache")
def test_dump_tracks(config: Config, snapshot: Any) -> None:
    snapshot.assert_match(json.loads(dump_all_tracks(config)))


@pytest.mark.usefixtures("static_cache")
def test_dump_tracks_with_matcher(config: Config, snapshot: Any) -> None:
    matcher = Matcher.parse("artist:Techno Man")
    snapshot.assert_match(json.loads(dump_all_tracks(config, matcher)))


@pytest.mark.usefixtures("static_cache")
def test_dump_track(config: Config, snapshot: Any) -> None:
    snapshot.assert_match(json.loads(dump_track(config, "t1")))


@pytest.mark.usefixtures("static_cache")
def test_dump_artist(config: Config, snapshot: Any) -> None:
    snapshot.assert_match(json.loads(dump_artist(config, "Bass Man")))


@pytest.mark.usefixtures("static_cache")
def test_dump_artists(config: Config, snapshot: Any) -> None:
    snapshot.assert_match(json.loads(dump_all_artists(config)))


@pytest.mark.usefixtures("static_cache")
def test_dump_genre(config: Config, snapshot: Any) -> None:
    snapshot.assert_match(json.loads(dump_genre(config, "Deep House")))


@pytest.mark.usefixtures("static_cache")
def test_dump_genres(config: Config, snapshot: Any) -> None:
    snapshot.assert_match(json.loads(dump_all_genres(config)))


@pytest.mark.usefixtures("static_cache")
def test_dump_label(config: Config, snapshot: Any) -> None:
    snapshot.assert_match(json.loads(dump_label(config, "Silk Music")))


@pytest.mark.usefixtures("static_cache")
def test_dump_labels(config: Config, snapshot: Any) -> None:
    snapshot.assert_match(json.loads(dump_all_labels(config)))


@pytest.mark.usefixtures("static_cache")
def test_dump_descriptor(config: Config, snapshot: Any) -> None:
    snapshot.assert_match(json.loads(dump_descriptor(config, "Warm")))


@pytest.mark.usefixtures("static_cache")
def test_dump_descriptors(config: Config, snapshot: Any) -> None:
    snapshot.assert_match(json.loads(dump_all_descriptors(config)))


@pytest.mark.usefixtures("static_cache")
def test_dump_collage(config: Config, snapshot: Any) -> None:
    snapshot.assert_match(json.loads(dump_collage(config, "Rose Gold")))


@pytest.mark.usefixtures("static_cache")
def test_dump_collages(config: Config, snapshot: Any) -> None:
    snapshot.assert_match(json.loads(dump_all_collages(config)))


@pytest.mark.usefixtures("static_cache")
def test_dump_playlist(config: Config, snapshot: Any) -> None:
    snapshot.assert_match(json.loads(dump_playlist(config, "Lala Lisa")))


@pytest.mark.usefixtures("static_cache")
def test_dump_playlists(config: Config, snapshot: Any) -> None:
    snapshot.assert_match(json.loads(dump_all_playlists(config)))
