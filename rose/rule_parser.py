"""
The rule_parser module provides a parser for the rules engine's DSL.

This is split out from the rules engine in order to avoid a dependency cycle between the config
module and the rules module.
"""

from __future__ import annotations

import io
import logging
import re
import shlex
from dataclasses import dataclass
from typing import Literal

import click

from rose.common import RoseError, RoseExpectedError

logger = logging.getLogger(__name__)


class InvalidRuleError(RoseExpectedError):
    pass


class RuleSyntaxError(InvalidRuleError):
    def __init__(self, *, rule_name: str, rule: str, index: int, feedback: str) -> None:
        self.rule_name = rule_name
        self.rule = rule
        self.index = index
        self.feedback = feedback
        super().__init__(str(self))

    def __str__(self) -> str:
        return f"""\
Failed to parse {self.rule_name}, invalid syntax:

    {self.rule}
    {" " * self.index}{click.style("^", fg="red")}
    {" " * self.index}{click.style(self.feedback, bold=True)}
"""


Tag = Literal[
    "tracktitle",
    "trackartist[main]",
    "trackartist[guest]",
    "trackartist[remixer]",
    "trackartist[producer]",
    "trackartist[composer]",
    "trackartist[conductor]",
    "trackartist[djmixer]",
    "tracknumber",
    "tracktotal",
    "discnumber",
    "disctotal",
    "releasetitle",
    "releaseartist[main]",
    "releaseartist[guest]",
    "releaseartist[remixer]",
    "releaseartist[producer]",
    "releaseartist[composer]",
    "releaseartist[conductor]",
    "releaseartist[djmixer]",
    "releasetype",
    "year",
    "genre",
    "label",
]

# Map of a tag to its "resolved" tags. Most tags simply resolve to themselves; however, we let
# certain tags be aliases for multiple other tags, purely for convenience.
ALL_TAGS: dict[str, list[Tag]] = {
    "tracktitle": ["tracktitle"],
    "trackartist": [
        "trackartist[main]",
        "trackartist[guest]",
        "trackartist[remixer]",
        "trackartist[producer]",
        "trackartist[composer]",
        "trackartist[conductor]",
        "trackartist[djmixer]",
    ],
    "trackartist[main]": ["trackartist[main]"],
    "trackartist[guest]": ["trackartist[guest]"],
    "trackartist[remixer]": ["trackartist[remixer]"],
    "trackartist[producer]": ["trackartist[producer]"],
    "trackartist[composer]": ["trackartist[composer]"],
    "trackartist[conductor]": ["trackartist[conductor]"],
    "trackartist[djmixer]": ["trackartist[djmixer]"],
    "tracknumber": ["tracknumber"],
    "tracktotal": ["tracktotal"],
    "discnumber": ["discnumber"],
    "disctotal": ["disctotal"],
    "releasetitle": ["releasetitle"],
    "releaseartist": [
        "releaseartist[main]",
        "releaseartist[guest]",
        "releaseartist[remixer]",
        "releaseartist[producer]",
        "releaseartist[composer]",
        "releaseartist[conductor]",
        "releaseartist[djmixer]",
    ],
    "releaseartist[main]": ["releaseartist[main]"],
    "releaseartist[guest]": ["releaseartist[guest]"],
    "releaseartist[remixer]": ["releaseartist[remixer]"],
    "releaseartist[producer]": ["releaseartist[producer]"],
    "releaseartist[composer]": ["releaseartist[composer]"],
    "releaseartist[conductor]": ["releaseartist[conductor]"],
    "releaseartist[djmixer]": ["releaseartist[djmixer]"],
    "releasetype": ["releasetype"],
    "year": ["year"],
    "genre": ["genre"],
    "label": ["label"],
    "artist": [
        "trackartist[main]",
        "trackartist[guest]",
        "trackartist[remixer]",
        "trackartist[producer]",
        "trackartist[composer]",
        "trackartist[conductor]",
        "trackartist[djmixer]",
        "releaseartist[main]",
        "releaseartist[guest]",
        "releaseartist[remixer]",
        "releaseartist[producer]",
        "releaseartist[composer]",
        "releaseartist[conductor]",
        "releaseartist[djmixer]",
    ],
}

