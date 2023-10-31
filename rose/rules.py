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

ALL_TAGS: list[Tag] = [
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

    def __str__(self) -> str:
        r = ",".join(self.tags)
        r += ":"
        r += self.matcher.replace(":", r"\:")
        r += ":"
        if isinstance(self.action, ReplaceAction):
            r += "replace:"
            r += self.action.replacement
        elif isinstance(self.action, ReplaceAllAction):
            r += "replaceall:"
            r += ";".join(self.action.replacement)
        elif isinstance(self.action, SedAction):
            r += "sed:"
            r += str(self.action.src.pattern).replace(":", r"\:")
            r += ":"
            r += self.action.dst.replace(":", r"\:")
        elif isinstance(self.action, SplitAction):
            r += "split:"
            r += self.action.delimiter
        elif isinstance(self.action, DeleteAction):
            r += "delete"
        return r


def execute_stored_rules(c: Config, confirm_yes: bool = False) -> None:
    rules: list[UpdateRule] = []
    for rule in rules:
        logger.info(f'Executing stored metadata rule "{rule}"')
        execute_rule(c, rule, confirm_yes)


def execute_rule(
    c: Config,
    rule: UpdateRule,
    confirm_yes: bool = False,
    enter_number_to_confirm_above_count: int = 25,
) -> None:
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
        strictstart = rule.matcher.startswith("^")
        strictend = rule.matcher.endswith("$")
        if strictstart and strictend:
            return x == rule.matcher[1:-1]
        if strictstart:
            return x.startswith(rule.matcher[1:])
        if strictend:
            return x.endswith(rule.matcher[:-1])
        return matchrule in x

    # 2. Find tracks to update.
    # We dynamically construct a SQL query that tests the matcher SQL
    # string against the specified tags.
    query = """
        SELECT DISTINCT t.source_path
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
            query += r" OR t.title LIKE ? ESCAPE '\'"
            args.append(matchsql)
        if field == "year":
            query += (
                r" OR COALESCE(CAST(r.release_year AS TEXT), '') LIKE ? ESCAPE '\'"  # noqa: E501
            )
            args.append(matchsql)
        if field == "tracknumber":
            query += r" OR t.track_number LIKE ? ESCAPE '\'"
            args.append(matchsql)
        if field == "discnumber":
            query += r" OR t.disc_number LIKE ? ESCAPE '\'"
            args.append(matchsql)
        if field == "albumtitle":
            query += r" OR r.title LIKE ? ESCAPE '\'"
            args.append(matchsql)
        if field == "releasetype":
            query += r" OR r.release_type LIKE ? ESCAPE '\'"
            args.append(matchsql)
        # For genres, labels, and artists, because SQLite lacks arrays, we create a string like
        # `\\ val1 \\ val2 \\` and match on `\\ {matcher} \\`.
        if field == "genre":
            query += r" OR rg.genre LIKE ? ESCAPE '\'"
            args.append(rf" \\ {matchsql} \\ ")
        if field == "label":
            query += r" OR rl.label LIKE ? ESCAPE '\'"
            args.append(rf" \\ {matchsql} \\ ")
        if field == "artist":
            query += r" OR ra.artist LIKE ? ESCAPE '\'"
            args.append(rf" \\ {matchsql} \\ ")
            query += r" OR ta.artist LIKE ? ESCAPE '\'"
            args.append(rf" \\ {matchsql} \\ ")
    query += " ORDER BY t.source_path"
    logger.debug(f"Constructed matching query {query} with args {args}")
    # And then execute the SQL query. Note that we don't pull the tag values here. This query is
    # only used to identify the matching tracks. Afterwards, we will read each track's tags from
    # disk and apply the action on those tag values.
    with connect(c) as conn:
        track_paths = [Path(row["source_path"]).resolve() for row in conn.execute(query, args)]
    logger.debug(f"Matched {len(track_paths)} tracks from the read cache")

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
        if isinstance(rule.action, ReplaceAllAction):
            return rule.action.replacement

        rval: list[str] = []
        for v in values:
            if not matches_rule(v):
                rval.append(v)
                continue
            with contextlib.suppress(InvalidRuleActionError):
                if newv := execute_single_action(v):
                    rval.append(newv)
                continue
            if isinstance(rule.action, SplitAction):
                for newv in v.split(rule.action.delimiter):
                    if newv:
                        rval.append(newv.strip())
                continue
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
        origtags = copy.deepcopy(tags)
        changes: list[str] = []
        for field in rule.tags:
            if field == "tracktitle":
                tags.title = execute_single_action(tags.title)
                if tags.title != origtags.title:
                    changes.append(f"tracktitle: {origtags.title} -> {tags.title}")
            if field == "year":
                v = execute_single_action(str(tags.year) if tags.year is not None else None)
                try:
                    tags.year = int(v) if v else None
                except ValueError as e:
                    raise InvalidReplacementValueError(
                        f"Failed to assign new value {v} to release_year: value must be integer"
                    ) from e
                if tags.year != origtags.year:
                    changes.append(f"year: {origtags.year} -> {tags.year}")
            if field == "tracknumber":
                tags.track_number = execute_single_action(tags.track_number)
                if tags.track_number != origtags.track_number:
                    changes.append(f"tracknumber: {origtags.track_number} -> {tags.track_number}")
            if field == "discnumber":
                tags.disc_number = execute_single_action(tags.disc_number)
                if tags.disc_number != origtags.disc_number:
                    changes.append(f"discnumber: {origtags.disc_number} -> {tags.disc_number}")
            if field == "albumtitle":
                tags.album = execute_single_action(tags.album)
                if tags.album != origtags.album:
                    changes.append(f"album: {origtags.album} -> {tags.album}")
            if field == "releasetype":
                tags.release_type = execute_single_action(tags.release_type) or "unknown"
                if tags.release_type != origtags.release_type:
                    changes.append(f"releasetype: {origtags.release_type} -> {tags.release_type}")
            if field == "genre":
                tags.genre = execute_multi_value_action(tags.genre)
                if tags.genre != origtags.genre:
                    changes.append(f'genre: {";".join(origtags.genre)} -> {";".join(tags.genre)}')
            if field == "label":
                tags.label = execute_multi_value_action(tags.label)
                if tags.label != origtags.label:
                    changes.append(f'label: {";".join(origtags.label)} -> {";".join(tags.label)}')
            if field == "artist":
                tags.artists.main = execute_multi_value_action(tags.artists.main)
                if tags.artists.main != origtags.artists.main:
                    changes.append(
                        f'artist.main: {";".join(origtags.artists.main)} -> '
                        f'{";".join(tags.artists.main)}'
                    )
                tags.artists.guest = execute_multi_value_action(tags.artists.guest)
                if tags.artists.guest != origtags.artists.guest:
                    changes.append(
                        f'artist.guest: {";".join(origtags.artists.guest)} -> '
                        f'{";".join(tags.artists.guest)}'
                    )
                tags.artists.remixer = execute_multi_value_action(tags.artists.remixer)
                if tags.artists.remixer != origtags.artists.remixer:
                    changes.append(
                        f'artist.remixer: {";".join(origtags.artists.remixer)} -> '
                        f'{";".join(tags.artists.remixer)}'
                    )
                tags.artists.producer = execute_multi_value_action(tags.artists.producer)
                if tags.artists.producer != origtags.artists.producer:
                    changes.append(
                        f'artist.producer: {";".join(origtags.artists.producer)} '
                        f'{";".join(tags.artists.producer)}'
                    )
                tags.artists.composer = execute_multi_value_action(tags.artists.composer)
                if tags.artists.composer != origtags.artists.composer:
                    changes.append(
                        f'artist.composer: {";".join(origtags.artists.composer)} '
                        f'{";".join(tags.artists.composer)}'
                    )
                tags.artists.djmixer = execute_multi_value_action(tags.artists.djmixer)
                if tags.artists.djmixer != origtags.artists.djmixer:
                    changes.append(
                        f'artist.djmixer: {";".join(origtags.artists.djmixer)} '
                        f'{";".join(tags.artists.djmixer)}'
                    )
                tags.album_artists.main = execute_multi_value_action(tags.album_artists.main)
                if tags.album_artists.main != origtags.album_artists.main:
                    changes.append(
                        f'album_artist.main: {";".join(origtags.album_artists.main)} '
                        f'{";".join(tags.album_artists.main)}'
                    )
                tags.album_artists.guest = execute_multi_value_action(tags.album_artists.guest)
                if tags.album_artists.guest != origtags.album_artists.guest:
                    changes.append(
                        f'album_artist.guest: {";".join(origtags.album_artists.guest)} '
                        f'{";".join(tags.album_artists.guest)}'
                    )
                tags.album_artists.remixer = execute_multi_value_action(tags.album_artists.remixer)
                if tags.album_artists.remixer != origtags.album_artists.remixer:
                    changes.append(
                        f'album_artist.remixer: {";".join(origtags.album_artists.remixer)} -> '
                        f'{";".join(tags.album_artists.remixer)}'
                    )
                tags.album_artists.producer = execute_multi_value_action(
                    tags.album_artists.producer
                )
                if tags.album_artists.producer != origtags.album_artists.producer:
                    changes.append(
                        f'album_artist.producer: {";".join(origtags.album_artists.producer)} -> '
                        f'{";".join(tags.album_artists.producer)}'
                    )
                tags.album_artists.composer = execute_multi_value_action(
                    tags.album_artists.composer
                )
                if tags.album_artists.composer != origtags.album_artists.composer:
                    changes.append(
                        f'album_artist.composer: {";".join(origtags.album_artists.composer)} -> '
                        f'{";".join(tags.album_artists.composer)}'
                    )
                tags.album_artists.djmixer = execute_multi_value_action(tags.album_artists.djmixer)
                if tags.album_artists.djmixer != origtags.album_artists.djmixer:
                    changes.append(
                        f'album_artists.djmixer: {";".join(origtags.album_artists.djmixer)} -> '
                        f'{";".join(tags.album_artists.djmixer)}'
                    )

        relativepath = str(tpath).lstrip(str(c.music_source_dir))
        if changes:
            changelog = f"[{relativepath}] {' | '.join(changes)}"
            if confirm_yes:
                print(changelog)
            else:
                logger.info(f"Scheduling tag update: {changelog}")
            audiotags.append(tags)
        else:
            logger.debug(f"Skipping relative path {relativepath}: no changes calculated off tags")

    if confirm_yes:
        if len(audiotags) > enter_number_to_confirm_above_count:
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


def parse_toml_rule(c: Config, toml: str) -> UpdateRule:  # noqa
    return UpdateRule(tags=["tracktitle"], matcher="", action=ReplaceAction(replacement=""))


def parse_dsl_rule(c: Config, text: str) -> UpdateRule:  # noqa
    return UpdateRule(tags=["tracktitle"], matcher="", action=ReplaceAction(replacement=""))
