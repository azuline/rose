"""
The rule_parser module provides a parser for the rules engine's DSL.

This is split out from the rules engine in order to avoid a dependency cycle between the config
module and the rules module.
"""

from __future__ import annotations

import dataclasses
import io
import logging
import re
import shlex
from collections.abc import Sequence
from typing import Literal

import click

from rose.common import RoseError, RoseExpectedError, uniq

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
    "releasedate",
    "originaldate",
    "compositiondate",
    "catalognumber",
    "edition",
    "genre",
    "secondarygenre",
    "descriptor",
    "label",
    "new",
]

ExpandableTag = Tag | Literal["artist", "trackartist", "releaseartist"]

# Map of a tag to its "resolved" tags. Most tags simply resolve to themselves; however, we let
# certain tags be aliases for multiple other tags, purely for convenience.
ALL_TAGS: dict[ExpandableTag, list[Tag]] = {
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
    "releasedate": ["releasedate"],
    "originaldate": ["originaldate"],
    "compositiondate": ["compositiondate"],
    "edition": ["edition"],
    "catalognumber": ["catalognumber"],
    "genre": ["genre"],
    "secondarygenre": ["secondarygenre"],
    "descriptor": ["descriptor"],
    "label": ["label"],
    "new": ["new"],
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
    "releasedate",
    "originaldate",
    "compositiondate",
    "edition",
    "catalognumber",
    "genre",
    "secondarygenre",
    "descriptor",
    "label",
    "new",
]

SINGLE_VALUE_TAGS: list[Tag] = [
    "tracktitle",
    "tracknumber",
    "tracktotal",
    "discnumber",
    "disctotal",
    "releasetitle",
    "releasetype",
    "releasedate",
    "originaldate",
    "compositiondate",
    "edition",
    "catalognumber",
    "new",
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
    "releasedate",
    "originaldate",
    "compositiondate",
    "edition",
    "catalognumber",
    "genre",
    "secondarygenre",
    "descriptor",
    "label",
    "disctotal",
    "new",
]


@dataclasses.dataclass
class ReplaceAction:
    """
    Replaces the matched tag with `replacement`. For multi-valued tags, `;` is treated as a
    delimiter between multiple replacement values.
    """

    replacement: str


@dataclasses.dataclass
class SedAction:
    """
    Executes a regex substitution on a tag value.
    """

    src: re.Pattern[str]
    dst: str


@dataclasses.dataclass
class SplitAction:
    """
    Splits a tag into multiple tags on the provided delimiter. This action is only allowed on
    multi-value tags.
    """

    delimiter: str


@dataclasses.dataclass
class AddAction:
    """
    Adds a value to the tag. This action is only allowed on multi-value tags. If the value already
    exists, this action No-Ops.
    """

    value: str


@dataclasses.dataclass
class DeleteAction:
    """
    Deletes the tag value.
    """


@dataclasses.dataclass(slots=True)
class Pattern:
    # Substring match with support for `^$` strict start / strict end matching.
    needle: str
    strict_start: bool = False
    strict_end: bool = False
    case_insensitive: bool = False

    def __init__(
        self,
        # The starting `^` and the trailing `$` are parsed to set strict_start/strict_end if they
        # are not passed explicitly. If they are passed explicitly, the needle is untouched. They
        # can be escaped with backslashes.
        needle: str,
        # Sets both strict_start and strict_end to true.
        strict: bool = False,
        strict_start: bool = False,
        strict_end: bool = False,
        case_insensitive: bool = False,
    ):
        self.needle = needle
        self.strict_start = strict_start or strict
        self.strict_end = strict_end or strict
        if not self.strict_start:
            if self.needle.startswith("^"):
                self.strict_start = True
                self.needle = self.needle[1:]
            elif self.needle.startswith(r"\^"):
                self.needle = self.needle[1:]
        if not self.strict_end:
            if self.needle.endswith(r"\$"):
                self.needle = self.needle[:-2] + "$"
            elif self.needle.endswith("$"):
                self.strict_end = True
                self.needle = self.needle[:-1]
        self.case_insensitive = case_insensitive

    def __str__(self) -> str:
        r = escape(self.needle)

        if self.strict_start:
            r = "^" + r
        elif self.needle.startswith("^"):
            r = "\\" + r

        if self.strict_end:
            r += "$"
        elif self.needle.endswith("$"):
            r = r[:-1] + r"\$"

        if self.case_insensitive:
            r += ":i"
        return r


