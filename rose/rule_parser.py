"""
The rule_parser module provides the typedef and parser for the rules engine. This is split out from
the rules engine in order to avoid a dependency cycle between the config module and the rules
module.
"""

from __future__ import annotations

import logging
import re
from dataclasses import dataclass
from typing import Any, Literal

from rose.common import RoseError

logger = logging.getLogger(__name__)


class InvalidRuleSpecError(RoseError):
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


SINGLE_VALUE_TAGS: list[Tag] = [
    "tracktitle",
    "year",
    "tracknumber",
    "discnumber",
    "albumtitle",
    "releasetype",
]

MULTI_VALUE_TAGS: list[Tag] = [
    "genre",
    "label",
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
class MetadataRule:
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
            r += "spliton:"
            r += self.action.delimiter
        elif isinstance(self.action, DeleteAction):
            r += "delete"
        return r

    @classmethod
    def parse_dict(cls, data: dict[str, Any]) -> MetadataRule:
        if not isinstance(data, dict):
            raise InvalidRuleSpecError(f"Type of metadata rule data must be dict: got {type(data)}")

        try:
            tags = data["tags"]
        except KeyError as e:
            raise InvalidRuleSpecError("Key `tags` not found") from e
        if isinstance(tags, str):
            tags = [tags]
        if not isinstance(tags, list):
            raise InvalidRuleSpecError(
                f"Key `tags` must be a string or a list of strings: got {type(tags)}"
            )
        for t in tags:
            if t not in ALL_TAGS:
                raise InvalidRuleSpecError(
                    f"Key `tags`'s values must be one of {', '.join(ALL_TAGS)}: got {t}"
                )

        try:
            matcher = data["matcher"]
        except KeyError as e:
            raise InvalidRuleSpecError("Key `matcher` not found") from e
        if not isinstance(matcher, str):
            raise InvalidRuleSpecError(f"Key `matcher` must be a string: got {type(matcher)}")

        try:
            action_dict = data["action"]
        except KeyError as e:
            raise InvalidRuleSpecError("Key `action` not found") from e
        if not isinstance(action_dict, dict):
            raise InvalidRuleSpecError(
                f"Key `action` must be a dictionary: got {type(action_dict)}"
            )

        try:
            action_kind = action_dict["kind"]
        except KeyError as e:
            raise InvalidRuleSpecError("Key `action.kind` not found") from e

        action: ReplaceAction | ReplaceAllAction | SedAction | SplitAction | DeleteAction
        if action_kind == "replace":
            try:
                action = ReplaceAction(replacement=action_dict["replacement"])
            except KeyError as e:
                raise InvalidRuleSpecError("Key `action.replacement` not found") from e
            if not isinstance(action.replacement, str):
                raise InvalidRuleSpecError(
                    f"Key `action.replacement` must be a string: got {type(action.replacement)}"
                )
        elif action_kind == "replaceall":
            try:
                action = ReplaceAllAction(replacement=action_dict["replacement"])
            except KeyError as e:
                raise InvalidRuleSpecError("Key `action.replacement` not found") from e
            if not isinstance(action.replacement, list):
                raise InvalidRuleSpecError(
                    "Key `action.replacement` must be a list of strings: "
                    f"got {type(action.replacement)}"
                )
            for t in action.replacement:
                if not isinstance(t, str):
                    raise InvalidRuleSpecError(
                        f"Key `action.replacement`'s values must be strings: got {type(t)}"
                    )
        elif action_kind == "sed":
            try:
                action_src = re.compile(action_dict["src"])
            except KeyError as e:
                raise InvalidRuleSpecError("Key `action.src` not found") from e
            except re.error as e:
                raise InvalidRuleSpecError(
                    "Key `action.src` contains an invalid regular expression"
                ) from e

            try:
                action_dst = action_dict["dst"]
            except KeyError as e:
                raise InvalidRuleSpecError("Key `action.dst` not found") from e
            if not isinstance(action_dst, str):
                raise InvalidRuleSpecError(
                    f"Key `action.dst` must be a string: got {type(action_dst)}"
                )

            action = SedAction(src=action_src, dst=action_dst)
        elif action_kind == "spliton":
            try:
                action = SplitAction(delimiter=action_dict["delimiter"])
            except KeyError as e:
                raise InvalidRuleSpecError("Key `action.delimiter` not found") from e
            if not isinstance(action.delimiter, str):
                raise InvalidRuleSpecError(
                    f"Key `action.delimiter` must be a string: got {type(action.delimiter)}"
                )
        elif action_kind == "delete":
            action = DeleteAction()
        else:
            raise InvalidRuleSpecError(
                "Key `action.kind` must be one of replace, replaceall, sed, spliton, delete: "
                f"got {action_kind}"
            )

        # Validate that the action kind and tags are acceptable. Mainly that we are not calling
        # `replaceall` and `splitall` on single-valued tags.
        multi_value_action = action_kind == "replaceall" or action_kind == "spliton"
        if multi_value_action:
            single_valued_tags = [t for t in tags if t in SINGLE_VALUE_TAGS]
            if single_valued_tags:
                raise InvalidRuleSpecError(
                    f"Single valued tags {', '.join(single_valued_tags)} cannot be modified by "
                    f"multi-value action {action_kind}"
                )

        return cls(
            tags=tags,
            matcher=matcher,
            action=action,
        )
