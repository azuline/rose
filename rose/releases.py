"""
The releases module provides functions for interacting with releases.
"""

from __future__ import annotations

import json
import logging
import re
import shlex
import shutil
from dataclasses import asdict, dataclass
from pathlib import Path

import click
import tomli_w
import tomllib
from send2trash import send2trash

from rose.audiotags import AudioTags
from rose.cache import (
    STORED_DATA_FILE_REGEX,
    CachedRelease,
    CachedTrack,
    calculate_release_logtext,
    get_release,
    get_tracks_associated_with_release,
    get_tracks_associated_with_releases,
    list_releases,
    lock,
    release_lock_name,
    update_cache_evict_nonexistent_releases,
    update_cache_for_collages,
    update_cache_for_playlists,
    update_cache_for_releases,
)
from rose.common import Artist, ArtistMapping, RoseError, RoseExpectedError
from rose.config import Config
from rose.rule_parser import MetadataAction, MetadataMatcher
from rose.rules import (
    execute_metadata_actions,
    fast_search_for_matching_releases,
    filter_release_false_positives_using_read_cache,
)
from rose.templates import artistsfmt

logger = logging.getLogger(__name__)


class InvalidCoverArtFileError(RoseExpectedError):
    pass


class ReleaseDoesNotExistError(RoseExpectedError):
    pass


class ReleaseEditFailedError(RoseExpectedError):
    pass


class InvalidReleaseEditResumeFileError(RoseExpectedError):
    pass


class UnknownArtistRoleError(RoseExpectedError):
    pass


def dump_release(c: Config, release_id: str) -> str:
    release = get_release(c, release_id)
    if not release:
        raise ReleaseDoesNotExistError(f"Release {release_id} does not exist")
    tracks = get_tracks_associated_with_release(c, release)
    return json.dumps(
        {**release.dump(), "tracks": [t.dump(with_release_info=False) for t in tracks]}
    )


def dump_all_releases(c: Config, matcher: MetadataMatcher | None = None) -> str:
    release_ids = None
    if matcher:
        release_ids = [x.id for x in fast_search_for_matching_releases(c, matcher)]
    releases = list_releases(c, release_ids)
    if matcher:
        releases = filter_release_false_positives_using_read_cache(matcher, releases)
    rt_pairs = get_tracks_associated_with_releases(c, releases)
    return json.dumps(
        [
            {**release.dump(), "tracks": [t.dump(with_release_info=False) for t in tracks]}
            for release, tracks in rt_pairs
        ]
    )


def delete_release(c: Config, release_id: str) -> None:
    release = get_release(c, release_id)
    if not release:
        raise ReleaseDoesNotExistError(f"Release {release_id} does not exist")
    with lock(c, release_lock_name(release_id)):
        send2trash(release.source_path)
    release_logtext = calculate_release_logtext(
        title=release.releasetitle,
        releasedate=release.releasedate,
        artists=release.releaseartists,
    )
    logger.info(f"Trashed release {release_logtext}")
    update_cache_evict_nonexistent_releases(c)
    # Update all collages and playlists so that the release is removed from whichever it was in.
    update_cache_for_collages(c, None, force=True)
    update_cache_for_playlists(c, None, force=True)


def toggle_release_new(c: Config, release_id: str) -> None:
    release = get_release(c, release_id)
    if not release:
        raise ReleaseDoesNotExistError(f"Release {release_id} does not exist")

    release_logtext = calculate_release_logtext(
        title=release.releasetitle,
        releasedate=release.releasedate,
        artists=release.releaseartists,
    )

    for f in release.source_path.iterdir():
        if not STORED_DATA_FILE_REGEX.match(f.name):
            continue
        with lock(c, release_lock_name(release_id)):
            with f.open("rb") as fp:
                data = tomllib.load(fp)
            data["new"] = not data["new"]
            with f.open("wb") as fp:
                tomli_w.dump(data, fp)
        logger.info(f'Toggled "new"-ness of release {release_logtext} to {data["new"]}')
        update_cache_for_releases(c, [release.source_path], force=True)
        return

    logger.critical(f"Failed to find .rose.toml in {release.source_path}")


