"""
The rule_parser module provides the typedef and parser for the rules engine. This is split out from
the rules engine in order to avoid a dependency cycle between the config module and the rules
module.
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
    "trackartist",
    "tracknumber",
    "discnumber",
    "albumtitle",
    "albumartist",
    "releasetype",
    "year",
    "genre",
    "label",
]

ALL_TAGS: list[Tag] = [
    "tracktitle",
    "trackartist",
    "tracknumber",
    "discnumber",
    "albumtitle",
    "albumartist",
    "releasetype",
    "year",
    "genre",
    "label",
]


SINGLE_VALUE_TAGS: list[Tag] = [
    "tracktitle",
    "tracknumber",
    "discnumber",
    "albumtitle",
    "releasetype",
    "year",
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
class MetadataMatcher:
    # Tags to test against the pattern. If any tags match the pattern, the action will be ran
    # against the track.
    tags: list[Tag]
    # The pattern to test the tag against. Substring match with support for `^$` strict start /
    # strict end matching.
    pattern: str

    def __str__(self) -> str:
        r = ",".join(self.tags)
        r += ":"
        r += self.pattern.replace(":", r"\:")
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
            for t in ALL_TAGS:
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
                tags.append(t)
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
        pattern, fwd = take(raw[idx:], ":", including=False)
        idx += fwd
        if raw[idx:]:
            raise RuleSyntaxError(
                **err,
                index=idx,
                feedback="Found another section after the pattern, but the pattern must be the last section. Perhaps you meant to escape this colon?",
            )

        matcher = cls(tags=tags, pattern=pattern)
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
    pattern: str | None = None

    def __str__(self) -> str:
        r = ""
        r += ",".join(self.tags)
        if self.pattern:
            r += ":" + self.pattern.replace(":", r"\:")
        if r:
            r += "::"

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
            r += ":" + str(self.behavior.src.pattern).replace(":", r"\:")
            r += ":"
            r += self.behavior.dst.replace(":", r"\:")
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
        # but present if there is an unescaped `::`.
        _, action_idx = take(raw, "::")
        has_tags_pattern_section = action_idx != len(raw)

        # Parse the (optional) tags+pattern section.
        if not has_tags_pattern_section:
            if not matcher:
                raise RuleSyntaxError(
                    **err,
                    index=idx,
                    feedback="Tags/pattern section not found. "
                    "Must specify tags to modify, since there is no matcher to default to. "
                    "Make sure you are formatting your action like {tags}:{pattern}::{kind}:{args} (where `:{pattern}` is optional)",
                )
            tags = matcher.tags
            pattern = matcher.pattern
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
                tags = matcher.tags
                pattern = matcher.pattern
            else:
                tags = []
                found_colon = False
                while True:
                    for t in ALL_TAGS:
                        if not raw[idx:].startswith(t):
                            continue
                        if raw[idx:][len(t)] not in [":", ","]:
                            continue
                        tags.append(t)
                        idx += len(t) + 1
                        found_colon = raw[idx - 1] == ":"
                        break
                    else:
                        feedback = f"Invalid tag: must be one of {{{', '.join(ALL_TAGS)}}}. The next character after a tag must be ':' or ','."
                        if matcher:
                            feedback = f"Invalid tag: must be one of matched, {{{', '.join(ALL_TAGS)}}}. (And if the value is matched, it must be alone.) The next character after a tag must be ':' or ','."
                        raise RuleSyntaxError(**err, index=idx, feedback=feedback)
                    if found_colon:
                        break

            # And now parse the optional pattern. If the next character is a `::`, then we have an
            # explicitly empty pattern, after which we reach the end of the tags+pattern section.
            pattern = None
            if raw[idx : idx + 2] == "::":
                idx += 2
            # Otherwise, if we hit a lone `:`, we've hit the end of the tags+pattern section, but
            # the pattern is not specified. In this case, default to the matcher's pattern, if we
            # have a matcher.
            # hit the end of the matcher, and we should proceed to the action.
            elif raw[idx] == ":":
                idx += 1
                if matcher and tags == matcher.tags:
                    pattern = matcher.pattern
            # And otherwise, parse the pattern!
            else:
                pattern, fwd = take(raw[idx:], ":")
                idx += fwd
                # Because we treat `::` as going to action, empty pattern should be impossible.
                if raw[idx] != ":":
                    raise RuleSyntaxError(
                        **err,
                        index=idx,
                        feedback="End of the action matcher not found. Please end the matcher with a `::`.",
                    )
                # Skip the second colon. Now we're at the start of the action.
                idx += 1
                # Set an empty pattern to null.
                pattern = pattern or None

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
            feedback = f"Invalid action kind: must be one of {{{', '.join(valid_actions)}}}."
            if idx == 0 and ":" in raw:
                feedback += " If this is pointing at your pattern, you forgot to put :: (double colons) between the matcher section and the action section."
            raise RuleSyntaxError(**err, index=idx, feedback=feedback)
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
            replacement, fwd = take(raw[idx:], ":", including=False)
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
            src_str, fwd = take(raw[idx:], ":", including=False)
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

            dst, fwd = take(raw[idx:], ":", including=False)
            idx += fwd
            if raw[idx:]:
                raise RuleSyntaxError(
                    **err,
                    index=idx,
                    feedback="Found another section after the sed replacement, but the sed replacement must be the last section. Perhaps you meant to escape this colon?",
                )
            behavior = SedAction(src=src, dst=dst)
        elif action_kind == "split":
            delimiter, fwd = take(raw[idx:], ":", including=False)
            idx += fwd
            if delimiter == "":
                feedback = "Delimiter not found: must specify a non-empty delimiter to split on."
                if len(raw) > idx and raw[idx] == ":":
                    feedback += " Perhaps you meant to escape this colon?"
                raise RuleSyntaxError(**err, index=idx, feedback=feedback)
            if raw[idx:]:
                raise RuleSyntaxError(
                    **err,
                    index=idx,
                    feedback="Found another section after the delimiter, but the delimiter must be the last section. Perhaps you meant to escape this colon?",
                )
            behavior = SplitAction(delimiter=delimiter)
        elif action_kind == "add":
            value, fwd = take(raw[idx:], ":", including=False)
            idx += fwd
            if value == "":
                feedback = "Value not found: must specify a non-empty value to add."
                if len(raw) > idx and raw[idx] == ":":
                    feedback += " Perhaps you meant to escape this colon?"
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

        action = cls(behavior=behavior, tags=tags, pattern=pattern)
        logger.debug(f"Parsed rule action {raw=} {matcher=} as {action=}")
        return action


@dataclass
class MetadataRule:
    matcher: MetadataMatcher
    actions: list[MetadataAction]

    def __str__(self) -> str:
        rval: list[str] = []
        rval.append(f"matcher={shlex.quote(str(self.matcher))}")
        for action in self.actions:
            rval.append(f"action={shlex.quote(str(action))}")
        return " ".join(rval)

    @classmethod
    def parse(cls, matcher: str, actions: list[str]) -> MetadataRule:
        parsed_matcher = MetadataMatcher.parse(matcher)
        return MetadataRule(
            matcher=parsed_matcher,
            actions=[MetadataAction.parse(a, i + 1, parsed_matcher) for i, a in enumerate(actions)],
        )


def take(x: str, until: str, including: bool = True) -> tuple[str, int]:
    """
    Reads until the next unescaped `until` is found. Returns the read string and the number of
    characters consumed from the input. `until` is counted as consumed if `including` is true.
    """
    r = io.StringIO()
    escaped = False
    seen_idx = 0
    for i, c in enumerate(x):
        if c == "\\" and not escaped:
            escaped = True
            seen_idx += 1
            continue
        if x[i : i + len(until)] == until and not escaped:
            if including:
                seen_idx += len(until)
            break
        escaped = False
        r.write(c)
        seen_idx += 1

    result = r.getvalue()
    r.close()
    return result, seen_idx
