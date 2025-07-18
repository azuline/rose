#!/usr/bin/env python

import json
import logging
import os
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

logger = logging.getLogger()
# logger.setLevel(logging.INFO)
logger.setLevel(logging.DEBUG)
formatter = logging.Formatter(
    "[%(asctime)s] %(levelname)s: %(message)s",
    datefmt="%H:%M:%S",
)
stream_handler = logging.StreamHandler(sys.stderr)
stream_handler.setFormatter(formatter)
logger.addHandler(stream_handler)


GENRES_FILE = Path(__file__).parent / "rym.txt"


@dataclass
class Genre:
    # Abuse Python's everything-is-a-reference.
    name: str
    parents: list["Genre"]
    children: list["Genre"]

    def __repr__(self) -> str:
        return f"Genre(name='{self.name}', parents=[{len(self.parents)}], children=[{len(self.children)}])"


with GENRES_FILE.open("r") as fp:
    raw_text = fp.read().splitlines()

# A DAG of genres. Genres can have multiple parents; thus, this is not a tree.
genres: dict[str, Genre] = {}
# State as we iterate. Stores the immediate parental lineage of the previous node.
current_parents: list[Genre] = []

for line in raw_text:
    logger.debug(f"Handling {line=}")
    logger.debug(f"    {current_parents=}")
    indent = (len(line) - len(line.lstrip(" "))) // 4
    logger.debug(f"    {indent=}")
    genre_name = line.removesuffix("::genre").lstrip(" ")
    logger.debug(f"    {genre_name=}")
    # Get the parent of the current node.
    parent = current_parents[indent - 1] if indent else None
    logger.debug(f"    {parent=}")
    # Get or create the Genre object.
    try:
        genre = genres[genre_name]
        logger.debug(f"    Found existing {genre=}")
    except KeyError:
        genre = Genre(
            name=genre_name,
            parents=[],
            children=[],
        )
        genres[genre_name] = genre
        logger.debug(f"    Created new {genre=}")
    # Update the parent's children.
    if parent:
        # The RYM doc has parents being labels and childrens being `::genre`s. We just treat
        # everything as genres; consequentally, all parents have themselves as children. Remedy that
        # here.
        if genre == parent:
            continue
        genre.parents.append(parent)
        logger.debug("    Updated child's parent list")
        parent.children.append(genre)
        logger.debug("    Updated parent's child list")
    # Update the parents and index of the current node.
    current_parents = current_parents[:indent]
    current_parents.append(genre)
    logger.debug(f"    Finished, updating {current_parents=}")

# Sort genres.
genres = dict(sorted(genres.items()))

# Genres is now assembled. Let's assemble write our outputs.
#
# 1. A mapping from child to all transitive parent genres. This is for populating the `parent_genres`
#    field and including all sub-genres in the top-level parent.
# 2. A mapping from parent to all transitive child genres. This is for existence checking of parent
#    genres.
# 3. A mapping from parent to immediate child genres. This is for populating the virtual
#    filesystem's genre sub-directories.

logger.debug("=== BUILDING LISTS ===")

BLACKLISTED = {"Uncategorised", "Descriptor"}

TRANSITIVE_PARENTS: dict[str, list[str]] = {}
for g in genres.values():
    if g.name in BLACKLISTED:
        continue
    # Do a graph traversal to get all the transitive parents.
    parents: set[str] = set()
    unvisited: list[str] = [g.name]
    logger.debug(f"Processing {g=}")
    while unvisited:
        cur = unvisited.pop()
        if cur in BLACKLISTED:
            continue
        cur_parents = {x.name for x in genres[cur].parents} - parents
        logger.debug(f"    Found new transitive parents {cur_parents=}")
        parents.update(cur_parents)
        unvisited.extend(cur_parents)
    TRANSITIVE_PARENTS[g.name] = sorted(parents - BLACKLISTED)

TRANSITIVE_CHILDREN: dict[str, list[str]] = {}
for g in genres.values():
    if g.name in BLACKLISTED:
        continue
    # Do a graph traversal to get all the transitive children.
    children: set[str] = set()
    unvisited = [g.name]
    logger.debug(f"Processing {g=}")
    while unvisited:
        cur = unvisited.pop()
        if cur in BLACKLISTED:
            continue
        cur_children = {x.name for x in genres[cur].children} - children
        logger.debug(f"    Found new transitive children {cur_children=}")
        children.update(cur_children)
        unvisited.extend(cur_children)
    TRANSITIVE_CHILDREN[g.name] = sorted(children - BLACKLISTED)

IMMEDIATE_CHILDREN: dict[str, list[str]] = {}
for g in genres.values():
    if g.name in BLACKLISTED:
        continue
    IMMEDIATE_CHILDREN[g.name] = sorted({x.name for x in g.children} - BLACKLISTED)

OUT_PATH_PYTHON = Path(os.environ["ROSE_ROOT"]) / "rose-py" / "rose" / "genre_hierarchy.py"
with OUT_PATH_PYTHON.open("w") as fp:
    fp.write(f"""\
# THIS FILE WAS GENERATED BY scripts/rym-genres/generate.py. DO NOT EDIT.

TRANSITIVE_PARENT_GENRES: dict[str, list[str]] = {repr(TRANSITIVE_PARENTS)}

TRANSITIVE_CHILD_GENRES: dict[str, list[str]] = {repr(TRANSITIVE_CHILDREN)}

IMMEDIATE_CHILD_GENRES: dict[str, list[str]] = {repr(IMMEDIATE_CHILDREN)}
""")
subprocess.run(["ruff", "format", str(OUT_PATH_PYTHON)])

rust_json = {
    "transitive_parent_genres": TRANSITIVE_PARENTS,
    "transitive_child_genres": TRANSITIVE_CHILDREN,
    "immediate_child_genres": IMMEDIATE_CHILDREN,
}
OUT_PATH_RUST = Path(os.environ["ROSE_ROOT"]) / "rose-rs" / "src" / "genre_hierarchy.json"
with OUT_PATH_RUST.open("w") as fp:
    json.dump(rust_json, fp)