@dataclasses.dataclass(slots=True)
class Matcher:
    # Tags to test against the pattern. If any tags match the pattern, the action will be ran
    # against the track.
    tags: list[Tag]
    # The pattern to test the tag against.
    pattern: Pattern

    def __init__(self, tags: Sequence[ExpandableTag], pattern: Pattern) -> None:
        _tags: list[Tag] = []
        for t in tags:
            _tags.extend(ALL_TAGS[t])
        self.tags = uniq(_tags)
        self.pattern = pattern

    def __str__(self) -> str:
        r = stringify_tags(self.tags)
        r += ":"
        r += str(self.pattern)
        return r

    @classmethod
    def parse(cls, raw: str, *, rule_name: str = "matcher") -> Matcher:
        idx = 0
        # Common arguments to feed into Syntax Error.
        err = {"rule_name": rule_name, "rule": raw}

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
                    feedback=f"Invalid tag: must be one of {{{", ".join(ALL_TAGS)}}}. The next character after a tag must be ':' or ','.",
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

        matcher = Matcher(
            tags=tags,
            pattern=Pattern(needle=pattern, case_insensitive=case_insensitive),
        )
        logger.debug(f"Parsed rule matcher {raw=} as {matcher=}")
        return matcher


@dataclasses.dataclass(slots=True)
class Action:
    # The tags to apply the action on. Defaults to the tag that the pattern matched.
    tags: list[Tag]
    # The behavior of the action, along with behavior-specific parameters.
    behavior: ReplaceAction | SedAction | SplitAction | AddAction | DeleteAction
    # Only apply the action on values that match this pattern. None means that all values are acted
    # upon.
    pattern: Pattern | None = None

    def __init__(
        self,
        tags: Sequence[ExpandableTag],
        behavior: ReplaceAction | SedAction | SplitAction | AddAction | DeleteAction,
        pattern: Pattern | None = None,
    ) -> None:
        _tags: list[Tag] = []
        for t in tags:
            _tags.extend(ALL_TAGS[t])
        self.tags = uniq(_tags)
        self.behavior = behavior
        self.pattern = pattern

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
        matcher: Matcher | None = None,
    ) -> Action:
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
                tags = [x for x in matcher.tags if x in MODIFIABLE_TAGS]
            else:
                tags = []
                found_end = False
                while True:
                    for t, resolved in ALL_TAGS.items():
                        if not raw[idx:].startswith(t):
                            continue
                        if raw[idx:][len(t)] not in [":", ",", "/"]:
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
                        found_end = raw[idx - 1] == ":" or raw[idx - 1] == "/"
                        break
                    else:
                        tags_to_print: list[str] = []
                        for t, resolvedtags in ALL_TAGS.items():
                            if all(r in MODIFIABLE_TAGS for r in resolvedtags):
                                tags_to_print.append(t)
                        feedback = f"Invalid tag: must be one of {{{", ".join(tags_to_print)}}}. The next character after a tag must be ':' or ','."
                        if matcher:
                            feedback = f"Invalid tag: must be one of matched, {{{", ".join(tags_to_print)}}}. (And if the value is matched, it must be alone.) The next character after a tag must be ':' or ','."
                        raise RuleSyntaxError(**err, index=idx, feedback=feedback)
                    if found_end:
                        break

            # And now parse the optional pattern. If the next character is a `/`, then we have an
            # explicitly empty pattern, after which we reach the end of the tags+pattern section.
            pattern = None
            # It's possible for us to have both `tracktitle:/` (explicitly empty pattern) or
            # `tracktitle/` (inherit pattern), which we handle in the following two cases:
            if take(raw[idx - 1 :], "/") == ("", 1):
                if matcher and tags == matcher.tags:
                    pattern = matcher.pattern
            elif take(raw[idx:], "/") == ("", 1):
                idx += 1
            # And otherwise, parse the pattern!
            else:
                # Take the earliest of a colon or slash.
                colon_pattern, colon_fwd = take(raw[idx:], ":")
                slash_pattern, slash_fwd = take(raw[idx:], "/")
                if colon_fwd < slash_fwd:
                    needle = colon_pattern
                    fwd = colon_fwd
                    has_flags = True
                else:
                    needle = slash_pattern
                    fwd = slash_fwd
                    has_flags = False
                idx += fwd

                if needle:
                    # If we don't see the second colon here, that means we are looking at
                    # single-character flags. Only check this if pattern is not null though.
                    case_insensitive = False
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

                    pattern = Pattern(needle, case_insensitive=case_insensitive)

        # Then let's start parsing the action!
        valid_actions = [
            "replace",
            "sed",
            "split",
            "add",
            "delete",
        ]
        for va in valid_actions:
            if raw[idx:].startswith(va + ":"):
                action_kind = va
                idx += len(va) + 1
                break
            if raw[idx:] == va:
                action_kind = va
                idx += len(va)
                break
        else:
            feedback = f"Invalid action kind: must be one of {{{", ".join(valid_actions)}}}."
            if idx == 0 and ":" in raw:
                feedback += " If this is pointing at your pattern, you forgot to put a `/` between the matcher section and the action section."
            raise RuleSyntaxError(**err, index=idx, feedback=feedback)

        # Validate that the action type is supported for the given tags.
        if action_kind == "split" or action_kind == "add":
            single_valued_tags = [t for t in tags if t in SINGLE_VALUE_TAGS]
            if single_valued_tags:
                raise InvalidRuleError(
                    f"Single valued tags {", ".join(single_valued_tags)} cannot be modified by multi-value action {action_kind}"
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

        action = Action(behavior=behavior, tags=tags, pattern=pattern)
        logger.debug(f"Parsed rule action {raw=} {matcher=} as {action=}")
        return action


@dataclasses.dataclass
class Rule:
    matcher: Matcher
    actions: list[Action]
    ignore: list[Matcher]

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
    ) -> Rule:
        parsed_matcher = Matcher.parse(matcher)
        return Rule(
            matcher=parsed_matcher,
            actions=[Action.parse(a, i + 1, parsed_matcher) for i, a in enumerate(actions)],
            ignore=[Matcher.parse(v, rule_name="ignore") for v in (ignore or [])],
        )


