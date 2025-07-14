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
from typing import TYPE_CHECKING, Any, no_type_check

from rose.common import Artist, ArtistMapping, RoseError, RoseExpectedError, flatten, uniq
from rose.genre_hierarchy import TRANSITIVE_PARENT_GENRES

if TYPE_CHECKING:
    from rose.config import Config

if typing.TYPE_CHECKING:
    pass

logger = logging.getLogger(__name__)


TAG_SPLITTER_REGEX = re.compile(r"\\\\| / |; ?| vs\. ")
YEAR_REGEX = re.compile(r"\d{4}$")
DATE_REGEX = re.compile(r"(\d{4})-(\d{2})-(\d{2})")

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
    "other",
    "bootleg",
    "loosetrack",
    "demo",
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


@dataclass(frozen=True)
class RoseDate:
    year: int
    month: int | None = None
    day: int | None = None

    @classmethod
    def parse(cls, value: str | None) -> RoseDate | None:
        if not value:
            return None
        with contextlib.suppress(ValueError):
            return RoseDate(year=int(value), month=None, day=None)
        # There may be a time value after the date... allow that and other crap.
        if m := DATE_REGEX.match(value):
            return RoseDate(year=int(m[1]), month=int(m[2]), day=int(m[3]))
        return None

    def __str__(self) -> str:
        if self.month is None and self.day is None:
            return f"{self.year:04}"
        return f"{self.year:04}-{self.month or 1:02}-{self.day or 1:02}"


