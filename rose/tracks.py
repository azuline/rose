"""
The releases module encapsulates all mutations that can occur on release and track entities.
"""

from __future__ import annotations

import json
import logging

from rose.audiotags import AudioTags
from rose.cache import (
    get_track,
    list_tracks,
)
from rose.common import RoseExpectedError
from rose.config import Config
from rose.rule_parser import MetadataAction
from rose.rules import execute_metadata_actions

logger = logging.getLogger(__name__)


class TrackDoesNotExistError(RoseExpectedError):
    pass


def dump_track(c: Config, track_id: str) -> str:
    track = get_track(c, track_id)
    if track is None:
        raise TrackDoesNotExistError(f"Track {track_id} does not exist")
    return json.dumps(track.dump())


def dump_tracks(c: Config) -> str:
    return json.dumps([t.dump() for t in list_tracks(c)])


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
