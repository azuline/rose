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
import logging
import re
import shlex
import time
from dataclasses import dataclass
from pathlib import Path

import click

from rose.audiotags import AudioTags
from rose.cache import (
    CachedRelease,
    CachedTrack,
    connect,
    list_releases,
    list_tracks,
    update_cache_for_releases,
)
from rose.common import Artist, RoseError, RoseExpectedError, uniq
from rose.config import Config
from rose.rule_parser import (
    RELEASE_TAGS,
    AddAction,
    DeleteAction,
    MatcherPattern,
    MetadataAction,
    MetadataMatcher,
    MetadataRule,
    ReplaceAction,
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
    rule: MetadataRule,
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
        logger.debug(
            f"Fetched tracks from cache for filtering in {time.time() - time_start} seconds"
        )
        tracks = filter_track_false_positives_using_read_cache(rule.matcher, tracks)
        track_ids = {x.id for x in tracks}
        fast_search_results = [t for t in fast_search_results if t.id in track_ids]
    if not fast_search_results:
        click.secho("No matching tracks found", dim=True, italic=True)
        click.echo()
        return

    matcher_audiotags = filter_track_false_positives_using_tags(
        rule.matcher, fast_search_results, rule.ignore
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


@dataclass
class FastSearchResult:
    id: str
    path: Path


def fast_search_for_matching_tracks(
    c: Config,
    matcher: MetadataMatcher,
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
    logger.debug(
        f"Matched {len(results)} tracks from the read cache in {time.time() - time_start} seconds"
    )
    return results


def _convert_matcher_to_fts_query(pattern: MatcherPattern) -> str:
    # Convert the matcher to a SQL expression for SQLite FTS. We won't be doing the precise
    # prefix/suffix matching here: for performance, we abuse SQLite FTS by making every character
    # its own token, which grants us the ability to search for arbitrary substrings. However, FTS
    # cannot guarantee ordering, which means that a search for `BLACKPINK` will also match
    # `PINKBLACK`. So we first pull all matching results, and then we use the previously written
    # precise Python matcher to ignore the false positives and only modify the tags we care about.
    #
    # Therefore we strip the `^$` and convert the text into SQLite FTS Match query. We use NEAR to
    # assert that all the characters are within a substring equivalent to the length of the query,
    # which should filter out most false positives.
    needle = pattern.pattern
    if needle.startswith("^"):
        needle = needle[1:]
    if needle.endswith("$"):
        needle = needle[:-1]
    # Construct the SQL string for the matcher. Escape quotes in the match string.
    matchsql = "¬".join(needle).replace("'", "''").replace('"', '""')
    # NEAR restricts the query such that the # of tokens in between the first and last tokens of the
    # matched substring must be less than or equal to a given number. For us, that number is
    # len(matchsqlstr) - 2, as we subtract the first and last characters.
    return f'NEAR("{matchsql}", {max(0, len(needle)-2)})'


def filter_track_false_positives_using_tags(
    matcher: MetadataMatcher,
    fast_search_results: list[FastSearchResult],
    ignore: list[MetadataMatcher],
) -> list[AudioTags]:
    time_start = time.time()
    rval = []
    for fsr in fast_search_results:
        tags = AudioTags.from_file(fsr.path)
        for field in matcher.tags:
            match = False
            # fmt: off
            match = match or (field == "tracktitle" and matches_pattern(matcher.pattern, tags.title))
            match = match or (field == "year" and matches_pattern(matcher.pattern, tags.year))
            match = match or (field == "tracknumber" and matches_pattern(matcher.pattern, tags.tracknumber))
            match = match or (field == "tracktotal" and matches_pattern(matcher.pattern, tags.tracktotal))
            match = match or (field == "discnumber" and matches_pattern(matcher.pattern, tags.discnumber))
            match = match or (field == "disctotal" and matches_pattern(matcher.pattern, tags.disctotal))
            match = match or (field == "albumtitle" and matches_pattern(matcher.pattern, tags.album))
            match = match or (field == "releasetype" and matches_pattern(matcher.pattern, tags.releasetype))
            match = match or (field == "genre" and any(matches_pattern(matcher.pattern, x) for x in tags.genre))
            match = match or (field == "label" and any(matches_pattern(matcher.pattern, x) for x in tags.label))
            match = match or (field == "trackartist[main]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.albumartists.main))
            match = match or (field == "trackartist[guest]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.albumartists.guest))
            match = match or (field == "trackartist[remixer]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.albumartists.remixer))
            match = match or (field == "trackartist[producer]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.albumartists.producer))
            match = match or (field == "trackartist[composer]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.albumartists.composer))
            match = match or (field == "trackartist[djmixer]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.albumartists.djmixer))
            match = match or (field == "albumartist[main]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.albumartists.main))
            match = match or (field == "albumartist[guest]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.albumartists.guest))
            match = match or (field == "albumartist[remixer]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.albumartists.remixer))
            match = match or (field == "albumartist[producer]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.albumartists.producer))
            match = match or (field == "albumartist[composer]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.albumartists.composer))
            match = match or (field == "albumartist[djmixer]" and any(matches_pattern(matcher.pattern, x.name) for x in tags.albumartists.djmixer))
            # fmt: on

            # If there is a match, check to see if the track is matched by one of the ignore values.
            # If it is ignored, skip the result entirely.
            if match and ignore:
                skip = False
                for i in ignore:
                    # fmt: off
                    skip = skip or (field == "tracktitle" and matches_pattern(i.pattern, tags.title))
                    skip = skip or (field == "year" and matches_pattern(i.pattern, tags.year))
                    skip = skip or (field == "tracknumber" and matches_pattern(i.pattern, tags.tracknumber))
                    skip = skip or (field == "tracktotal" and matches_pattern(i.pattern, tags.tracktotal))
                    skip = skip or (field == "discnumber" and matches_pattern(i.pattern, tags.discnumber))
                    skip = skip or (field == "disctotal" and matches_pattern(i.pattern, tags.disctotal))
                    skip = skip or (field == "albumtitle" and matches_pattern(i.pattern, tags.album))
                    skip = skip or (field == "releasetype" and matches_pattern(i.pattern, tags.releasetype))
                    skip = skip or (field == "genre" and any(matches_pattern(i.pattern, x) for x in tags.genre))
                    skip = skip or (field == "label" and any(matches_pattern(i.pattern, x) for x in tags.label))
                    skip = skip or (field == "trackartist[main]" and any(matches_pattern(i.pattern, x.name) for x in tags.albumartists.main))
                    skip = skip or (field == "trackartist[guest]" and any(matches_pattern(i.pattern, x.name) for x in tags.albumartists.guest))
                    skip = skip or (field == "trackartist[remixer]" and any(matches_pattern(i.pattern, x.name) for x in tags.albumartists.remixer))
                    skip = skip or (field == "trackartist[producer]" and any(matches_pattern(i.pattern, x.name) for x in tags.albumartists.producer))
                    skip = skip or (field == "trackartist[composer]" and any(matches_pattern(i.pattern, x.name) for x in tags.albumartists.composer))
                    skip = skip or (field == "trackartist[djmixer]" and any(matches_pattern(i.pattern, x.name) for x in tags.albumartists.djmixer))
                    skip = skip or (field == "albumartist[main]" and any(matches_pattern(i.pattern, x.name) for x in tags.albumartists.main))
                    skip = skip or (field == "albumartist[guest]" and any(matches_pattern(i.pattern, x.name) for x in tags.albumartists.guest))
                    skip = skip or (field == "albumartist[remixer]" and any(matches_pattern(i.pattern, x.name) for x in tags.albumartists.remixer))
                    skip = skip or (field == "albumartist[producer]" and any(matches_pattern(i.pattern, x.name) for x in tags.albumartists.producer))
                    skip = skip or (field == "albumartist[composer]" and any(matches_pattern(i.pattern, x.name) for x in tags.albumartists.composer))
                    skip = skip or (field == "albumartist[djmixer]" and any(matches_pattern(i.pattern, x.name) for x in tags.albumartists.djmixer))
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


Changes = tuple[str, str | int | None | list[str], str | int | None | list[str]]


def execute_metadata_actions(
    c: Config,
    actions: list[MetadataAction],
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

    actionable_audiotags: list[tuple[AudioTags, list[Changes]]] = []
    for tags in audiotags:
        origtags = copy.deepcopy(tags)
        potential_changes: list[Changes] = []
        for act in actions:
            fields_to_update = act.tags
            for field in fields_to_update:
                # fmt: off
                if field == "tracktitle":
                    tags.title = execute_single_action(act, tags.title)
                    potential_changes.append(("title", origtags.title, tags.title))
                elif field == "year":
                    v = execute_single_action(act, tags.year)
                    try:
                        tags.year = int(v) if v else None
                    except ValueError as e:
                        raise InvalidReplacementValueError(
                            f"Failed to assign new value {v} to year: value must be integer"
                        ) from e
                    potential_changes.append(("year", origtags.year, tags.year))
                elif field == "tracknumber":
                    tags.tracknumber = execute_single_action(act, tags.tracknumber)
                    potential_changes.append(("tracknumber", origtags.tracknumber, tags.tracknumber))
                elif field == "discnumber":
                    tags.discnumber = execute_single_action(act, tags.discnumber)
                    potential_changes.append(("discnumber", origtags.discnumber, tags.discnumber))
                elif field == "albumtitle":
                    tags.album = execute_single_action(act, tags.album)
                    potential_changes.append(("album", origtags.album, tags.album))
                elif field == "releasetype":
                    tags.releasetype = execute_single_action(act, tags.releasetype) or "unknown"
                    potential_changes.append(("releasetype", origtags.releasetype, tags.releasetype))
                elif field == "genre":
                    tags.genre = execute_multi_value_action(act, tags.genre)
                    potential_changes.append(("genre", origtags.genre, tags.genre))
                elif field == "label":
                    tags.label = execute_multi_value_action(act, tags.label)
                    potential_changes.append(("label", origtags.label, tags.label))
                elif field == "trackartist[main]":
                    tags.trackartists.main = artists(execute_multi_value_action(act, names(tags.trackartists.main)))
                    potential_changes.append(("trackartist[main]", names(origtags.trackartists.main), names(tags.trackartists.main)))
                elif field == "trackartist[guest]":
                    tags.trackartists.guest = artists(execute_multi_value_action(act, names(tags.trackartists.guest)))
                    potential_changes.append(("trackartist[guest]", names(origtags.trackartists.guest), names(tags.trackartists.guest)))
                elif field == "trackartist[remixer]":
                    tags.trackartists.remixer = artists(execute_multi_value_action(act, names(tags.trackartists.remixer)))
                    potential_changes.append(("trackartist[remixer]", names(origtags.trackartists.remixer), names(tags.trackartists.remixer)))
                elif field == "trackartist[producer]":
                    tags.trackartists.producer = artists(execute_multi_value_action(act, names(tags.trackartists.producer)))
                    potential_changes.append(("trackartist[producer]", names(origtags.trackartists.producer), names(tags.trackartists.producer)))
                elif field == "trackartist[composer]":
                    tags.trackartists.composer = artists(execute_multi_value_action(act, names(tags.trackartists.composer)))
                    potential_changes.append(("trackartist[composer]", names(origtags.trackartists.composer), names(tags.trackartists.composer)))
                elif field == "trackartist[djmixer]":
                    tags.trackartists.djmixer = artists(execute_multi_value_action(act, names(tags.trackartists.djmixer)))
                    potential_changes.append(("trackartist[djmixer]", names(origtags.trackartists.djmixer), names(tags.trackartists.djmixer)))
                elif field == "albumartist[main]":
                    tags.albumartists.main = artists(execute_multi_value_action(act, names(tags.albumartists.main)))
                    potential_changes.append(("albumartist[main]", names(origtags.albumartists.main), names(tags.albumartists.main)))
                elif field == "albumartist[guest]":
                    tags.albumartists.guest = artists(execute_multi_value_action(act, names(tags.albumartists.guest)))
                    potential_changes.append(("albumartist[guest]", names(origtags.albumartists.guest), names(tags.albumartists.guest)))
                elif field == "albumartist[remixer]":
                    tags.albumartists.remixer = artists(execute_multi_value_action(act, names(tags.albumartists.remixer)))
                    potential_changes.append(("albumartist[remixer]", names(origtags.albumartists.remixer), names(tags.albumartists.remixer)))
                elif field == "albumartist[producer]":
                    tags.albumartists.producer = artists(execute_multi_value_action(act, names(tags.albumartists.producer)) )
                    potential_changes.append(("albumartist[producer]", names(origtags.albumartists.producer), names(tags.albumartists.producer)))
                elif field == "albumartist[composer]":
                    tags.albumartists.composer = artists(execute_multi_value_action(act, names(tags.albumartists.composer)) )
                    potential_changes.append(("albumartist[composer]", names(origtags.albumartists.composer), names(tags.albumartists.composer)))
                elif field == "albumartist[djmixer]":
                    tags.albumartists.djmixer = artists(execute_multi_value_action(act, names(tags.albumartists.djmixer)))
                    potential_changes.append(("albumartist[djmixer]", names(origtags.albumartists.djmixer), names(tags.albumartists.djmixer)))
                # fmt: on

        # Compute real changes by diffing the tags, and then store.
        changes = [(x, y, z) for x, y, z in potential_changes if y != z]
        if changes:
            actionable_audiotags.append((tags, changes))
        else:
            logger.debug(f"Skipping matched track {tags.path}: no changes calculated off tags")
    if not actionable_audiotags:
        click.secho("No matching tracks found", dim=True, italic=True)
        click.echo()
        return

    # === Step 4: Display changes and ask for user confirmation ===

    # Compute the text to display:
    todisplay: list[tuple[str, list[Changes]]] = []
    maxpathwidth = 0
    for tags, changes in actionable_audiotags:
        pathtext = str(tags.path).removeprefix(str(c.music_source_dir) + "/")
        if len(pathtext) >= 120:
            pathtext = pathtext[:59] + ".." + pathtext[-59:]
        maxpathwidth = max(maxpathwidth, len(pathtext))
        todisplay.append((pathtext, changes))

    # And then display it.
    for pathtext, changes in todisplay:
        click.secho(pathtext, underline=True)
        for name, old, new in changes:
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
    if confirm_yes:
        click.echo()
        if len(actionable_audiotags) > enter_number_to_confirm_above_count:
            while True:
                userconfirmation = click.prompt(
                    f"Write changes to {len(actionable_audiotags)} tracks? Enter {click.style(len(actionable_audiotags), bold=True)} to confirm (or 'no' to abort)"
                )
                if userconfirmation == "no":
                    logger.debug("Aborting planned tag changes after user confirmation")
                    return
                if userconfirmation == str(len(actionable_audiotags)):
                    click.echo()
                    break
        else:
            if not click.confirm(
                f"Write changes to {click.style(len(actionable_audiotags), bold=True)} tracks?",
                default=True,
                prompt_suffix="",
            ):
                logger.debug("Aborting planned tag changes after user confirmation")
                return
            click.echo()

    # === Step 5: Flush writes to disk ===

    logger.info(
        f"Writing tag changes for actions {' '.join([shlex.quote(str(a)) for a in actions])}"
    )
    changed_release_ids: set[str] = set()
    for tags, changes in actionable_audiotags:
        if tags.release_id:
            changed_release_ids.add(tags.release_id)
        pathtext = str(tags.path).removeprefix(str(c.music_source_dir) + "/")
        logger.debug(
            f"Attempting to write {pathtext} changes: {' //// '.join([str(x)+' -> '+str(y) for _, x, y in changes])}"
        )
        tags.flush()
        logger.info(f"Wrote tag changes to {pathtext}")

    click.echo()
    click.echo(f"Applied tag changes to {len(actionable_audiotags)} tracks!")

    # == Step 6: Trigger cache update ===

    click.echo()
    source_paths = [r.source_path for r in list_releases(c, list(changed_release_ids))]
    update_cache_for_releases(c, source_paths)


def matches_pattern(pattern: MatcherPattern, value: str | int | None) -> bool:
    value = str(value) if value is not None else ""

    needle = pattern.pattern
    haystack = value
    if pattern.case_insensitive:
        needle = needle.lower()
        haystack = haystack.lower()

    strictstart = needle.startswith("^")
    strictend = needle.endswith("$")
    if strictstart and strictend:
        return haystack == needle[1:-1]
    if strictstart:
        return haystack.startswith(needle[1:])
    if strictend:
        return haystack.endswith(needle[:-1])
    return needle in haystack


# Factor out the logic for executing an action on a single-value tag and a multi-value tag.
def execute_single_action(action: MetadataAction, value: str | int | None) -> str | None:
    if action.pattern and not matches_pattern(action.pattern, value):
        return str(value)

    bhv = action.behavior
    strvalue = str(value) if value is not None else None

    if isinstance(bhv, ReplaceAction):
        return bhv.replacement
    elif isinstance(bhv, SedAction):
        if strvalue is None:
            return None
        return bhv.src.sub(bhv.dst, strvalue)
    elif isinstance(bhv, DeleteAction):
        return None
    raise RoseError(
        f"Invalid action {type(bhv)} for single-value tag: Should have been caught in parsing"
    )


def execute_multi_value_action(
    action: MetadataAction,
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
    matcher: MetadataMatcher,
) -> list[FastSearchResult]:
    """Basically the same thing as fast_search_for_matching_tracks but with releases."""
    time_start = time.time()
    if track_tags := [t for t in matcher.tags if t not in RELEASE_TAGS]:
        # But allow an exception if both trackartist and albumartist are defined: means a shorthand
        # was used. Just ignore trackartist.
        if any(t.startswith("albumartist") for t in matcher.tags):
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
    logger.debug(
        f"Matched {len(results)} releases from the read cache in {time.time() - time_start} seconds"
    )
    return results


def filter_track_false_positives_using_read_cache(
    matcher: MetadataMatcher,
    tracks: list[CachedTrack],
) -> list[CachedTrack]:
    time_start = time.time()
    rval = []
    for t in tracks:
        for field in matcher.tags:
            match = False
            # fmt: off
            match = match or (field == "tracktitle" and matches_pattern(matcher.pattern, t.tracktitle))
            match = match or (field == "year" and matches_pattern(matcher.pattern, t.release.year))
            match = match or (field == "tracknumber" and matches_pattern(matcher.pattern, t.tracknumber))
            match = match or (field == "tracktotal" and matches_pattern(matcher.pattern, t.tracktotal))
            match = match or (field == "discnumber" and matches_pattern(matcher.pattern, t.discnumber))
            match = match or (field == "disctotal" and matches_pattern(matcher.pattern, t.disctotal))
            match = match or (field == "albumtitle" and matches_pattern(matcher.pattern, t.release.albumtitle))
            match = match or (field == "releasetype" and matches_pattern(matcher.pattern, t.release.releasetype))
            match = match or (field == "genre" and any(matches_pattern(matcher.pattern, x) for x in t.release.genres))
            match = match or (field == "label" and any(matches_pattern(matcher.pattern, x) for x in t.release.labels))
            match = match or (field == "trackartist[main]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.main))
            match = match or (field == "trackartist[guest]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.guest))
            match = match or (field == "trackartist[remixer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.remixer))
            match = match or (field == "trackartist[producer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.producer))
            match = match or (field == "trackartist[composer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.composer))
            match = match or (field == "trackartist[djmixer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.djmixer))
            match = match or (field == "albumartist[main]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.albumartists.main))
            match = match or (field == "albumartist[guest]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.albumartists.guest))
            match = match or (field == "albumartist[remixer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.albumartists.remixer))
            match = match or (field == "albumartist[producer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.albumartists.producer))
            match = match or (field == "albumartist[composer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.albumartists.composer))
            match = match or (field == "albumartist[djmixer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.albumartists.djmixer))
            # fmt: on
            if match:
                rval.append(t)
                break
    logger.debug(
        f"Filtered {len(tracks)} tracks down to {len(rval)} tracks in {time.time() - time_start} seconds"
    )
    return rval


def filter_release_false_positives_using_read_cache(
    matcher: MetadataMatcher,
    releases: list[CachedRelease],
) -> list[CachedRelease]:
    time_start = time.time()
    rval = []
    for r in releases:
        for field in matcher.tags:
            match = False
            # Only attempt to match the release tags; ignore track tags.
            # fmt: off
            match = match or (field == "year" and matches_pattern(matcher.pattern, r.year))
            match = match or (field == "albumtitle" and matches_pattern(matcher.pattern, r.albumtitle))
            match = match or (field == "releasetype" and matches_pattern(matcher.pattern, r.releasetype))
            match = match or (field == "genre" and any(matches_pattern(matcher.pattern, x) for x in r.genres))
            match = match or (field == "label" and any(matches_pattern(matcher.pattern, x) for x in r.labels))
            match = match or (field == "albumartist[main]" and any(matches_pattern(matcher.pattern, x.name) for x in r.albumartists.main))
            match = match or (field == "albumartist[guest]" and any(matches_pattern(matcher.pattern, x.name) for x in r.albumartists.guest))
            match = match or (field == "albumartist[remixer]" and any(matches_pattern(matcher.pattern, x.name) for x in r.albumartists.remixer))
            match = match or (field == "albumartist[producer]" and any(matches_pattern(matcher.pattern, x.name) for x in r.albumartists.producer))
            match = match or (field == "albumartist[composer]" and any(matches_pattern(matcher.pattern, x.name) for x in r.albumartists.composer))
            match = match or (field == "albumartist[djmixer]" and any(matches_pattern(matcher.pattern, x.name) for x in r.albumartists.djmixer))
            # fmt: on
            if match:
                rval.append(r)
                break
    logger.debug(
        f"Filtered {len(releases)} releases down to {len(rval)} releases in {time.time() - time_start} seconds"
    )
    return rval