def take(x: str, until: Literal[":", "/"], consume_until: bool = True) -> tuple[str, int]:
    """
    Reads until the next unescaped `until` or end of string is found. Returns the read string and
    the number of characters consumed from the input. `until` is counted (in the returned int) as
    consumed if `consume_until` is true, though it is never included in the returned string.

    The returned string is unescaped; that is, `//` become `/` and `::` become `:`.
    """
    # Loop until we find an unescaped colon.
    match = ""
    fwd = 0
    while True:
        match_, fwd_ = _take_escaped(x[fwd:], until, consume_until)
        match += match_.replace("::", ":").replace("//", "/")
        fwd += fwd_

        next_idx = fwd + (0 if consume_until else 1)
        escaped_special_char = x[next_idx:].startswith(until)
        if not escaped_special_char:
            break
        match += until
        fwd = next_idx + 1

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

# TESTS

import re

import click
import pytest

from rose.rule_parser import (
    Action,
    AddAction,
    DeleteAction,
    InvalidRuleError,
    Matcher,
    Pattern,
    ReplaceAction,
    Rule,
    RuleSyntaxError,
    SedAction,
    SplitAction,
    take,
)


def test_rule_str() -> None:
    rule = Rule.parse("tracktitle:Track", ["releaseartist,genre/replace:lalala"])
    assert str(rule) == "matcher=tracktitle:Track action=releaseartist,genre/replace:lalala"

    # Test that rules are quoted properly.
    rule = Rule.parse(r"tracktitle,releaseartist,genre::: ", [r"sed::::; "])
    assert (
        str(rule)
        == r"matcher='tracktitle,releaseartist,genre::: ' action='tracktitle,releaseartist,genre::: /sed::::; '"
    )

    # Test that custom action matcher is printed properly.
    rule = Rule.parse("tracktitle:Track", ["genre:lala/replace:lalala"])
    assert str(rule) == "matcher=tracktitle:Track action=genre:lala/replace:lalala"

    # Test that we print `matched` when action pattern is not null.
    rule = Rule.parse("genre:b", ["genre:h/replace:hi"])
    assert str(rule) == r"matcher=genre:b action=genre:h/replace:hi"