def set_release_cover_art(
    c: Config,
    release_id: str,
    new_cover_art_path: Path,
) -> None:
    """
    This function removes all potential cover arts in the release source directory and copies the
    file located at the passed in path to `cover.{ext}` in the release source directory.
    """
    suffix = new_cover_art_path.suffix.lower()
    if suffix[1:] not in c.valid_art_exts:
        raise InvalidCoverArtFileError(
            f"File {new_cover_art_path.name}'s extension is not supported for cover images: "
            "To change this, please read the configuration documentation"
        )

    release = get_release(c, release_id)
    if not release:
        raise ReleaseDoesNotExistError(f"Release {release_id} does not exist")

    release_logtext = calculate_release_logtext(
        title=release.releasetitle,
        releasedate=release.releasedate,
        artists=release.releaseartists,
    )

    for f in release.source_path.iterdir():
        if f.name.lower() in c.valid_cover_arts:
            logger.debug(f"Deleting existing cover art {f.name} in {release_logtext}")
            send2trash(f)
    shutil.copyfile(new_cover_art_path, release.source_path / f"cover{new_cover_art_path.suffix}")
    logger.info(f"Set the cover of release {release_logtext} to {new_cover_art_path.name}")
    update_cache_for_releases(c, [release.source_path])


def delete_release_cover_art(c: Config, release_id: str) -> None:
    """This function deletes all potential cover arts in the release source directory."""
    release = get_release(c, release_id)
    if not release:
        raise ReleaseDoesNotExistError(f"Release {release_id} does not exist")

    release_logtext = calculate_release_logtext(
        title=release.releasetitle,
        releasedate=release.releasedate,
        artists=release.releaseartists,
    )

    found = False
    for f in release.source_path.iterdir():
        if f.name.lower() in c.valid_cover_arts:
            logger.debug(f"Deleting existing cover art {f.name} in {release_logtext}")
            send2trash(f)
            found = True
    if found:
        logger.info(f"Deleted cover arts of release {release_logtext}")
    else:
        logger.info(f"No-Op: No cover arts found for release {release_logtext}")
    update_cache_for_releases(c, [release.source_path])


@dataclass
class MetadataArtist:
    name: str
    role: str

    @staticmethod
    def from_mapping(mapping: ArtistMapping) -> list[MetadataArtist]:
        return [
            MetadataArtist(name=art.name, role=role)
            for role, artists in mapping.items()
            for art in artists
            if not art.alias
        ]

    @staticmethod
    def to_mapping(artists: list[MetadataArtist]) -> ArtistMapping:
        m = ArtistMapping()
        for a in artists:
            try:
                getattr(m, a.role.lower()).append(Artist(name=a.name))
            except AttributeError as e:
                raise UnknownArtistRoleError(
                    f"Failed to write tags: Unknown role for artist {a.name}: {a.role}"
                ) from e
        return m


@dataclass
class MetadataTrack:
    discnumber: str
    tracknumber: str
    title: str
    artists: list[MetadataArtist]


@dataclass
class MetadataRelease:
    title: str
    new: bool
    releasetype: str
    releasedate: int | None
    originaldate: int | None
    compositiondate: int | None
    artists: list[MetadataArtist]
    labels: list[str]
    edition: str | None
    catalognumber: str | None
    genres: list[str]
    secondary_genres: list[str]
    descriptors: list[str]
    tracks: dict[str, MetadataTrack]

    @classmethod
    def from_cache(cls, release: CachedRelease, tracks: list[CachedTrack]) -> MetadataRelease:
        return MetadataRelease(
            title=release.releasetitle,
            new=release.new,
            releasetype=release.releasetype,
            releasedate=release.releasedate,
            originaldate=release.originaldate,
            compositiondate=release.compositiondate,
            edition=release.catalognumber,
            catalognumber=release.edition,
            labels=release.labels,
            genres=release.genres,
            secondary_genres=release.secondary_genres,
            descriptors=release.descriptors,
            artists=MetadataArtist.from_mapping(release.releaseartists),
            tracks={
                t.id: MetadataTrack(
                    discnumber=t.discnumber,
                    tracknumber=t.tracknumber,
                    title=t.tracktitle,
                    artists=MetadataArtist.from_mapping(t.trackartists),
                )
                for t in tracks
            },
        )

    def serialize(self) -> str:
        # LOL TOML DOESN'T HAVE A NULL TYPE. Use -9999 as sentinel. If your music is legitimately
        # released in -9999, you should probably lay off the shrooms.
        data = asdict(self)
        data["releasedate"] = self.releasedate or -9999
        data["originaldate"] = self.originaldate or -9999
        data["compositiondate"] = self.compositiondate or -9999
        data["edition"] = self.edition or -9999
        data["catalognumber"] = self.catalognumber or ""
        return tomli_w.dumps(data)

    @classmethod
    def from_toml(cls, toml: str) -> MetadataRelease:
        d = tomllib.loads(toml)
        return MetadataRelease(
            title=d["title"],
            new=d["new"],
            releasetype=d["releasetype"],
            originaldate=d["originaldate"] if d["originaldate"] != -9999 else None,
            releasedate=d["releasedate"] if d["releasedate"] != -9999 else None,
            compositiondate=d["compositiondate"] if d["compositiondate"] != -9999 else None,
            genres=d["genres"],
            secondary_genres=d["secondary_genres"],
            descriptors=d["descriptors"],
            labels=d["labels"],
            catalognumber=d["catalognumber"] or None,
            edition=d["edition"] or None,
            artists=[MetadataArtist(name=a["name"], role=a["role"]) for a in d["artists"]],
            tracks={
                tid: MetadataTrack(
                    tracknumber=t["tracknumber"],
                    discnumber=t["discnumber"],
                    title=t["title"],
                    artists=[MetadataArtist(name=a["name"], role=a["role"]) for a in t["artists"]],
                )
                for tid, t in d["tracks"].items()
            },
        )


