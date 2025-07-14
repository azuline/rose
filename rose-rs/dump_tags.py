#!/usr/bin/env python3
import sys
import os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "rose-py"))

from rose.audiotags import AudioTags
from pathlib import Path

if len(sys.argv) < 2:
    print("Usage: dump_tags.py <audio_file>")
    sys.exit(1)

file_path = Path(sys.argv[1])
try:
    tags = AudioTags.from_file(file_path)
    print(f"File: {file_path}")
    print(f"ID: {tags.id}")
    print(f"Release ID: {tags.release_id}")
    print(f"Release Title: {tags.releasetitle}")
    print(f"Release Type: {tags.releasetype}")
    print(f"Release Date: {tags.releasedate}")
    print(f"Original Date: {tags.originaldate}")
    print(f"Composition Date: {tags.compositiondate}")
    print(f"Genre: {tags.genre}")
    print(f"Secondary Genre: {tags.secondarygenre}")
    print(f"Descriptor: {tags.descriptor}")
    print(f"Label: {tags.label}")
    print(f"Catalog Number: {tags.catalognumber}")
    print(f"Edition: {tags.edition}")
    print(f"Track Title: {tags.tracktitle}")
    print(f"Track Number: {tags.tracknumber}")
    print(f"Track Total: {tags.tracktotal}")
    print(f"Disc Number: {tags.discnumber}")
    print(f"Disc Total: {tags.disctotal}")
    print(f"Duration: {tags.duration_sec}")
    print(f"Release Artists: {tags.releaseartists}")
    print(f"Track Artists: {tags.trackartists}")
except Exception as e:
    print(f"Error: {e}")
    import traceback
    traceback.print_exc()