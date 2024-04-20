#!/usr/bin/env python

from typing import Any

import mutagen
import mutagen.id3


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


f = mutagen.File("track1.flac")  # type: ignore
f.tags["compositiondate"] = "1984"
f.tags["catalognumber"] = "DN-420"
f.save()

f = mutagen.File("track2.m4a")  # type: ignore
f.tags["----:net.sunsetglow.rose:COMPOSITIONDATE"] = b"1984"
f.tags["----:com.apple.iTunes:CATALOGNUMBER"] = b"DN-420"
f.save()

f = mutagen.File("track3.mp3")  # type: ignore
write_tag_with_description(f, "TXXX:COMPOSITIONDATE", "1984")
write_tag_with_description(f, "TXXX:CATALOGNUMBER", "DN-420")
f.save()

f = mutagen.File("track4.vorbis.ogg")  # type: ignore
f.tags["compositiondate"] = "1984"
f.tags["catalognumber"] = "DN-420"
f.save()

f = mutagen.File("track5.opus.ogg")  # type: ignore
f.tags["compositiondate"] = "1984"
f.tags["catalognumber"] = "DN-420"
f.save()