MODIFIABLE_TAGS: list[Tag] = [
    "tracktitle",
    "trackartist[main]",
    "trackartist[guest]",
    "trackartist[remixer]",
    "trackartist[producer]",
    "trackartist[composer]",
    "trackartist[conductor]",
    "trackartist[djmixer]",
    "tracknumber",
    "discnumber",
    "releasetitle",
    "releaseartist[main]",
    "releaseartist[guest]",
    "releaseartist[remixer]",
    "releaseartist[producer]",
    "releaseartist[composer]",
    "releaseartist[conductor]",
    "releaseartist[djmixer]",
    "releasetype",
    "year",
    "genre",
    "label",
]

SINGLE_VALUE_TAGS: list[Tag] = [
    "tracktitle",
    "tracknumber",
    "tracktotal",
    "discnumber",
    "disctotal",
    "releasetitle",
    "releasetype",
    "year",
]

RELEASE_TAGS: list[Tag] = [
    "releasetitle",
    "releaseartist[main]",
    "releaseartist[guest]",
    "releaseartist[remixer]",
    "releaseartist[producer]",
    "releaseartist[composer]",
    "releaseartist[conductor]",
    "releaseartist[djmixer]",
    "releasetype",
    "releasetype",
    "year",
    "genre",
    "label",
    "disctotal",
]


@dataclass
class ReplaceAction:
    """
    Replaces the matched tag with `replacement`. For multi-valued tags, `;` is treated as a
    delimiter between multiple replacement values.
    """

    replacement: str


@dataclass
class SedAction:
    """
    Executes a regex substitution on a tag value.
    """

    src: re.Pattern[str]
    dst: str


@dataclass
class SplitAction:
    """
    Splits a tag into multiple tags on the provided delimiter. This action is only allowed on
    multi-value tags.
    """

    delimiter: str


@dataclass
class AddAction:
    """
    Adds a value to the tag. This action is only allowed on multi-value tags. If the value already
    exists, this action No-Ops.
    """

    value: str


@dataclass
class DeleteAction:
    """
    Deletes the tag value.
    """


@dataclass
class MatcherPattern:
    # Substring match with support for `^$` strict start / strict end matching.
    pattern: str
    case_insensitive: bool = False

    def __str__(self) -> str:
        r = escape(self.pattern)
        if self.case_insensitive:
            r += ":i"
        return r


