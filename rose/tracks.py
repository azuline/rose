"""
The releases module encapsulates all mutations that can occur on release and track entities.
"""

from __future__ import annotations

import logging

from rose.audiotags import AudioTags
from rose.cache import (
    Track,
    get_track,
    list_tracks,
)
from rose.common import RoseExpectedError
from rose.config import Config
from rose.rule_parser import ALL_TAGS, MetadataAction, MetadataMatcher
from rose.rules import (
    execute_metadata_actions,
    fast_search_for_matching_tracks,
    filter_track_false_positives_using_read_cache,
)

logger = logging.getLogger(__name__)


class TrackDoesNotExistError(RoseExpectedError):
    pass


def find_tracks_matching_rule(c: Config, matcher: MetadataMatcher) -> list[Track]:
    # Implement optimizations for common lookups. Only applies to strict lookups.
    # TODO: Morning
    if matcher.pattern.pattern.startswith("^") and matcher.pattern.pattern.endswith("$"):
        if matcher.tags == ALL_TAGS["artist"]:
            pass
        if matcher.tags == ["genre"]:
            pass
        if matcher.tags == ["label"]:
            pass
        if matcher.tags == ["descriptor"]:
            pass

    track_ids = [t.id for t in fast_search_for_matching_tracks(c, matcher)]
    tracks = list_tracks(c, track_ids)
    return filter_track_false_positives_using_read_cache(matcher, tracks)


def run_actions_on_track(
    c: Config,
    track_id: str,
    actions: list[MetadataAction],
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
