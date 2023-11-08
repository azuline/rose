"""
The audiotags module abstracts over tag reading and writing for five different audio formats,
exposing a single standard interface for all audio files.

The audiotags module also handles Rose-specific tagging semantics, such as multi-valued tags,
normalization, artist formatting, and enum validation.
"""

from __future__ import annotations

import contextlib
import logging
import re
import sys
import typing
from dataclasses import dataclass
from pathlib import Path
from typing import Any, no_type_check

import mutagen
import mutagen.flac
import mutagen.id3
import mutagen.mp3
import mutagen.mp4
import mutagen.oggopus
import mutagen.oggvorbis

from rose.common import Artist, ArtistMapping, RoseError, RoseExpectedError, uniq

if typing.TYPE_CHECKING:
    pass

logger = logging.getLogger(__name__)


TAG_SPLITTER_REGEX = re.compile(r" \\\\ | / |; ?| vs\. ")
YEAR_REGEX = re.compile(r"\d{4}$")
DATE_REGEX = re.compile(r"(\d{4})-\d{2}-\d{2}")

SUPPORTED_AUDIO_EXTENSIONS = [
    ".mp3",
    ".m4a",
    ".ogg",
    ".opus",
    ".flac",
]

SUPPORTED_RELEASE_TYPES = [
    "album",
    "single",
    "ep",
    "compilation",
    "anthology",
    "soundtrack",
    "live",
    "remix",
    "djmix",
    "mixtape",
    "bootleg",
    "demo",
    "other",
    "unknown",
]


def _normalize_rtype(x: str | None) -> str:
    """Determine the release type of a release."""
    if not x:
        return "unknown"
    x = x.lower()
    if x in SUPPORTED_RELEASE_TYPES:
        return x
    return "unknown"


class UnsupportedFiletypeError(RoseExpectedError):
    pass


class UnsupportedTagValueTypeError(RoseExpectedError):
    pass