def test_rule_parse_matcher() -> None:
    assert Matcher.parse("tracktitle:Track") == Matcher(["tracktitle"], Pattern("Track"))
    assert Matcher.parse("tracktitle,tracknumber:Track") == Matcher(["tracktitle", "tracknumber"], Pattern("Track"))
    assert Matcher.parse(r"tracktitle,tracknumber:Tr::ck") == Matcher(["tracktitle", "tracknumber"], Pattern("Tr:ck"))
    assert Matcher.parse("tracktitle,tracknumber:Track:i") == Matcher(
        ["tracktitle", "tracknumber"], Pattern("Track", case_insensitive=True)
    )
    assert Matcher.parse(r"tracktitle:") == Matcher(["tracktitle"], Pattern(""))

    assert Matcher.parse("tracktitle:^Track") == Matcher(["tracktitle"], Pattern("Track", strict_start=True))
    assert Matcher.parse("tracktitle:Track$") == Matcher(["tracktitle"], Pattern("Track", strict_end=True))
    assert Matcher.parse(r"tracktitle:\^Track") == Matcher(["tracktitle"], Pattern(r"\^Track"))
    assert Matcher.parse(r"tracktitle:Track\$") == Matcher(["tracktitle"], Pattern(r"Track\$"))
    assert Matcher.parse(r"tracktitle:\^Track\$") == Matcher(["tracktitle"], Pattern(r"\^Track\$"))

    def test_err(rule: str, err: str) -> None:
        with pytest.raises(RuleSyntaxError) as exc:
            Matcher.parse(rule)
        assert click.unstyle(str(exc.value)) == err

    test_err(
        "tracknumber^Track$",
        """\
Failed to parse matcher, invalid syntax:

    tracknumber^Track$
    ^
    Invalid tag: must be one of {tracktitle, trackartist, trackartist[main], trackartist[guest], trackartist[remixer], trackartist[producer], trackartist[composer], trackartist[conductor], trackartist[djmixer], tracknumber, tracktotal, discnumber, disctotal, releasetitle, releaseartist, releaseartist[main], releaseartist[guest], releaseartist[remixer], releaseartist[producer], releaseartist[composer], releaseartist[conductor], releaseartist[djmixer], releasetype, releasedate, originaldate, compositiondate, edition, catalognumber, genre, secondarygenre, descriptor, label, new, artist}. The next character after a tag must be ':' or ','.
""",
    )

    test_err(
        "tracknumber",
        """\
Failed to parse matcher, invalid syntax:

    tracknumber
               ^
               Expected to find ',' or ':', found end of string.
""",
    )

    test_err(
        "tracktitle:Tr:ck",
        """\
Failed to parse matcher, invalid syntax:

    tracktitle:Tr:ck
                  ^
                  Unrecognized flag: Please specify one of the supported flags: `i` (case insensitive).
""",
    )

    test_err(
        "tracktitle:hi:i:hihi",
        """\
Failed to parse matcher, invalid syntax:

    tracktitle:hi:i:hihi
                    ^
                    Extra input found after end of matcher. Perhaps you meant to escape this colon?
""",
    )