@dataclass
class MetadataMatcher:
    # Tags to test against the pattern. If any tags match the pattern, the action will be ran
    # against the track.
    tags: list[Tag]
    # The pattern to test the tag against.
    pattern: MatcherPattern

    def __str__(self) -> str:
        r = stringify_tags(self.tags)
        r += "/"
        r += str(self.pattern)
        return r

    @classmethod
    def parse(cls, raw: str) -> MetadataMatcher:
        idx = 0
        # Common arguments to feed into Syntax Error.
        err = {"rule_name": "matcher", "rule": raw}

        # First, parse the tags.
        tags: list[Tag] = []
        found_colon = False
        while True:
            for t, resolved in ALL_TAGS.items():
                if not raw[idx:].startswith(t):
                    continue
                try:
                    if raw[idx:][len(t)] not in [":", ","]:
                        continue
                except IndexError:
                    raise RuleSyntaxError(
                        **err,
                        index=idx + len(t),
                        feedback="Expected to find ',' or ':', found end of string.",
                    ) from None
                tags.extend(resolved)
                idx += len(t) + 1
                found_colon = raw[idx - 1] == ":"
                break
            else:
                raise RuleSyntaxError(
                    **err,
                    index=idx,
                    feedback=f"Invalid tag: must be one of {{{', '.join(ALL_TAGS)}}}. The next character after a tag must be ':' or ','.",
                )
            if found_colon:
                break

        # Then parse the pattern.
        pattern, fwd = take(raw[idx:], ":", consume_until=False)
        idx += fwd

        # If more input is remaining, it should be optional single-character flags.
        case_insensitive = False
        if idx < len(raw) and take(raw[idx:], ":") == ("", 1):
            idx += 1
            flags, fwd = take(raw[idx:], ":")
            if not flags:
                raise RuleSyntaxError(
                    **err,
                    index=idx,
                    feedback="No flags specified: Please remove this section (by deleting the colon) or specify one of the supported flags: `i` (case insensitive).",
                )
            for i, flag in enumerate(flags):
                if flag == "i":
                    case_insensitive = True
                    continue
                raise RuleSyntaxError(
                    **err,
                    index=idx + i,
                    feedback="Unrecognized flag: Please specify one of the supported flags: `i` (case insensitive).",
                )
            idx += fwd

        if raw[idx:]:
            raise RuleSyntaxError(
                **err,
                index=idx,
                feedback="Extra input found after end of matcher. Perhaps you meant to escape this colon?",
            )

        matcher = MetadataMatcher(
            tags=tags,
            pattern=MatcherPattern(pattern=pattern, case_insensitive=case_insensitive),
        )
        logger.debug(f"Parsed rule matcher {raw=} as {matcher=}")
        return matcher