@dataclass
class AudioTags:
    id: str | None
    release_id: str | None
    title: str | None
    year: int | None
    tracknumber: str | None
    tracktotal: int | None
    discnumber: str | None
    disctotal: int | None
    album: str | None
    genre: list[str]
    label: list[str]
    releasetype: str

    albumartists: ArtistMapping
    trackartists: ArtistMapping

    duration_sec: int

    path: Path

    @classmethod
    def from_file(cls, p: Path) -> AudioTags:
        """Read the tags of an audio file on disk."""
        if not any(p.suffix.lower() == ext for ext in SUPPORTED_AUDIO_EXTENSIONS):
            raise UnsupportedFiletypeError(f"{p.suffix} not a supported filetype")
        try:
            m = mutagen.File(p)  # type: ignore
        except mutagen.MutagenError as e:  # type: ignore
            raise UnsupportedFiletypeError(f"Failed to open file: {e}") from e
        if isinstance(m, mutagen.mp3.MP3):
            # ID3 returns trackno/discno tags as no/total. We have to parse.
            tracknumber = discnumber = tracktotal = disctotal = None
            if tracknos := _get_tag(m.tags, ["TRCK"]):
                try:
                    tracknumber, tracktotalstr = tracknos.split("/", 1)
                    tracktotal = _parse_int(tracktotalstr)
                except ValueError:
                    tracknumber = tracknos
            if discnos := _get_tag(m.tags, ["TPOS"]):
                try:
                    discnumber, disctotalstr = discnos.split("/", 1)
                    disctotal = _parse_int(disctotalstr)
                except ValueError:
                    discnumber = discnos

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

            return AudioTags(
                id=_get_tag(m.tags, ["TXXX:ROSEID"]),
                release_id=_get_tag(m.tags, ["TXXX:ROSERELEASEID"]),
                title=_get_tag(m.tags, ["TIT2"]),
                year=_parse_year(_get_tag(m.tags, ["TDRC", "TYER"])),
                tracknumber=tracknumber,
                tracktotal=tracktotal,
                discnumber=discnumber,
                disctotal=disctotal,
                album=_get_tag(m.tags, ["TALB"]),
                genre=_split_tag(_get_tag(m.tags, ["TCON"], split=True)),
                label=_split_tag(_get_tag(m.tags, ["TPUB"], split=True)),
                releasetype=_normalize_rtype(_get_tag(m.tags, ["TXXX:RELEASETYPE"], first=True)),
                albumartists=parse_artist_string(main=_get_tag(m.tags, ["TPE2"], split=True)),
                trackartists=parse_artist_string(
                    main=_get_tag(m.tags, ["TPE1"], split=True),
                    remixer=_get_tag(m.tags, ["TPE4"], split=True),
                    composer=_get_tag(m.tags, ["TCOM"], split=True),
                    conductor=_get_tag(m.tags, ["TPE3"], split=True),
                    producer=_get_paired_frame("producer"),
                    dj=_get_paired_frame("DJ-mix"),
                ),
                duration_sec=round(m.info.length),
                path=p,
            )
        if isinstance(m, mutagen.mp4.MP4):
            tracknumber = discnumber = tracktotal = disctotal = None
            with contextlib.suppress(ValueError):
                tracknumber, tracktotalstr = _get_tuple_tag(m.tags, ["trkn"])  # type: ignore
                tracktotal = _parse_int(tracktotalstr)
            with contextlib.suppress(ValueError):
                discnumber, disctotalstr = _get_tuple_tag(m.tags, ["disk"])  # type: ignore
                disctotal = _parse_int(disctotalstr)

            return AudioTags(
                id=_get_tag(m.tags, ["----:net.sunsetglow.rose:ID"]),
                release_id=_get_tag(m.tags, ["----:net.sunsetglow.rose:RELEASEID"]),
                title=_get_tag(m.tags, ["\xa9nam"]),
                year=_parse_year(_get_tag(m.tags, ["\xa9day"])),
                tracknumber=str(tracknumber),
                tracktotal=tracktotal,
                discnumber=str(discnumber),
                disctotal=disctotal,
                album=_get_tag(m.tags, ["\xa9alb"]),
                genre=_split_tag(_get_tag(m.tags, ["\xa9gen"], split=True)),
                label=_split_tag(_get_tag(m.tags, ["----:com.apple.iTunes:LABEL"], split=True)),
                releasetype=_normalize_rtype(
                    _get_tag(m.tags, ["----:com.apple.iTunes:RELEASETYPE"], first=True)
                ),
                albumartists=parse_artist_string(main=_get_tag(m.tags, ["aART"], split=True)),
                trackartists=parse_artist_string(
                    main=_get_tag(m.tags, ["\xa9ART"], split=True),
                    remixer=_get_tag(m.tags, ["----:com.apple.iTunes:REMIXER"], split=True),
                    producer=_get_tag(m.tags, ["----:com.apple.iTunes:PRODUCER"], split=True),
                    composer=_get_tag(m.tags, ["\xa9wrt"], split=True),
                    conductor=_get_tag(m.tags, ["----:com.apple.iTunes:CONDUCTOR"], split=True),
                    dj=_get_tag(m.tags, ["----:com.apple.iTunes:DJMIXER"], split=True),
                ),
                duration_sec=round(m.info.length),  # type: ignore
                path=p,
            )
        if isinstance(m, (mutagen.flac.FLAC, mutagen.oggvorbis.OggVorbis, mutagen.oggopus.OggOpus)):
            return AudioTags(
                id=_get_tag(m.tags, ["roseid"]),
                release_id=_get_tag(m.tags, ["rosereleaseid"]),
                title=_get_tag(m.tags, ["title"]),
                year=_parse_year(_get_tag(m.tags, ["date", "year"])),
                tracknumber=_get_tag(m.tags, ["tracknumber"], first=True),
                tracktotal=_parse_int(_get_tag(m.tags, ["tracktotal"], first=True)),
                discnumber=_get_tag(m.tags, ["discnumber"], first=True),
                disctotal=_parse_int(_get_tag(m.tags, ["disctotal"], first=True)),
                album=_get_tag(m.tags, ["album"]),
                genre=_split_tag(_get_tag(m.tags, ["genre"], split=True)),
                label=_split_tag(
                    _get_tag(m.tags, ["organization", "label", "recordlabel"], split=True)
                ),
                releasetype=_normalize_rtype(_get_tag(m.tags, ["releasetype"], first=True)),
                albumartists=parse_artist_string(
                    main=_get_tag(m.tags, ["albumartist"], split=True)
                ),
                trackartists=parse_artist_string(
                    main=_get_tag(m.tags, ["artist"], split=True),
                    remixer=_get_tag(m.tags, ["remixer"], split=True),
                    producer=_get_tag(m.tags, ["producer"], split=True),
                    composer=_get_tag(m.tags, ["composer"], split=True),
                    conductor=_get_tag(m.tags, ["conductor"], split=True),
                    dj=_get_tag(m.tags, ["djmixer"], split=True),
                ),
                duration_sec=round(m.info.length),  # type: ignore
                path=p,
            )
        raise UnsupportedFiletypeError(f"{p} is not a supported audio file")

    @no_type_check
    def flush(self, *, validate: bool = True) -> None:
        """Flush the current tags to the file on disk."""
        m = mutagen.File(self.path)
        if not validate and "pytest" not in sys.modules:
            raise Exception("Validate can only be turned off by tests.")

        self.releasetype = (self.releasetype or "unknown").lower()
        if validate and self.releasetype not in SUPPORTED_RELEASE_TYPES:
            raise UnsupportedTagValueTypeError(
                f"Release type {self.releasetype} is not a supported release type.\n"
                f"Supported release types: {', '.join(SUPPORTED_RELEASE_TYPES)}"
            )

        if isinstance(m, mutagen.mp3.MP3):
            if m.tags is None:
                m.tags = mutagen.id3.ID3()

            def _write_standard_tag(key: str, value: str | None) -> None:
                m.tags.delall(key)
                frame = getattr(mutagen.id3, key)(text=value)
                if value:
                    m.tags.add(frame)

            def _write_tag_with_description(name: str, value: str | None) -> None:
                key, desc = name.split(":", 1)
                # Since the ID3 tags work with the shared prefix key before `:`, manually preserve
                # the other tags with the shared prefix key.
                keep_fields = [f for f in m.tags.getall(key) if getattr(f, "desc", None) != desc]
                m.tags.delall(key)
                if value:
                    frame = getattr(mutagen.id3, key)(desc=desc, text=value)
                    m.tags.add(frame)
                for f in keep_fields:
                    m.tags.add(f)

            _write_tag_with_description("TXXX:ROSEID", self.id)
            _write_tag_with_description("TXXX:ROSERELEASEID", self.release_id)
            _write_standard_tag("TIT2", self.title)
            _write_standard_tag("TDRC", str(self.year).zfill(4))
            _write_standard_tag("TRCK", self.tracknumber)
            _write_standard_tag("TPOS", self.discnumber)
            _write_standard_tag("TALB", self.album)
            _write_standard_tag("TCON", ";".join(self.genre))
            _write_standard_tag("TPUB", ";".join(self.label))
            _write_tag_with_description("TXXX:RELEASETYPE", self.releasetype)
            _write_standard_tag("TPE2", format_artist_string(self.albumartists))
            _write_standard_tag("TPE1", format_artist_string(self.trackartists))
            # Wipe the alt. role artist tags, since we encode the full artist into the main tag.
            m.tags.delall("TPE4")
            m.tags.delall("TCOM")
            m.tags.delall("TPE3")
            # Delete all paired text frames, since these represent additional artist roles. We don't
            # want to preserve them.
            m.tags.delall("TIPL")
            m.tags.delall("IPLS")
            m.save()
            return
        if isinstance(m, mutagen.mp4.MP4):
            if m.tags is None:
                m.tags = mutagen.mp4.MP4Tags()
            m.tags["----:net.sunsetglow.rose:ID"] = (self.id or "").encode()
            m.tags["----:net.sunsetglow.rose:RELEASEID"] = (self.release_id or "").encode()
            m.tags["\xa9nam"] = self.title or ""
            m.tags["\xa9day"] = str(self.year).zfill(4)
            m.tags["\xa9alb"] = self.album or ""
            m.tags["\xa9gen"] = ";".join(self.genre)
            m.tags["----:com.apple.iTunes:LABEL"] = ";".join(self.label).encode()
            m.tags["----:com.apple.iTunes:RELEASETYPE"] = self.releasetype.encode()
            m.tags["aART"] = format_artist_string(self.albumartists)
            m.tags["\xa9ART"] = format_artist_string(self.trackartists)
            # Wipe the alt. role artist tags, since we encode the full artist into the main tag.
            with contextlib.suppress(KeyError):
                del m.tags["----:com.apple.iTunes:REMIXER"]
            with contextlib.suppress(KeyError):
                del m.tags["----:com.apple.iTunes:PRODUCER"]
            with contextlib.suppress(KeyError):
                del m.tags["\xa9wrt"]
            with contextlib.suppress(KeyError):
                del m.tags["----:com.apple.iTunes:CONDUCTOR"]
            with contextlib.suppress(KeyError):
                del m.tags["----:com.apple.iTunes:DJMIXER"]

            # The track and disc numbers in MP4 are a bit annoying, because they must be a
            # single-element list of 2-tuple ints. We preserve the previous tracktotal/disctotal (as
            # Rose does not care about those values), and then attempt to write our own tracknumber
            # and discnumber.
            try:
                prev_tracktotal = m.tags["trkn"][0][1]
            except (KeyError, IndexError):
                prev_tracktotal = 1
            try:
                prev_disctotal = m.tags["disk"][0][1]
            except (KeyError, IndexError):
                prev_disctotal = 1
            try:
                m.tags["trkn"] = [(int(self.tracknumber or "0"), prev_tracktotal)]
                m.tags["disk"] = [(int(self.discnumber or "0"), prev_disctotal)]
            except ValueError as e:
                raise UnsupportedTagValueTypeError(
                    "Could not write m4a trackno/discno tags: must be integers. "
                    f"Got: {self.tracknumber=} / {self.discnumber=}"
                ) from e

            m.save()
            return
        if isinstance(m, (mutagen.flac.FLAC, mutagen.oggvorbis.OggVorbis, mutagen.oggopus.OggOpus)):
            if m.tags is None:
                if isinstance(m, mutagen.flac.FLAC):
                    m.tags = mutagen.flac.VCFLACDict()
                elif isinstance(m, mutagen.oggvorbis.OggVorbis):
                    m.tags = mutagen.oggvorbis.OggVCommentDict()
                else:
                    m.tags = mutagen.oggopus.OggOpusVComment()
            assert not isinstance(m.tags, mutagen.flac.MetadataBlock)
            m.tags["roseid"] = self.id or ""
            m.tags["rosereleaseid"] = self.release_id or ""
            m.tags["title"] = self.title or ""
            m.tags["date"] = str(self.year).zfill(4)
            m.tags["tracknumber"] = self.tracknumber or ""
            m.tags["discnumber"] = self.discnumber or ""
            m.tags["album"] = self.album or ""
            m.tags["genre"] = ";".join(self.genre)
            m.tags["organization"] = ";".join(self.label)
            m.tags["releasetype"] = self.releasetype
            m.tags["albumartist"] = format_artist_string(self.albumartists)
            m.tags["artist"] = format_artist_string(self.trackartists)
            # Wipe the alt. role artist tags, since we encode the full artist into the main tag.
            with contextlib.suppress(KeyError):
                del m.tags["remixer"]
            with contextlib.suppress(KeyError):
                del m.tags["producer"]
            with contextlib.suppress(KeyError):
                del m.tags["composer"]
            with contextlib.suppress(KeyError):
                del m.tags["conductor"]
            with contextlib.suppress(KeyError):
                del m.tags["djmixer"]
            m.save()
            return

        raise RoseError(f"Impossible: unknown mutagen type: {type(m)=} ({repr(m)=})")