def test_rule_parse_action() -> None:
    assert Action.parse(
        "replace:lalala",
        matcher=Matcher(tags=["tracktitle"], pattern=Pattern("haha")),
    ) == Action(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["tracktitle"],
        pattern=Pattern("haha"),
    )
    assert Action.parse("genre/replace:lalala") == Action(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["genre"],
        pattern=None,
    )
    assert Action.parse("tracknumber,genre/replace:lalala") == Action(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["tracknumber", "genre"],
        pattern=None,
    )
    assert Action.parse("genre:lala/replace:lalala") == Action(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["genre"],
        pattern=Pattern("lala"),
    )
    assert Action.parse("genre:lala:i/replace:lalala") == Action(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["genre"],
        pattern=Pattern("lala", case_insensitive=True),
    )
    assert Action.parse(
        "matched:^x/replace:lalala",
        matcher=Matcher(tags=["tracktitle"], pattern=Pattern("haha")),
    ) == Action(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["tracktitle"],
        pattern=Pattern("^x"),
    )

    # Test that case insensitivity is inherited from the matcher.
    assert Action.parse(
        "replace:lalala",
        matcher=Matcher(tags=["tracktitle"], pattern=Pattern("haha", case_insensitive=True)),
    ) == Action(
        behavior=ReplaceAction(replacement="lalala"),
        tags=["tracktitle"],
        pattern=Pattern("haha", case_insensitive=True),
    )

    # Test that the action excludes the immutable *total tags.
    assert Action.parse(
        "replace:5",
        matcher=Matcher(
            tags=["tracknumber", "tracktotal", "discnumber", "disctotal"],
            pattern=Pattern("1"),
        ),
    ) == Action(
        behavior=ReplaceAction(replacement="5"),
        tags=["tracknumber", "discnumber"],
        pattern=Pattern("1"),
    )

    assert Action.parse(
        "sed:lalala:hahaha",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    ) == Action(
        behavior=SedAction(src=re.compile("lalala"), dst="hahaha"),
        tags=["genre"],
        pattern=Pattern("haha"),
    )
    assert Action.parse(
        r"split:::",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    ) == Action(
        behavior=SplitAction(delimiter=":"),
        tags=["genre"],
        pattern=Pattern("haha"),
    )
    assert Action.parse(
        r"split:::",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    ) == Action(
        behavior=SplitAction(delimiter=":"),
        tags=["genre"],
        pattern=Pattern("haha"),
    )
    assert Action.parse(
        r"add:cute",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    ) == Action(
        behavior=AddAction(value="cute"),
        tags=["genre"],
        pattern=Pattern("haha"),
    )
    assert Action.parse(
        r"delete:",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    ) == Action(
        behavior=DeleteAction(),
        tags=["genre"],
        pattern=Pattern("haha"),
    )
    assert Action.parse(
        r"delete:",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    ) == Action(
        behavior=DeleteAction(),
        tags=["genre"],
        pattern=Pattern("haha"),
    )

    def test_err(rule: str, err: str, matcher: Matcher | None = None) -> None:
        with pytest.raises(RuleSyntaxError) as exc:
            Action.parse(rule, 1, matcher)
        assert click.unstyle(str(exc.value)) == err

    test_err(
        "tracktitle:hello/:delete",
        """\
Failed to parse action 1, invalid syntax:

    tracktitle:hello/:delete
                     ^
                     Invalid action kind: must be one of {replace, sed, split, add, delete}.
""",
    )

    test_err(
        "haha/delete",
        """\
Failed to parse action 1, invalid syntax:

    haha/delete
    ^
    Invalid tag: must be one of {tracktitle, trackartist, trackartist[main], trackartist[guest], trackartist[remixer], trackartist[producer], trackartist[composer], trackartist[conductor], trackartist[djmixer], tracknumber, discnumber, releasetitle, releaseartist, releaseartist[main], releaseartist[guest], releaseartist[remixer], releaseartist[producer], releaseartist[composer], releaseartist[conductor], releaseartist[djmixer], releasetype, releasedate, originaldate, compositiondate, edition, catalognumber, genre, secondarygenre, descriptor, label, new, artist}. The next character after a tag must be ':' or ','.
""",
    )

    test_err(
        "tracktitler/delete",
        """\
Failed to parse action 1, invalid syntax:

    tracktitler/delete
    ^
    Invalid tag: must be one of {tracktitle, trackartist, trackartist[main], trackartist[guest], trackartist[remixer], trackartist[producer], trackartist[composer], trackartist[conductor], trackartist[djmixer], tracknumber, discnumber, releasetitle, releaseartist, releaseartist[main], releaseartist[guest], releaseartist[remixer], releaseartist[producer], releaseartist[composer], releaseartist[conductor], releaseartist[djmixer], releasetype, releasedate, originaldate, compositiondate, edition, catalognumber, genre, secondarygenre, descriptor, label, new, artist}. The next character after a tag must be ':' or ','.
""",
    )

    test_err(
        "tracktitle:haha:delete",
        """\
Failed to parse action 1, invalid syntax:

    tracktitle:haha:delete
    ^
    Invalid action kind: must be one of {replace, sed, split, add, delete}. If this is pointing at your pattern, you forgot to put a `/` between the matcher section and the action section.
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "tracktitle:haha:sed/hi:bye",
        """\
Failed to parse action 1, invalid syntax:

    tracktitle:haha:sed/hi:bye
                    ^
                    Unrecognized flag: Either you forgot a colon here (to end the matcher), or this is an invalid matcher flag. The only supported flag is `i` (case insensitive).
""",
    )

    test_err(
        "hahaha",
        """\
Failed to parse action 1, invalid syntax:

    hahaha
    ^
    Invalid action kind: must be one of {replace, sed, split, add, delete}.
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "replace",
        """\
Failed to parse action 1, invalid syntax:

    replace
           ^
           Replacement not found: must specify a non-empty replacement. Use the delete action to remove a value.
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )
    test_err(
        "replace:haha:",
        """\
Failed to parse action 1, invalid syntax:

    replace:haha:
                ^
                Found another section after the replacement, but the replacement must be the last section. Perhaps you meant to escape this colon?
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "sed",
        """\
Failed to parse action 1, invalid syntax:

    sed
       ^
       Empty sed pattern found: must specify a non-empty pattern. Example: sed:pattern:replacement
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "sed:hihi",
        """\
Failed to parse action 1, invalid syntax:

    sed:hihi
            ^
            Sed replacement not found: must specify a sed replacement section. Example: sed:hihi:replacement.
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "sed:invalid[",
        """\
Failed to parse action 1, invalid syntax:

    sed:invalid[
        ^
        Failed to compile the sed pattern regex: invalid pattern: unterminated character set at position 7
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "sed:hihi:byebye:",
        """\
Failed to parse action 1, invalid syntax:

    sed:hihi:byebye:
                   ^
                   Found another section after the sed replacement, but the sed replacement must be the last section. Perhaps you meant to escape this colon?
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "split",
        """\
Failed to parse action 1, invalid syntax:

    split
         ^
         Delimiter not found: must specify a non-empty delimiter to split on.
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "split:hi:",
        """\
Failed to parse action 1, invalid syntax:

    split:hi:
            ^
            Found another section after the delimiter, but the delimiter must be the last section. Perhaps you meant to escape this colon?
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "split:",
        """\
Failed to parse action 1, invalid syntax:

    split:
          ^
          Delimiter not found: must specify a non-empty delimiter to split on.
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "add",
        """\
Failed to parse action 1, invalid syntax:

    add
       ^
       Value not found: must specify a non-empty value to add.
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "add:hi:",
        """\
Failed to parse action 1, invalid syntax:

    add:hi:
          ^
          Found another section after the value, but the value must be the last section. Perhaps you meant to escape this colon?
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "add:",
        """\
Failed to parse action 1, invalid syntax:

    add:
        ^
        Value not found: must specify a non-empty value to add.
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "delete:h",
        """\
Failed to parse action 1, invalid syntax:

    delete:h
           ^
           Found another section after the action kind, but the delete action has no parameters. Please remove this section.
""",
        matcher=Matcher(tags=["genre"], pattern=Pattern("haha")),
    )

    test_err(
        "delete",
        """\
Failed to parse action 1, invalid syntax:

    delete
    ^
    Tags/pattern section not found. Must specify tags to modify, since there is no matcher to default to. Make sure you are formatting your action like {tags}:{pattern}/{kind}:{args} (where `:{pattern}` is optional)
""",
    )

    test_err(
        "tracktotal/replace:1",
        """\
Failed to parse action 1, invalid syntax:

    tracktotal/replace:1
    ^
    Invalid tag: tracktotal is not modifiable.
""",
    )

    test_err(
        "disctotal/replace:1",
        """\
Failed to parse action 1, invalid syntax:

    disctotal/replace:1
    ^
    Invalid tag: disctotal is not modifiable.
""",
    )


