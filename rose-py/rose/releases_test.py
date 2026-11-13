import re
import shutil
import tomllib
from pathlib import Path

import pytest

from conftest import TEST_RELEASE_1
from rose.audiotags import AudioTags, RoseDate
from rose.cache import (
    Release,
    Track,
    connect,
    get_release,
    get_tracks_of_release,
    update_cache,
)
from rose.common import Artist, ArtistMapping
from rose.config import Config
from rose.releases import (
    MetadataRelease,
    ReleaseEditFailedError,
    create_single_release,
    delete_release,
    delete_release_cover_art,
    edit_release,
    find_releases_matching_rule,
    run_actions_on_release,
    set_release_cover_art,
    toggle_release_favorite,
    toggle_release_new,
)
from rose.rule_parser import Action, Matcher


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


def test_toggle_release_favorite(config: Config) -> None:
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    update_cache(config)
    with connect(config) as conn:
        cursor = conn.execute("SELECT id FROM releases")
        release_id = cursor.fetchone()["id"]
    datafile = config.music_source_dir / TEST_RELEASE_1.name / f".rose.{release_id}.toml"

    # Default should be False
    with datafile.open("rb") as fp:
        data = tomllib.load(fp)
        assert data["favorite"] is False
    with connect(config) as conn:
        cursor = conn.execute("SELECT favorite FROM releases")
        assert not cursor.fetchone()["favorite"]

    # Set favorite.
    toggle_release_favorite(config, release_id)
    with datafile.open("rb") as fp:
        data = tomllib.load(fp)
        assert data["favorite"] is True
    with connect(config) as conn:
        cursor = conn.execute("SELECT favorite FROM releases")
        assert cursor.fetchone()["favorite"]

    # Set not favorite.
    toggle_release_favorite(config, release_id)
    with datafile.open("rb") as fp:
        data = tomllib.load(fp)
        assert data["favorite"] is False
    with connect(config) as conn:
        cursor = conn.execute("SELECT favorite FROM releases")
        assert not cursor.fetchone()["favorite"]


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


