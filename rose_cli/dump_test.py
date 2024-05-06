import json
from typing import Any

import pytest

from rose import Config, MetadataMatcher
from rose_cli.dump import (
    dump_all_collages,
    dump_all_playlists,
    dump_all_releases,
    dump_all_tracks,
    dump_collage,
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
    matcher = MetadataMatcher.parse("releasetitle:2$")
    snapshot.assert_match(json.loads(dump_all_releases(config, matcher)))


@pytest.mark.usefixtures("static_cache")
def test_dump_tracks(config: Config, snapshot: Any) -> None:
    snapshot.assert_match(json.loads(dump_all_tracks(config)))


@pytest.mark.usefixtures("static_cache")
def test_dump_tracks_with_matcher(config: Config, snapshot: Any) -> None:
    matcher = MetadataMatcher.parse("artist:Techno Man")
    snapshot.assert_match(json.loads(dump_all_tracks(config, matcher)))


@pytest.mark.usefixtures("static_cache")
def test_dump_track(config: Config, snapshot: Any) -> None:
    snapshot.assert_match(json.loads(dump_track(config, "t1")))


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
