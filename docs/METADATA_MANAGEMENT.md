# Managing Your Music Metadata

Rosé relies on the metadata embedded in your music files to organize your music
into a useful virtual filesystem. This means that the quality of the music tags
is important for getting the most out of Rosé.

Therefore, Rosé also provides tools to improve the metadata of your music.
Currently, Rosé provides:

- A text-based interface for manually modifying release metadata.
- Metadata importing from third-party sources.
- Rules engine to automatically update metadata based on patterns

In this document, we'll first cover the conventions that Rosé expects and
applies towards tags, and then go through each of the the functionalities
listed above.

# Tagging Conventions

This section describes how Rosé reads and writes tags from files. Rosé applies
fairly rigid conventions in the tags it writes, and applies a relaxed version
of those conventions when ingesting tags from audio files.

## Managed Tags

Rosé manages the following tags:

- Release Tags:
  - Title
  - Album Artists
  - Release Year
  - Release Type (e.g. Album, EP, Single)
  - Genre
  - Label
- Track Tags:
  - Title
  - Artists
  - Track Number
  - Disc Number
  - Rosé ID

Rosé does not care about any other tags and does not do anything with them.

## Field Mappings

Rosé supports three tag container formats:

- ID3: `.mp3` files
- MP4: `.m4a` files
- Vorbis: `.ogg`, `.opus`, and `.flac` files

In this section, we will list out the per-container fields that we read/write.
Rosé will only write to a single field for each tag; however, for tags with
multiple conventions out in the rest of the world, Rosé will support reading
from additional fields.

### MP3

| Tag           | Field Name         | Will Ingest These Fields                                                                                               |
| ------------- | ------------------ | ---------------------------------------------------------------------------------------------------------------------- |
| Release Title | `TALB`             |                                                                                                                        |
| Album Artists | `TPE2`             |                                                                                                                        |
| Release Year  | `TDRC`             | `TYER`                                                                                                                 |
| Release Type  | `TXXX:RELEASETYPE` |                                                                                                                        |
| Genre         | `TCON`             |                                                                                                                        |
| Label         | `TPUB`             |                                                                                                                        |
| Track Title   | `TIT2`             |                                                                                                                        |
| Track Artists | `TPE1`             | `TPE4` (Remixer), `TCOM` (Composer), `TPE3` (Conductor), `TIPL,IPLS/producer` (producer), `TIPL,IPLS/DJ-mix` (djmixer) |
| Track Number  | `TRCK`             |                                                                                                                        |
| Disc Number   | `TPOS`             |                                                                                                                        |
| Rose ID       | `TXXX:ROSEID`      |                                                                                                                        |

### MP4

| Tag           | Field Name                          | Will Ingest These Fields                                                                                                                                                                               |
| ------------- | ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Release Title | `\xa9alb`                           |                                                                                                                                                                                                        |
| Album Artists | `aART`                              |                                                                                                                                                                                                        |
| Release Year  | `\xa9day`                           |                                                                                                                                                                                                        |
| Release Type  | `----:com.apple.iTunes:RELEASETYPE` |                                                                                                                                                                                                        |
| Genre         | `\xa9gen`                           |                                                                                                                                                                                                        |
| Label         | `----:com.apple.iTunes:LABEL`       |                                                                                                                                                                                                        |
| Track Title   | `\xa9nam`                           |                                                                                                                                                                                                        |
| Track Artists | `\xa9ART`                           | `----:com.apple.iTunes:REMIXER` (Remixer), `\xa9wrt` (Composer), `----:com.apple.iTunes:CONDUCTOR` (Conductor), `----:com.apple.iTunes:PRODUCER` (producer), `----:com.apple.iTunes:DJMIXER` (djmixer) |
| Track Number  | `trkn`                              |                                                                                                                                                                                                        |
| Disc Number   | `disk`                              |                                                                                                                                                                                                        |
| Rose ID       | `----:net.sunsetglow.rose:ID`       |                                                                                                                                                                                                        |

### Vorbis

| Tag           | Field Name     | Will Ingest These Fields                                                                                        |
| ------------- | -------------- | --------------------------------------------------------------------------------------------------------------- |
| Release Title | `album`        |                                                                                                                 |
| Album Artists | `albumartist`  |                                                                                                                 |
| Release Year  | `date`         | `year`                                                                                                          |
| Release Type  | `releasetype`  |                                                                                                                 |
| Genre         | `genre`        |                                                                                                                 |
| Label         | `organization` | `label`, `recordlabel`                                                                                          |
| Track Title   | `title`        |                                                                                                                 |
| Track Artists | `artist`       | `remixer` (Remixer), `composer` (Composer), `conductor` (Conductor), `producer` (producer), `djmixer` (djmixer) |
| Track Number  | `tracknumber`  |                                                                                                                 |
| Disc Number   | `discnumber`   |                                                                                                                 |
| Rose ID       | `roseid`       |                                                                                                                 |

