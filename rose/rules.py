"""
The rules module implements the Rules Engine for updating metadata.

There are 3 major components in this module:

- Rules Engine: A Python function that accepts, previews, and execute rules.
- TOML Parser: Parses TOML-encoded rules and returns the Python dataclass.
- DSL: A small language for defining rules, intended for use in the shell.
"""
import contextlib
import copy
import logging
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Literal

import click

from rose.audiotags import AudioTags
from rose.cache import connect
from rose.common import RoseError
from rose.config import Config

logger = logging.getLogger(__name__)


class InvalidRuleActionError(RoseError):
    pass


class InvalidReplacementValueError(RoseError):
    pass


Tag = Literal[
    "tracktitle",
    "year",
    "tracknumber",
    "discnumber",
    "albumtitle",
    "genre",
    "label",
    "releasetype",
    "artist",
]


@dataclass
class ReplaceAction:
    """
    Replaces the matched tag with `replacement`. For multi-valued tags, only the matched value is
    replaced; the other values are left alone.
    """

    replacement: str


@dataclass
class ReplaceAllAction:
    """Specifically useful for multi-valued tags, replaces all values."""

    replacement: list[str]


@dataclass
class SedAction:
    """
    Executes a regex substitution on a tag value. For multi-valued tags, only the matched tag is
    modified; the other values are left alone.
    """

    src: re.Pattern[str]
    dst: str


@dataclass
class SplitAction:
    """
    Splits a tag into multiple tags on the provided delimiter. For multi-valued tags, only the
    matched tag is split; the other values are left alone.
    """

    delimiter: str


@dataclass
class DeleteAction:
    """
    Deletes the tag value. In a multi-valued tag, only the matched value is deleted; the other
    values are left alone.
    """

    pass


@dataclass
class UpdateRule:
    tags: list[Tag]
    matcher: str
    action: ReplaceAction | ReplaceAllAction | SedAction | SplitAction | DeleteAction


def execute_stored_rules(c: Config) -> None:
    pass