@pytest.mark.parametrize(
    ("matcher", "action"),
    [("tracktitle:Track", "delete")],
)
def test_rule_parsing_end_to_end_1(matcher: str, action: str) -> None:
    assert str(Rule.parse(matcher, [action])) == f"matcher={matcher} action={matcher}/{action}"


@pytest.mark.parametrize(
    ("matcher", "action"),
    [
        (r"tracktitle:\^Track", "delete"),
        (r"tracktitle:Track\$", "delete"),
        (r"tracktitle:\^Track\$", "delete"),
    ],
)
def test_rule_parsing_end_to_end_2(matcher: str, action: str) -> None:
    assert str(Rule.parse(matcher, [action])) == f"matcher='{matcher}' action='{matcher}/{action}'"


@pytest.mark.parametrize(
    ("matcher", "action"),
    [
        ("tracktitle:Track", "genre:lala/replace:lalala"),
        ("tracktitle,genre,trackartist:Track", "tracktitle,genre,artist/delete"),
    ],
)
def test_rule_parsing_end_to_end_3(matcher: str, action: str) -> None:
    assert str(Rule.parse(matcher, [action])) == f"matcher={matcher} action={action}"


def test_rule_parsing_multi_value_validation() -> None:
    with pytest.raises(InvalidRuleError) as e:
        Rule.parse("tracktitle:h", ["split:x"])
    assert str(e.value) == "Single valued tags tracktitle cannot be modified by multi-value action split"
    with pytest.raises(InvalidRuleError):
        Rule.parse("tracktitle:h", ["split:x"])
    assert str(e.value) == "Single valued tags tracktitle cannot be modified by multi-value action split"
    with pytest.raises(InvalidRuleError):
        Rule.parse("genre:h", ["tracktitle/split:x"])
    assert str(e.value) == "Single valued tags tracktitle cannot be modified by multi-value action split"
    with pytest.raises(InvalidRuleError):
        Rule.parse("genre:h", ["split:y", "tracktitle/split:x"])
    assert str(e.value) == "Single valued tags tracktitle cannot be modified by multi-value action split"


def test_rule_parsing_defaults() -> None:
    rule = Rule.parse("tracktitle:Track", ["replace:hi"])
    assert rule.actions[0].pattern is not None
    assert rule.actions[0].pattern.needle == "Track"
    rule = Rule.parse("tracktitle:Track", ["tracktitle/replace:hi"])
    assert rule.actions[0].pattern is not None
    assert rule.actions[0].pattern.needle == "Track"
    rule = Rule.parse("tracktitle:Track", ["tracktitle:Lack/replace:hi"])
    assert rule.actions[0].pattern is not None
    assert rule.actions[0].pattern.needle == "Lack"


def test_parser_take() -> None:
    assert take("hello", ":") == ("hello", 5)
    assert take("hello:hi", ":") == ("hello", 6)
    assert take(r"h::lo:hi", ":") == ("h:lo", 6)
    assert take(r"h:://lo:hi", ":") == ("h:/lo", 8)
    assert take(r"h::lo/hi", "/") == ("h:lo", 6)
    assert take(r"h:://lo/hi", "/") == ("h:/lo", 8)
