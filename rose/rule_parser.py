"""
The rule_parser module provides the typedef and parser for the rules engine. This is split out from
the rules engine in order to avoid a dependency cycle between the config module and the rules
module.
"""

from __future__ import annotations

import io
import logging
import re
from dataclasses import dataclass
from typing import Literal

from rose.common import RoseError

logger = logging.getLogger(__name__)


class InvalidRuleError(RoseError):
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
    {" " * self.index}^
    {" " * self.index}{self.feedback}
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


@dataclass
class MetadataAction:
    # The behavior of the action, along with behavior-specific parameters.
    behavior: ReplaceAction | SedAction | SplitAction | AddAction | DeleteAction
    # The tags to apply the action on. Defaults to the tag that the pattern matched.
    tags: list[Tag] | Literal["matched"] = "matched"
    # Only apply the action on values that match this pattern. None means that all values are acted
    # upon.
    match_pattern: str | None = None


@dataclass
class MetadataRule:
    matcher: MetadataMatcher
    actions: list[MetadataAction]

    def __str__(self) -> str:
        rval: list[str] = []

        matcher = ",".join(self.matcher.tags)
        matcher += ":"
        matcher += self.matcher.pattern.replace(":", r"\:")
        rval.append(f"matcher={quote(matcher)}")

        for action in self.actions:
            aout = ""
            if action.tags != "matched" or action.match_pattern:
                aout += ",".join(action.tags) if action.tags != "matched" else "matched"
            if action.match_pattern:
                aout += ":" + action.match_pattern.replace(":", r"\:")
            if aout:
                aout += "::"

            if isinstance(action.behavior, ReplaceAction):
                aout += "replace"
            elif isinstance(action.behavior, SedAction):
                aout += "sed"
            elif isinstance(action.behavior, SplitAction):
                aout += "split"
            elif isinstance(action.behavior, AddAction):
                aout += "add"
            elif isinstance(action.behavior, DeleteAction):
                aout += "delete"

            if isinstance(action.behavior, ReplaceAction):
                aout += ":" + action.behavior.replacement
            elif isinstance(action.behavior, SedAction):
                aout += ":" + str(action.behavior.src.pattern).replace(":", r"\:")
                aout += ":"
                aout += action.behavior.dst.replace(":", r"\:")
            elif isinstance(action.behavior, SplitAction):
                aout += ":" + action.behavior.delimiter
            rval.append(f"action={quote(aout)}")

        return " ".join(rval)

    @classmethod
    def parse(cls, matcher: str, actions: list[str]) -> MetadataRule:
        rule = cls(
            matcher=parse_matcher(matcher),
            actions=[parse_action(a, i + 1) for i, a in enumerate(actions)],
        )

        # If the matching tag is `matched`, default the actions matchers to the pattern.
        for a in rule.actions:
            if a.match_pattern is None and (a.tags == "matched" or a.tags == rule.matcher.tags):
                a.match_pattern = rule.matcher.pattern

        # And validate that multi-value actions are not invoked on single-value tags.
        for action in rule.actions:
            b = action.behavior
            if not isinstance(b, SplitAction) and not isinstance(b, AddAction):
                continue  # Isn't a multi-value action.
            tags = action.tags
            if tags == "matched":
                tags = rule.matcher.tags
            single_valued_tags = [t for t in tags if t in SINGLE_VALUE_TAGS]
            if single_valued_tags:
                raise InvalidRuleError(
                    f"Single valued tags {', '.join(single_valued_tags)} cannot be modified by "
                    f"multi-value action {type(action.behavior).__name__}"
                )

        return rule


def parse_matcher(raw: str) -> MetadataMatcher:
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
                feedback=f"Invalid tag: must be one of {{{', '.join(ALL_TAGS)}}}. "
                "The next character after a tag must be ':' or ','.",
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
            feedback="Found another section after the pattern, but the pattern must be "
            "the last section. Perhaps you meant to escape this colon?",
        )

    return MetadataMatcher(tags=tags, pattern=pattern)


