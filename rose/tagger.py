from __future__ import annotations

import contextlib
import re
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

from rose.artiststr import ArtistMapping, format_artist_string, parse_artist_string
from rose.common import RoseError

TAG_SPLITTER_REGEX = re.compile(r" \\\\ | / |; ?| vs\. ")
YEAR_REGEX = re.compile(r"\d{4}$")
DATE_REGEX = re.compile(r"(\d{4})-\d{2}-\d{2}")


class UnsupportedFiletypeError(RoseError):
    pass


class UnsupportedTagValueTypeError(RoseError):
    pass


@dataclass
class AudioFile:
    id: str | None
    title: str | None
    year: int | None
    track_number: str | None
    disc_number: str | None
    album: str | None
    genre: list[str]
    label: list[str]
    release_type: str | None

    album_artists: ArtistMapping
    artists: ArtistMapping

    duration_sec: int

    _m: Any

    @classmethod
    def from_file(cls, p: Path) -> AudioFile:
        """Read the tags of an audio file on disk."""
        m = mutagen.File(p)  # type: ignore
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
                id=_get_tag(m.tags, ["TXXX:ROSEID"]),
                title=_get_tag(m.tags, ["TIT2"]),
                year=_parse_year(_get_tag(m.tags, ["TDRC", "TYER"])),
                track_number=_parse_num(_get_tag(m.tags, ["TRCK"], first=True)),
                disc_number=_parse_num(_get_tag(m.tags, ["TPOS"], first=True)),
                album=_get_tag(m.tags, ["TALB"]),
                genre=_split_tag(_get_tag(m.tags, ["TCON"])),
                label=_split_tag(_get_tag(m.tags, ["TPUB"])),
                release_type=_get_tag(m.tags, ["TXXX:RELEASETYPE"], first=True),
                album_artists=parse_artist_string(main=_get_tag(m.tags, ["TPE2"])),
                artists=parse_artist_string(
                    main=_get_tag(m.tags, ["TPE1"]),
                    remixer=_get_tag(m.tags, ["TPE4"]),
                    composer=_get_tag(m.tags, ["TCOM"]),
                    conductor=_get_tag(m.tags, ["TPE3"]),
                    producer=_get_paired_frame("producer"),
                    dj=_get_paired_frame("DJ-mix"),
                ),
                duration_sec=round(m.info.length),
                _m=m,
            )
        if isinstance(m, mutagen.mp4.MP4):
            return AudioFile(
                id=_get_tag(m.tags, ["----:net.sunsetglow.rose:ID"]),
                title=_get_tag(m.tags, ["\xa9nam"]),
                year=_parse_year(_get_tag(m.tags, ["\xa9day"])),
                track_number=_get_tag(m.tags, ["trkn"], first=True),
                disc_number=_get_tag(m.tags, ["disk"], first=True),
                album=_get_tag(m.tags, ["\xa9alb"]),
                genre=_split_tag(_get_tag(m.tags, ["\xa9gen"])),
                label=_split_tag(_get_tag(m.tags, ["----:com.apple.iTunes:LABEL"])),
                release_type=_get_tag(m.tags, ["----:com.apple.iTunes:RELEASETYPE"], first=True),
                album_artists=parse_artist_string(main=_get_tag(m.tags, ["aART"])),
                artists=parse_artist_string(
                    main=_get_tag(m.tags, ["\xa9ART"]),
                    remixer=_get_tag(m.tags, ["----:com.apple.iTunes:REMIXER"]),
                    producer=_get_tag(m.tags, ["----:com.apple.iTunes:PRODUCER"]),
                    composer=_get_tag(m.tags, ["\xa9wrt"]),
                    conductor=_get_tag(m.tags, ["----:com.apple.iTunes:CONDUCTOR"]),
                    dj=_get_tag(m.tags, ["----:com.apple.iTunes:DJMIXER"]),
                ),
                duration_sec=round(m.info.length),  # type: ignore
                _m=m,
            )
        if isinstance(m, (mutagen.flac.FLAC, mutagen.oggvorbis.OggVorbis, mutagen.oggopus.OggOpus)):
            return AudioFile(
                id=_get_tag(m.tags, ["roseid"]),
                title=_get_tag(m.tags, ["title"]),
                year=_parse_year(_get_tag(m.tags, ["date", "year"])),
                track_number=_get_tag(m.tags, ["tracknumber"], first=True),
                disc_number=_get_tag(m.tags, ["discnumber"], first=True),
                album=_get_tag(m.tags, ["album"]),
                genre=_split_tag(_get_tag(m.tags, ["genre"])),
                label=_split_tag(_get_tag(m.tags, ["organization", "label", "recordlabel"])),
                release_type=_get_tag(m.tags, ["releasetype"], first=True),
                album_artists=parse_artist_string(main=_get_tag(m.tags, ["albumartist"])),
                artists=parse_artist_string(
                    main=_get_tag(m.tags, ["artist"]),
                    remixer=_get_tag(m.tags, ["remixer"]),
                    producer=_get_tag(m.tags, ["producer"]),
                    composer=_get_tag(m.tags, ["composer"]),
                    conductor=_get_tag(m.tags, ["conductor"]),
                    dj=_get_tag(m.tags, ["djmixer"]),
                ),
                duration_sec=round(m.info.length),  # type: ignore
                _m=m,
            )
        raise UnsupportedFiletypeError(f"{p} is not a supported audio file")

    @no_type_check
    def flush(self) -> None:
        """Flush the current tags to the file on disk."""
        m = self._m

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
            _write_standard_tag("TIT2", self.title)
            _write_standard_tag("TDRC", str(self.year))
            _write_standard_tag("TRCK", self.track_number)
            _write_standard_tag("TPOS", self.disc_number)
            _write_standard_tag("TALB", self.album)
            _write_standard_tag("TCON", ";".join(self.genre))
            _write_standard_tag("TPUB", ";".join(self.label))
            _write_tag_with_description("TXXX:RELEASETYPE", self.release_type)
            _write_standard_tag("TPE2", format_artist_string(self.album_artists, self.genre))
            _write_standard_tag("TPE1", format_artist_string(self.artists, self.genre))
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
            m.tags["\xa9nam"] = self.title or ""
            m.tags["\xa9day"] = str(self.year)
            m.tags["\xa9alb"] = self.album or ""
            m.tags["\xa9gen"] = ";".join(self.genre)
            m.tags["----:com.apple.iTunes:LABEL"] = ";".join(self.label).encode()
            m.tags["----:com.apple.iTunes:RELEASETYPE"] = (self.release_type or "").encode()
            m.tags["aART"] = format_artist_string(self.album_artists, self.genre)
            m.tags["\xa9ART"] = format_artist_string(self.artists, self.genre)
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
            # Rose does not care about those values), and then attempt to write our own track_number
            # and disc_number.
            try:
                prev_tracktotal = m.tags["trkn"][0][1]
            except (KeyError, IndexError):
                prev_tracktotal = 1
            try:
                prev_disctotal = m.tags["disk"][0][1]
            except (KeyError, IndexError):
                prev_disctotal = 1
            try:
                m.tags["trkn"] = [(int(self.track_number or "0"), prev_tracktotal)]
                m.tags["disk"] = [(int(self.disc_number or "0"), prev_disctotal)]
            except ValueError as e:
                raise UnsupportedTagValueTypeError(
                    "Could not write m4a trackno/discno tags: must be integers. "
                    f"Got: {self.track_number=} / {self.disc_number=}"
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
            m.tags["title"] = self.title or ""
            m.tags["date"] = str(self.year)
            m.tags["tracknumber"] = self.track_number or ""
            m.tags["discnumber"] = self.disc_number or ""
            m.tags["album"] = self.album or ""
            m.tags["genre"] = ";".join(self.genre)
            m.tags["organization"] = ";".join(self.label)
            m.tags["releasetype"] = self.release_type or ""
            m.tags["albumartist"] = format_artist_string(self.album_artists, self.genre)
            m.tags["artist"] = format_artist_string(self.artists, self.genre)
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


def _parse_year(value: str | None) -> int | None:
    if not value:
        return None
    if YEAR_REGEX.match(value):
        return int(value)
    # There may be a time value after the date... allow that and other crap.
    if m := DATE_REGEX.match(value):
        return int(m[1])
    return None
