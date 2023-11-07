"""
The releases module encapsulates all mutations that can occur on release and track entities.
"""

from __future__ import annotations

import json
import logging

from rose.audiotags import AudioTags
from rose.cache import (
    get_releases_associated_with_tracks,
    get_track,
    list_tracks,
)
from rose.common import RoseExpectedError
from rose.config import Config
from rose.rule_parser import MetadataAction, MetadataMatcher
from rose.rules import (
    execute_metadata_actions,
    fast_search_for_matching_tracks,
    filter_track_false_positives_using_read_cache,
)

logger = logging.getLogger(__name__)


class TrackDoesNotExistError(RoseExpectedError):
    pass


def dump_track(c: Config, track_id: str) -> str:
    track = get_track(c, track_id)
    if track is None:
        raise TrackDoesNotExistError(f"Track {track_id} does not exist")
    return json.dumps(track.dump())


def dump_tracks(c: Config, matcher: MetadataMatcher | None = None) -> str:
    track_ids = None
    if matcher:
        track_ids = [t.id for t in fast_search_for_matching_tracks(c, matcher)]
    tracks = list_tracks(c, track_ids)
    if matcher:
        tr_pairs = get_releases_associated_with_tracks(c, tracks)
        tr_pairs = filter_track_false_positives_using_read_cache(matcher, tr_pairs)
        tracks = [x[0] for x in tr_pairs]
    return json.dumps([t.dump() for t in tracks])


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