FAILED_RELEASE_EDIT_FILENAME_REGEX = re.compile(r"failed-release-edit\.([^.]+)\.toml")


def edit_release(
    c: Config,
    release_id: str,
    *,
    # Will use this file as the starting TOML instead of reading the cache.
    resume_file: Path | None = None,
) -> None:
    release = get_release(c, release_id)
    if not release:
        raise ReleaseDoesNotExistError(f"Release {release_id} does not exist")

    # Trigger a quick cache update to ensure we are reading the liveliest data.
    update_cache_for_releases(c, [release.source_path])

    # TODO: Read from tags directly to ensure that we are not writing stale data.
    with lock(c, release_lock_name(release_id)):
        assert release is not None
        tracks = get_tracks_associated_with_release(c, release)

        if resume_file is not None:
            m = FAILED_RELEASE_EDIT_FILENAME_REGEX.match(resume_file.name)
            if not m:
                raise InvalidReleaseEditResumeFileError(
                    f"{resume_file.name} is not a valid release edit resume file"
                )
            resume_uuid = m[1]
            if resume_uuid != release_id:
                raise InvalidReleaseEditResumeFileError(
                    f"{resume_file.name} is not associated with this release"
                )
            with resume_file.open("r") as fp:
                original_toml = fp.read()
        else:
            original_metadata = MetadataRelease.from_cache(release, tracks)
            original_toml = original_metadata.serialize()

        toml = click.edit(original_toml, extension=".toml")
        if not toml:
            logger.info("Aborting manual release edit: metadata file not submitted.")
            return
        if original_toml == toml:
            logger.info("Aborting manual release edit: no metadata change detected.")
            return

        try:
            release_meta = MetadataRelease.from_toml(toml)
            for t in tracks:
                track_meta = release_meta.tracks[t.id]
                tags = AudioTags.from_file(t.source_path)

                dirty = False

                # Track tags.
                if tags.tracknumber != track_meta.tracknumber:
                    tags.tracknumber = track_meta.tracknumber
                    dirty = True
                    logger.debug(f"Modified tag detected for {t.source_path}: tracknumber")
                if tags.discnumber != track_meta.discnumber:
                    tags.discnumber = track_meta.discnumber
                    dirty = True
                    logger.debug(f"Modified tag detected for {t.source_path}: discnumber")
                if tags.tracktitle != track_meta.title:
                    tags.tracktitle = track_meta.title
                    dirty = True
                    logger.debug(f"Modified tag detected for {t.source_path}: title")
                tart = MetadataArtist.to_mapping(track_meta.artists)
                if tags.trackartists != tart:
                    tags.trackartists = tart
                    dirty = True
                    logger.debug(f"Modified tag detected for {t.source_path}: artists")

                # Album tags.
                if tags.releasetitle != release_meta.title:
                    tags.releasetitle = release_meta.title
                    dirty = True
                    logger.debug(f"Modified tag detected for {t.source_path}: release")
                if tags.releasetype != release_meta.releasetype:
                    tags.releasetype = release_meta.releasetype.lower()
                    dirty = True
                    logger.debug(f"Modified tag detected for {t.source_path}: releasetype")
                if tags.releasedate != release_meta.releasedate:
                    tags.releasedate = release_meta.releasedate
                    dirty = True
                    logger.debug(f"Modified tag detected for {t.source_path}: releasedate")
                if tags.originaldate != release_meta.originaldate:
                    tags.originaldate = release_meta.originaldate
                    dirty = True
                    logger.debug(f"Modified tag detected for {t.source_path}: originaldate")
                if tags.compositiondate != release_meta.compositiondate:
                    tags.compositiondate = release_meta.compositiondate
                    dirty = True
                    logger.debug(f"Modified tag detected for {t.source_path}: compositiondate")
                if tags.edition != release_meta.edition:
                    tags.edition = release_meta.edition
                    dirty = True
                    logger.debug(f"Modified tag detected for {t.source_path}: edition")
                if tags.catalognumber != release_meta.catalognumber:
                    tags.catalognumber = release_meta.catalognumber
                    dirty = True
                    logger.debug(f"Modified tag detected for {t.source_path}: catalognumber")
                if tags.genre != release_meta.genres:
                    tags.genre = release_meta.genres
                    dirty = True
                    logger.debug(f"Modified tag detected for {t.source_path}: genre")
                if tags.secondarygenre != release_meta.secondary_genres:
                    tags.secondarygenre = release_meta.secondary_genres
                    dirty = True
                    logger.debug(f"Modified tag detected for {t.source_path}: secondarygenre")
                if tags.descriptor != release_meta.descriptors:
                    tags.descriptor = release_meta.descriptors
                    dirty = True
                    logger.debug(f"Modified tag detected for {t.source_path}: descriptor")
                if tags.label != release_meta.labels:
                    tags.label = release_meta.labels
                    dirty = True
                    logger.debug(f"Modified tag detected for {t.source_path}: label")
                aart = MetadataArtist.to_mapping(release_meta.artists)
                if tags.releaseartists != aart:
                    tags.releaseartists = aart
                    dirty = True
                    logger.debug(f"Modified tag detected for {t.source_path}: release_artists")

                if dirty:
                    logger.info(
                        f"Flushing changed tags to {str(t.source_path).removeprefix(str(c.music_source_dir) + '/')}"
                    )
                    tags.flush()

            if release_meta.new != release.new:
                toggle_release_new(c, release.id)
        except RoseError as e:
            new_resume_path = c.cache_dir / f"failed-release-edit.{release_id}.toml"
            with new_resume_path.open("w") as fp:
                fp.write(toml)
            raise ReleaseEditFailedError(
                f"""\
Failed to apply release edit: {e}

--------

The submitted metadata TOML file has been written to {new_resume_path.resolve()}.

You can reattempt the release edit and fix the metadata file with the command:

    $ rose releases edit --resume {shlex.quote(str(new_resume_path.resolve()))} {shlex.quote(release_id)}
        """
            ) from e

    if resume_file:
        resume_file.unlink()

    update_cache_for_releases(c, [release.source_path], force=True)


