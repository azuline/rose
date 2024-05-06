import json
from typing import Any

from rose import (
    CollageDoesNotExistError,
    Config,
    DescriptorDoesNotExistError,
    GenreDoesNotExistError,
    LabelDoesNotExistError,
    MetadataMatcher,
    PlaylistDoesNotExistError,
    Release,
    ReleaseDoesNotExistError,
    Track,
    TrackDoesNotExistError,
    descriptor_exists,
    find_releases_matching_rule,
    find_tracks_matching_rule,
    genre_exists,
    get_collage,
    get_collage_releases,
    get_playlist,
    get_playlist_tracks,
    get_release,
    get_track,
    get_tracks_of_release,
    get_tracks_of_releases,
    label_exists,
    list_collages,
    list_descriptors,
    list_genres,
    list_labels,
    list_playlists,
    list_releases,
    list_tracks,
)
from rose.cache import artist_exists, list_artists, list_releases_delete_this
from rose.common import ArtistDoesNotExistError


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


def dump_artist(c: Config, artist_name: str) -> str:
    if not artist_exists(c, artist_name):
        raise ArtistDoesNotExistError(f"artist {artist_name} does not exist")
    artist_releases = list_releases_delete_this(c, artist_filter=artist_name)
    roles = _partition_releases_by_role(artist_name, artist_releases)
    roles_json = {k: [release_to_json(x) for x in v] for k, v in roles.items()}
    return json.dumps({"name": artist_name, "roles": roles_json})


def dump_all_artists(c: Config) -> str:
    out: list[dict[str, Any]] = []
    for name in list_artists(c):
        artist_releases = list_releases_delete_this(c, artist_filter=name)
        roles = _partition_releases_by_role(name, artist_releases)
        roles_json = {k: [release_to_json(x) for x in v] for k, v in roles.items()}
        out.append({"name": name, "roles": roles_json})
    return json.dumps(out)


def _partition_releases_by_role(artist: str, releases: list[Release]) -> dict[str, list[Release]]:
    rval: dict[str, list[Release]] = {
        "main": [],
        "guest": [],
        "remixer": [],
        "producer": [],
        "composer": [],
        "conductor": [],
        "djmixer": [],
    }
    for release in releases:
        # It is possible for a release to end up in multiple roles. That's intentional.
        for role, names in release.releaseartists.items():
            if any(artist == x.name for x in names):
                rval[role].append(release)
                break
    return rval


def dump_genre(c: Config, genre_name: str) -> str:
    if not genre_exists(c, genre_name):
        raise GenreDoesNotExistError(f"Genre {genre_name} does not exist")
    genre_releases = list_releases_delete_this(c, genre_filter=genre_name)
    releases = [release_to_json(r) for r in genre_releases]
    return json.dumps({"name": genre_name, "releases": releases})


def dump_all_genres(c: Config) -> str:
    out: list[dict[str, Any]] = []
    for name in list_genres(c):
        genre_releases = list_releases_delete_this(c, genre_filter=name.genre)
        releases = [release_to_json(r) for r in genre_releases]
        out.append({"name": name, "releases": releases})
    return json.dumps(out)


def dump_label(c: Config, label_name: str) -> str:
    if not label_exists(c, label_name):
        raise LabelDoesNotExistError(f"label {label_name} does not exist")
    label_releases = list_releases_delete_this(c, label_filter=label_name)
    releases = [release_to_json(r) for r in label_releases]
    return json.dumps({"name": label_name, "releases": releases})


def dump_all_labels(c: Config) -> str:
    out: list[dict[str, Any]] = []
    for name in list_labels(c):
        label_releases = list_releases_delete_this(c, label_filter=name.label)
        releases = [release_to_json(r) for r in label_releases]
        out.append({"name": name, "releases": releases})
    return json.dumps(out)


def dump_descriptor(c: Config, descriptor_name: str) -> str:
    if not descriptor_exists(c, descriptor_name):
        raise DescriptorDoesNotExistError(f"descriptor {descriptor_name} does not exist")
    descriptor_releases = list_releases_delete_this(c, descriptor_filter=descriptor_name)
    releases = [release_to_json(r) for r in descriptor_releases]
    return json.dumps({"name": descriptor_name, "releases": releases})


def dump_all_descriptors(c: Config) -> str:
    out: list[dict[str, Any]] = []
    for name in list_descriptors(c):
        descriptor_releases = list_releases_delete_this(c, descriptor_filter=name.descriptor)
        releases = [release_to_json(r) for r in descriptor_releases]
        out.append({"name": name, "releases": releases})
    return json.dumps(out)


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
