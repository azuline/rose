#!/usr/bin/env python

import os
from typing import Any

import mutagen
import mutagen.id3


def write_standard_tag(f: Any, key: str, value: str | None) -> None:
    f.tags.delall(key)
    if value:
        frame = getattr(mutagen.id3, key)(text=value)
        f.tags.add(frame)


def write_tag_with_description(f: Any, name: str, value: str | None) -> None:
    key, desc = name.split(":", 1)
    # Since the ID3 tags work with the shared prefix key before `:`, manually preserve
    # the other tags with the shared prefix key.
    keep_fields = [f for f in f.tags.getall(key) if getattr(f, "desc", None) != desc]
    f.tags.delall(key)
    if value:
        frame = getattr(mutagen.id3, key)(desc=desc, text=[value])
        f.tags.add(frame)
    for x in keep_fields:
        f.tags.add(x)


os.chdir(os.environ["ROSE_ROOT"] + "/testdata/Tagger")

f = mutagen.File("track1.flac")  # type: ignore
f.tags["originaldate"] = "1990"
f.tags["secondarygenre"] = "Minimal;Ambient"
f.tags["descriptor"] = "Lush;Warm"
f.tags["edition"] = "Japan"
f.save()

f = mutagen.File("track2.m4a")  # type: ignore
f.tags["----:net.sunsetglow.rose:ORIGINALDATE"] = b"1990"
f.tags["----:net.sunsetglow.rose:SECONDARYGENRE"] = b"Minimal;Ambient"
f.tags["----:net.sunsetglow.rose:DESCRIPTOR"] = b"Lush;Warm"
f.tags["----:net.sunsetglow.rose:EDITION"] = b"Japan"
f.save()

f = mutagen.File("track3.mp3")  # type: ignore
write_standard_tag(f, "TDOR", "1990")
write_tag_with_description(f, "TXXX:SECONDARYGENRE", "Minimal;Ambient")
write_tag_with_description(f, "TXXX:DESCRIPTOR", "Lush;Warm")
write_tag_with_description(f, "TXXX:EDITION", "Japan")
f.save()

f = mutagen.File("track4.vorbis.ogg")  # type: ignore
f.tags["originaldate"] = "1990"
f.tags["secondarygenre"] = "Minimal;Ambient"
f.tags["descriptor"] = "Lush;Warm"
f.tags["edition"] = "Japan"
f.save()

f = mutagen.File("track5.opus.ogg")  # type: ignore
f.tags["originaldate"] = "1990"
f.tags["secondarygenre"] = "Minimal;Ambient"
f.tags["descriptor"] = "Lush;Warm"
f.tags["edition"] = "Japan"
f.save()