@dataclass
class MetadataAction:
    # The behavior of the action, along with behavior-specific parameters.
    behavior: ReplaceAction | SedAction | SplitAction | AddAction | DeleteAction
    # The tags to apply the action on. Defaults to the tag that the pattern matched.
    tags: list[Tag]
    # Only apply the action on values that match this pattern. None means that all values are acted
    # upon.
    pattern: MatcherPattern | None = None

    def __str__(self) -> str:
        r = ""
        r += stringify_tags(self.tags)
        if self.pattern:
            r += ":" + str(self.pattern)
        if r:
            r += "/"

        if isinstance(self.behavior, ReplaceAction):
            r += "replace"
        elif isinstance(self.behavior, SedAction):
            r += "sed"
        elif isinstance(self.behavior, SplitAction):
            r += "split"
        elif isinstance(self.behavior, AddAction):
            r += "add"
        elif isinstance(self.behavior, DeleteAction):
            r += "delete"

        if isinstance(self.behavior, ReplaceAction):
            r += ":" + self.behavior.replacement
        elif isinstance(self.behavior, SedAction):
            r += ":" + escape(str(self.behavior.src.pattern))
            r += ":"
            r += escape(self.behavior.dst)
        elif isinstance(self.behavior, SplitAction):
            r += ":" + self.behavior.delimiter
        return r

    @classmethod
    def parse(
        cls,
        raw: str,
        action_number: int | None = None,
        # If there is a matcher for the action, pass it here to set the defaults.
        matcher: MetadataMatcher | None = None,
    ) -> MetadataAction:
        idx = 0
        # Common arguments to feed into Syntax Error.
        err = {"rule": raw, "rule_name": "action"}
        if action_number:
            err["rule_name"] += f" {action_number}"

        # First, determine whether we have a matcher section or not. The matcher section is optional,
        # but present if there is an unescaped `/`.
        _, action_idx = take(raw, "/")
        has_tags_pattern_section = action_idx != len(raw)

        # Parse the (optional) tags+pattern section.
        if not has_tags_pattern_section:
            if not matcher:
                raise RuleSyntaxError(
                    **err,
                    index=idx,
                    feedback="Tags/pattern section not found. "
                    "Must specify tags to modify, since there is no matcher to default to. "
                    "Make sure you are formatting your action like {tags}:{pattern}/{kind}:{args} (where `:{pattern}` is optional)",
                )
            tags: list[Tag] = [x for x in matcher.tags if x in MODIFIABLE_TAGS]
            pattern = matcher.pattern.pattern
            case_insensitive = matcher.pattern.case_insensitive
        else:
            # First, parse the tags. If the tag is matched, keep going, otherwise employ the list
            # parsing logic.
            if raw[idx:].startswith("matched:"):
                if not matcher:
                    raise RuleSyntaxError(
                        **err,
                        index=idx,
                        feedback="Cannot use `matched` in this context: there is no matcher to default to.",
                    )
                idx += len("matched:")
                tags = [x for x in matcher.tags if x in MODIFIABLE_TAGS]
                pattern = matcher.pattern.pattern
                case_insensitive = matcher.pattern.case_insensitive
            else:
                tags = []
                found_colon = False
                while True:
                    for t, resolved in ALL_TAGS.items():
                        if not raw[idx:].startswith(t):
                            continue
                        if raw[idx:][len(t)] not in [":", ","]:
                            continue
                        for resolvedtag in resolved:
                            if resolvedtag not in MODIFIABLE_TAGS:
                                raise RuleSyntaxError(
                                    **err,
                                    index=idx,
                                    feedback=f"Invalid tag: {t} is not modifiable.",
                                )
                            tags.append(resolvedtag)
                        idx += len(t) + 1
                        found_colon = raw[idx - 1] == ":"
                        break
                    else:
                        tags_to_print: list[str] = []
                        for t, resolvedtags in ALL_TAGS.items():
                            if all(r in MODIFIABLE_TAGS for r in resolvedtags):
                                tags_to_print.append(t)
                        feedback = f"Invalid tag: must be one of {{{', '.join(tags_to_print)}}}. The next character after a tag must be ':' or ','."
                        if matcher:
                            feedback = f"Invalid tag: must be one of matched, {{{', '.join(tags_to_print)}}}. (And if the value is matched, it must be alone.) The next character after a tag must be ':' or ','."
                        raise RuleSyntaxError(**err, index=idx, feedback=feedback)
                    if found_colon:
                        break

            # And now parse the optional pattern. If the next character is a `/`, then we have an
            # explicitly empty pattern, after which we reach the end of the tags+pattern section.
            pattern = None
            case_insensitive = False
            if take(raw[idx:], "/") == ("", 1):
                idx += 2
            # Otherwise, if we hit a lone `:`, we've hit the end of the tags+pattern section, but
            # the pattern is not specified. In this case, default to the matcher's pattern, if we
            # have a matcher.
            # hit the end of the matcher, and we should proceed to the action.
            elif take(raw[idx:], ":") == ("", 1):
                idx += 1
                if matcher and tags == matcher.tags:
                    pattern = matcher.pattern.pattern
            # And otherwise, parse the pattern!
            else:
                has_flags = True
                pattern, fwd = take(raw[idx:], ":")
                if fwd == 0:
                    has_flags = False
                    pattern, fwd = take(raw[idx:], "/")
                idx += fwd
                # Set an empty pattern to null.
                pattern = pattern or None

                # If we don't see the second colon here, that means we are looking at
                # single-character flags. Only check this if pattern is not null though.
                if has_flags:
                    flags, fwd = take(raw[idx:], "/")
                    if not flags:
                        raise RuleSyntaxError(
                            **err,
                            index=idx,
                            feedback="No flags specified: Please remove this section (by deleting the colon) or specify one of the supported flags: `i` (case insensitive).",
                        )
                    for i, flag in enumerate(flags):
                        if flag == "i":
                            case_insensitive = True
                            continue
                        raise RuleSyntaxError(
                            **err,
                            index=idx + i,
                            feedback="Unrecognized flag: Either you forgot a colon here (to end the matcher), or this is an invalid matcher flag. The only supported flag is `i` (case insensitive).",
                        )
                    idx += fwd

        # Then let's start parsing the action!
        action_kind, fwd = take(raw[idx:], ":")
        valid_actions = [
            "replace",
            "sed",
            "split",
            "add",
            "delete",
        ]
        if action_kind not in valid_actions:
            raise RuleSyntaxError(
                **err,
                index=idx,
                feedback=f"Invalid action kind: must be one of {{{', '.join(valid_actions)}}}.",
            )
        idx += fwd

        # Validate that the action type is supported for the given tags.
        if action_kind == "split" or action_kind == "add":
            single_valued_tags = [t for t in tags if t in SINGLE_VALUE_TAGS]
            if single_valued_tags:
                raise InvalidRuleError(
                    f"Single valued tags {', '.join(single_valued_tags)} cannot be modified by multi-value action {action_kind}"
                )

        # And then parse each action kind separately.
        behavior: ReplaceAction | SedAction | SplitAction | AddAction | DeleteAction
        if action_kind == "replace":
            replacement, fwd = take(raw[idx:], ":", consume_until=False)
            idx += fwd
            if replacement == "":
                raise RuleSyntaxError(
                    **err,
                    index=idx,
                    feedback="Replacement not found: must specify a non-empty replacement. Use the delete action to remove a value.",
                )
            if raw[idx:]:
                raise RuleSyntaxError(
                    **err,
                    index=idx,
                    feedback="Found another section after the replacement, but the replacement must be the last section. Perhaps you meant to escape this colon?",
                )
            behavior = ReplaceAction(replacement=replacement)
        elif action_kind == "sed":
            src_str, fwd = take(raw[idx:], ":", consume_until=False)
            if src_str == "":
                raise RuleSyntaxError(
                    **err,
                    index=idx,
                    feedback=f"Empty sed pattern found: must specify a non-empty pattern. Example: {raw}:pattern:replacement",
                )
            try:
                src = re.compile(src_str)
            except re.error as e:
                raise RuleSyntaxError(
                    **err,
                    index=idx,
                    feedback=f"Failed to compile the sed pattern regex: invalid pattern: {e}",
                ) from e
            idx += fwd

            if len(raw) == idx or raw[idx] != ":":
                raise RuleSyntaxError(
                    **err,
                    index=idx,
                    feedback=f"Sed replacement not found: must specify a sed replacement section. Example: {raw}:replacement.",
                )
            idx += 1

            dst, fwd = take(raw[idx:], ":", consume_until=False)
            idx += fwd
            if raw[idx:]:
                raise RuleSyntaxError(
                    **err,
                    index=idx,
                    feedback="Found another section after the sed replacement, but the sed replacement must be the last section. Perhaps you meant to escape this colon?",
                )
            behavior = SedAction(src=src, dst=dst)
        elif action_kind == "split":
            delimiter, fwd = take(raw[idx:], ":", consume_until=False)
            idx += fwd
            if delimiter == "":
                feedback = "Delimiter not found: must specify a non-empty delimiter to split on."
                raise RuleSyntaxError(**err, index=idx, feedback=feedback)
            if raw[idx:]:
                raise RuleSyntaxError(
                    **err,
                    index=idx,
                    feedback="Found another section after the delimiter, but the delimiter must be the last section. Perhaps you meant to escape this colon?",
                )
            behavior = SplitAction(delimiter=delimiter)
        elif action_kind == "add":
            value, fwd = take(raw[idx:], ":", consume_until=False)
            idx += fwd
            if value == "":
                feedback = "Value not found: must specify a non-empty value to add."
                raise RuleSyntaxError(**err, index=idx, feedback=feedback)
            if raw[idx:]:
                raise RuleSyntaxError(
                    **err,
                    index=idx,
                    feedback="Found another section after the value, but the value must be the last section. Perhaps you meant to escape this colon?",
                )
            behavior = AddAction(value=value)
        elif action_kind == "delete":
            if raw[idx:]:
                raise RuleSyntaxError(
                    **err,
                    index=idx,
                    feedback="Found another section after the action kind, but the delete action has no parameters. Please remove this section.",
                )
            behavior = DeleteAction()
        else:  # pragma: no cover
            raise RoseError(f"Impossible: unknown action_kind {action_kind=}")

        action = MetadataAction(
            behavior=behavior,
            tags=tags,
            pattern=MatcherPattern(pattern=pattern, case_insensitive=case_insensitive)
            if pattern
            else None,
        )
        logger.debug(f"Parsed rule action {raw=} {matcher=} as {action=}")
        return action


