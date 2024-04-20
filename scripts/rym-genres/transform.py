#!/usr/bin/env python

from pathlib import Path
import logging
import sys
from pprint import pprint
from dataclasses import dataclass

logger = logging.getLogger()
logger.setLevel(logging.INFO)
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

# A (hopefully) DAG of genres. Genres can have multiple parents; thus, this is not a tree.
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
            parents=[parent] if parent else [],
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
        parent.children.append(genre)
        logger.debug("    Updated parent's child list")
    # Update the parents and index of the current node.
    current_parents = current_parents[:indent]
    current_parents.append(genre)
    logger.debug(f"    Finished, updating {current_parents=}")

# Genres is now assembled. Let's assemble write our outputs.
#
# 1. A mapping from child to all transient parent genres. This is for populating the `parent_genres`
#    field and including all sub-genres in the top-level parent.
# 2. A mapping from parent to immediate child genres. This is for populating the virtual
#    filesystem's genre sub-directories.

TRANSIENT_PARENTS: dict[str, list[str]] = {}
for g in genres.values():
    # Do a graph traversal to get all the transient parents.
    parents: set[str] = set()
    unvisited: list[str] = [g.name]
    while unvisited:
        cur = unvisited.pop()
        cur_parents = {x.name for x in genres[cur].parents} - parents
        parents.update(cur_parents)
        unvisited.extend(cur_parents)
    TRANSIENT_PARENTS[g.name] = sorted(parents)

CHILDREN: dict[str, list[str]] = {}
for g in genres.values():
    CHILDREN[g.name] = sorted([x.name for x in g.children])

with open("genres.gen.py", "w") as fp:
    fp.write(f"""\
# All transient parent genres.
PARENT_GENRES = {repr(TRANSIENT_PARENTS)}

# The immediate children genres.
CHILDREN_GENRES = {repr(CHILDREN)}
""")
