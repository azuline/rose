"""
The rules module implements the Rules Engine for updating metadata.

There are 3 major components in this module:

- Rules Engine: A Python function that accepts, previews, and execute rules.
- TOML Parser: Parses TOML-encoded rules and returns the Python dataclass.
- DSL: A small language for defining rules, intended for use in the shell.
"""


import logging
import re
from dataclasses import dataclass

from rose.config import Config

logger = logging.getLogger(__name__)


def execute_stored_rules(c: Config) -> None:
    pass


@dataclass
class ReplaceAction:
    replacement: str | list[str]


@dataclass
class SedAction:
    src: re.Pattern
    dst: re.Pattern


@dataclass
class SplitAction:
    delimiter: str


@dataclass
class UpdateRule:
    matcher: str
    action: ReplaceAction | SedAction | SplitAction


def execute_rule(c: Config, rule: UpdateRule) -> None:
    # 1. Matcher
    pass
    # 2. Action
    pass


def execute_rule(c: Config, rule: UpdateRule) -> None:
    pass


def parse_toml_rule(c: Config, toml: str) -> UpdateRule:
    pass


def parse_dsl_rule(c: Config, text: str) -> UpdateRule:
    pass
