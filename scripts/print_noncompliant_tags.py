#!/usr/bin/env python

"""
Print the genres, secondary genres, and descriptors in the read cache that are not part of the
known vocabulary, along with the releases that use them.

Genres and secondary genres are checked against the RYM genre hierarchy in
rose/genre_hierarchy.py. Descriptors are checked against the Descriptors section of the vendored
RYM dump in scripts/rym-genres/rym.txt, or against a file of known descriptors (one per line)
passed with --descriptors. Comparison is exact: a tag that differs from a known value only by case
is noncompliant, and the known spelling is printed beside it.

This reads the cache as-is; run `rose cache update` first if the cache may be stale.
"""

import argparse
import sys
from collections import defaultdict
from collections.abc import Iterable
from pathlib import Path

from rose import Config, Release, RoseError, list_releases
from rose.genre_hierarchy import TRANSITIVE_PARENT_GENRES

FIELDS = ["genre", "secondary genre", "descriptor"]

RYM_FILE = Path(__file__).parent / "rym-genres" / "rym.txt"
RYM_DESCRIPTORS_SECTION = "Descriptors"
RYM_DESCRIPTOR_SUFFIX = "::mood"


class DescriptorsSectionNotFoundError(RoseError):
    pass


def find_noncompliant_tags(
    releases: Iterable[Release],
    known_genres: Iterable[str],
    known_descriptors: Iterable[str],
) -> dict[str, dict[str, list[Release]]]:
    """Returns field -> noncompliant value -> releases."""
    genres = set(known_genres)
    known = {"genre": genres, "secondary genre": genres, "descriptor": set(known_descriptors)}
    rval: dict[str, dict[str, list[Release]]] = {f: defaultdict(list) for f in known}
    for r in releases:
        tags = {"genre": r.genres, "secondary genre": r.secondary_genres, "descriptor": r.descriptors}
        for field, known_values in known.items():
            for value in tags[field]:
                if value not in known_values:
                    rval[field][value].append(r)
    return rval


def format_report(
    noncompliant: dict[str, dict[str, list[Release]]],
    known_genres: Iterable[str],
    known_descriptors: Iterable[str],
) -> str:
    genres_by_casefold = {x.casefold(): x for x in known_genres}
    suggestions = {
        "genre": genres_by_casefold,
        "secondary genre": genres_by_casefold,
        "descriptor": {x.casefold(): x for x in known_descriptors},
    }
    lines: list[str] = []
    for field in FIELDS:
        values = noncompliant[field]
        lines.append(f"{len(values)} noncompliant {field}{'' if len(values) == 1 else 's'}")
        for value in sorted(values):
            suggestion = suggestions[field].get(value.casefold())
            lines.append(f"  {value}" + (f" (case mismatch: known as {suggestion})" if suggestion else ""))
            lines.extend(f"    {r.source_path}" for r in sorted(values[value], key=lambda r: r.source_path))
        lines.append("")
    return "\n".join(lines)


def read_rym_descriptors(*, path: Path = RYM_FILE) -> list[str]:
    """
    Read the descriptors out of the RYM dump. Descriptors are the `::mood` leaves of its top-level
    Descriptors section; the other lines there are category headings.
    """
    descriptors: set[str] = set()
    in_section = False
    with path.open("r") as fp:
        for line in fp.read().splitlines():
            if not line.startswith(" "):
                in_section = line == RYM_DESCRIPTORS_SECTION
            elif in_section and line.endswith(RYM_DESCRIPTOR_SUFFIX):
                descriptors.add(line.strip().removesuffix(RYM_DESCRIPTOR_SUFFIX))
    if not descriptors:
        raise DescriptorsSectionNotFoundError(f"No {RYM_DESCRIPTORS_SECTION} section found in {path}")
    return sorted(descriptors)


def read_descriptors_file(path: Path) -> list[str]:
    with path.open("r") as fp:
        return [x for x in (line.strip() for line in fp) if x and not x.startswith("#")]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--config", "-c", type=Path, help="Override the config file location.")
    parser.add_argument(
        "--descriptors",
        type=Path,
        help="File of known descriptors, one per line. Defaults to the descriptors in the vendored RYM dump.",
    )
    args = parser.parse_args()

    config = Config.parse(config_path_override=args.config)
    known_genres = list(TRANSITIVE_PARENT_GENRES)
    known_descriptors = read_descriptors_file(args.descriptors) if args.descriptors else read_rym_descriptors()

    noncompliant = find_noncompliant_tags(list_releases(config), known_genres, known_descriptors)
    print(format_report(noncompliant, known_genres, known_descriptors), end="")
    return 1 if any(noncompliant.values()) else 0


if __name__ == "__main__":
    sys.exit(main())
