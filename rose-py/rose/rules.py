"""
The rules module implements the Rules Engine, which provides performant substring tag querying and
bulk metadata updating.

The first part of this file implements the Rule Engine pipeline, which:

1. Fetches a superset of possible tracks from the Read Cache.
2. Filters out false positives via tags.
3. Executes actions to update metadata.

The second part of this file provides performant release/track querying entirely from the read
cache, which is used by other modules to provide release/track filtering capabilities.
"""

import copy
import dataclasses
import logging
import re
import shlex
import time
import tomllib
from datetime import datetime
from pathlib import Path

import click
import tomli_w

from rose.audiotags import AudioTags, RoseDate
from rose.cache import (
    STORED_DATA_FILE_REGEX,
    Release,
    StoredDataFile,
    Track,
    connect,
    list_releases,
    list_tracks,
    update_cache_for_releases,
)
from rose.common import Artist, RoseError, RoseExpectedError, uniq
from rose.config import Config
from rose.rule_parser import (
    RELEASE_TAGS,
    Action,
    AddAction,
    DeleteAction,
    Matcher,
    Pattern,
    ReplaceAction,
    Rule,
    SedAction,
    SplitAction,
)

logger = logging.getLogger(__name__)


class TrackTagNotAllowedError(RoseExpectedError):
    pass


class InvalidReplacementValueError(RoseExpectedError):
    pass


def execute_stored_metadata_rules(
    c: Config,
    *,
    dry_run: bool = False,
    confirm_yes: bool = False,
) -> None:
    for rule in c.stored_metadata_rules:
        click.secho(f"Executing stored metadata rule {rule}", dim=True)
        execute_metadata_rule(c, rule, dry_run=dry_run, confirm_yes=confirm_yes)


def execute_metadata_rule(
    c: Config,
    rule: Rule,
    *,
    dry_run: bool = False,
    confirm_yes: bool = False,
    enter_number_to_confirm_above_count: int = 25,
) -> None:
    """
    This function executes a metadata update rule. It runs in five parts:

    1. Run a search query on our Full Text Search index. This is far more performant than the SQL
       LIKE operation; however, it is also less precise. It produces false positives, but should not
       produce false negatives. So we then run:
    2. Read the files returned from the search query and remove all false positives.
    3. We then run the actions on each valid matched file and store all the intended changes
       in-memory. No changes are written to disk.
    4. We then prompt the user to confirm the changes, assuming confirm_yes is True.
    5. We then flush the intended changes to disk.
    """
    # Newline for appearance.
    click.echo()
    fast_search_results = fast_search_for_matching_tracks(c, rule.matcher)
    if not fast_search_results:
        click.secho("No matching tracks found", dim=True, italic=True)
        click.echo()
        return
    # If there are more than 400 tracks matched, first filter the matched tracks using the cache,
    # has a sublinear time complexity (but higher baseline). Only then run the tag filter, which has
    # linear time complexity.
    if len(fast_search_results) > 400:
        time_start = time.time()
        tracks = list_tracks(c, [t.id for t in fast_search_results])
        logger.debug(f"Fetched tracks from cache for filtering in {time.time() - time_start} seconds")
        tracks = filter_track_false_positives_using_read_cache(rule.matcher, tracks)
        track_ids = {x.id for x in tracks}
        fast_search_results = [t for t in fast_search_results if t.id in track_ids]
    if not fast_search_results:
        click.secho("No matching tracks found", dim=True, italic=True)
        click.echo()
        return

    matcher_audiotags = filter_track_false_positives_using_tags(
        rule.matcher,
        fast_search_results,
        rule.ignore,
    )
    if not matcher_audiotags:
        click.secho("No matching tracks found", dim=True, italic=True)
        click.echo()
        return

    execute_metadata_actions(
        c,
        rule.actions,
        matcher_audiotags,
        dry_run=dry_run,
        confirm_yes=confirm_yes,
        enter_number_to_confirm_above_count=enter_number_to_confirm_above_count,
    )


TAG_ROLE_REGEX = re.compile(r"\[[^\]]+\]$")


@dataclasses.dataclass(slots=True)
class FastSearchResult:
    id: str
    path: Path


def fast_search_for_matching_tracks(
    c: Config,
    matcher: Matcher,
) -> list[FastSearchResult]:
    """
    Run a search for tracks with the matcher on the Full Text Search index. This is _fast_, but will
    produce false positives. The caller must filter out the false positives after pulling the
    results.
    """
    time_start = time.time()
    matchsql = _convert_matcher_to_fts_query(matcher.pattern)
    logger.debug(f"Converted match {matcher=} to {matchsql=}")

    # Build the query to fetch a superset of tracks to attempt to execute the rules against. Note
    # that we directly use string interpolation here instead of prepared queries, because we are
    # constructing a complex match string and everything is escaped and spaced-out with a random
    # paragraph character, so there's no risk of SQL being interpreted.
    #
    # Remove the "artist role" from the tag, as we do not track the role information in the FTS
    # table. The false positives should be minimal enough that performance should be roughly the
    # same if we filter them out in the tag checking step.
    columns = uniq([TAG_ROLE_REGEX.sub("", t) for t in matcher.tags])
    ftsquery = f"{{{' '.join(columns)}}} : {matchsql}"
    query = f"""
        SELECT DISTINCT t.id, t.source_path
        FROM rules_engine_fts
        JOIN tracks t ON rules_engine_fts.rowid = t.rowid
        WHERE rules_engine_fts MATCH '{ftsquery}'
        ORDER BY t.source_path
    """
    logger.debug(f"Constructed matching query {query}")
    # And then execute the SQL query. Note that we don't pull the tag values here. This query is
    # only used to identify the matching tracks. Afterwards, we will read each track's tags from
    # disk and apply the action on those tag values.
    results: list[FastSearchResult] = []
    with connect(c) as conn:
        for row in conn.execute(query):
            results.append(
                FastSearchResult(
                    id=row["id"],
                    path=Path(row["source_path"]).resolve(),
                )
            )
    logger.debug(f"Matched {len(results)} tracks from the read cache in {time.time() - time_start} seconds")
    return results