@dataclass
class MetadataRule:
    matcher: MetadataMatcher
    actions: list[MetadataAction]
    ignore: list[MetadataMatcher]

    def __str__(self) -> str:
        rval: list[str] = []
        rval.append(f"matcher={shlex.quote(str(self.matcher))}")
        for action in self.actions:
            rval.append(f"action={shlex.quote(str(action))}")
        return " ".join(rval)

    @classmethod
    def parse(
        cls,
        matcher: str,
        actions: list[str],
        ignore: list[str] | None = None,
    ) -> MetadataRule:
        parsed_matcher = MetadataMatcher.parse(matcher)
        return MetadataRule(
            matcher=parsed_matcher,
            actions=[MetadataAction.parse(a, i + 1, parsed_matcher) for i, a in enumerate(actions)],
            ignore=[MetadataMatcher.parse(v) for v in (ignore or [])],
        )


def take(x: str, until: Literal[":", "/"], consume_until: bool = True) -> tuple[str, int]:
    """Take until the next unescaped special character."""
    # Loop until we find an unescaped colon.
    match = ""
    fwd = 0
    while True:
        match_, fwd_ = _take_escaped(x, until, consume_until)
        match += match_
        fwd += fwd_

        next_idx = fwd + (0 if consume_until else 1)
        escaped_special_char = x[next_idx:].startswith(until)
        if not escaped_special_char:
            break
        match += until
        fwd = next_idx
        x = x[next_idx:]

    return match, fwd


