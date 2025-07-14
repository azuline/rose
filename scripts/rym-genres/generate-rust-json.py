#!/usr/bin/env python3
"""
Generate genre_hierarchy.json for rose-rs from the rym.txt genre data.
This script parses the RYM genre hierarchy and outputs a JSON file.
"""

import json
from dataclasses import dataclass
from pathlib import Path


@dataclass
class Genre:
    """Represents a genre with its parent and child relationships."""
    name: str
    parents: list["Genre"]
    children: list["Genre"]

    def __repr__(self) -> str:
        return f"Genre(name='{self.name}', parents=[{len(self.parents)}], children=[{len(self.children)}])"


def parse_rym_genres():
    """Parse the rym.txt file and build the genre hierarchy."""
    genres_file = Path(__file__).parent / "rym.txt"
    
    with genres_file.open("r") as fp:
        raw_text = fp.read().splitlines()
    
    # A DAG of genres. Genres can have multiple parents; thus, this is not a tree.
    genres: dict[str, Genre] = {}
    # State as we iterate. Stores the immediate parental lineage of the previous node.
    current_parents: list[Genre] = []
    
    for line in raw_text:
        # Calculate indentation level (4 spaces per level)
        indent = (len(line) - len(line.lstrip(" "))) // 4
        # Extract genre name, removing ::genre suffix and leading spaces
        genre_name = line.removesuffix("::genre").lstrip(" ")
        
        # Get the parent of the current node
        parent = current_parents[indent - 1] if indent else None
        
        # Get or create the Genre object
        if genre_name in genres:
            genre = genres[genre_name]
        else:
            genre = Genre(
                name=genre_name,
                parents=[],
                children=[],
            )
            genres[genre_name] = genre
        
        # Update the parent's children
        if parent:
            # The RYM doc has parents being labels and children being `::genre`s.
            # We treat everything as genres; skip self-references
            if genre == parent:
                continue
            genre.parents.append(parent)
            parent.children.append(genre)
        
        # Update the parents and index of the current node
        current_parents = current_parents[:indent]
        current_parents.append(genre)
    
    return genres


def build_transitive_parents(genres):
    """Build the transitive parent mapping for all genres."""
    BLACKLISTED = {"Uncategorised", "Descriptor"}
    
    transitive_parents: dict[str, list[str]] = {}
    
    for g in genres.values():
        if g.name in BLACKLISTED:
            continue
        
        # Do a graph traversal to get all the transitive parents
        parents: set[str] = set()
        unvisited: list[str] = [g.name]
        
        while unvisited:
            cur = unvisited.pop()
            if cur in BLACKLISTED:
                continue
            cur_parents = {x.name for x in genres[cur].parents} - parents
            parents.update(cur_parents)
            unvisited.extend(cur_parents)
        
        transitive_parents[g.name] = sorted(parents - BLACKLISTED)
    
    return transitive_parents


def main():
    # Parse the genre hierarchy
    genres = parse_rym_genres()
    
    # Build the transitive parent mapping
    transitive_parents = build_transitive_parents(genres)
    
    # Output path for the JSON file
    output_path = Path(__file__).parent.parent.parent / "rose-rs" / "src" / "genre_hierarchy.json"
    
    # Ensure the output directory exists
    output_path.parent.mkdir(parents=True, exist_ok=True)
    
    # Write the JSON file
    with open(output_path, 'w', encoding='utf-8') as f:
        json.dump(transitive_parents, f, ensure_ascii=False, indent=2, sort_keys=True)
    
    print(f"Generated {output_path}")
    print(f"Total genres: {len(transitive_parents)}")


if __name__ == "__main__":
    main()