def _convert_matcher_to_fts_query(pattern: Pattern) -> str:
    # Convert the matcher to a SQL expression for SQLite FTS. We won't be doing the precise
    # prefix/suffix matching here: for performance, we abuse SQLite FTS by making every character
    # its own token, which grants us the ability to search for arbitrary substrings. However, FTS
    # cannot guarantee ordering, which means that a search for `BLACKPINK` will also match
    # `PINKBLACK`. So we first pull all matching results, and then we use the previously written
    # precise Python matcher to ignore the false positives and only modify the tags we care about.
    # Construct the SQL string for the matcher. Escape quotes in the match string.
    matchsql = "¬".join(pattern.needle).replace("'", "''").replace('"', '""')
    # NEAR restricts the query such that the # of tokens in between the first and last tokens of the
    # matched substring must be less than or equal to a given number. For us, that number is
    # len(matchsqlstr) - 2, as we subtract the first and last characters.
    return f'NEAR("{matchsql}", {max(0, len(pattern.needle)-2)})'


def filter_track_false_positives_using_tags(
    matcher: Matcher,
    fast_search_results: list[FastSearchResult],
    ignore: list[Matcher],
) -> list[AudioTags]:
    time_start = time.time()
    rval = []
    for fsr in fast_search_results:
        tags = AudioTags.from_file(fsr.path)
        # Cached datafile. As it's an extra disk read, we only read it when necessary.
        datafile: StoredDataFile | None = None
        for field in matcher.tags:
            match = False
            # fmt: off
            match = match or (field == "tracktitle" and matches_pattern(matcher.pattern, tags.tracktitle))
            match = match or (field == "releasedate" and matches_pattern(matcher.pattern, tags.releasedate))
            match = match or (field == "originaldate" and matches_pattern(matcher.pattern, tags.originaldate))
            match = match or (field == "compositiondate" and matches_pattern(matcher.pattern, tags.compositiondate))
            match = match or (field == "edition" and matches_pattern(matcher.pattern, tags.edition))
            match = match or (field == "catalognumber" and matches_pattern(matcher.pattern, tags.catalognumber))
            match = match or (field == "tracknumber" and matches_pattern(matcher.pattern, tags.tracknumber))
            match = match or (field == "tracktotal" and matches_pattern(matcher.pattern, tags.tracktotal))
            match = match or (field == "discnumber" and matches_pattern(matcher.pattern, tags.discnumber))
            match = match or (field == "disctotal" and matches_pattern(matcher.pattern, tags.disctotal))
            match = match or (field == "releasetitle" and matches_pattern(matcher.pattern, tags.releasetitle))
            match = match or (field == "releasetype" and matches_pattern(matcher.pattern, tags.releasetype))
            match = match or (field == "genre" and any(matches_pattern(matcher.pattern, x) for x in tags.genre))
            match = match or (field == "secondarygenre" and any(matches_pattern(matcher.pattern, x) for x in tags.secondarygenre))
            match = match or (field == "descriptor" and any(matches_pattern(matcher.pattern, x) for x in tags.descriptor))
            match = match or (field == "label" and any(matches_pattern(matcher.pattern, x) for x in tags.label))
            match = match or (field == "trackartist[main]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.trackartists.main))
            match = match or (field == "trackartist[guest]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.trackartists.guest))
            match = match or (field == "trackartist[remixer]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.trackartists.remixer))
            match = match or (field == "trackartist[producer]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.trackartists.producer))
            match = match or (field == "trackartist[composer]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.trackartists.composer))
            match = match or (field == "trackartist[conductor]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.trackartists.conductor))
            match = match or (field == "trackartist[djmixer]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.trackartists.djmixer))
            match = match or (field == "releaseartist[main]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.releaseartists.main))
            match = match or (field == "releaseartist[guest]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.releaseartists.guest))
            match = match or (field == "releaseartist[remixer]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.releaseartists.remixer))
            match = match or (field == "releaseartist[producer]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.releaseartists.producer))
            match = match or (field == "releaseartist[composer]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.releaseartists.composer))
            match = match or (field == "releaseartist[conductor]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.releaseartists.conductor))
            match = match or (field == "releaseartist[djmixer]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.releaseartists.djmixer))
            # fmt: on
            # And if necessary, open the datafile to match `new`.
            if not match and field == "new":
                if not datafile:
                    datafile = _get_release_datafile_of_directory(tags.path.parent)
                match = matches_pattern(matcher.pattern, datafile.new)

            # If there is a match, check to see if the track is matched by one of the ignore values.
            # If it is ignored, skip the result entirely.
            if match and ignore:
                skip = False
                for i in ignore:
                    # fmt: off
                    skip = skip or (field == "tracktitle" and matches_pattern(i.pattern, tags.tracktitle))
                    skip = skip or (field == "releasedate" and matches_pattern(i.pattern, tags.releasedate))
                    skip = skip or (field == "originaldate" and matches_pattern(i.pattern, tags.originaldate))
                    skip = skip or (field == "compositiondate" and matches_pattern(i.pattern, tags.compositiondate))
                    skip = skip or (field == "edition" and matches_pattern(i.pattern, tags.edition))
                    skip = skip or (field == "catalognumber" and matches_pattern(i.pattern, tags.catalognumber))
                    skip = skip or (field == "tracknumber" and matches_pattern(i.pattern, tags.tracknumber))
                    skip = skip or (field == "tracktotal" and matches_pattern(i.pattern, tags.tracktotal))
                    skip = skip or (field == "discnumber" and matches_pattern(i.pattern, tags.discnumber))
                    skip = skip or (field == "disctotal" and matches_pattern(i.pattern, tags.disctotal))
                    skip = skip or (field == "releasetitle" and matches_pattern(i.pattern, tags.releasetitle))
                    skip = skip or (field == "releasetype" and matches_pattern(i.pattern, tags.releasetype))
                    skip = skip or (field == "genre" and any(matches_pattern(i.pattern, x) for x in tags.genre))
                    skip = skip or (field == "secondarygenre" and any(matches_pattern(i.pattern, x) for x in tags.secondarygenre))
                    skip = skip or (field == "descriptor" and any(matches_pattern(i.pattern, x) for x in tags.descriptor))
                    skip = skip or (field == "label" and any(matches_pattern(i.pattern, x) for x in tags.label))
                    skip = skip or (field == "trackartist[main]" and any(matches_pattern(i.pattern, x.name) for x in tags.trackartists.main))
                    skip = skip or (field == "trackartist[guest]" and any(matches_pattern(i.pattern, x.name) for x in tags.trackartists.guest))
                    skip = skip or (field == "trackartist[remixer]" and any(matches_pattern(i.pattern, x.name) for x in tags.trackartists.remixer))
                    skip = skip or (field == "trackartist[producer]" and any(matches_pattern(i.pattern, x.name) for x in tags.trackartists.producer))
                    skip = skip or (field == "trackartist[composer]" and any(matches_pattern(i.pattern, x.name) for x in tags.trackartists.composer))
                    skip = skip or (field == "trackartist[conductor]" and any(matches_pattern(i.pattern, x.name) for x in tags.trackartists.conductor))
                    skip = skip or (field == "trackartist[djmixer]" and any(matches_pattern(i.pattern, x.name) for x in tags.trackartists.djmixer))
                    skip = skip or (field == "releaseartist[main]" and any(matches_pattern(i.pattern, x.name) for x in tags.releaseartists.main))
                    skip = skip or (field == "releaseartist[guest]" and any(matches_pattern(i.pattern, x.name) for x in tags.releaseartists.guest))
                    skip = skip or (field == "releaseartist[remixer]" and any(matches_pattern(i.pattern, x.name) for x in tags.releaseartists.remixer))
                    skip = skip or (field == "releaseartist[producer]" and any(matches_pattern(i.pattern, x.name) for x in tags.releaseartists.producer))
                    skip = skip or (field == "releaseartist[composer]" and any(matches_pattern(i.pattern, x.name) for x in tags.releaseartists.composer))
                    skip = skip or (field == "releaseartist[conductor]" and any(matches_pattern(i.pattern, x.name) for x in tags.releaseartists.conductor))
                    skip = skip or (field == "releaseartist[djmixer]" and any(matches_pattern(i.pattern, x.name) for x in tags.releaseartists.djmixer))
                    # And finally, check the datafile.
                    if not skip and field == "new":
                        if not datafile:
                            datafile = _get_release_datafile_of_directory(tags.path.parent)
                        match = matches_pattern(i.pattern, datafile.new)
                    # fmt: on
                    if skip:
                        break
                # Break out of the outer loop too; we don't want to try any more fields if the track
                # matches an ignore value.
                if skip:
                    break
            if match:
                rval.append(tags)
                break
    logger.debug(
        f"Filtered {len(fast_search_results)} tracks down to {len(rval)} tracks in {time.time() - time_start} seconds"
    )
    return rval


def _get_release_datafile_of_directory(d: Path) -> StoredDataFile:
    for f in d.iterdir():
        if not STORED_DATA_FILE_REGEX.match(f.name):
            continue
        with f.open("rb") as fp:
            diskdata = tomllib.load(fp)
        return StoredDataFile(
            new=diskdata.get("new", True),
            added_at=diskdata.get("added_at", datetime.now().astimezone().replace(microsecond=0).isoformat()),
        )
    raise RoseError(f"Release data file not found in {d}. How is it in the library?")


Changes = tuple[
    str,
    str | int | bool | RoseDate | None | list[str],
    str | int | bool | RoseDate | None | list[str],
]


def execute_metadata_actions(
    c: Config,
    actions: list[Action],
    audiotags: list[AudioTags],
    *,
    dry_run: bool = False,
    confirm_yes: bool = False,
    enter_number_to_confirm_above_count: int = 25,
) -> None:
    """
    This function executes steps 3-5 of the rule executor. See that function's docstring. This is
    split out to enable running actions on known releases/tracks.
    """
    # === Step 3: Prepare updates on in-memory tags ===

    def names(xs: list[Artist]) -> list[str]:
        # NOTE: Nothing should be an alias in this fn because we get data from tags.
        return [x.name for x in xs]

    def artists(xs: list[str]) -> list[Artist]:
        # NOTE: Nothing should be an alias in this fn because we get data from tags.
        return [Artist(x) for x in xs]

    # Map from str(audiofile.path.parent) to datafile.
    opened_datafiles: dict[str, StoredDataFile] = {}

    def open_datafile(path: Path) -> StoredDataFile:
        try:
            return opened_datafiles[str(path.parent)]
        except KeyError:
            datafile = _get_release_datafile_of_directory(path.parent)
            opened_datafiles[str(path.parent)] = datafile
            return datafile

    actionable_audiotags: list[tuple[AudioTags, list[Changes]]] = []
    # Map from parent directory to tuple.
    actionable_datafiles: dict[str, tuple[AudioTags, StoredDataFile, list[Changes]]] = {}

    # We loop over audiotags as the main loop since the rules engine operates on tracks. Perhaps in
    # the future we better arrange this into release-level as well as track-level and make datafile
    # part of the release loop. We apply the datafile updates as-we-go, so even if we have 12 tracks
    # updating a datafile, the update should only apply and be shown once.
    for tags in audiotags:
        origtags = copy.deepcopy(tags)
        potential_audiotag_changes: list[Changes] = []
        # Load the datafile if we use it. Then we know that we have potential datafile changes for
        # this datafile.
        datafile = None
        potential_datafile_changes: list[Changes] = []
        for act in actions:
            fields_to_update = act.tags
            for field in fields_to_update:
                # Datafile actions.
                # Only read the datafile if it's necessary; we don't want to pay the extra cost
                # every time for rarer fields. Store the opened datafiles in opened_datafiles.
                if field == "new":
                    datafile = datafile or open_datafile(tags.path)
                    v = execute_single_action(act, datafile.new)
                    if v != "true" and v != "false":
                        raise InvalidReplacementValueError(
                            f"Failed to assign new value {v} to new: value must be string `true` or `false`"
                        )
                    orig_value = datafile.new
                    datafile.new = v == "true"
                    if orig_value != datafile.new:
                        potential_datafile_changes.append(("new", orig_value, datafile.new))

                # AudioTag Actions
                # fmt: off
                if field == "tracktitle":
                    tags.tracktitle = execute_single_action(act, tags.tracktitle)
                    potential_audiotag_changes.append(("title", origtags.tracktitle, tags.tracktitle))
                elif field == "releasedate":
                    v = execute_single_action(act, tags.releasedate)
                    try:
                        tags.releasedate = RoseDate.parse(v)
                    except ValueError as e:
                        raise InvalidReplacementValueError(
                            f"Failed to assign new value {v} to releasedate: value must be date string"
                        ) from e
                    potential_audiotag_changes.append(("releasedate", origtags.releasedate, tags.releasedate))
                elif field == "originaldate":
                    v = execute_single_action(act, tags.originaldate)
                    try:
                        tags.originaldate = RoseDate.parse(v)
                    except ValueError as e:
                        raise InvalidReplacementValueError(
                            f"Failed to assign new value {v} to originaldate: value must be date string"
                        ) from e
                    potential_audiotag_changes.append(("originaldate", origtags.originaldate, tags.originaldate))
                elif field == "compositiondate":
                    v = execute_single_action(act, tags.compositiondate)
                    try:
                        tags.compositiondate = RoseDate.parse(v)
                    except ValueError as e:
                        raise InvalidReplacementValueError(
                            f"Failed to assign new value {v} to compositiondate: value must be date string"
                        ) from e
                    potential_audiotag_changes.append(("compositiondate", origtags.compositiondate, tags.compositiondate))
                elif field == "edition":
                    tags.edition = execute_single_action(act, tags.edition)
                    potential_audiotag_changes.append(("edition", origtags.edition, tags.edition))
                elif field == "catalognumber":
                    tags.catalognumber = execute_single_action(act, tags.catalognumber)
                    potential_audiotag_changes.append(("catalognumber", origtags.catalognumber, tags.catalognumber))
                elif field == "tracknumber":
                    tags.tracknumber = execute_single_action(act, tags.tracknumber)
                    potential_audiotag_changes.append(("tracknumber", origtags.tracknumber, tags.tracknumber))
                elif field == "discnumber":
                    tags.discnumber = execute_single_action(act, tags.discnumber)
                    potential_audiotag_changes.append(("discnumber", origtags.discnumber, tags.discnumber))
                elif field == "releasetitle":
                    tags.releasetitle = execute_single_action(act, tags.releasetitle)
                    potential_audiotag_changes.append(("release", origtags.releasetitle, tags.releasetitle))
                elif field == "releasetype":
                    tags.releasetype = execute_single_action(act, tags.releasetype) or "unknown"
                    potential_audiotag_changes.append(("releasetype", origtags.releasetype, tags.releasetype))
                elif field == "genre":
                    tags.genre = execute_multi_value_action(act, tags.genre)
                    potential_audiotag_changes.append(("genre", origtags.genre, tags.genre))
                elif field == "secondarygenre":
                    tags.secondarygenre = execute_multi_value_action(act, tags.secondarygenre)
                    potential_audiotag_changes.append(("secondarygenre", origtags.secondarygenre, tags.secondarygenre))
                elif field == "descriptor":
                    tags.descriptor = execute_multi_value_action(act, tags.descriptor)
                    potential_audiotag_changes.append(("descriptor", origtags.descriptor, tags.descriptor))
                elif field == "label":
                    tags.label = execute_multi_value_action(act, tags.label)
                    potential_audiotag_changes.append(("label", origtags.label, tags.label))
                elif field == "trackartist[main]":
                    tags.trackartists.main = artists(execute_multi_value_action(act, names(tags.trackartists.main)))
                    potential_audiotag_changes.append(("trackartist[main]", names(origtags.trackartists.main), names(tags.trackartists.main)))
                elif field == "trackartist[guest]":
                    tags.trackartists.guest = artists(execute_multi_value_action(act, names(tags.trackartists.guest)))
                    potential_audiotag_changes.append(("trackartist[guest]", names(origtags.trackartists.guest), names(tags.trackartists.guest)))
                elif field == "trackartist[remixer]":
                    tags.trackartists.remixer = artists(execute_multi_value_action(act, names(tags.trackartists.remixer)))
                    potential_audiotag_changes.append(("trackartist[remixer]", names(origtags.trackartists.remixer), names(tags.trackartists.remixer)))
                elif field == "trackartist[producer]":
                    tags.trackartists.producer = artists(execute_multi_value_action(act, names(tags.trackartists.producer)))
                    potential_audiotag_changes.append(("trackartist[producer]", names(origtags.trackartists.producer), names(tags.trackartists.producer)))
                elif field == "trackartist[composer]":
                    tags.trackartists.composer = artists(execute_multi_value_action(act, names(tags.trackartists.composer)))
                    potential_audiotag_changes.append(("trackartist[composer]", names(origtags.trackartists.composer), names(tags.trackartists.composer)))
                elif field == "trackartist[conductor]":
                    tags.trackartists.conductor = artists(execute_multi_value_action(act, names(tags.trackartists.conductor)))
                    potential_audiotag_changes.append(("trackartist[conductor]", names(origtags.trackartists.conductor), names(tags.trackartists.conductor)))
                elif field == "trackartist[djmixer]":
                    tags.trackartists.djmixer = artists(execute_multi_value_action(act, names(tags.trackartists.djmixer)))
                    potential_audiotag_changes.append(("trackartist[djmixer]", names(origtags.trackartists.djmixer), names(tags.trackartists.djmixer)))
                elif field == "releaseartist[main]":
                    tags.releaseartists.main = artists(execute_multi_value_action(act, names(tags.releaseartists.main)))
                    potential_audiotag_changes.append(("releaseartist[main]", names(origtags.releaseartists.main), names(tags.releaseartists.main)))
                elif field == "releaseartist[guest]":
                    tags.releaseartists.guest = artists(execute_multi_value_action(act, names(tags.releaseartists.guest)))
                    potential_audiotag_changes.append(("releaseartist[guest]", names(origtags.releaseartists.guest), names(tags.releaseartists.guest)))
                elif field == "releaseartist[remixer]":
                    tags.releaseartists.remixer = artists(execute_multi_value_action(act, names(tags.releaseartists.remixer)))
                    potential_audiotag_changes.append(("releaseartist[remixer]", names(origtags.releaseartists.remixer), names(tags.releaseartists.remixer) ))
                elif field == "releaseartist[producer]":
                    tags.releaseartists.producer = artists(execute_multi_value_action(act, names(tags.releaseartists.producer)))
                    potential_audiotag_changes.append(("releaseartist[producer]", names(origtags.releaseartists.producer), names(tags.releaseartists.producer)))
                elif field == "releaseartist[composer]":
                    tags.releaseartists.composer = artists(execute_multi_value_action(act, names(tags.releaseartists.composer)))
                    potential_audiotag_changes.append(("releaseartist[composer]", names(origtags.releaseartists.composer), names(tags.releaseartists.composer)))
                elif field == "releaseartist[conductor]":
                    tags.releaseartists.conductor = artists(execute_multi_value_action(act, names(tags.releaseartists.conductor)))
                    potential_audiotag_changes.append(("releaseartist[conductor]", names(origtags.releaseartists.conductor), names(tags.releaseartists.conductor)))
                elif field == "releaseartist[djmixer]":
                    tags.releaseartists.djmixer = artists(execute_multi_value_action(act, names(tags.releaseartists.djmixer)))
                    potential_audiotag_changes.append(( "releaseartist[djmixer]", names(origtags.releaseartists.djmixer), names(tags.releaseartists.djmixer)))
                # fmt: on

        # Compute real changes by diffing the tags, and then store.
        tag_changes = [(x, y, z) for x, y, z in potential_audiotag_changes if y != z]
        if tag_changes:
            actionable_audiotags.append((tags, tag_changes))

        # We already handled diffing for the datafile above. This moves the inner-track-loop
        # datafile updates to the outer scope.
        if datafile and potential_datafile_changes:
            try:
                _, _, datafile_changes = actionable_datafiles[str(tags.path.parent)]
            except KeyError:
                datafile_changes = []
                actionable_datafiles[str(tags.path.parent)] = (tags, datafile, datafile_changes)
            datafile_changes.extend(potential_datafile_changes)

        if not tag_changes and not (datafile and potential_datafile_changes):
            logger.debug(f"Skipping matched track {tags.path}: no changes calculated off tags and datafile")

    if not actionable_audiotags and not actionable_datafiles:
        click.secho("No matching tracks found", dim=True, italic=True)
        click.echo()
        return

    # === Step 4: Display changes and ask for user confirmation ===

    # Compute the text to display:
    todisplay: list[tuple[str, list[Changes]]] = []
    maxpathwidth = 0
    for tags, tag_changes in actionable_audiotags:
        pathtext = str(tags.path).removeprefix(str(c.music_source_dir) + "/")
        if len(pathtext) >= 120:
            pathtext = pathtext[:59] + ".." + pathtext[-59:]
        maxpathwidth = max(maxpathwidth, len(pathtext))
        todisplay.append((pathtext, tag_changes))
    for path, (_, _, datafile_changes) in actionable_datafiles.items():
        pathtext = path.removeprefix(str(c.music_source_dir) + "/")
        if len(pathtext) >= 120:
            pathtext = pathtext[:59] + ".." + pathtext[-59:]
        maxpathwidth = max(maxpathwidth, len(pathtext))
        todisplay.append((pathtext, datafile_changes))

    # And then display it.
    for pathtext, tag_changes in todisplay:
        click.secho(pathtext, underline=True)
        for name, old, new in tag_changes:
            click.echo(f"      {name}: ", nl=False)
            click.secho(old, fg="red", nl=False)
            click.echo(" -> ", nl=False)
            click.secho(new, fg="green", bold=True)

    # If we're dry-running, then abort here.
    if dry_run:
        click.echo()
        click.secho(
            f"This is a dry run, aborting. {len(actionable_audiotags)} tracks would have been modified.",
            dim=True,
        )
        return

    # And then let's go for the confirmation.
    num_changes = len(actionable_audiotags) + len(actionable_datafiles)
    if confirm_yes:
        click.echo()
        if num_changes > enter_number_to_confirm_above_count:
            while True:
                userconfirmation = click.prompt(
                    f"Write changes to {num_changes} tracks? Enter {click.style(num_changes, bold=True)} to confirm (or 'no' to abort)"
                )
                if userconfirmation == "no":
                    logger.debug("Aborting planned tag changes after user confirmation")
                    return
                if userconfirmation == str(num_changes):
                    click.echo()
                    break
        else:
            if not click.confirm(
                f"Write changes to {click.style(num_changes, bold=True)} tracks?",
                default=True,
                prompt_suffix="",
            ):
                logger.debug("Aborting planned tag changes after user confirmation")
                return
            click.echo()

    # === Step 5: Flush writes to disk ===

    logger.info(f"Writing tag changes for actions {' '.join([shlex.quote(str(a)) for a in actions])}")
    changed_release_ids: set[str] = set()
    for tags, tag_changes in actionable_audiotags:
        if tags.release_id:
            changed_release_ids.add(tags.release_id)
        pathtext = str(tags.path).removeprefix(str(c.music_source_dir) + "/")
        logger.debug(
            f"Attempting to write {pathtext} changes: {' //// '.join([str(x)+' -> '+str(y) for _, x, y in tag_changes])}"
        )
        tags.flush()
        logger.info(f"Wrote tag changes to {pathtext}")
    for path, (tags, datafile, datafile_changes) in actionable_datafiles.items():
        if tags.release_id:
            changed_release_ids.add(tags.release_id)
        pathtext = path.removeprefix(str(c.music_source_dir) + "/")
        logger.debug(
            f"Attempting to write {pathtext} changes: {' //// '.join([str(x)+' -> '+str(y) for _, x, y in datafile_changes])}"
        )
        for f in Path(path).iterdir():
            if not STORED_DATA_FILE_REGEX.match(f.name):
                continue
            with f.open("wb") as fp:
                tomli_w.dump(dataclasses.asdict(datafile), fp)
        logger.info(f"Wrote datafile changes to {pathtext}")

    click.echo()
    click.echo(f"Applied tag changes to {num_changes} tracks!")

    # == Step 6: Trigger cache update ===

    click.echo()
    source_paths = [r.source_path for r in list_releases(c, list(changed_release_ids))]
    update_cache_for_releases(c, source_paths)


TagValue = str | int | bool | RoseDate | None


def value_to_str(value: TagValue) -> str:
    if isinstance(value, bool):
        return str(value).lower()
    if value:
        return str(value)
    return ""


def matches_pattern(pattern: Pattern, value: str | int | bool | RoseDate | None) -> bool:
    value = value_to_str(value)

    needle = pattern.needle
    haystack = value
    if pattern.case_insensitive:
        needle = needle.lower()
        haystack = haystack.lower()

    if pattern.strict_start and pattern.strict_end:
        return haystack == needle
    if pattern.strict_start:
        return haystack.startswith(needle)
    if pattern.strict_end:
        return haystack.endswith(needle)
    return needle in haystack


# Factor out the logic for executing an action on a single-value tag and a multi-value tag.
def execute_single_action(
    action: Action,
    value: str | int | bool | RoseDate | None,
) -> str | None:
    if action.pattern and not matches_pattern(action.pattern, value):
        return value_to_str(value)

    bhv = action.behavior
    strvalue = value_to_str(value)

    if isinstance(bhv, ReplaceAction):
        return bhv.replacement
    elif isinstance(bhv, SedAction):
        if strvalue is None:
            return None
        return bhv.src.sub(bhv.dst, strvalue)
    elif isinstance(bhv, DeleteAction):
        return None
    raise RoseError(f"Invalid action {type(bhv)} for single-value tag: Should have been caught in parsing")


def execute_multi_value_action(
    action: Action,
    values: list[str],
) -> list[str]:
    bhv = action.behavior

    # If match_pattern is specified, check which values match. And if none match, bail out.
    matching_idx = list(range(len(values)))
    if action.pattern:
        matching_idx = []
        for i, v in enumerate(values):
            if matches_pattern(action.pattern, v):
                matching_idx.append(i)
        if not matching_idx:
            return values

    if isinstance(bhv, AddAction):
        return uniq([*values, bhv.value])

    rval: list[str] = []
    for i, v in enumerate(values):
        if i not in matching_idx:
            rval.append(v)
            continue
        if isinstance(bhv, DeleteAction):
            continue
        newvals = [v]
        if isinstance(bhv, ReplaceAction):
            newvals = bhv.replacement.split(";")
        elif isinstance(bhv, SedAction):
            newvals = bhv.src.sub(bhv.dst, v).split(";")
        elif isinstance(bhv, SplitAction):
            newvals = v.split(bhv.delimiter)
        for nv in newvals:
            nv = nv.strip()
            if nv:
                rval.append(nv)
    return uniq(rval)


# The following functions are for leveraging the rules engine as a performant query engine.


def fast_search_for_matching_releases(
    c: Config,
    matcher: Matcher,
) -> list[FastSearchResult]:
    """Basically the same thing as fast_search_for_matching_tracks but with releases."""
    time_start = time.time()
    if track_tags := [t for t in matcher.tags if t not in RELEASE_TAGS]:
        # But allow an exception if both trackartist and releaseartist are defined: means a shorthand
        # was used. Just ignore trackartist.
        if any(t.startswith("releaseartist") for t in matcher.tags):
            track_tags = [t for t in track_tags if not t.startswith("trackartist")]
        else:
            raise TrackTagNotAllowedError(
                f"Track tags are not allowed when matching against releases: {', '.join(track_tags)}"
            )

    matchsql = _convert_matcher_to_fts_query(matcher.pattern)
    logger.debug(f"Converted match {matcher=} to {matchsql=}")
    columns = uniq([TAG_ROLE_REGEX.sub("", t) for t in matcher.tags])
    ftsquery = f"{{{' '.join(columns)}}} : {matchsql}"
    query = f"""
        SELECT DISTINCT r.id, r.source_path
        FROM rules_engine_fts
        JOIN tracks t ON rules_engine_fts.rowid = t.rowid
        JOIN releases r ON r.id = t.release_id
        WHERE rules_engine_fts MATCH '{ftsquery}'
        ORDER BY r.source_path
    """
    logger.debug(f"Constructed matching query {query}")
    results: list[FastSearchResult] = []
    with connect(c) as conn:
        for row in conn.execute(query):
            results.append(FastSearchResult(id=row["id"], path=Path(row["source_path"]).resolve()))
    logger.debug(f"Matched {len(results)} releases from the read cache in {time.time() - time_start} seconds")
    return results


def filter_track_false_positives_using_read_cache(
    matcher: Matcher,
    tracks: list[Track],
) -> list[Track]:
    time_start = time.time()
    rval = []
    for t in tracks:
        for field in matcher.tags:
            match = False
            # fmt: off
            match = match or (field == "tracktitle" and matches_pattern(matcher.pattern, t.tracktitle))
            match = match or (field == "releasedate" and matches_pattern(matcher.pattern, t.release.releasedate))
            match = match or (field == "originaldate" and matches_pattern(matcher.pattern, t.release.originaldate))
            match = match or (field == "compositiondate" and matches_pattern(matcher.pattern, t.release.compositiondate))
            match = match or (field == "edition" and matches_pattern(matcher.pattern, t.release.edition))
            match = match or (field == "catalognumber" and matches_pattern(matcher.pattern, t.release.catalognumber))
            match = match or (field == "tracknumber" and matches_pattern(matcher.pattern, t.tracknumber))
            match = match or (field == "tracktotal" and matches_pattern(matcher.pattern, t.tracktotal))
            match = match or (field == "discnumber" and matches_pattern(matcher.pattern, t.discnumber))
            match = match or (field == "disctotal" and matches_pattern(matcher.pattern, t.release.disctotal))
            match = match or (field == "releasetitle" and matches_pattern(matcher.pattern, t.release.releasetitle))
            match = match or (field == "releasetype" and matches_pattern(matcher.pattern, t.release.releasetype))
            match = match or (field == "new" and matches_pattern(matcher.pattern, t.release.new))
            match = match or (field == "genre" and any(matches_pattern(matcher.pattern, x) for x in t.release.genres))
            match = match or (field == "secondarygenre" and any(matches_pattern(matcher.pattern, x) for x in t.release.secondary_genres))
            match = match or (field == "descriptor" and any(matches_pattern(matcher.pattern, x) for x in t.release.descriptors))
            match = match or (field == "label" and any(matches_pattern(matcher.pattern, x) for x in t.release.labels))
            match = match or (field == "trackartist[main]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.main))
            match = match or (field == "trackartist[guest]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.guest))
            match = match or (field == "trackartist[remixer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.remixer))
            match = match or (field == "trackartist[producer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.producer))
            match = match or (field == "trackartist[composer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.composer))
            match = match or (field == "trackartist[conductor]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.conductor))
            match = match or (field == "trackartist[djmixer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.djmixer))
            match = match or (field == "releaseartist[main]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.releaseartists.main))
            match = match or (field == "releaseartist[guest]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.releaseartists.guest))
            match = match or (field == "releaseartist[remixer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.releaseartists.remixer))
            match = match or (field == "releaseartist[producer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.releaseartists.producer))
            match = match or (field == "releaseartist[composer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.releaseartists.composer))
            match = match or (field == "releaseartist[conductor]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.releaseartists.conductor))
            match = match or (field == "releaseartist[djmixer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.releaseartists.djmixer))
            # fmt: on
            if match:
                rval.append(t)
                break
    logger.debug(f"Filtered {len(tracks)} tracks down to {len(rval)} tracks in {time.time() - time_start} seconds")
    return rval


def filter_release_false_positives_using_read_cache(
    matcher: Matcher,
    releases: list[Release],
) -> list[Release]:
    time_start = time.time()
    rval = []
    for r in releases:
        for field in matcher.tags:
            match = False
            # Only attempt to match the release tags; ignore track tags.
            # fmt: off
            match = match or (field == "releasedate" and matches_pattern(matcher.pattern, r.releasedate))
            match = match or (field == "originaldate" and matches_pattern(matcher.pattern, r.originaldate))
            match = match or (field == "compositiondate" and matches_pattern(matcher.pattern, r.compositiondate))
            match = match or (field == "edition" and matches_pattern(matcher.pattern, r.edition))
            match = match or (field == "catalognumber" and matches_pattern(matcher.pattern, r.catalognumber))
            match = match or (field == "releasetitle" and matches_pattern(matcher.pattern, r.releasetitle))
            match = match or (field == "releasetype" and matches_pattern(matcher.pattern, r.releasetype))
            match = match or (field == "new" and matches_pattern(matcher.pattern, r.new))
            match = match or (field == "genre" and any(matches_pattern(matcher.pattern, x) for x in r.genres))
            match = match or (field == "secondarygenre" and any(matches_pattern(matcher.pattern, x) for x in r.secondary_genres))
            match = match or (field == "descriptor" and any(matches_pattern(matcher.pattern, x) for x in r.descriptors))
            match = match or (field == "label" and any(matches_pattern(matcher.pattern, x) for x in r.labels))
            match = match or (field == "releaseartist[main]" and any(matches_pattern(matcher.pattern, x.name) for x in r.releaseartists.main))
            match = match or (field == "releaseartist[guest]" and any(matches_pattern(matcher.pattern, x.name) for x in r.releaseartists.guest))
            match = match or (field == "releaseartist[remixer]" and any(matches_pattern(matcher.pattern, x.name) for x in r.releaseartists.remixer))
            match = match or (field == "releaseartist[conductor]" and any(matches_pattern(matcher.pattern, x.name) for x in r.releaseartists.conductor))
            match = match or (field == "releaseartist[producer]" and any(matches_pattern(matcher.pattern, x.name) for x in r.releaseartists.producer))
            match = match or (field == "releaseartist[composer]" and any(matches_pattern(matcher.pattern, x.name) for x in r.releaseartists.composer))
            match = match or (field == "releaseartist[djmixer]" and any(matches_pattern(matcher.pattern, x.name) for x in r.releaseartists.djmixer))
            # fmt: on
            if match:
                rval.append(r)
                break
    logger.debug(
        f"Filtered {len(releases)} releases down to {len(rval)} releases in {time.time() - time_start} seconds"
    )
    return rval
