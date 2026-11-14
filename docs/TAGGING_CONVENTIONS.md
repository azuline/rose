# Tagging Conventions

This document describes how Rose reads and writes tags from files. Rose applies fairly rigid
conventions in the tags it writes, and applies a relaxed version of those conventions when ingesting
tags from audio files.

# Managed Tags

Rose manages the following tags:

- Release Tags:
  - Title
  - Release Artists
  - Release Year
  - Original Year
  - Composition Year (for Classical)
  - Release Type (e.g. Album, EP, Single)
  - Genre
  - Secondary Genre
  - Descriptor
  - Label
  - Catalog Number
  - Edition
  - Rose Release ID
- Track Tags:
  - Title
  - Artists
  - Track Number
  - Disc Number
  - Rose ID

Rose does not care about any other tags and does not do anything with them.

For documentation on the specific field names that Rose uses for each tag container format, please
see [Tag Field Mappings](#tag-field-mappings).

# Multi-Valued Tags

Rose supports multiple values for the artists, genres, and labels tags. Rose writes a single tag
field and with fields concatenated together with a `;` delimiter. For example, `genre=Deep
House;Techno`. Rose does not write multiple frames for a single tag (where each value gets one
frame) due to inconsistent support by other useful programs.

When reading tags, Rose is more relaxed in the delimiters it accepts. For the Genre, Label, Artists,
and Release Artists tags, Rose will attempt to split a single tag into multiple tags by the
following delimiters: <code>&nbsp;\\\\&nbsp;</code>, <code>&nbsp;/&nbsp;</code>, <code>;</code>, and
<code>&nbsp;vs.&nbsp;</code>.

## Parent Genres

If the `write_parent_genres` configuration option is enabled (it is disabled by default; see
[Configuration](./CONFIGURATION.md)), Rose also writes the transitive parent genres of a release's
genres into the Genre and Secondary Genre tags. The parent genres are sorted and appended after a
`\\PARENTS:\\` marker. For example, a release tagged with the genre `Deep House` is written as:

```
genre=Deep House\\PARENTS:\\Dance;Electronic;Electronic Dance Music;House
```

This makes the parent genres visible to other programs that read the tags. When reading tags, Rose
discards everything after the `\\PARENTS:\\` marker, so the parent genres are never treated as
genres that were explicitly assigned to the release.

# Artist Tags

Rose preserves the artists' role in the artist tag by using specialized delimiters. An example
artist tag is: `Pyotr Ilyich Tchaikovsky performed by André Previn;London Symphony Orchestra feat.
Barack Obama`.

The artist tag is described by the following grammar:

```
<artist-tag> ::= [<composer>] [<djmixer>] <main> [<conductor>] [<guest>] [<remixer>] [<producer>]
<composer>   ::= <name> ' performed by '
<djmixer>    ::= <name> ' pres. '
<main>       ::= <name>
<conductor>  ::= ' under. ' <name>
<guest>      ::= ' feat. ' <name>
<remixer>    ::= ' remixed by ' <name>
<producer>   ::= ' produced by ' <name>
<name>       ::= string ';' <name> | string
```

Rose supports the following artist roles:

- `main`
- `guest`
- `producer`
- `composer`
- `conductor`
- `remixer`
- `djmixer`

Rose writes a single tag value into the _Track Artists_ and _Release Artists_ tags. Though some
conventions exist for writing each role into its own tag, Rose does not follow them, due to
inconsistent (mainly nonexistent) support by other useful programs.

# Release Type Tags

Rose supports tagging the release _type_. The supported values are:

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
- `loosetrack`
- `unknown`

# Tag Field Mappings

Rose supports three tag container formats:

- ID3: `.mp3` files
- MP4: `.m4a` files
- Vorbis: `.ogg`, `.opus`, and `.flac` files

In this section, we list out the per-container field names that we read/write. Rose will only write
to a single field for each tag; however, for tags with multiple conventions out in the rest of the
world, Rose will support reading from additional fields.

## MP3

| Tag              | Field Name             | Will Ingest These Fields                                                                                               |
| ---------------- | ---------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| Release Title    | `TALB`                 |                                                                                                                        |
| Release Artists  | `TPE2`                 |                                                                                                                        |
| Release Type     | `TXXX:RELEASETYPE`     | `TXXX:MusicBrainz Album Type`                                                                                          |
| Release Year     | `TDRC`                 | `TYER`, `TDAT`                                                                                                         |
| Original Year    | `TDOR`                 | `TORY`                                                                                                                 |
| Composition Year | `TXXX:COMPOSITIONDATE` |                                                                                                                        |
| Genre            | `TCON`                 |                                                                                                                        |
| Secondary Genre  | `TXXX:SECONDARYGENRE`  |                                                                                                                        |
| Descriptor       | `TXXX:DESCRIPTOR`      |                                                                                                                        |
| Label            | `TPUB`                 |                                                                                                                        |
| Catalog Number   | `TXXX:CATALOGNUMBER`   |                                                                                                                        |
| Edition          | `TXXX:EDITION`         |                                                                                                                        |
| Track Title      | `TIT2`                 |                                                                                                                        |
| Track Artists    | `TPE1`                 | `TPE4` (Remixer), `TCOM` (Composer), `TPE3` (Conductor), `TIPL,IPLS/producer` (producer), `TIPL,IPLS/DJ-mix` (djmixer) |
| Track Number     | `TRCK`                 |                                                                                                                        |
| Disc Number      | `TPOS`                 |                                                                                                                        |
| Rose ID          | `TXXX:ROSEID`          |                                                                                                                        |
| Rose Release ID  | `TXXX:ROSERELEASEID`   |                                                                                                                        |

## MP4

| Tag              | Field Name                                 | Will Ingest These Fields                                                                                                                                                                               |
| ---------------- | ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Release Title    | `\xa9alb`                                  |                                                                                                                                                                                                        |
| Release Artists  | `aART`                                     |                                                                                                                                                                                                        |
| Release Type     | `----:com.apple.iTunes:RELEASETYPE`        | `----:com.apple.iTunes:MusicBrainz Album Type`                                                                                                                                                         |
| Release Year     | `\xa9day`                                  |                                                                                                                                                                                                        |
| Original Year    | `----:net.sunsetglow.rose:ORIGINALDATE`    | `----:com.apple.iTunes:ORIGINALDATE`, `----:com.apple.iTunes:ORIGINALYEAR`                                                                                                                             |
| Composition Year | `----:net.sunsetglow.rose:COMPOSITIONDATE` |                                                                                                                                                                                                        |
| Genre            | `\xa9gen`                                  |                                                                                                                                                                                                        |
| Secondary Genre  | `----:net.sunsetglow.rose:SECONDARYGENRE`  |                                                                                                                                                                                                        |
| Descriptor       | `----:net.sunsetglow.rose:DESCRIPTOR`      |                                                                                                                                                                                                        |
| Label            | `----:com.apple.iTunes:LABEL`              |                                                                                                                                                                                                        |
| Catalog Number   | `----:com.apple.iTunes:CATALOGNUMBER`      |                                                                                                                                                                                                        |
| Edition          | `----:net.sunsetglow.rose:EDITION`         |                                                                                                                                                                                                        |
| Track Title      | `\xa9nam`                                  |                                                                                                                                                                                                        |
| Track Artists    | `\xa9ART`                                  | `----:com.apple.iTunes:REMIXER` (Remixer), `\xa9wrt` (Composer), `----:com.apple.iTunes:CONDUCTOR` (Conductor), `----:com.apple.iTunes:PRODUCER` (producer), `----:com.apple.iTunes:DJMIXER` (djmixer) |
| Track Number     | `trkn`                                     |                                                                                                                                                                                                        |
| Disc Number      | `disk`                                     |                                                                                                                                                                                                        |
| Rose ID          | `----:net.sunsetglow.rose:ID`              |                                                                                                                                                                                                        |
| Rose Release ID  | `----:net.sunsetglow.rose:RELEASEID`       |                                                                                                                                                                                                        |

## Vorbis

| Tag              | Field Name        | Will Ingest These Fields                                                                                        |
| ---------------- | ----------------- | --------------------------------------------------------------------------------------------------------------- |
| Release Title    | `album`           |                                                                                                                 |
| Release Artists  | `albumartist`     |                                                                                                                 |
| Release Type     | `releasetype`     |                                                                                                                 |
| Release Year     | `date`            | `year`                                                                                                          |
| Original Year    | `originaldate`    | `originalyear`                                                                                                  |
| Composition Year | `compositiondate` |                                                                                                                 |
| Genre            | `genre`           |                                                                                                                 |
| Secondary Genre  | `secondarygenre`  |                                                                                                                 |
| Descriptor       | `descriptor`      |                                                                                                                 |
| Label            | `label`           | `organization`, `recordlabel`                                                                                   |
| Catalog Number   | `catalognumber`   |                                                                                                                 |
| Edition          | `edition`         |                                                                                                                 |
| Track Title      | `title`           |                                                                                                                 |
| Track Artists    | `artist`          | `remixer` (Remixer), `composer` (Composer), `conductor` (Conductor), `producer` (producer), `djmixer` (djmixer) |
| Track Number     | `tracknumber`     |                                                                                                                 |
| Disc Number      | `discnumber`      |                                                                                                                 |
| Rose ID          | `roseid`          |                                                                                                                 |
| Rose Release ID  | `rosereleaseid`   |                                                                                                                 |
