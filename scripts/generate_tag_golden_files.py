#!/usr/bin/env python3
"""
Generate golden JSON files from Python audio tag reading for Rust port validation.

This script reads all audio files in testdata/Tagger/ using the Python AudioTags
implementation and dumps the parsed tags as JSON golden files. The Rust port can
then test against these golden files to ensure identical output.

Usage:
    python scripts/generate_tag_golden_files.py

Output:
    rose-rs/testdata/golden/tags_flac.json
    rose-rs/testdata/golden/tags_mp3.json
    rose-rs/testdata/golden/tags_m4a.json
    rose-rs/testdata/golden/tags_ogg_vorbis.json
    rose-rs/testdata/golden/tags_ogg_opus.json
"""

from __future__ import annotations

import base64
import json
import sys
from pathlib import Path

# Ensure rose-py is importable
REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT / "rose-py"))

import mutagen
import mutagen.flac
import mutagen.mp3
import mutagen.mp4
import mutagen.oggopus
import mutagen.oggvorbis

from rose.audiotags import AudioTags, RoseDate
from rose.common import Artist, ArtistMapping

TESTDATA_DIR = REPO_ROOT / "testdata" / "Tagger"
GOLDEN_DIR = REPO_ROOT / "rose-rs" / "testdata" / "golden"


def artist_to_dict(a: Artist) -> dict:
    return {"name": a.name, "alias": a.alias}


def artist_mapping_to_dict(m: ArtistMapping) -> dict:
    return {
        "main": [artist_to_dict(a) for a in m.main],
        "guest": [artist_to_dict(a) for a in m.guest],
        "remixer": [artist_to_dict(a) for a in m.remixer],
        "producer": [artist_to_dict(a) for a in m.producer],
        "composer": [artist_to_dict(a) for a in m.composer],
        "conductor": [artist_to_dict(a) for a in m.conductor],
        "djmixer": [artist_to_dict(a) for a in m.djmixer],
    }


def rosedate_to_str(d: RoseDate | None) -> str | None:
    if d is None:
        return None
    return str(d)


def tags_to_dict(tags: AudioTags) -> dict:
    """Serialize an AudioTags instance to a JSON-compatible dict."""
    return {
        "id": tags.id,
        "release_id": tags.release_id,
        "tracktitle": tags.tracktitle,
        "tracknumber": tags.tracknumber,
        "tracktotal": tags.tracktotal,
        "discnumber": tags.discnumber,
        "disctotal": tags.disctotal,
        "trackartists": artist_mapping_to_dict(tags.trackartists),
        "releasetitle": tags.releasetitle,
        "releasetype": tags.releasetype,
        "releasedate": rosedate_to_str(tags.releasedate),
        "originaldate": rosedate_to_str(tags.originaldate),
        "compositiondate": rosedate_to_str(tags.compositiondate),
        "genre": tags.genre,
        "secondarygenre": tags.secondarygenre,
        "descriptor": tags.descriptor,
        "label": tags.label,
        "edition": tags.edition,
        "catalognumber": tags.catalognumber,
        "releaseartists": artist_mapping_to_dict(tags.releaseartists),
        "duration_sec": tags.duration_sec,
        "path": str(tags.path.relative_to(REPO_ROOT)),
    }