def _split_tag(t: str | None) -> list[str]:
    return TAG_SPLITTER_REGEX.split(t) if t else []


def _get_tag(t: Any, keys: list[str], *, split: bool = False, first: bool = False) -> str | None:
    if not t:
        return None
    for k in keys:
        try:
            values: list[str] = []
            raw_values = t[k].text if isinstance(t, mutagen.id3.ID3) else t[k]
            for val in raw_values:
                if isinstance(val, str):
                    values.extend(_split_tag(val) if split else [val])
                elif isinstance(val, bytes):
                    values.extend(_split_tag(val.decode()) if split else [val.decode()])
                elif isinstance(val, mutagen.id3.ID3TimeStamp):  # type: ignore
                    values.extend(_split_tag(val.text) if split else [val.text])
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


def _get_tuple_tag(t: Any, keys: list[str]) -> tuple[str, str] | tuple[None, None]:
    if not t:
        return None, None
    for k in keys:
        try:
            raw_values = t[k].text if isinstance(t, mutagen.id3.ID3) else t[k]
            for val in raw_values:
                if isinstance(val, tuple):
                    return val  # type: ignore
                else:
                    raise UnsupportedTagValueTypeError(
                        f"Encountered a tag value of type {type(val)}: expected tuple"
                    )
        except (KeyError, ValueError):
            pass
    return None, None


