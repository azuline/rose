from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import mutagen
import mutagen.flac
import mutagen.id3
import mutagen.mp3
import mutagen.mp4
import mutagen.oggopus
import mutagen.oggvorbis

from rose.foundation.errors import RoseError


class UnsupportedFiletypeError(RoseError):
    pass


class UnsupportedTagValueTypeError(RoseError):
    pass


@dataclass
class ArtistTags:
    main: list[str]
    guest: list[str]
    remixer: list[str]
    producer: list[str]
    composer: list[str]
    djmixer: list[str]


@dataclass
class AudioFile:
    title: str | None
    year: int | None
    track_number: str | None
    disc_number: str | None
    album: str | None
    genre: list[str]
    label: list[str]
    release_type: str | None

    album_artists: ArtistTags
    artists: ArtistTags

    @classmethod
    def from_file(cls, p: Path) -> AudioFile:
        return _convert_mutagen(mutagen.File(p), p)  # type: ignore


def _convert_mutagen(m: Any, p: Path) -> AudioFile:
    if isinstance(m, mutagen.mp3.MP3):
        # ID3 returns trackno/discno tags as no/total. We have to parse.
        def _parse_num(x: str | None) -> str | None:
            return x.split("/")[0] if x else None

        def _get_paired_frame(x: str) -> str | None:
            if not m.tags:
                return None
            for tag in ["TIPL", "IPLS"]:
                try:
                    frame = m.tags[tag]
                except KeyError:
                    continue
                return r" \\ ".join([p[1] for p in frame.people if p[0].lower() == x.lower()])
            return None

        return AudioFile(
            title=_get_tag(m.tags, ["TIT2"]),
            year=_parse_year(_get_tag(m.tags, ["TDRC", "TYER"])),
            track_number=_parse_num(_get_tag(m.tags, ["TRCK"], first=True)),
            disc_number=_parse_num(_get_tag(m.tags, ["TPOS"], first=True)),
            album=_get_tag(m.tags, ["TALB"]),
            genre=_split_tag(_get_tag(m.tags, ["TCON"])),
            label=_split_tag(_get_tag(m.tags, ["TPUB"])),
            release_type=_get_tag(m.tags, ["TXXX:RELEASETYPE"]),
            album_artists=_parse_artists(main=_get_tag(m.tags, ["TPE2"])),
            artists=_parse_artists(
                main=_get_tag(m.tags, ["TPE1"]),
                remixer=_get_tag(m.tags, ["TPE4"]),
                composer=_get_tag(m.tags, ["TCOM"]),
                conductor=_get_tag(m.tags, ["TPE3"]),
                producer=_get_paired_frame("producer"),
                dj=_get_paired_frame("DJ-mix"),
            ),
        )
    if isinstance(m, mutagen.mp4.MP4):
        return AudioFile(
            title=_get_tag(m.tags, ["\xa9nam"]),
            year=_parse_year(_get_tag(m.tags, ["\xa9day"])),
            track_number=_get_tag(m.tags, ["trkn"], first=True),
            disc_number=_get_tag(m.tags, ["disk"], first=True),
            album=_get_tag(m.tags, ["\xa9alb"]),
            genre=_split_tag(_get_tag(m.tags, ["\xa9gen"])),
            label=_split_tag(_get_tag(m.tags, ["----:com.apple.iTunes:LABEL"])),
            release_type=_get_tag(m.tags, ["----:com.apple.iTunes:RELEASETYPE"]),
            album_artists=_parse_artists(main=_get_tag(m.tags, ["aART"])),
            artists=_parse_artists(
                main=_get_tag(m.tags, ["\xa9ART"]),
                remixer=_get_tag(m.tags, ["----:com.apple.iTunes:REMIXER"]),
                producer=_get_tag(m.tags, ["----:com.apple.iTunes:PRODUCER"]),
                composer=_get_tag(m.tags, ["\xa9wrt"]),
                conductor=_get_tag(m.tags, ["----:com.apple.iTunes:CONDUCTOR"]),
                dj=_get_tag(m.tags, ["----:com.apple.iTunes:DJMIXER"]),
            ),
        )
    if isinstance(m, (mutagen.flac.FLAC, mutagen.oggvorbis.OggVorbis, mutagen.oggopus.OggOpus)):
        return AudioFile(
            title=_get_tag(m.tags, ["title"]),
            year=_parse_year(_get_tag(m.tags, ["date", "year"])),
            track_number=_get_tag(m.tags, ["tracknumber"], first=True),
            disc_number=_get_tag(m.tags, ["discnumber"], first=True),
            album=_get_tag(m.tags, ["album"]),
            genre=_split_tag(_get_tag(m.tags, ["genre"])),
            label=_split_tag(_get_tag(m.tags, ["organization", "label", "recordlabel"])),
            release_type=_get_tag(m.tags, ["releasetype"]),
            album_artists=_parse_artists(main=_get_tag(m.tags, ["albumartist"])),
            artists=_parse_artists(
                main=_get_tag(m.tags, ["artist"]),
                remixer=_get_tag(m.tags, ["remixer"]),
                producer=_get_tag(m.tags, ["producer"]),
                composer=_get_tag(m.tags, ["composer"]),
                conductor=_get_tag(m.tags, ["conductor"]),
                dj=_get_tag(m.tags, ["djmixer"]),
            ),
        )
    raise UnsupportedFiletypeError(f"{p} is not a supported audio file.")