def parse_action(raw: str, action_number: int) -> MetadataAction:
    idx = 0
    # Common arguments to feed into Syntax Error.
    err = {"rule_name": f"action {action_number}", "rule": raw}

    # First, determine whether we have a matcher section or not. The matcher section is optional,
    # but present if there is an unescaped `::`.
    _, action_idx = take(raw, "::")
    has_matcher = action_idx != len(raw)

    # Parse the (optional) matcher.
    tags: Literal["matched"] | list[Tag] = "matched"
    pattern: str | None = None
    if has_matcher:
        # First, parse the tags. If the tag is matched, keep going, otherwise employ the list
        # parsing logic.
        if raw[idx:].startswith("matched:"):
            idx += len("matched:")
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
                    raise RuleSyntaxError(
                        **err,
                        index=idx,
                        feedback=f"Invalid tag: must be one of matched, {{{', '.join(ALL_TAGS)}}}. "
                        "(And if the value is matched, it must be alone.) "
                        "The next character after a tag must be ':' or ','.",
                    )
                if found_colon:
                    break

        # And now parse the optional pattern. If the next character is a `:`, then we've hit the end
        # of the matcher, and we should proceed to the action.
        if raw[idx] == ":":
            idx += 1
        else:
            pattern, fwd = take(raw[idx:], ":")
            idx += fwd
            # Because we treat `::` as going to action, empty pattern should be impossible.
            if raw[idx] != ":":
                raise RuleSyntaxError(
                    **err,
                    index=idx,
                    feedback="End of the action matcher not found. Please end the matcher "
                    "with a `::`.",
                )
            # Skip the second colon. Now we're at the start of the action.
            idx += 1

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
            feedback += (
                " If this is pointing at your pattern, you forgot to put :: (double colons) "
                "between the matcher section and the action section."
            )
        raise RuleSyntaxError(**err, index=idx, feedback=feedback)
    idx += fwd
    # And then parse each action kind separately.
    behavior: ReplaceAction | SedAction | SplitAction | AddAction | DeleteAction
    if action_kind == "replace":
        replacement, fwd = take(raw[idx:], ":", including=False)
        idx += fwd
        if replacement == "":
            raise RuleSyntaxError(
                **err,
                index=idx,
                feedback="Replacement not found: must specify a non-empty replacement. "
                "Use the delete action to remove a value.",
            )
        if raw[idx:]:
            raise RuleSyntaxError(
                **err,
                index=idx,
                feedback="Found another section after the replacement, but the replacement must be "
                "the last section. Perhaps you meant to escape this colon?",
            )
        behavior = ReplaceAction(replacement=replacement)
    elif action_kind == "sed":
        src_str, fwd = take(raw[idx:], ":", including=False)
        if src_str == "":
            raise RuleSyntaxError(
                **err,
                index=idx,
                feedback="Empty sed pattern found: must specify a non-empty pattern. "
                f"Example: {raw}:pattern:replacement",
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
                feedback="Sed replacement not found: must specify a sed replacement section. "
                f"Example: {raw}:replacement.",
            )
        idx += 1

        dst, fwd = take(raw[idx:], ":", including=False)
        idx += fwd
        if raw[idx:]:
            raise RuleSyntaxError(
                **err,
                index=idx,
                feedback="Found another section after the sed replacement, but the sed replacement "
                "must be the last section. Perhaps you meant to escape this colon?",
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
                feedback="Found another section after the delimiter, but the delimiter must be "
                "the last section. Perhaps you meant to escape this colon?",
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
                feedback="Found another section after the value, but the value must be "
                "the last section. Perhaps you meant to escape this colon?",
            )
        behavior = AddAction(value=value)
    elif action_kind == "delete":
        if raw[idx:]:
            raise RuleSyntaxError(
                **err,
                index=idx,
                feedback="Found another section after the action kind, but the delete action has "
                "no parameters. Please remove this section.",
            )
        behavior = DeleteAction()
    else:  # pragma: no cover
        raise RoseError(f"Impossible: unknown action_kind {action_kind=}")

    return MetadataAction(behavior=behavior, tags=tags, match_pattern=pattern)


def quote(x: str) -> str:
    return f'"{x}"' if " " in x else x


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
