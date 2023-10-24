import logging
import re
from dataclasses import dataclass, field

logger = logging.getLogger(__name__)


TAG_SPLITTER_REGEX = re.compile(r" \\\\ | / |; ?| vs\. ")


@dataclass
class Artists:
    main: list[str] = field(default_factory=list)
    guest: list[str] = field(default_factory=list)
    remixer: list[str] = field(default_factory=list)
    producer: list[str] = field(default_factory=list)
    composer: list[str] = field(default_factory=list)
    djmixer: list[str] = field(default_factory=list)


def parse_artist_string(
    main: str | None,
    *,
    remixer: str | None = None,
    composer: str | None = None,
    conductor: str | None = None,
    producer: str | None = None,
    dj: str | None = None,
) -> Artists:
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

    rval = Artists(
        main=_deduplicate(li_main),
        guest=_deduplicate(li_guests),
        remixer=_deduplicate(li_remixer),
        composer=_deduplicate(li_composer),
        producer=_deduplicate(li_producer),
        djmixer=_deduplicate(li_dj),
    )
    logger.debug(
        f"Parsed args {main=} {remixer=} {composer=} {conductor=} {producer=} {dj=} as {rval=}"
    )
    return rval


def format_artist_string(a: Artists, genres: list[str]) -> str:
    r = ";".join(a.main)
    if a.composer and "Classical" in genres:
        r = ";".join(a.composer) + " performed by " + r
    if a.djmixer:
        r = ";".join(a.djmixer) + " pres. " + r
    if a.guest:
        r += " feat. " + ";".join(a.guest)
    if a.remixer:
        r += " remixed by " + ";".join(a.remixer)
    if a.producer:
        r += " produced by " + ";".join(a.producer)
    logger.debug(f"Formatted {a} ({genres=}) as {r}")
    return r


def _deduplicate(xs: list[str]) -> list[str]:
    seen: set[str] = set()
    r: list[str] = []
    for x in xs:
        if x not in seen:
            r.append(x)
        seen.add(x)
    return r
