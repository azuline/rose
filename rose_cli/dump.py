import json
from typing import Any

from rose import (
    CollageDoesNotExistError,
    Config,
    MetadataMatcher,
    PlaylistDoesNotExistError,
    Release,
    ReleaseDoesNotExistError,
    Track,
    TrackDoesNotExistError,
    find_releases_matching_rule,
    find_tracks_matching_rule,
    get_collage,
    get_collage_releases,
    get_playlist,
    get_playlist_tracks,
    get_release,
    get_track,
    get_tracks_of_release,
    get_tracks_of_releases,
    list_collages,
    list_playlists,
    list_releases,
    list_tracks,
)


def release_to_json(r: Release) -> dict[str, Any]:
    return {
        "id": r.id,
        "source_path": str(r.source_path.resolve()),
        "cover_image_path": str(r.cover_image_path.resolve()) if r.cover_image_path else None,
        "added_at": r.added_at,
        "releasetitle": r.releasetitle,
        "releasetype": r.releasetype,
        "releasedate": str(r.releasedate) if r.releasedate else None,
        "originaldate": str(r.originaldate) if r.originaldate else None,
        "compositiondate": str(r.compositiondate) if r.compositiondate else None,
        "catalognumber": r.catalognumber,
        "edition": r.edition,
        "new": r.new,
        "disctotal": r.disctotal,
        "genres": r.genres,
        "parent_genres": r.parent_genres,
        "secondary_genres": r.secondary_genres,
        "parent_secondary_genres": r.parent_secondary_genres,
        "descriptors": r.descriptors,
        "labels": r.labels,
        "releaseartists": r.releaseartists.dump(),
    }


def track_to_json(t: Track, with_release_info: bool = True) -> dict[str, Any]:
    r = {
        "id": t.id,
        "source_path": str(t.source_path.resolve()),
        "tracktitle": t.tracktitle,
        "tracknumber": t.tracknumber,
        "tracktotal": t.tracktotal,
        "discnumber": t.discnumber,
        "duration_seconds": t.duration_seconds,
        "trackartists": t.trackartists.dump(),
    }
    if with_release_info:
        r.update(
            {
                "release_id": t.release.id,
                "added_at": t.release.added_at,
                "releasetitle": t.release.releasetitle,
                "releasetype": t.release.releasetype,
                "disctotal": t.release.disctotal,
                "releasedate": str(t.release.releasedate) if t.release.releasedate else None,
                "originaldate": str(t.release.originaldate) if t.release.originaldate else None,
                "compositiondate": str(t.release.compositiondate)
                if t.release.compositiondate
                else None,
                "catalognumber": t.release.catalognumber,
                "edition": t.release.edition,
                "new": t.release.new,
                "genres": t.release.genres,
                "parent_genres": t.release.parent_genres,
                "secondary_genres": t.release.secondary_genres,
                "parent_secondary_genres": t.release.parent_secondary_genres,
                "descriptors": t.release.descriptors,
                "labels": t.release.labels,
                "releaseartists": t.release.releaseartists.dump(),
            }
        )
    return r


def dump_release(c: Config, release_id: str) -> str:
    release = get_release(c, release_id)
    if not release:
        raise ReleaseDoesNotExistError(f"Release {release_id} does not exist")
    tracks = get_tracks_of_release(c, release)
    return json.dumps(
        {
            **release_to_json(release),
            "tracks": [track_to_json(t, with_release_info=False) for t in tracks],
        }
    )


def dump_all_releases(c: Config, matcher: MetadataMatcher | None = None) -> str:
    releases = find_releases_matching_rule(c, matcher) if matcher else list_releases(c)
    return json.dumps(
        [
            {
                **release_to_json(release),
                "tracks": [track_to_json(t, with_release_info=False) for t in tracks],
            }
            for release, tracks in get_tracks_of_releases(c, releases)
        ]
    )


def dump_track(c: Config, track_id: str) -> str:
    track = get_track(c, track_id)
    if track is None:
        raise TrackDoesNotExistError(f"Track {track_id} does not exist")
    return json.dumps(track_to_json(track))


def dump_all_tracks(c: Config, matcher: MetadataMatcher | None = None) -> str:
    tracks = find_tracks_matching_rule(c, matcher) if matcher else list_tracks(c)
    return json.dumps([track_to_json(t) for t in tracks])


def dump_collage(c: Config, collage_name: str) -> str:
    collage = get_collage(c, collage_name)
    if collage is None:
        raise CollageDoesNotExistError(f"Collage {collage_name} does not exist")
    collage_releases = get_collage_releases(c, collage_name)
    releases: list[dict[str, Any]] = []
    for idx, rls in enumerate(collage_releases):
        releases.append({"position": idx + 1, **release_to_json(rls)})
    return json.dumps({"name": collage_name, "releases": releases})


def dump_all_collages(c: Config) -> str:
    out: list[dict[str, Any]] = []
    for name in list_collages(c):
        collage = get_collage(c, name)
        assert collage is not None
        collage_releases = get_collage_releases(c, name)
        releases: list[dict[str, Any]] = []
        for idx, rls in enumerate(collage_releases):
            releases.append({"position": idx + 1, **release_to_json(rls)})
        out.append({"name": name, "releases": releases})
    return json.dumps(out)


def dump_playlist(c: Config, playlist_name: str) -> str:
    playlist = get_playlist(c, playlist_name)
    if playlist is None:
        raise PlaylistDoesNotExistError(f"Playlist {playlist_name} does not exist")
    playlist_tracks = get_playlist_tracks(c, playlist_name)
    tracks: list[dict[str, Any]] = []
    for idx, trk in enumerate(playlist_tracks):
        tracks.append({"position": idx + 1, **track_to_json(trk)})
    return json.dumps(
        {
            "name": playlist_name,
            "cover_image_path": str(playlist.cover_path) if playlist.cover_path else None,
            "tracks": tracks,
        }
    )


def dump_all_playlists(c: Config) -> str:
    out: list[dict[str, Any]] = []
    for name in list_playlists(c):
        playlist = get_playlist(c, name)
        assert playlist is not None
        playlist_tracks = get_playlist_tracks(c, name)
        tracks: list[dict[str, Any]] = []
        for idx, trk in enumerate(playlist_tracks):
            tracks.append({"position": idx + 1, **track_to_json(trk)})
        out.append(
            {
                "name": name,
                "cover_image_path": str(playlist.cover_path) if playlist.cover_path else None,
                "tracks": tracks,
            }
        )
    return json.dumps(out)