def extract_raw_custom_tags(filepath: Path) -> dict:
    """Extract raw custom tag values, especially MP4 freeform atoms as base64 bytes."""
    m = mutagen.File(filepath)
    raw = {}

    if isinstance(m, mutagen.mp4.MP4):
        # MP4 freeform atoms are bytes — dump both decoded string and raw base64
        freeform_keys = [
            "----:net.sunsetglow.rose:ID",
            "----:net.sunsetglow.rose:RELEASEID",
            "----:net.sunsetglow.rose:COMPOSITIONDATE",
            "----:net.sunsetglow.rose:ORIGINALDATE",
            "----:net.sunsetglow.rose:SECONDARYGENRE",
            "----:net.sunsetglow.rose:DESCRIPTOR",
            "----:net.sunsetglow.rose:EDITION",
            "----:com.apple.iTunes:LABEL",
            "----:com.apple.iTunes:CATALOGNUMBER",
            "----:com.apple.iTunes:RELEASETYPE",
            "----:com.apple.iTunes:MusicBrainz Album Type",
            "----:com.apple.iTunes:REMIXER",
            "----:com.apple.iTunes:PRODUCER",
            "----:com.apple.iTunes:CONDUCTOR",
            "----:com.apple.iTunes:DJMIXER",
        ]
        for key in freeform_keys:
            try:
                values = m.tags[key]
                raw[key] = []
                for v in values:
                    raw_bytes = bytes(v)
                    raw[key].append({
                        "decoded": raw_bytes.decode("utf-8", errors="replace"),
                        "base64": base64.b64encode(raw_bytes).decode("ascii"),
                    })
            except KeyError:
                pass

    elif isinstance(m, mutagen.mp3.MP3):
        # ID3 TXXX frames for Rose custom tags
        txxx_keys = [
            "TXXX:ROSEID",
            "TXXX:ROSERELEASEID",
            "TXXX:COMPOSITIONDATE",
            "TXXX:SECONDARYGENRE",
            "TXXX:DESCRIPTOR",
            "TXXX:CATALOGNUMBER",
            "TXXX:EDITION",
            "TXXX:RELEASETYPE",
            "TXXX:MusicBrainz Album Type",
        ]
        if m.tags:
            for key in txxx_keys:
                try:
                    frame = m.tags[key]
                    raw[key] = [str(v) for v in frame.text]
                except KeyError:
                    pass

    elif isinstance(m, mutagen.flac.FLAC | mutagen.oggvorbis.OggVorbis | mutagen.oggopus.OggOpus):
        # Vorbis comments for Rose custom tags
        vc_keys = [
            "roseid",
            "rosereleaseid",
            "compositiondate",
            "secondarygenre",
            "descriptor",
            "catalognumber",
            "edition",
            "releasetype",
        ]
        if m.tags:
            for key in vc_keys:
                try:
                    raw[key] = m.tags[key]
                except KeyError:
                    pass

    return raw


# Map from file extension / type to golden file name
FORMAT_MAP = {
    ".flac": "tags_flac.json",
    ".m4a": "tags_m4a.json",
    ".mp3": "tags_mp3.json",
    ".vorbis.ogg": "tags_ogg_vorbis.json",
    ".opus.ogg": "tags_ogg_opus.json",
}


def classify_format(filepath: Path) -> str:
    """Return the golden file name for a given audio file."""
    name = filepath.name.lower()
    # Check compound extensions first
    if name.endswith(".vorbis.ogg"):
        return "tags_ogg_vorbis.json"
    if name.endswith(".opus.ogg"):
        return "tags_ogg_opus.json"
    # Then simple extensions
    suffix = filepath.suffix.lower()
    if suffix == ".flac":
        return "tags_flac.json"
    if suffix == ".m4a":
        return "tags_m4a.json"
    if suffix == ".mp3":
        return "tags_mp3.json"
    raise ValueError(f"Unknown format for {filepath}")


def main() -> None:
    GOLDEN_DIR.mkdir(parents=True, exist_ok=True)

    # Collect all audio files
    audio_files = sorted(TESTDATA_DIR.iterdir())
    audio_files = [f for f in audio_files if f.is_file() and not f.name.startswith(".")]

    # Group by format
    by_format: dict[str, list[dict]] = {}
    for filepath in audio_files:
        try:
            golden_name = classify_format(filepath)
        except ValueError:
            print(f"Skipping unknown format: {filepath}", file=sys.stderr)
            continue

        print(f"Reading tags from: {filepath.relative_to(REPO_ROOT)}")
        tags = AudioTags.from_file(filepath)
        entry = tags_to_dict(tags)

        # Add raw custom tag values
        raw_tags = extract_raw_custom_tags(filepath)
        if raw_tags:
            entry["raw_custom_tags"] = raw_tags

        by_format.setdefault(golden_name, []).append(entry)

    # Write golden files
    for golden_name, entries in sorted(by_format.items()):
        output_path = GOLDEN_DIR / golden_name
        with open(output_path, "w") as f:
            json.dump(entries, f, indent=2, sort_keys=True, ensure_ascii=False)
            f.write("\n")
        print(f"Wrote {output_path.relative_to(REPO_ROOT)} ({len(entries)} entries)")

    # Verify all 5 formats are present
    expected = set(FORMAT_MAP.values())
    actual = set(by_format.keys())
    missing = expected - actual
    if missing:
        print(f"\nWARNING: Missing formats: {missing}", file=sys.stderr)
        sys.exit(1)

    print("\nAll golden files generated successfully.")


if __name__ == "__main__":
    main()