## Multi-Valued Tags

Rosé supports multiple values for the artists, genres, and labels tags. Rosé
writes a single tag field and with fields concatenated together with a `;`
delimiter. For example, `genre=Deep House;Techno`. Rosé does not write one tag
per frame due to inconsistent support by other useful programs.

## Artist Tags

Rosé preserves the artists' role in the artist tag by using specialized
delimiters. An example artist tag is: `Pyotr Ilyich Tchaikovsky performed by André Previn;London Symphony Orchestra feat. Barack Obama`.

The artist tag is described by the following grammar:

```
<artist-tag> ::= <composer> <djmixer> <main> <guest> <remixer> <producer>
<composer>   ::= <name> ' performed by '
<djmixer>    ::= <name> ' pres. '
<main>       ::= <name>
<guest>      ::= ' feat. ' <name>
<remixer>    ::= ' remixed by ' <name>
<producer>   ::= ' produced by ' <name>
<name>       ::= string ';' <name> | string
```

Rosé only supports the artist roles:

- `main`
- `guest`
- `producer`
- `composer`
- `conductor`
- `djmixer`

Rosé writes a single tag value into the _Track Artists_ and _Album Artists_
tags. Though some conventions exist for writing each role into its own tag,
Rosé does not follow them, due to inconsistent (mainly nonexistent) support by
other useful programs.

## Release Type Tags

Rosé supports tagging the release _type_. The supported values are:

- `album`
- `single`
- `ep`
- `compilation`
- `soundtrack`
- `live`
- `remix`
- `djmix`
- `mixtape`
- `other`
- `bootleg`
- `demo`
- `unknown`

# Text-Based Release Editing

Rosé supports editing a release's metadata as a text file via the
`rose releases edit` command. This command accepts a Release ID or a Release's
Virtual Filesystem Directory Name.

So for example:

```bash
$ rose releases edit "LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [K-Pop] {BlockBerry Creative}"
$ rose releases edit "018b4ff1-acdf-7ff1-bcd6-67757aea0fed"
```

This command opens up a TOML representation of the release's metadata in your
`$EDITOR`. Upon save and exit, the TOML's metadata is written to the file tags.

Rosé validates the Artist Role and Release Type fields. The values provided
must be one of the supported values. The supported values are documented in
[Artist Tags](#artist-tags) and [Release Type Tags](#release-type-tags).

An example of the TOML representation is:

```toml
title = "Mix & Match"
releasetype = "ep"
year = 2017
genres = [
    "K-Pop",
]
labels = [
    "BlockBerry Creative",
]
artists = [
    { name = "LOOΠΔ ODD EYE CIRCLE", role = "main" },
]

[tracks.018b6514-6fb8-729f-bf86-7590187ff377]
disc_number = "1"
track_number = "1"
title = "ODD"
artists = [
    { name = "LOOΠΔ ODD EYE CIRCLE", role = "main" },
]

[tracks.018b6514-6fba-7508-8576-c8e82ad4b7bc]
disc_number = "1"
track_number = "2"
title = "Girl Front"
artists = [
    { name = "LOOΠΔ ODD EYE CIRCLE", role = "main" },
]

[tracks.018b6514-6fb9-73f1-a139-18ecefcf55da]
disc_number = "1"
track_number = "3"
title = "LOONATIC"
artists = [
    { name = "LOOΠΔ ODD EYE CIRCLE", role = "main" },
]

[tracks.018b6514-6fb7-7cc6-9d23-8eaf0b1beee8]
disc_number = "1"
track_number = "4"
title = "Chaotic"
artists = [
    { name = "LOOΠΔ ODD EYE CIRCLE", role = "main" },
]

[tracks.018b6514-6fb6-766f-8430-c6ea3f48966d]
disc_number = "1"
track_number = "5"
title = "Starlight"
artists = [
    { name = "LOOΠΔ ODD EYE CIRCLE", role = "main" },
]
```

# Metadata Import & Cover Art Downloading

_In Development_

Sources: Discogs, MusicBrainz, Tidal, Deezer, Apple, Junodownload, Beatport, and fanart.tv

# Rules Engine

_In Development_
