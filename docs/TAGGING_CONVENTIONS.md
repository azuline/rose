# Tagging Conventions

This document describes how Rosé reads and writes tags from files. Rosé applies
fairly rigid conventions in the tags it writes, and applies a relaxed version
of those conventions when ingesting tags from audio files.

# Managed Tags

Rosé manages the following tags:

- Release Tags:
  - Title
  - Release Artists
  - Release Year
  - Release Type (e.g. Album, EP, Single)
  - Genre
  - Label
- Track Tags:
  - Title
  - Artists
  - Track Number
  - Disc Number

Rosé does not care about any other tags and does not do anything with them.

For documentation on the specific field names that Rosé uses for each tag
container format, please see [Tag Field Mappings](#tag-field-mappings).

# Multi-Valued Tags

Rosé supports multiple values for the artists, genres, and labels tags. Rosé
writes a single tag field and with fields concatenated together with a `;`
delimiter. For example, `genre=Deep House;Techno`. Rosé does not write multiple
frames for a single tag (where each value gets one frame) due to inconsistent
support by other useful programs.

When reading tags, Rosé is more relaxed in the delimiters it accepts. For the
Genre, Label, Artists, and Release Artists tags, Rosé will attempt to split a
single tag into multiple tags by the following delimiters:
<code>&nbsp;\\\\&nbsp;</code>, <code>&nbsp;/&nbsp;</code>, <code>;</code>, and
<code>&nbsp;vs.&nbsp;</code>.

# Artist Tags

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

Rosé supports the following artist roles:

- `main`
- `guest`
- `producer`
- `composer`
- `conductor`
- `djmixer`

Rosé writes a single tag value into the _Track Artists_ and _Release Artists_
tags. Though some conventions exist for writing each role into its own tag,
Rosé does not follow them, due to inconsistent (mainly nonexistent) support by
other useful programs.

# Release Type Tags

Rosé supports tagging the release _type_. The supported values are:

- `album`
- `single`
- `ep`
- `compilation`
- `anthology`
- `soundtrack`
- `live`
- `remix`
- `djmix`
- `mixtape`
- `other`
- `bootleg`
- `demo`
- `unknown`

# Tag Field Mappings

Rosé supports three tag container formats:

- ID3: `.mp3` files
- MP4: `.m4a` files
- Vorbis: `.ogg`, `.opus`, and `.flac` files

In this section, we list out the per-container field names that we read/write.
Rosé will only write to a single field for each tag; however, for tags with
multiple conventions out in the rest of the world, Rosé will support reading
from additional fields.

## MP3

| Tag             | Field Name           | Will Ingest These Fields                                                                                               |
| --------------- | -------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| Release Title   | `TALB`               |                                                                                                                        |
| Release Artists | `TPE2`               |                                                                                                                        |
| Release Year    | `TDRC`               | `TYER`                                                                                                                 |
| Release Type    | `TXXX:RELEASETYPE`   |                                                                                                                        |
| Genre           | `TCON`               |                                                                                                                        |
| Label           | `TPUB`               |                                                                                                                        |
| Track Title     | `TIT2`               |                                                                                                                        |
| Track Artists   | `TPE1`               | `TPE4` (Remixer), `TCOM` (Composer), `TPE3` (Conductor), `TIPL,IPLS/producer` (producer), `TIPL,IPLS/DJ-mix` (djmixer) |
| Track Number    | `TRCK`               |                                                                                                                        |
| Disc Number     | `TPOS`               |                                                                                                                        |
| Rosé ID         | `TXXX:ROSEID`        |                                                                                                                        |
| Rosé Release ID | `TXXX:ROSERELEASEID` |                                                                                                                        |

## MP4

| Tag             | Field Name                           | Will Ingest These Fields                                                                                                                                                                               |
| --------------- | ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Release Title   | `\xa9alb`                            |                                                                                                                                                                                                        |
| Release Artists | `aART`                               |                                                                                                                                                                                                        |
| Release Year    | `\xa9day`                            |                                                                                                                                                                                                        |
| Release Type    | `----:com.apple.iTunes:RELEASETYPE`  |                                                                                                                                                                                                        |
| Genre           | `\xa9gen`                            |                                                                                                                                                                                                        |
| Label           | `----:com.apple.iTunes:LABEL`        |                                                                                                                                                                                                        |
| Track Title     | `\xa9nam`                            |                                                                                                                                                                                                        |
| Track Artists   | `\xa9ART`                            | `----:com.apple.iTunes:REMIXER` (Remixer), `\xa9wrt` (Composer), `----:com.apple.iTunes:CONDUCTOR` (Conductor), `----:com.apple.iTunes:PRODUCER` (producer), `----:com.apple.iTunes:DJMIXER` (djmixer) |
| Track Number    | `trkn`                               |                                                                                                                                                                                                        |
| Disc Number     | `disk`                               |                                                                                                                                                                                                        |
| Rosé ID         | `----:net.sunsetglow.rose:ID`        |                                                                                                                                                                                                        |
| Rosé Release ID | `----:net.sunsetglow.rose:RELEASEID` |                                                                                                                                                                                                        |

## Vorbis

| Tag             | Field Name      | Will Ingest These Fields                                                                                        |
| --------------- | --------------- | --------------------------------------------------------------------------------------------------------------- |
| Release Title   | `release`       |                                                                                                                 |
| Release Artists | `albumartist`   |                                                                                                                 |
| Release Year    | `date`          | `year`                                                                                                          |
| Release Type    | `releasetype`   |                                                                                                                 |
| Genre           | `genre`         |                                                                                                                 |
| Label           | `organization`  | `label`, `recordlabel`                                                                                          |
| Track Title     | `title`         |                                                                                                                 |
| Track Artists   | `artist`        | `remixer` (Remixer), `composer` (Composer), `conductor` (Conductor), `producer` (producer), `djmixer` (djmixer) |
| Track Number    | `tracknumber`   |                                                                                                                 |
| Disc Number     | `discnumber`    |                                                                                                                 |
| Rosé ID         | `roseid`        |                                                                                                                 |
| Rosé Release ID | `rosereleaseid` |                                                                                                                 |
