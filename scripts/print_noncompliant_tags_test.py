from pathlib import Path

import pytest
from rose import Config, list_releases

from scripts.print_noncompliant_tags import find_noncompliant_tags, format_report, read_descriptors_file

KNOWN_GENRES = ["Techno", "Deep House", "Ambient", "Orchestral Music", "Modern Classical"]


@pytest.mark.usefixtures("seeded_cache")
def test_find_noncompliant_tags(config: Config) -> None:
    # Case mismatches are noncompliant: the cache has `Deep House`, `Warm`, and `Hot`.
    known_genres = [*KNOWN_GENRES[:1], "Deep house", *KNOWN_GENRES[2:]]
    known_descriptors = ["warm", "Wet"]
    noncompliant = find_noncompliant_tags(list_releases(config), known_genres, known_descriptors)
    assert {f: {v: [r.id for r in rs] for v, rs in vs.items()} for f, vs in noncompliant.items()} == {
        "genre": {"Deep House": ["r1"]},
        "secondary genre": {"Rominimal": ["r1"]},
        "descriptor": {"Warm": ["r1"], "Hot": ["r1"]},
    }

    r1 = config.music_source_dir / "r1"
    assert (
        format_report(noncompliant, known_genres, known_descriptors)
        == f"""\
1 noncompliant genre
  Deep House (case mismatch: known as Deep house)
    {r1}

1 noncompliant secondary genre
  Rominimal
    {r1}

2 noncompliant descriptors
  Hot
    {r1}
  Warm (case mismatch: known as warm)
    {r1}
"""
    )


@pytest.mark.usefixtures("seeded_cache")
def test_find_noncompliant_tags_without_descriptors(config: Config) -> None:
    noncompliant = find_noncompliant_tags(list_releases(config), [*KNOWN_GENRES, "Rominimal"], None)
    assert noncompliant == {"genre": {}, "secondary genre": {}}
    assert (
        format_report(noncompliant, KNOWN_GENRES, None) == "0 noncompliant genres\n\n0 noncompliant secondary genres\n"
    )


def test_read_descriptors_file(tmp_path: Path) -> None:
    path = tmp_path / "descriptors.txt"
    path.write_text("# comment\nwarm\n\n  melancholic  \n")
    assert read_descriptors_file(path) == ["warm", "melancholic"]