def _parse_int(x: str | None) -> int | None:
    if x is None:
        return None
    try:
        return int(x)
    except ValueError:
        return None


def _parse_year(value: str | None) -> int | None:
    if not value:
        return None
    if YEAR_REGEX.match(value):
        return int(value)
    # There may be a time value after the date... allow that and other crap.
    if m := DATE_REGEX.match(value):
        return int(m[1])
    return None


TAG_SPLITTER_REGEX = re.compile(r" \\\\ | / |; ?| vs\. ")


def parse_artist_string(
    main: str | None,
    *,
    remixer: str | None = None,
    composer: str | None = None,
    conductor: str | None = None,
    producer: str | None = None,
    dj: str | None = None,
) -> ArtistMapping:
    def _split_tag(t: str | None) -> list[str]:
        return TAG_SPLITTER_REGEX.split(t) if t else []

    li_main = _split_tag(conductor)
    li_guests = []
    li_remixer = _split_tag(remixer)
    li_composer = _split_tag(composer)
    li_producer = _split_tag(producer)
    li_dj = _split_tag(dj)
    if main and "produced by " in main:
        main, producer = re.split(r" ?produced by ", main, maxsplit=1)
        li_producer.extend(_split_tag(producer))
    if main and "remixed by " in main:
        main, remixer = re.split(r" ?remixed by ", main, maxsplit=1)
        li_remixer.extend(_split_tag(remixer))
    if main and "feat. " in main:
        main, guests = re.split(r" ?feat. ", main, maxsplit=1)
        li_guests.extend(_split_tag(guests))
    if main and "pres. " in main:
        dj, main = re.split(r" ?pres. ", main, maxsplit=1)
        li_dj.extend(_split_tag(dj))
    if main and "performed by " in main:
        composer, main = re.split(r" ?performed by ", main, maxsplit=1)
        li_composer.extend(_split_tag(composer))
    if main:
        li_main.extend(_split_tag(main))

    def to_artist(xs: list[str]) -> list[Artist]:
        return [Artist(name=x, alias=False) for x in xs]

    rval = ArtistMapping(
        main=to_artist(uniq(li_main)),
        guest=to_artist(uniq(li_guests)),
        remixer=to_artist(uniq(li_remixer)),
        composer=to_artist(uniq(li_composer)),
        producer=to_artist(uniq(li_producer)),
        djmixer=to_artist(uniq(li_dj)),
    )
    # logger.debug(
    #     f"Parsed args {main=} {remixer=} {composer=} {conductor=} {producer=} {dj=} as {rval=}"
    # )
    return rval


def format_artist_string(mapping: ArtistMapping) -> str:
    def format_role(xs: list[Artist]) -> str:
        return ";".join([x.name for x in xs if not x.alias])

    r = format_role(mapping.main)
    if mapping.composer:
        r = format_role(mapping.composer) + " performed by " + r
    if mapping.djmixer:
        r = format_role(mapping.djmixer) + " pres. " + r
    if mapping.guest:
        r += " feat. " + format_role(mapping.guest)
    if mapping.remixer:
        r += " remixed by " + format_role(mapping.remixer)
    if mapping.producer:
        r += " produced by " + format_role(mapping.producer)
    # logger.debug(f"Formatted {mapping} as {r}")
    return r
