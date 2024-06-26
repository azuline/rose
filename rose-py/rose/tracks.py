"""
The releases module encapsulates all mutations that can occur on release and track entities.
"""

from __future__ import annotations

import logging

from rose.audiotags import AudioTags
from rose.cache import (
    Track,
    filter_tracks,
    get_track,
    list_tracks,
)
from rose.common import RoseExpectedError
from rose.config import Config
from rose.rule_parser import ALL_TAGS, Action, Matcher
from rose.rules import (
    execute_metadata_actions,
    fast_search_for_matching_tracks,
    filter_track_false_positives_using_read_cache,
)

logger = logging.getLogger(__name__)


class TrackDoesNotExistError(RoseExpectedError):
    pass


def find_tracks_matching_rule(c: Config, matcher: Matcher) -> list[Track]:
    # Implement optimizations for common lookups. Only applies to strict lookups.
    # TODO: Morning
    if matcher.pattern.strict_start and matcher.pattern.strict_end:
        if matcher.tags == ALL_TAGS["artist"]:
            return filter_tracks(c, all_artist_filter=matcher.pattern.needle)
        if matcher.tags == ALL_TAGS["trackartist"]:
            return filter_tracks(c, track_artist_filter=matcher.pattern.needle)
        if matcher.tags == ALL_TAGS["releaseartist"]:
            return filter_tracks(c, release_artist_filter=matcher.pattern.needle)
        if matcher.tags == ["genre"]:
            return filter_tracks(c, genre_filter=matcher.pattern.needle)
        if matcher.tags == ["label"]:
            return filter_tracks(c, label_filter=matcher.pattern.needle)
        if matcher.tags == ["descriptor"]:
            return filter_tracks(c, descriptor_filter=matcher.pattern.needle)

    track_ids = [t.id for t in fast_search_for_matching_tracks(c, matcher)]
    tracks = list_tracks(c, track_ids)
    return filter_track_false_positives_using_read_cache(matcher, tracks)


def run_actions_on_track(
    c: Config,
    track_id: str,
    actions: list[Action],
    *,
    dry_run: bool = False,
    confirm_yes: bool = False,
) -> None:
    """Run rule engine actions on a release."""
    track = get_track(c, track_id)
    if track is None:
        raise TrackDoesNotExistError(f"Track {track_id} does not exist")
    audiotag = AudioTags.from_file(track.source_path)
    execute_metadata_actions(c, actions, [audiotag], dry_run=dry_run, confirm_yes=confirm_yes)
