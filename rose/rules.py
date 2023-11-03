"""
The rules module implements the Rules Engine for updating metadata. The rules engine accepts,
previews, and executes rules.
"""

import copy
import logging
from pathlib import Path
from typing import Any

import click

from rose.audiotags import AudioTags
from rose.cache import (
    connect,
    get_release_source_paths_from_ids,
    update_cache_for_releases,
)
from rose.common import RoseError, uniq
from rose.config import Config
from rose.rule_parser import (
    AddAction,
    DeleteAction,
    MetadataAction,
    MetadataRule,
    ReplaceAction,
    SedAction,
    SplitAction,
)

logger = logging.getLogger(__name__)


class InvalidRuleActionError(RoseError):
    pass


class InvalidReplacementValueError(RoseError):
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

    # === Step 1: Fast search for matching files ===

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
    matchsqlstr = rule.matcher.pattern
    if matchsqlstr.startswith("^"):
        matchsqlstr = matchsqlstr[1:]
    if matchsqlstr.endswith("$"):
        matchsqlstr = matchsqlstr[:-1]
    # Construct the SQL string for the matcher. Escape double quotes in the match string.
    matchsql = "¬".join(matchsqlstr).replace('"', '""')
    # NEAR restricts the query such that the # of tokens in between the first and last tokens of the
    # matched substring must be less than or equal to a given number. For us, that number is
    # len(matchsqlstr) - 2, as we subtract the first and last characters.
    matchsql = f'NEAR("{matchsql}", {max(0, len(matchsqlstr)-2)})'
    logger.debug(f"Converted match {rule.matcher=} to {matchsql=}")

    # Build the query to fetch a superset of tracks to attempt to execute the rules against. Note
    # that we directly use string interpolation here instead of prepared queries, because we are
    # constructing a complex match string and everything is escaped and spaced-out with a random
    # paragraph character, so there's no risk of SQL being interpreted.
    ftsquery = f"{{{' '.join(rule.matcher.tags)}}} : {matchsql}"
    query = f"""
        SELECT DISTINCT t.source_path
        FROM rules_engine_fts
        JOIN tracks t ON rules_engine_fts.rowid = t.rowid
        WHERE rules_engine_fts MATCH '{ftsquery}'
        ORDER BY t.source_path
    """
    logger.debug(f"Constructed matching query {query}")
    # And then execute the SQL query. Note that we don't pull the tag values here. This query is
    # only used to identify the matching tracks. Afterwards, we will read each track's tags from
    # disk and apply the action on those tag values.
    with connect(c) as conn:
        track_paths = [Path(row["source_path"]).resolve() for row in conn.execute(query)]
    logger.debug(f"Matched {len(track_paths)} tracks from the read cache")
    if not track_paths:
        click.secho("No matching tracks found", dim=True, italic=True)
        click.echo()
        return

    # === Step 2: Filter out false positives ===

    matcher_audiotags: list[AudioTags] = []
    for tpath in track_paths:
        tags = AudioTags.from_file(tpath)
        for field in rule.matcher.tags:
            match = False
            # fmt: off
            match = match or (field == "tracktitle" and matches_pattern(rule.matcher.pattern, tags.title))  # noqa: E501
            match = match or (field == "year" and matches_pattern(rule.matcher.pattern, tags.year))  # noqa: E501
            match = match or (field == "tracknumber" and matches_pattern(rule.matcher.pattern, tags.tracknumber))  # noqa: E501
            match = match or (field == "discnumber" and matches_pattern(rule.matcher.pattern, tags.discnumber))  # noqa: E501
            match = match or (field == "albumtitle" and matches_pattern(rule.matcher.pattern, tags.album))  # noqa: E501
            match = match or (field == "releasetype" and matches_pattern(rule.matcher.pattern, tags.releasetype))  # noqa: E501
            match = match or (field == "genre" and any(matches_pattern(rule.matcher.pattern, x) for x in tags.genre))  # noqa: E501
            match = match or (field == "label" and any(matches_pattern(rule.matcher.pattern, x) for x in tags.label))  # noqa: E501
            match = match or (field == "trackartist" and any(matches_pattern(rule.matcher.pattern, x) for x in tags.trackartists.all))  # noqa: E501
            match = match or (field == "albumartist" and any(matches_pattern(rule.matcher.pattern, x) for x in tags.albumartists.all))  # noqa: E501
            # fmt: on
            if match:
                matcher_audiotags.append(tags)
                break
    if not matcher_audiotags:
        click.secho("No matching tracks found", dim=True, italic=True)
        click.echo()
        return

    # === Step 3: Prepare updates on in-memory tags ===

    Changes = tuple[str, Any, Any]  # (old, new)  # noqa: N806
    actionable_audiotags: list[tuple[AudioTags, list[Changes]]] = []
    for tags in matcher_audiotags:
        origtags = copy.deepcopy(tags)
        potential_changes: list[Changes] = []
        for act in rule.actions:
            fields_to_update = act.tags
            if fields_to_update == "matched":
                fields_to_update = rule.matcher.tags
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
                    potential_changes.append(("tracknumber", origtags.tracknumber, tags.tracknumber))  # noqa: E501
                elif field == "discnumber":
                    tags.discnumber = execute_single_action(act, tags.discnumber)
                    potential_changes.append(("discnumber", origtags.discnumber, tags.discnumber))  # noqa: E501
                elif field == "albumtitle":
                    tags.album = execute_single_action(act, tags.album)
                    potential_changes.append(("album", origtags.album, tags.album))
                elif field == "releasetype":
                    tags.releasetype = execute_single_action(act, tags.releasetype) or "unknown"
                    potential_changes.append(("releasetype", origtags.releasetype, tags.releasetype))  # noqa: E501
                elif field == "genre":
                    tags.genre = execute_multi_value_action(act, tags.genre)
                    potential_changes.append(("genre", origtags.genre, tags.genre))
                elif field == "label":
                    tags.label = execute_multi_value_action(act, tags.label)
                    potential_changes.append(("label", origtags.label, tags.label))
                elif field == "trackartist":
                    tags.trackartists.main = execute_multi_value_action(act, tags.trackartists.main)
                    potential_changes.append(("trackartist[main]", origtags.trackartists.main, tags.trackartists.main))  # noqa: E501
                    tags.trackartists.guest = execute_multi_value_action(act, tags.trackartists.guest)
                    potential_changes.append(("trackartist[guest]", origtags.trackartists.guest, tags.trackartists.guest))  # noqa: E501
                    tags.trackartists.remixer = execute_multi_value_action(act, tags.trackartists.remixer)
                    potential_changes.append(("trackartist[remixer]", origtags.trackartists.remixer, tags.trackartists.remixer))  # noqa: E501
                    tags.trackartists.producer = execute_multi_value_action(act, tags.trackartists.producer)
                    potential_changes.append(("trackartist[producer]", origtags.trackartists.producer, tags.trackartists.producer))  # noqa: E501
                    tags.trackartists.composer = execute_multi_value_action(act, tags.trackartists.composer)
                    potential_changes.append(("trackartist[composer]", origtags.trackartists.composer, tags.trackartists.composer))  # noqa: E501
                    tags.trackartists.djmixer = execute_multi_value_action(act, tags.trackartists.djmixer)
                    potential_changes.append(("trackartist[djmixer]", origtags.trackartists.djmixer, tags.trackartists.djmixer))  # noqa: E501
                elif field == "albumartist":
                    tags.albumartists.main = execute_multi_value_action(act, tags.albumartists.main)  # noqa: E501
                    potential_changes.append(("albumartist[main]", origtags.albumartists.main, tags.albumartists.main))  # noqa: E501
                    tags.albumartists.guest = execute_multi_value_action(act, tags.albumartists.guest)  # noqa: E501
                    potential_changes.append(("albumartist[guest]", origtags.albumartists.guest, tags.albumartists.guest))  # noqa: E501
                    tags.albumartists.remixer = execute_multi_value_action(act, tags.albumartists.remixer)  # noqa: E501
                    potential_changes.append(("albumartist[remixer]", origtags.albumartists.remixer, tags.albumartists.remixer))  # noqa: E501
                    tags.albumartists.producer = execute_multi_value_action(act, tags.albumartists.producer)  # noqa: E501
                    potential_changes.append(("albumartist[producer]", origtags.albumartists.producer, tags.albumartists.producer))  # noqa: E501
                    tags.albumartists.composer = execute_multi_value_action(act, tags.albumartists.composer)  # noqa: E501
                    potential_changes.append(("albumartist[composer]", origtags.albumartists.composer, tags.albumartists.composer))  # noqa: E501
                    tags.albumartists.djmixer = execute_multi_value_action(act, tags.albumartists.djmixer)  # noqa: E501
                    potential_changes.append(("albumartist[djmixer]", origtags.albumartists.djmixer, tags.albumartists.djmixer))  # noqa: E501
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

    logger.info(f"Writing tag changes for rule {rule}")
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
    source_paths = get_release_source_paths_from_ids(c, list(changed_release_ids))
    update_cache_for_releases(c, source_paths)


def matches_pattern(pattern: str, value: str | int | None) -> bool:
    value = str(value) if value is not None else ""
    strictstart = pattern.startswith("^")
    strictend = pattern.endswith("$")
    if strictstart and strictend:
        return value == pattern[1:-1]
    if strictstart:
        return value.startswith(pattern[1:])
    if strictend:
        return value.endswith(pattern[:-1])
    return pattern in value


# Factor out the logic for executing an action on a single-value tag and a multi-value tag.
def execute_single_action(action: MetadataAction, value: str | int | None) -> str | None:
    if action.match_pattern and not matches_pattern(action.match_pattern, value):
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
    raise InvalidRuleActionError(f"Invalid action {type(bhv)} for single-value tag")


def execute_multi_value_action(
    action: MetadataAction,
    values: list[str],
) -> list[str]:
    bhv = action.behavior

    # If match_pattern is specified, check which values match. And if none match, bail out.
    matching_idx = list(range(len(values)))
    if action.match_pattern:
        matching_idx = []
        for i, v in enumerate(values):
            if matches_pattern(action.match_pattern, v):
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
