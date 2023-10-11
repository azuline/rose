import re
from dataclasses import dataclass

TAG_SPLITTER_REGEX = re.compile(r" \\\\ | / |; ?| vs\. ")


@dataclass
class Artists:
    main: list[str]
    guest: list[str]
    remixer: list[str]
    producer: list[str]
    composer: list[str]
    djmixer: list[str]


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

    return Artists(
        main=li_main,
        guest=li_guests,
        remixer=li_remixer,
        composer=li_composer,
        producer=li_producer,
        djmixer=li_dj,
    )


def format_artist_string(a: Artists, genres: list[str]) -> str:
    r = ";".join(a.producer + a.main + a.remixer)
    if a.composer and "Classical" in genres:
        r = ";".join(a.composer) + " performed by. " + r
    if a.djmixer:
        r = ";".join(a.djmixer) + " pres. " + r
    if a.guest:
        r += " feat. " + ";".join(a.guest)
    return r