def _take_escaped(x: str, until: str, consume_until: bool = True) -> tuple[str, int]:
    """DO NOT USE THIS FUNCTION DIRECTLY. USE take."""
    r = io.StringIO()
    escaped: Literal[":", "/", None] = None
    seen_idx = 0
    for i, c in enumerate(x):
        if x[i : i + len(until)] == until:
            if consume_until:
                seen_idx += len(until)
            break
        # We have a potential escape here. Store the escaped character to verify it in the next
        # iteration.
        if (c == ":" or c == "/") and not escaped:
            escaped = c  # type: ignore
            seen_idx += 1
            continue
        # If this is true, then nothing was actually escaped. Write the first character and the
        # second character to the output.
        if escaped and c != escaped:
            r.write(escaped)
            escaped = None
        r.write(c)
        seen_idx += 1

    result = r.getvalue()
    r.close()
    return result, seen_idx


def escape(x: str) -> str:
    """Escape the special characters in a string."""
    return x.replace(":", "::").replace("/", "//")


def stringify_tags(tags_input: list[Tag]) -> str:
    """Basically a ",".join(tags), except we collapse aliases down to their shorthand form."""
    # Yes, I know the computational complexity of this is high, but the size of `n` is so small that
    # it's irrelevant.
    tags: list[str] = [*tags_input]
    if all(x in tags for x in ALL_TAGS["artist"]):
        idx = tags.index(ALL_TAGS["artist"][0])
        for x in ALL_TAGS["artist"]:
            tags.remove(x)
        tags.insert(idx, "artist")
    if all(x in tags for x in ALL_TAGS["trackartist"]):
        idx = tags.index(ALL_TAGS["trackartist"][0])
        for x in ALL_TAGS["trackartist"]:
            tags.remove(x)
        tags.insert(idx, "trackartist")
    if all(x in tags for x in ALL_TAGS["releaseartist"]):
        idx = tags.index(ALL_TAGS["releaseartist"][0])
        for x in ALL_TAGS["releaseartist"]:
            tags.remove(x)
        tags.insert(idx, "releaseartist")
    return ",".join(tags)