def test_edit_release(config: Config, source_dir: Path) -> None:
    release_path = source_dir / TEST_RELEASE_1.name
    with connect(config) as conn:
        cursor = conn.execute("SELECT id FROM releases WHERE source_path = ?", (str(release_path),))
        release_id = cursor.fetchone()["id"]
        cursor = conn.execute("SELECT id FROM tracks WHERE release_id = ? ORDER BY tracknumber", (str(release_id),))
        track_ids = [r["id"] for r in cursor]
        assert len(track_ids) == 2

    new_toml = f"""
        title = "I Really Love Blackpink"
        new = false
        favorite = false
        releasetype = "single"
        releasedate = "2222"
        originaldate = "2000"
        compositiondate = "1800"
        artists = [
            {{ name = "BLACKPINK", role = "main" }},
            {{ name = "JISOO", role = "main" }},
        ]
        catalognumber = "Lalala"
        edition = "Blabla"
        labels = [
            "YG Entertainment",
        ]
        genres = [
            "J-Pop",
            "Pop Rap",
        ]
        secondary_genres = [
            "Twerk",
        ]
        descriptors = [
            "Playful",
            "Cryptic",
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
    edit_release(config, release_id, editor_fn=lambda _: new_toml)
    release = get_release(config, release_id)
    assert release is not None
    assert release == Release(
        id=release_id,
        source_path=release_path,
        cover_image_path=None,
        added_at=release.added_at,
        datafile_mtime=release.datafile_mtime,
        releasetitle="I Really Love Blackpink",
        releasetype="single",
        releasedate=RoseDate(2222),
        originaldate=RoseDate(2000),
        compositiondate=RoseDate(1800),
        catalognumber="Lalala",
        edition="Blabla",
        new=False,
        favorite=False,
        rating=None,
        disctotal=1,
        genres=["J-Pop", "Pop Rap"],
        parent_genres=["Hip Hop", "Pop"],
        secondary_genres=["Twerk"],
        parent_secondary_genres=[
            "Dance",
            "Electronic",
            "Electronic Dance Music",
            "Trap [EDM]",
        ],
        descriptors=["Playful", "Cryptic"],
        labels=["YG Entertainment"],
        releaseartists=ArtistMapping(main=[Artist("BLACKPINK"), Artist("JISOO")]),
        metahash=release.metahash,
    )
    tracks = get_tracks_of_release(config, release)
    assert tracks == [
        Track(
            id=track_ids[0],
            source_path=release_path / "01.m4a",
            source_mtime=tracks[0].source_mtime,
            tracktitle="I Do Like That",
            tracknumber="1",
            tracktotal=2,
            discnumber="1",
            duration_seconds=2,
            trackartists=ArtistMapping(main=[Artist("BLACKPINK")]),
            metahash=tracks[0].metahash,
            release=release,
        ),
        Track(
            id=track_ids[1],
            source_path=release_path / "02.m4a",
            source_mtime=tracks[1].source_mtime,
            tracktitle="All Eyes On Me",
            tracknumber="2",
            tracktotal=2,
            discnumber="1",
            duration_seconds=2,
            trackartists=ArtistMapping(main=[Artist("JISOO")]),
            metahash=tracks[1].metahash,
            release=release,
        ),
    ]


def test_edit_release_reads_audio_tags(config: Config, source_dir: Path) -> None:
    release_path = source_dir / TEST_RELEASE_1.name
    with connect(config) as conn:
        cursor = conn.execute("SELECT id FROM releases WHERE source_path = ?", (str(release_path),))
        release_id = cursor.fetchone()["id"]

    release = get_release(config, release_id)
    assert release is not None
    tracks = get_tracks_of_release(config, release)
    tags = AudioTags.from_file(tracks[0].source_path)
    tags.releasetitle = "Changed on disk"
    tags.tracktitle = "Track changed on disk"
    tags.flush(config)

    metadata = MetadataRelease.from_audiotags(
        release,
        tracks,
        [AudioTags.from_file(track.source_path) for track in tracks],
    )
    assert metadata.title == "Changed on disk"
    assert metadata.tracks[tracks[0].id].title == "Track changed on disk"

    def editfn(toml: str) -> str:
        metadata = MetadataRelease.from_toml(toml)
        assert metadata.title == "Changed on disk"
        assert metadata.tracks[tracks[0].id].title == "Track changed on disk"
        return toml

    edit_release(config, release_id, editor_fn=editfn)


def test_edit_release_failure_and_resume(config: Config, source_dir: Path) -> None:
    release_path = source_dir / TEST_RELEASE_1.name
    with connect(config) as conn:
        cursor = conn.execute("SELECT id FROM releases WHERE source_path = ?", (str(release_path),))
        release_id = cursor.fetchone()["id"]
        cursor = conn.execute("SELECT id FROM tracks WHERE release_id = ? ORDER BY tracknumber", (str(release_id),))
        track_ids = [r["id"] for r in cursor]
        assert len(track_ids) == 2

    # Notice the bullshit releasetype.
    bad_toml = f"""
        title = "I Really Love Blackpink"
        new = false
        favorite = false
        releasetype = "bullshit"
        releasedate = "2222"
        originaldate = ""
        compositiondate = ""
        artists = [
            {{ name = "BLACKPINK", role = "main" }},
            {{ name = "JISOO", role = "main" }},
        ]
        catalognumber = ""
        edition = ""
        labels = [
            "YG Entertainment",
        ]
        genres = [
            "J-Pop",
            "Pop Rap",
        ]
        secondary_genres = []
        descriptors = []

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
    with pytest.raises(ReleaseEditFailedError) as exc:
        edit_release(config, release_id, editor_fn=lambda _: bad_toml)
    errmsg = str(exc.value)
    match = re.search(r"--resume ([^ ]+)", errmsg)
    assert match is not None
    resume_file = Path(match[1])

    correct_toml = f"""
        title = "I Really Love Blackpink"
        new = false
        favorite = false
        releasetype = "single"
        releasedate = "2222"
        originaldate = ""
        compositiondate = ""
        artists = [
            {{ name = "BLACKPINK", role = "main" }},
            {{ name = "JISOO", role = "main" }},
        ]
        catalognumber = ""
        edition = ""
        labels = [
            "YG Entertainment",
        ]
        genres = [
            "J-Pop",
            "Pop Rap",
        ]
        secondary_genres = []
        descriptors = []

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

    def editfn(text: str) -> str:
        assert text == bad_toml
        return correct_toml

    edit_release(config, release_id, resume_file=resume_file, editor_fn=editfn)

    # Assert the file got deleted.
    assert not resume_file.exists()

    release = get_release(config, release_id)
    assert release is not None
    assert release == Release(
        id=release_id,
        source_path=release_path,
        cover_image_path=None,
        added_at=release.added_at,
        datafile_mtime=release.datafile_mtime,
        releasetitle="I Really Love Blackpink",
        releasetype="single",
        releasedate=RoseDate(2222),
        originaldate=None,
        compositiondate=None,
        catalognumber=None,
        edition=None,
        new=False,
        favorite=False,
        rating=None,
        disctotal=1,
        genres=["J-Pop", "Pop Rap"],
        parent_genres=["Hip Hop", "Pop"],
        labels=["YG Entertainment"],
        secondary_genres=[],
        parent_secondary_genres=[],
        descriptors=[],
        releaseartists=ArtistMapping(main=[Artist("BLACKPINK"), Artist("JISOO")]),
        metahash=release.metahash,
    )
    tracks = get_tracks_of_release(config, release)
    assert tracks == [
        Track(
            id=track_ids[0],
            source_path=release_path / "01.m4a",
            source_mtime=tracks[0].source_mtime,
            tracktitle="I Do Like That",
            tracknumber="1",
            tracktotal=2,
            discnumber="1",
            duration_seconds=2,
            trackartists=ArtistMapping(main=[Artist("BLACKPINK")]),
            metahash=tracks[0].metahash,
            release=release,
        ),
        Track(
            id=track_ids[1],
            source_path=release_path / "02.m4a",
            source_mtime=tracks[1].source_mtime,
            tracktitle="All Eyes On Me",
            tracknumber="2",
            tracktotal=2,
            discnumber="1",
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
    assert af.releasetitle == "Track 2"
    assert af.tracknumber == "1"
    assert af.discnumber == "1"
    assert af.releasetype == "single"
    assert af.releaseartists == af.trackartists


def test_extract_single_release_with_trailing_space(config: Config) -> None:
    release_dir = config.music_source_dir / TEST_RELEASE_1.name
    shutil.copytree(TEST_RELEASE_1, release_dir)
    af = AudioTags.from_file(release_dir / "02.m4a")
    af.tracktitle = "Trailing Space "
    af.flush(config)
    update_cache(config)
    create_single_release(config, release_dir / "02.m4a")
    # Assert that we've successfully written/copied our files.
    source_path = config.music_source_dir / "BLACKPINK - 1990. Trailing Space"
    assert source_path.is_dir()
    assert (source_path / "01. Trailing Space.m4a").is_file()


def test_extract_single_release_update_references(config: Config) -> None:
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    update_cache(config)
    # Get the track ID of the track we're about to extract.
    track_path = config.music_source_dir / TEST_RELEASE_1.name / "02.m4a"
    af = AudioTags.from_file(track_path)
    old_track_id = af.id
    assert old_track_id is not None
    # Create a playlist containing that track.
    playlists_dir = config.music_source_dir / "!playlists"
    playlists_dir.mkdir(parents=True, exist_ok=True)
    playlist_toml = playlists_dir / "Test Playlist.toml"
    import tomli_w

    with playlist_toml.open("wb") as fp:
        tomli_w.dump({"tracks": [{"uuid": old_track_id, "description_meta": "test track"}]}, fp)
    update_cache(config)
    # Create single with update_references=True.
    create_single_release(config, track_path, update_references=True)
    # Read the playlist TOML and assert the track reference was updated.
    with playlist_toml.open("rb") as fp:
        data = tomllib.load(fp)
    assert len(data["tracks"]) == 1
    new_track_id = data["tracks"][0]["uuid"]
    assert new_track_id != old_track_id
    # Verify the new track ID corresponds to the newly created single's track.
    source_path = config.music_source_dir / "BLACKPINK - 1990. Track 2"
    new_af = AudioTags.from_file(source_path / "01. Track 2.m4a")
    assert new_af.id == new_track_id


def test_extract_single_release_update_references_no_match(config: Config) -> None:
    shutil.copytree(TEST_RELEASE_1, config.music_source_dir / TEST_RELEASE_1.name)
    update_cache(config)
    track_path = config.music_source_dir / TEST_RELEASE_1.name / "02.m4a"
    # Create a playlist with a different track UUID.
    playlists_dir = config.music_source_dir / "!playlists"
    playlists_dir.mkdir(parents=True, exist_ok=True)
    playlist_toml = playlists_dir / "Test Playlist.toml"
    import tomli_w

    with playlist_toml.open("wb") as fp:
        tomli_w.dump({"tracks": [{"uuid": "some-other-uuid", "description_meta": "other track"}]}, fp)
    update_cache(config)
    # Create single with update_references=True. Playlist should be unchanged.
    create_single_release(config, track_path, update_references=True)
    with playlist_toml.open("rb") as fp:
        data = tomllib.load(fp)
    assert len(data["tracks"]) == 1
    assert data["tracks"][0]["uuid"] == "some-other-uuid"


def test_run_action_on_release(config: Config, source_dir: Path) -> None:
    action = Action.parse("tracktitle/replace:Bop")
    run_actions_on_release(config, "ilovecarly", [action])
    af = AudioTags.from_file(source_dir / "Test Release 2" / "01.m4a")
    assert af.tracktitle == "Bop"


@pytest.mark.usefixtures("seeded_cache")
def test_find_matching_releases(config: Config) -> None:
    results = find_releases_matching_rule(config, Matcher.parse("releasetitle:Release 2"))
    assert {r.id for r in results} == {"r2"}
    results = find_releases_matching_rule(config, Matcher.parse("artist:^Techno Man$"))
    assert {r.id for r in results} == {"r1"}
    results = find_releases_matching_rule(config, Matcher.parse("artist:Techno Man"))
    assert {r.id for r in results} == {"r1"}
    results = find_releases_matching_rule(config, Matcher.parse("genre:^Deep House$"))
    assert {r.id for r in results} == {"r1"}
    results = find_releases_matching_rule(config, Matcher.parse("genre:Deep House"))
    assert {r.id for r in results} == {"r1"}
    results = find_releases_matching_rule(config, Matcher.parse("descriptor:^Wet$"))
    assert {r.id for r in results} == {"r2"}
    results = find_releases_matching_rule(config, Matcher.parse("descriptor:Wet"))
    assert {r.id for r in results} == {"r2"}
    results = find_releases_matching_rule(config, Matcher.parse("label:^Native State$"))
    assert {r.id for r in results} == {"r2"}
    results = find_releases_matching_rule(config, Matcher.parse("label:Native State"))
    assert {r.id for r in results} == {"r2"}