def _get_tag(t: Any, keys: list[str], *, first: bool = False) -> str | None:
    if not t:
        return None
    for k in keys:
        try:
            values: list[str] = []
            raw_values = t[k].text if isinstance(t, mutagen.id3.ID3) else t[k]
            for val in raw_values:
                if isinstance(val, str):
                    values.extend(_split_tag(val))
                elif isinstance(val, bytes):
                    values.extend(_split_tag(val.decode()))
                elif isinstance(val, mutagen.id3.ID3TimeStamp):  # type: ignore
                    values.append(val.text)
                elif isinstance(val, tuple):
                    for v in val:
                        values.extend(_split_tag(str(v)))
                else:
                    raise UnsupportedTagValueTypeError(
                        f"Encountered a tag value of type {type(val)}"
                    )
            if first:
                return values[0] if values else None
            return r" \\ ".join(values)
        except (KeyError, ValueError):
            pass
    return None


def _split_tag(t: str | None) -> list[str]:
    return re.split(r" \\\\ | / |; ?| vs\. ", t) if t else []


def _parse_artists(
    *,
    main: str | None,
    remixer: str | None = None,
    composer: str | None = None,
    conductor: str | None = None,
    producer: str | None = None,
    dj: str | None = None,
) -> ArtistTags:
    li_main = _split_tag(conductor)
    li_guests = []
    li_remixer = _split_tag(remixer)
    li_composer = _split_tag(composer)
    li_producer = _split_tag(producer)
    li_dj = _split_tag(dj)
    if main and "feat. " in main:
        main, guests = re.split(r" ?feat. ", main, maxsplit=1)
        li_guests.extend(_split_tag(guests))
    if main and " pres. " in main:
        dj, main = re.split(r" ?pres. ", main, maxsplit=1)
        li_dj.extend(_split_tag(dj))
    if main and " performed by " in main:
        composer, main = re.split(r" ?performed by. ", main, maxsplit=1)
        li_composer.extend(_split_tag(composer))
    if main:
        li_main.extend(_split_tag(main))

    return ArtistTags(
        main=li_main,
        guest=li_guests,
        remixer=li_remixer,
        composer=li_composer,
        producer=li_producer,
        djmixer=li_dj,
    )


def _parse_year(value: str | None) -> int | None:
    if not value:
        return None
    value = str(value)  # ID3TimeStamp object sometimes comes through.
    if re.match(r"\d{4}$", value):
        return int(value)
    # There may be a time value after the date... allow that and other crap.
    if m := re.match(r"(\d{4})-\d{2}-\d{2}", value):
        return int(m[1])
    return None