@dataclass
class AudioTags:
    id: str | None
    release_id: str | None

    tracktitle: str | None
    tracknumber: str | None
    tracktotal: int | None
    discnumber: str | None
    disctotal: int | None
    trackartists: ArtistMapping

    releasetitle: str | None
    releasetype: str
    releasedate: RoseDate | None
    originaldate: RoseDate | None
    compositiondate: RoseDate | None
    genre: list[str]
    secondarygenre: list[str]
    descriptor: list[str]
    edition: str | None
    label: list[str]
    catalognumber: str | None
    releaseartists: ArtistMapping

    duration_sec: int
    path: Path

    @classmethod
    def from_file(cls, p: Path) -> AudioTags:
        """Read the tags of an audio file on disk."""
        import mutagen
        import mutagen.flac
        import mutagen.id3
        import mutagen.mp3
        import mutagen.mp4
        import mutagen.oggopus
        import mutagen.oggvorbis

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
                id=_get_tag(m.tags, ["TXXX:ROSEID"], first=True),
                release_id=_get_tag(m.tags, ["TXXX:ROSERELEASEID"], first=True),
                tracktitle=_get_tag(m.tags, ["TIT2"]),
                releasedate=RoseDate.parse(_get_tag(m.tags, ["TDRC", "TYER", "TDAT"])),
                originaldate=RoseDate.parse(_get_tag(m.tags, ["TDOR", "TORY"])),
                compositiondate=RoseDate.parse(_get_tag(m.tags, ["TXXX:COMPOSITIONDATE"], first=True)),
                tracknumber=tracknumber,
                tracktotal=tracktotal,
                discnumber=discnumber,
                disctotal=disctotal,
                releasetitle=_get_tag(m.tags, ["TALB"]),
                genre=_split_genre_tag(_get_tag(m.tags, ["TCON"], split=True)),
                secondarygenre=_split_genre_tag(_get_tag(m.tags, ["TXXX:SECONDARYGENRE"], split=True)),
                descriptor=_split_tag(_get_tag(m.tags, ["TXXX:DESCRIPTOR"], split=True)),
                label=_split_tag(_get_tag(m.tags, ["TPUB"], split=True)),
                catalognumber=_get_tag(m.tags, ["TXXX:CATALOGNUMBER"], first=True),
                edition=_get_tag(m.tags, ["TXXX:EDITION"], first=True),
                releasetype=_normalize_rtype(
                    _get_tag(m.tags, ["TXXX:RELEASETYPE", "TXXX:MusicBrainz Album Type"], first=True)
                ),
                releaseartists=parse_artist_string(main=_get_tag(m.tags, ["TPE2"], split=True)),
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
                tracktitle=_get_tag(m.tags, ["\xa9nam"]),
                releasedate=RoseDate.parse(_get_tag(m.tags, ["\xa9day"])),
                originaldate=RoseDate.parse(
                    _get_tag(
                        m.tags,
                        [
                            "----:net.sunsetglow.rose:ORIGINALDATE",
                            "----:com.apple.iTunes:ORIGINALDATE",
                            "----:com.apple.iTunes:ORIGINALYEAR",
                        ],
                    )
                ),
                compositiondate=RoseDate.parse(_get_tag(m.tags, ["----:net.sunsetglow.rose:COMPOSITIONDATE"])),
                tracknumber=str(tracknumber),
                tracktotal=tracktotal,
                discnumber=str(discnumber),
                disctotal=disctotal,
                releasetitle=_get_tag(m.tags, ["\xa9alb"]),
                genre=_split_genre_tag(_get_tag(m.tags, ["\xa9gen"], split=True)),
                secondarygenre=_split_genre_tag(
                    _get_tag(m.tags, ["----:net.sunsetglow.rose:SECONDARYGENRE"], split=True)
                ),
                descriptor=_split_tag(_get_tag(m.tags, ["----:net.sunsetglow.rose:DESCRIPTOR"], split=True)),
                label=_split_tag(_get_tag(m.tags, ["----:com.apple.iTunes:LABEL"], split=True)),
                catalognumber=_get_tag(m.tags, ["----:com.apple.iTunes:CATALOGNUMBER"]),
                edition=_get_tag(m.tags, ["----:net.sunsetglow.rose:EDITION"]),
                releasetype=_normalize_rtype(
                    _get_tag(
                        m.tags,
                        [
                            "----:com.apple.iTunes:RELEASETYPE",
                            "----:com.apple.iTunes:MusicBrainz Album Type",
                        ],
                        first=True,
                    )
                ),
                releaseartists=parse_artist_string(main=_get_tag(m.tags, ["aART"], split=True)),
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
        if isinstance(m, mutagen.flac.FLAC | mutagen.oggvorbis.OggVorbis | mutagen.oggopus.OggOpus):
            return AudioTags(
                id=_get_tag(m.tags, ["roseid"]),
                release_id=_get_tag(m.tags, ["rosereleaseid"]),
                tracktitle=_get_tag(m.tags, ["title"]),
                releasedate=RoseDate.parse(_get_tag(m.tags, ["date", "year"])),
                originaldate=RoseDate.parse(_get_tag(m.tags, ["originaldate", "originalyear"])),
                compositiondate=RoseDate.parse(_get_tag(m.tags, ["compositiondate"])),
                tracknumber=_get_tag(m.tags, ["tracknumber"], first=True),
                tracktotal=_parse_int(_get_tag(m.tags, ["tracktotal"], first=True)),
                discnumber=_get_tag(m.tags, ["discnumber"], first=True),
                disctotal=_parse_int(_get_tag(m.tags, ["disctotal"], first=True)),
                releasetitle=_get_tag(m.tags, ["album"]),
                genre=_split_genre_tag(_get_tag(m.tags, ["genre"], split=True)),
                secondarygenre=_split_genre_tag(_get_tag(m.tags, ["secondarygenre"], split=True)),
                descriptor=_split_tag(_get_tag(m.tags, ["descriptor"], split=True)),
                label=_split_tag(_get_tag(m.tags, ["label", "organization", "recordlabel"], split=True)),
                catalognumber=_get_tag(m.tags, ["catalognumber"]),
                edition=_get_tag(m.tags, ["edition"]),
                releasetype=_normalize_rtype(_get_tag(m.tags, ["releasetype"], first=True)),
                releaseartists=parse_artist_string(main=_get_tag(m.tags, ["albumartist"], split=True)),
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
    def flush(self, c: Config, *, validate: bool = True) -> None:
        """Flush the current tags to the file on disk."""
        import mutagen
        import mutagen.flac
        import mutagen.id3
        import mutagen.mp3
        import mutagen.mp4
        import mutagen.oggopus
        import mutagen.oggvorbis

        m = mutagen.File(self.path)
        if not validate and "pytest" not in sys.modules:
            raise Exception("Validate can only be turned off by tests.")

        self.releasetype = (self.releasetype or "unknown").lower()
        if validate and self.releasetype not in SUPPORTED_RELEASE_TYPES:
            raise UnsupportedTagValueTypeError(
                f"Release type {self.releasetype} is not a supported release type.\n"
                f"Supported release types: {", ".join(SUPPORTED_RELEASE_TYPES)}"
            )

        if isinstance(m, mutagen.mp3.MP3):
            if m.tags is None:
                m.tags = mutagen.id3.ID3()

            def _write_standard_tag(key: str, value: str | None) -> None:
                m.tags.delall(key)
                if value:
                    frame = getattr(mutagen.id3, key)(text=value)
                    m.tags.add(frame)

            def _write_tag_with_description(name: str, value: str | None) -> None:
                key, desc = name.split(":", 1)
                # Since the ID3 tags work with the shared prefix key before `:`, manually preserve
                # the other tags with the shared prefix key.
                keep_fields = [f for f in m.tags.getall(key) if getattr(f, "desc", None) != desc]
                m.tags.delall(key)
                if value:
                    frame = getattr(mutagen.id3, key)(desc=desc, text=[value])
                    m.tags.add(frame)
                for f in keep_fields:
                    m.tags.add(f)

            _write_tag_with_description("TXXX:ROSEID", self.id)
            _write_tag_with_description("TXXX:ROSERELEASEID", self.release_id)
            _write_standard_tag("TIT2", self.tracktitle)
            _write_standard_tag("TDRC", str(self.releasedate))
            _write_standard_tag("TDOR", str(self.originaldate))
            _write_tag_with_description("TXXX:COMPOSITIONDATE", str(self.compositiondate))
            _write_standard_tag("TRCK", self.tracknumber)
            _write_standard_tag("TPOS", self.discnumber)
            _write_standard_tag("TALB", self.releasetitle)
            _write_standard_tag("TCON", _format_genre_tag(c, self.genre))
            _write_tag_with_description("TXXX:SECONDARYGENRE", _format_genre_tag(c, self.secondarygenre))
            _write_tag_with_description("TXXX:DESCRIPTOR", ";".join(self.descriptor))
            _write_standard_tag("TPUB", ";".join(self.label))
            _write_tag_with_description("TXXX:CATALOGNUMBER", self.catalognumber)
            _write_tag_with_description("TXXX:EDITION", self.edition)
            _write_tag_with_description("TXXX:RELEASETYPE", self.releasetype)
            _write_standard_tag("TPE2", format_artist_string(self.releaseartists))
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
            m.tags["\xa9nam"] = self.tracktitle or ""
            m.tags["\xa9day"] = str(self.releasedate)
            m.tags["----:net.sunsetglow.rose:ORIGINALDATE"] = str(self.originaldate).encode()
            m.tags["----:net.sunsetglow.rose:COMPOSITIONDATE"] = str(self.compositiondate).encode()
            m.tags["\xa9alb"] = self.releasetitle or ""
            m.tags["\xa9gen"] = _format_genre_tag(c, self.genre)
            m.tags["----:net.sunsetglow.rose:SECONDARYGENRE"] = _format_genre_tag(c, self.secondarygenre).encode()
            m.tags["----:net.sunsetglow.rose:DESCRIPTOR"] = ";".join(self.descriptor).encode()
            m.tags["----:com.apple.iTunes:LABEL"] = ";".join(self.label).encode()
            m.tags["----:com.apple.iTunes:CATALOGNUMBER"] = (self.catalognumber or "").encode()
            m.tags["----:net.sunsetglow.rose:EDITION"] = (self.edition or "").encode()
            m.tags["----:com.apple.iTunes:RELEASETYPE"] = self.releasetype.encode()
            m.tags["aART"] = format_artist_string(self.releaseartists)
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
                # Not sure why they can be a None string, but whatever...
                if self.tracknumber == "None":
                    self.tracknumber = None
                if self.discnumber == "None":
                    self.discnumber = None
                m.tags["trkn"] = [(int(self.tracknumber or "0"), prev_tracktotal)]
                m.tags["disk"] = [(int(self.discnumber or "0"), prev_disctotal)]
            except ValueError as e:
                raise UnsupportedTagValueTypeError(
                    "Could not write m4a trackno/discno tags: must be integers. "
                    f"Got: {self.tracknumber=} / {self.discnumber=}"
                ) from e

            m.save()
            return
        if isinstance(m, mutagen.flac.FLAC | mutagen.oggvorbis.OggVorbis | mutagen.oggopus.OggOpus):
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
            m.tags["title"] = self.tracktitle or ""
            m.tags["date"] = str(self.releasedate)
            m.tags["originaldate"] = str(self.originaldate)
            m.tags["compositiondate"] = str(self.compositiondate)
            m.tags["tracknumber"] = self.tracknumber or ""
            m.tags["discnumber"] = self.discnumber or ""
            m.tags["album"] = self.releasetitle or ""
            m.tags["genre"] = _format_genre_tag(c, self.genre)
            m.tags["secondarygenre"] = _format_genre_tag(c, self.secondarygenre)
            m.tags["descriptor"] = ";".join(self.descriptor)
            m.tags["label"] = ";".join(self.label)
            m.tags["catalognumber"] = self.catalognumber or ""
            m.tags["edition"] = self.edition or ""
            m.tags["releasetype"] = self.releasetype
            m.tags["albumartist"] = format_artist_string(self.releaseartists)
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


def _split_genre_tag(t: str | None) -> list[str]:
    if not t:
        return []
    with contextlib.suppress(ValueError):
        t, _ = t.split("\\\\PARENTS:\\\\", 1)
    return TAG_SPLITTER_REGEX.split(t)


def _format_genre_tag(c: Config, t: list[str]) -> str:
    if not c.write_parent_genres:
        return ";".join(t)
    if parent_genres := set(flatten([TRANSITIVE_PARENT_GENRES.get(g, []) for g in t])) - set(t):
        return ";".join(t) + "\\\\PARENTS:\\\\" + ";".join(sorted(parent_genres))
    return ";".join(t)


def _get_tag(t: Any, keys: list[str], *, split: bool = False, first: bool = False) -> str | None:
    import mutagen.id3

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
                    raise UnsupportedTagValueTypeError(f"Encountered a tag value of type {type(val)}")
            if first:
                return (values[0] or None) if values else None
            return r" \\ ".join(values) or None
        except (KeyError, ValueError):
            pass
    return None


def _get_tuple_tag(t: Any, keys: list[str]) -> tuple[str, str] | tuple[None, None]:
    import mutagen.id3

    if not t:
        return None, None
    for k in keys:
        try:
            raw_values = t[k].text if isinstance(t, mutagen.id3.ID3) else t[k]
            for val in raw_values:
                if isinstance(val, tuple):
                    return val
                else:
                    raise UnsupportedTagValueTypeError(f"Encountered a tag value of type {type(val)}: expected tuple")
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

    li_main = []
    li_conductor = _split_tag(conductor)
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
    if main and "under. " in main:
        main, conductor = re.split(r" ?under. ", main, maxsplit=1)
        li_conductor.extend(_split_tag(conductor))
    if main:
        li_main.extend(_split_tag(main))

    def to_artist(xs: list[str]) -> list[Artist]:
        return [Artist(name=x, alias=False) for x in xs]

    rval = ArtistMapping(
        main=to_artist(uniq(li_main)),
        guest=to_artist(uniq(li_guests)),
        remixer=to_artist(uniq(li_remixer)),
        composer=to_artist(uniq(li_composer)),
        conductor=to_artist(uniq(li_conductor)),
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
    if mapping.conductor:
        r += " under. " + format_role(mapping.conductor)
    if mapping.guest:
        r += " feat. " + format_role(mapping.guest)
    if mapping.remixer:
        r += " remixed by " + format_role(mapping.remixer)
    if mapping.producer:
        r += " produced by " + format_role(mapping.producer)
    # logger.debug(f"Formatted {mapping} as {r}")
    return r

# TESTS

import shutil
from dataclasses import replace
from pathlib import Path

import mutagen
import pytest

from conftest import TEST_TAGGER
from rose.audiotags import (
    AudioTags,
    RoseDate,
    UnsupportedTagValueTypeError,
    _split_tag,
    format_artist_string,
    parse_artist_string,
)
from rose.common import Artist, ArtistMapping
from rose.config import Config


@pytest.mark.parametrize(
    ("filename", "track_num", "duration"),
    [
        ("track1.flac", "1", 2),
        ("track2.m4a", "2", 2),
        ("track3.mp3", "3", 1),
        ("track4.vorbis.ogg", "4", 1),
        ("track5.opus.ogg", "5", 1),
    ],
)
def test_getters(filename: str, track_num: str, duration: int) -> None:
    af = AudioTags.from_file(TEST_TAGGER / filename)
    assert af.releasetitle == "A Cool Album"
    assert af.releasetype == "album"
    assert af.releasedate == RoseDate(1990, 2, 5)
    assert af.originaldate == RoseDate(1990)
    assert af.compositiondate == RoseDate(1984)
    assert af.genre == ["Electronic", "House"]
    assert af.secondarygenre == ["Minimal", "Ambient"]
    assert af.descriptor == ["Lush", "Warm"]
    assert af.label == ["A Cool Label"]
    assert af.catalognumber == "DN-420"
    assert af.edition == "Japan"
    assert af.releaseartists.main == [Artist("Artist A"), Artist("Artist B")]

    assert af.tracknumber == track_num
    assert af.tracktotal == 5
    assert af.discnumber == "1"
    assert af.disctotal == 1

    assert af.tracktitle == f"Track {track_num}"
    assert af.trackartists == ArtistMapping(
        main=[Artist("Artist A"), Artist("Artist B")],
        guest=[Artist("Artist C"), Artist("Artist D")],
        remixer=[Artist("Artist AB"), Artist("Artist BC")],
        producer=[Artist("Artist CD"), Artist("Artist DE")],
        composer=[Artist("Artist EF"), Artist("Artist FG")],
        conductor=[Artist("Artist GH"), Artist("Artist HI")],
        djmixer=[Artist("Artist IJ"), Artist("Artist JK")],
    )
    assert af.duration_sec == duration


@pytest.mark.parametrize(
    ("filename", "track_num", "duration"),
    [
        ("track1.flac", "1", 2),
        ("track2.m4a", "2", 2),
        ("track3.mp3", "3", 1),
        ("track4.vorbis.ogg", "4", 1),
        ("track5.opus.ogg", "5", 1),
    ],
)
def test_flush(config: Config, isolated_dir: Path, filename: str, track_num: str, duration: int) -> None:
    """Test the flush by flushing the file, then asserting that all the tags still read properly."""
    fpath = isolated_dir / filename
    shutil.copyfile(TEST_TAGGER / filename, fpath)
    af = AudioTags.from_file(fpath)
    # Inject one special case into here: modify the djmixer artist. This checks that we also clear
    # the original djmixer tag, so that the next read does not contain Artist EF and Artist FG.
    af.trackartists.djmixer = [Artist("New")]
    # Also test date writing.
    af.originaldate = RoseDate(1990, 4, 20)
    af.flush(config)
    af = AudioTags.from_file(fpath)

    assert af.releasetitle == "A Cool Album"
    assert af.releasetype == "album"
    assert af.releasedate == RoseDate(1990, 2, 5)
    assert af.originaldate == RoseDate(1990, 4, 20)
    assert af.compositiondate == RoseDate(1984)
    assert af.genre == ["Electronic", "House"]
    assert af.secondarygenre == ["Minimal", "Ambient"]
    assert af.descriptor == ["Lush", "Warm"]
    assert af.label == ["A Cool Label"]
    assert af.catalognumber == "DN-420"
    assert af.edition == "Japan"
    assert af.releaseartists.main == [Artist("Artist A"), Artist("Artist B")]

    assert af.tracknumber == track_num
    assert af.discnumber == "1"

    assert af.tracktitle == f"Track {track_num}"
    assert af.trackartists == ArtistMapping(
        main=[Artist("Artist A"), Artist("Artist B")],
        guest=[Artist("Artist C"), Artist("Artist D")],
        remixer=[Artist("Artist AB"), Artist("Artist BC")],
        producer=[Artist("Artist CD"), Artist("Artist DE")],
        composer=[Artist("Artist EF"), Artist("Artist FG")],
        conductor=[Artist("Artist GH"), Artist("Artist HI")],
        djmixer=[Artist("New")],
    )
    assert af.duration_sec == duration


def test_write_parent_genres(config: Config, isolated_dir: Path) -> None:
    config = replace(config, write_parent_genres=True)

    filename = "track1.flac"
    fpath = isolated_dir / filename
    shutil.copyfile(TEST_TAGGER / filename, fpath)
    af = AudioTags.from_file(fpath)
    # Inject one special case into here: modify the djmixer artist. This checks that we also clear
    # the original djmixer tag, so that the next read does not contain Artist EF and Artist FG.
    af.trackartists.djmixer = [Artist("New")]
    # Also test date writing.
    af.originaldate = RoseDate(1990, 4, 20)
    af.flush(config)

    # Check that parents show up in raw tags (or not if there are no parents).
    mf = mutagen.File(fpath)  # type: ignore
    assert mf is not None
    assert mf.tags["genre"] == ["Electronic;House\\\\PARENTS:\\\\Dance;Electronic Dance Music"]
    assert mf.tags["secondarygenre"] == ["Minimal;Ambient"]

    af = AudioTags.from_file(fpath)
    assert af.genre == ["Electronic", "House"]
    assert af.secondarygenre == ["Minimal", "Ambient"]


@pytest.mark.parametrize(
    "filename",
    [
        "track1.flac",
        "track2.m4a",
        "track3.mp3",
        "track4.vorbis.ogg",
        "track5.opus.ogg",
    ],
)
def test_id_assignment(config: Config, isolated_dir: Path, filename: str) -> None:
    """Test the read/write for the nonstandard Rose ID tags."""
    fpath = isolated_dir / filename
    shutil.copyfile(TEST_TAGGER / filename, fpath)

    af = AudioTags.from_file(fpath)
    af.id = "ahaha"
    af.release_id = "bahaha"
    af.flush(config)

    af = AudioTags.from_file(fpath)
    assert af.id == "ahaha"
    assert af.release_id == "bahaha"


@pytest.mark.parametrize(
    "filename",
    ["track1.flac", "track2.m4a", "track3.mp3", "track4.vorbis.ogg", "track5.opus.ogg"],
)
def test_releasetype_normalization(config: Config, isolated_dir: Path, filename: str) -> None:
    """Test the flush by flushing the file, then asserting that all the tags still read properly."""
    fpath = isolated_dir / filename
    shutil.copyfile(TEST_TAGGER / filename, fpath)

    # Check that release type is read correctly.
    af = AudioTags.from_file(fpath)
    assert af.releasetype == "album"
    # Assert that attempting to flush a stupid value fails.
    af.releasetype = "lalala"
    with pytest.raises(UnsupportedTagValueTypeError):
        af.flush(config)
    # Flush it anyways...
    af.flush(config, validate=False)
    # Check that stupid release type is normalized as unknown.
    af = AudioTags.from_file(fpath)
    assert af.releasetype == "unknown"
    # And now assert that the read is case insensitive.
    af.releasetype = "ALBUM"
    af.flush(config, validate=False)
    af = AudioTags.from_file(fpath)
    assert af.releasetype == "album"


def test_split_tag() -> None:
    assert _split_tag(r"a \\ b") == ["a", "b"]
    assert _split_tag(r"a \ b") == [r"a \ b"]
    assert _split_tag("a;b") == ["a", "b"]
    assert _split_tag("a; b") == ["a", "b"]
    assert _split_tag("a vs. b") == ["a", "b"]
    assert _split_tag("a / b") == ["a", "b"]


def test_parse_artist_string() -> None:
    assert parse_artist_string("A;B feat. C;D") == ArtistMapping(
        main=[Artist("A"), Artist("B")],
        guest=[Artist("C"), Artist("D")],
    )
    assert parse_artist_string("A pres. C;D") == ArtistMapping(
        djmixer=[Artist("A")],
        main=[Artist("C"), Artist("D")],
    )
    assert parse_artist_string("A performed by C;D") == ArtistMapping(
        composer=[Artist("A")],
        main=[Artist("C"), Artist("D")],
    )
    assert parse_artist_string("A pres. B;C feat. D;E") == ArtistMapping(
        djmixer=[Artist("A")],
        main=[Artist("B"), Artist("C")],
        guest=[Artist("D"), Artist("E")],
    )
    # Test the deduplication handling.
    assert parse_artist_string("A pres. B", dj="A") == ArtistMapping(
        djmixer=[Artist("A")],
        main=[Artist("B")],
    )


def test_format_artist_string() -> None:
    assert (
        format_artist_string(
            ArtistMapping(
                main=[Artist("A"), Artist("B")],
                guest=[Artist("C"), Artist("D")],
            )
        )
        == "A;B feat. C;D"
    )
    assert format_artist_string(ArtistMapping(djmixer=[Artist("A")], main=[Artist("C"), Artist("D")])) == "A pres. C;D"
    assert (
        format_artist_string(ArtistMapping(composer=[Artist("A")], main=[Artist("C"), Artist("D")]))
        == "A performed by C;D"
    )
    assert (
        format_artist_string(
            ArtistMapping(
                djmixer=[Artist("A")],
                main=[Artist("B"), Artist("C")],
                guest=[Artist("D"), Artist("E")],
            )
        )
        == "A pres. B;C feat. D;E"
    )