def execute_rule(c: Config, rule: UpdateRule, confirm_yes: bool = False) -> None:
    # 1. Convert the matcher to SQL. We default to a substring search, and support '^$' characters,
    # in the regex style, to match the beginning and end of the string.
    matchsqlstart = ""
    matchrule = rule.matcher
    # If rule starts with ^, hard match the start.
    if matchrule.startswith("^"):
        matchrule = matchrule[1:]
    else:
        matchsqlstart += "%"
    # If rule ends with $, hard match the end.
    matchsqlend = ""
    if matchrule.endswith("$"):
        matchrule = matchrule[:-1]
    else:
        matchsqlend = "%"
    # And escape the match rule.
    matchrule = matchrule.replace("%", r"\%").replace("_", r"\_")
    # Construct the SQL string for the matcher.
    matchsql = matchsqlstart + matchrule + matchsqlend
    logger.debug(f"Converted match {rule.matcher=} to {matchsql=}")

    # And also create a Python function for the matcher. We'll use this in the actual substitutions.
    def matches_rule(x: str) -> bool:
        strictstart = matchrule.startswith("^")
        strictend = matchrule.endswith("$")
        if strictstart and strictend:
            return x == matchrule[1:-1]
        if strictstart:
            return x.startswith(matchrule[1:])
        if strictend:
            return x.endswith(matchrule[:1])
        return matchrule in x

    # 2. Find tracks to update.
    # We dynamically construct a SQL query that tests the matcher SQL
    # string against the specified tags.
    query = """
        SELECT t.source_path
        FROM tracks t
        JOIN releases r ON r.id = t.release_id
        LEFT JOIN releases_genres rg ON rg.release_id = r.id
        LEFT JOIN releases_labels rl ON rg.release_id = r.id
        LEFT JOIN releases_artists ra ON ra.release_id = r.id
        LEFT JOIN tracks_artists ta ON ta.track_id = t.id
        WHERE 1=1
    """
    args: list[str] = []
    for field in rule.tags:
        if field == "tracktitle":
            query += r" AND WHERE t.title LIKE ? ESCAPE '\'"
            args.append(matchsql)
        if field == "year":
            query += r" AND WHERE COALESCE(CAST(r.release_year AS TEXT), '') LIKE ? ESCAPE '\'"  # noqa: E501
            args.append(matchsql)
        if field == "tracknumber":
            query += r" AND WHERE t.track_number LIKE ? ESCAPE '\'"
            args.append(matchsql)
        if field == "discnumber":
            query += r" AND WHERE t.disc_number LIKE ? ESCAPE '\'"
            args.append(matchsql)
        if field == "albumtitle":
            query += r" AND WHERE r.title LIKE ? ESCAPE '\'"
            args.append(matchsql)
        if field == "releasetype":
            query += r" AND WHERE r.release_type LIKE ? ESCAPE '\'"
            args.append(matchsql)
        # For genres, labels, and artists, because SQLite lacks arrays, we create a string like
        # `\\ val1 \\ val2 \\` and match on `\\ {matcher} \\`.
        if field == "genre":
            query += r" AND WHERE rg.genres LIKE ? ESCAPE '\'"
            args.append(rf" \\ {matchsql} \\ ")
        if field == "label":
            query += r" AND WHERE rl.labels LIKE ? ESCAPE '\'"
            args.append(rf" \\ {matchsql} \\ ")
        if field == "artist":
            query += r" AND WHERE ra.artists LIKE ? ESCAPE '\'"
            args.append(rf" \\ {matchsql} \\ ")
            query += r" AND WHERE ta.artists LIKE ? ESCAPE '\'"
            args.append(rf" \\ {matchsql} \\ ")
    logger.debug(f"Constructed matching query {query} with args {args}")
    # And then execute the SQL query. Note that we don't pull the tag values here. This query is
    # only used to identify the matching tracks. Afterwards, we will read each track's tags from
    # disk and apply the action on those tag values.
    with connect(c) as conn:
        track_paths = [Path(row["source_path"]).resolve() for row in conn.execute(query, args)]

    # Factor out the logic for executing an action on a single-value tag and a multi-value tag.
    def execute_single_action(value: str | None) -> str | None:
        if not matches_rule(value or ""):
            return value
        if isinstance(rule.action, ReplaceAction):
            return rule.action.replacement
        elif isinstance(rule.action, SedAction):
            if not value:
                return value
            return rule.action.src.sub(rule.action.dst, str(value or ""))
        elif isinstance(rule.action, DeleteAction):
            return None
        raise InvalidRuleActionError(f"Invalid action {type(rule.action)} for single-value tag")

    def execute_multi_value_action(values: list[str]) -> list[str]:
        rval: list[str] = []
        for v in values:
            if not matches_rule(v):
                continue
            with contextlib.suppress(InvalidRuleActionError):
                if newv := execute_single_action(v):
                    rval.append(newv)
                continue
            if isinstance(rule.action, ReplaceAllAction):
                return rule.action.replacement
            if isinstance(rule.action, SplitAction):
                for newv in v.split(rule.action.delimiter):
                    if newv:
                        rval.append(newv)
            raise InvalidRuleActionError(f"Invalid action {type(rule.action)} for multi-value tag")
        return rval

    # 3. Execute update on tags.
    # We make two passes here to enable preview:
    # - 1st pass: Read all audio files metadata and identify what must be changed. Store changed
    #   audiotags into the `audiotag` list. Print planned changes for user confirmation.
    # - 2nd pass: Flush the changes.
    audiotags: list[AudioTags] = []
    for tpath in track_paths:
        tags = AudioTags.from_file(tpath)
        origtags = copy.copy(AudioTags)
        changes: list[str] = []
        for field in rule.tags:
            if field == "tracktitle":
                tags.title = execute_single_action(tags.title)
                if tags.title != origtags.title:
                    changes.append(f'tracktitle:"{_quote(origtags.title)} -> {_quote(tags.title)}"')
            if field == "year":
                v = execute_single_action(tags.title)
                try:
                    tags.year = int(v) if v else None
                except ValueError as e:
                    raise InvalidReplacementValueError(
                        f"Failed to assign new value {v} to release_year: value must be integer"
                    ) from e
                if tags.year != origtags.year:
                    changes.append(f'year:"{_quote(origtags.year)} -> {_quote(tags.year)}"')
            if field == "tracknumber":
                tags.track_number = execute_single_action(tags.title)
                if tags.track_number != origtags.track_number:
                    changes.append(
                        f'tracknumber:"{_quote(origtags.track_number)} -> '
                        f'{_quote(tags.track_number)}"'
                    )
            if field == "discnumber":
                tags.disc_number = execute_single_action(tags.title)
                if tags.disc_number != origtags.disc_number:
                    changes.append(
                        f'discnumber:"{_quote(origtags.disc_number)} -> {_quote(tags.disc_number)}"'
                    )
            if field == "albumtitle":
                tags.album = execute_single_action(tags.title)
                if tags.album != origtags.album:
                    changes.append(f'album:"{_quote(origtags.album)} -> {_quote(tags.album)}"')
            if field == "releasetype":
                tags.release_type = execute_single_action(tags.title) or "unknown"
                if tags.release_type != origtags.release_type:
                    changes.append(
                        f'releasetype:"{_quote(origtags.release_type)} -> '
                        f'{_quote(tags.release_type)}"'
                    )
            if field == "genre":
                tags.genre = execute_multi_value_action(tags.genre)
                if tags.genre != origtags.genre:
                    changes.append(
                        f'releasetype:"{_quote(";".join(origtags.genre))} -> '
                        f'{_quote(";".join(tags.genre))}"'
                    )
            if field == "label":
                tags.label = execute_multi_value_action(tags.genre)
                if tags.label != origtags.label:
                    changes.append(
                        f'releasetype:"{_quote(";".join(origtags.label))} -> '
                        f'{_quote(";".join(tags.label))}"'
                    )
            if field == "artist":
                tags.artists.main = execute_multi_value_action(tags.artists.main)
                if tags.artists.main != origtags.artists.main:
                    changes.append(
                        f'artists.main:"{_quote(";".join(origtags.artists.main))}" '
                        f'{_quote(";".join(tags.artists.main))}'
                    )
                tags.artists.guest = execute_multi_value_action(tags.artists.guest)
                if tags.artists.guest != origtags.artists.guest:
                    changes.append(
                        f'artists.guest:"{_quote(";".join(origtags.artists.guest))}" '
                        f'{_quote(";".join(tags.artists.guest))}'
                    )
                tags.artists.remixer = execute_multi_value_action(tags.artists.remixer)
                if tags.artists.remixer != origtags.artists.remixer:
                    changes.append(
                        f'artists.remixer:"{_quote(";".join(origtags.artists.remixer))}" '
                        f'{_quote(";".join(tags.artists.remixer))}'
                    )
                tags.artists.producer = execute_multi_value_action(tags.artists.producer)
                if tags.artists.producer != origtags.artists.producer:
                    changes.append(
                        f'artists.producer:"{_quote(";".join(origtags.artists.producer))}" '
                        f'{_quote(";".join(tags.artists.producer))}'
                    )
                tags.artists.composer = execute_multi_value_action(tags.artists.composer)
                if tags.artists.composer != origtags.artists.composer:
                    changes.append(
                        f'artists.composer:"{_quote(";".join(origtags.artists.composer))}" '
                        f'{_quote(";".join(tags.artists.composer))}'
                    )
                tags.artists.djmixer = execute_multi_value_action(tags.artists.djmixer)
                if tags.artists.djmixer != origtags.artists.djmixer:
                    changes.append(
                        f'artists.djmixer:"{_quote(";".join(origtags.artists.djmixer))}" '
                        f'{_quote(";".join(tags.artists.djmixer))}'
                    )
                tags.album_artists.main = execute_multi_value_action(tags.album_artists.main)
                if tags.album_artists.main != origtags.album_artists.main:
                    changes.append(
                        f'album_artists.main:"{_quote(";".join(origtags.album_artists.main))}" '
                        f'{_quote(";".join(tags.album_artists.main))}'
                    )
                tags.album_artists.guest = execute_multi_value_action(tags.album_artists.guest)
                if tags.album_artists.guest != origtags.album_artists.guest:
                    changes.append(
                        f'album_artists.guest:"{_quote(";".join(origtags.album_artists.guest))}" '
                        f'{_quote(";".join(tags.album_artists.guest))}'
                    )
                tags.album_artists.remixer = execute_multi_value_action(tags.album_artists.remixer)
                if tags.album_artists.remixer != origtags.album_artists.remixer:
                    changes.append(
                        "album_artists.remixer:"
                        f'"{_quote(";".join(origtags.album_artists.remixer))}" '
                        f'{_quote(";".join(tags.album_artists.remixer))}'
                    )
                tags.album_artists.producer = execute_multi_value_action(
                    tags.album_artists.producer
                )
                if tags.album_artists.producer != origtags.album_artists.producer:
                    changes.append(
                        "album_artists.producer:"
                        f'"{_quote(";".join(origtags.album_artists.producer))}" '
                        f'{_quote(";".join(tags.album_artists.producer))}'
                    )
                tags.album_artists.composer = execute_multi_value_action(
                    tags.album_artists.composer
                )
                if tags.album_artists.composer != origtags.album_artists.composer:
                    changes.append(
                        "album_artists.composer:"
                        f'"{_quote(";".join(origtags.album_artists.composer))}" '
                        f'{_quote(";".join(tags.album_artists.composer))}'
                    )
                tags.album_artists.djmixer = execute_multi_value_action(tags.album_artists.djmixer)
                if tags.album_artists.djmixer != origtags.album_artists.djmixer:
                    changes.append(
                        "album_artists.djmixer:"
                        f'"{_quote(";".join(origtags.album_artists.djmixer))}" '
                        f'{_quote(";".join(tags.album_artists.djmixer))}'
                    )

        if changes:
            changelog = f"{str(tpath).lstrip(str(c.music_source_dir))}: {' | '.join(changes)}"
            if confirm_yes:
                print(changelog)
            else:
                logger.info(f"Scheduling tag update: {changelog}")
            audiotags.append(tags)

    if confirm_yes:
        if len(audiotags) > 20:
            while True:
                userconfirmation = click.prompt(
                    f"Apply the planned tag changes to {len(audiotags)} tracks? "
                    f"Enter {len(audiotags)} to confirm (or 'no' to abort)"
                )
                if userconfirmation == "no":
                    logger.debug("Aborting planned tag changes after user confirmation")
                    return
                if userconfirmation == str(len(audiotags)):
                    break
        else:
            if not click.confirm(
                f"Apply the planned tag changes to {len(audiotags)} tracks? ",
                default=True,
                prompt_suffix="",
            ):
                logger.debug("Aborting planned tag changes after user confirmation")
                return

    for tags in audiotags:
        logger.info(f"Flushing rule-applied tags for {tags.path}.")
        tags.flush()
    logger.info(f"Successfully flushed all {len(audiotags)} rule-applied tags")


def _quote(x: int | str | None) -> str | int | None:
    """Quote the string if there are spaces in it."""
    if not x or isinstance(x, int):
        return x
    return '"' + x + '"' if " " in x else x


def parse_toml_rule(c: Config, toml: str) -> UpdateRule:
    return UpdateRule(tags=["tracktitle"], matcher="", action=ReplaceAction(replacement=""))


def parse_dsl_rule(c: Config, text: str) -> UpdateRule:
    return UpdateRule(tags=["tracktitle"], matcher="", action=ReplaceAction(replacement=""))