def run_actions_on_release(
    c: Config,
    release_id: str,
    actions: list[MetadataAction],
    *,
    dry_run: bool = False,
    confirm_yes: bool = False,
) -> None:
    """Run rule engine actions on a release."""
    release = get_release(c, release_id)
    if release is None:
        raise ReleaseDoesNotExistError(f"Release {release_id} does not exist")
    tracks = get_tracks_associated_with_release(c, release)
    audiotags = [AudioTags.from_file(t.source_path) for t in tracks]
    execute_metadata_actions(c, actions, audiotags, dry_run=dry_run, confirm_yes=confirm_yes)


def create_single_release(c: Config, track_path: Path) -> None:
    """Takes a track and copies it into a brand new "single" release with only that track."""
    if not track_path.is_file():
        raise FileNotFoundError(f"Failed to extract single: file {track_path} not found")

    # Step 1. Compute the new directory name for the single.
    af = AudioTags.from_file(track_path)
    title = (af.tracktitle or "Unknown Title").strip()

    dirname = f"{artistsfmt(af.trackartists)} - "
    if af.releasedate:
        dirname += f"{af.releasedate}. "
    dirname += title
    # Handle directory name collisions.
    collision_no = 2
    original_dirname = dirname
    while True:
        if not (c.music_source_dir / dirname).exists():
            break
        dirname = f"{original_dirname} [{collision_no}]"
        collision_no += 1
    # Step 2. Make the new directory and copy the track. If cover art is in track's current
    # directory, copy that over too.
    source_path = c.music_source_dir / dirname
    source_path.mkdir()
    new_track_path = source_path / f"01. {title}{track_path.suffix}"
    shutil.copyfile(track_path, new_track_path)
    for f in track_path.parent.iterdir():
        if f.name.lower() in c.valid_cover_arts:
            shutil.copyfile(f, source_path / f.name)
            break
    # Step 3. Update the tags of the new track. Clear the Rose IDs too: this is a brand new track.
    af = AudioTags.from_file(new_track_path)
    af.releasetitle = title
    af.releasetype = "single"
    af.releaseartists = af.trackartists
    af.tracknumber = "1"
    af.discnumber = "1"
    af.release_id = None
    af.id = None
    af.flush()
    af = AudioTags.from_file(new_track_path)
    logger.info(f"Created phony single release {source_path.name}")
    # Step 4: Update the cache!
    update_cache_for_releases(c, [source_path])
    # Step 5: Default extracted singles to not new: if it is new, why are you meddling with it?
    for f in source_path.iterdir():
        if m := STORED_DATA_FILE_REGEX.match(f.name):
            release_id = m[1]
            break
    else:
        raise RoseError(
            f"Impossible: Failed to parse release ID from newly created single directory {source_path}"
        )
    toggle_release_new(c, release_id)
