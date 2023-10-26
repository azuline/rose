# Managing Your Music Metadata

TODO Intro: motivations, reasons, fields we care about

## Tag Conventions

Rosé is lenient in the tags it ingests, but has opinionated conventions for the
tags it writes. Impl detail: how exactly does rose edit the files?

### Field Mappings

TODO

### Multi-Valued Tags

Rosé supports multiple values for the artists, genres, and labels tags. Rosé
writes a single tag field and with fields concatenated together with a `;`
delimiter. For example, `genre=Deep House;Techno`. Rosé does not write one tag
per frame due to inconsistent support by other useful programs.

### Artist Tags

Rosé preserves the artists' role in the artist tag by using specialized
delimiters. An example artist tag is: `Pyotr Ilyich Tchaikovsky performed by André Previn;London Symphony Orchestra feat. Barack Obama`.

The artist tag is described by the following grammar:

```
<artist-tag> ::= <composer> <dj> <main> <guest> <remixer> <producer>
<composer>   ::= <name> ' performed by '
<dj>         ::= <name> ' pres. '
<main>       ::= <name>
<guest>      ::= ' feat. ' <name>
<remixer>    ::= ' remixed by ' <name>
<producer>   ::= ' produced by ' <name>
<name>       ::= string ';' <name> | string
```

## Manual Editing

TODO: Text file editing

## Metadata Import & Cover Downloading

TBD

## Rules Engine

TBD
