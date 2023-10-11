# Rosé

A virtual filesystem music library and a music metadata manager.

## The Virtual Filesystem Library

Rosé reads a source directory of albums like this:

```
.
├── BLACKPINK - 2016. SQUARE ONE
├── BLACKPINK - 2016. SQUARE TWO
├── LOOΠΔ - 2019. [X X]
├── LOOΠΔ - 2020. [#]
├── LOOΠΔ 1_3 - 2017. Love & Evil
├── LOOΠΔ ODD EYE CIRCLE - 2017. Max & Match
└── YUZION - 2019. Young Trapper
```

And constructs a virtual filesystem from the source directory's audio tags. The
virtual filesystem enables viewing various subcollections of the source
directory based on multiple types of tags as a filesystem.

While music players and music servers enable viewing releases with similar
filters, those filters are only available in a proprietary UI. Rosé provides
this filtering as a filesystem, which is easily composable with other tools and
systems.

The virtual filesystem constructed from the above source directory is:

```
.
├── albums
│   ├── BLACKPINK - 2016. SQUARE ONE - Single [K-Pop] {YG Entertainment}
│   ├── BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}
│   ├── LOOΠΔ 1_3 - 2017. Love & Evil [K-Pop] {BlockBerry Creative}
│   ├── LOOΠΔ - 2019. [X X] [K-Pop] {BlockBerry Creative}
│   ├── LOOΠΔ - 2020. [#] [K-Pop]
│   ├── LOOΠΔ ODD EYE CIRCLE - 2017. Max & Match [K-Pop]
│   └── YUZION - 2019. Young Trapper [Hip Hop]
├── artists
│   ├── BLACKPINK
│   │   ├── BLACKPINK - 2016. SQUARE ONE - Single [K-Pop] {YG Entertainment}
│   │   └── BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}
│   ├── LOOΠΔ
│   │   ├── LOOΠΔ - 2019. [X X] [K-Pop] {BlockBerry Creative}
│   │   └── LOOΠΔ - 2020. [#] [K-Pop]
│   ├── LOOΠΔ 1_3
│   │   └── LOOΠΔ 1_3 - 2017. Love & Evil [K-Pop] {BlockBerry Creative}
│   ├── LOOΠΔ ODD EYE CIRCLE
│   │   └── LOOΠΔ ODD EYE CIRCLE - 2017. Max & Match [K-Pop]
│   └── YUZION
│       └── YUZION - 2019. Young Trapper [Hip Hop]
├── genres
│   ├── Hip-Hop
│   │   └── YUZION - 2019. Young Trapper [Hip Hop]
│   └── K-Pop
│       ├── BLACKPINK - 2016. SQUARE ONE - Single [K-Pop] {YG Entertainment}
│       ├── BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}
│       ├── LOOΠΔ 1_3 - 2017. Love & Evil [K-Pop] {BlockBerry Creative}
│       ├── LOOΠΔ - 2019. [X X] [K-Pop] {BlockBerry Creative}
│       ├── LOOΠΔ - 2020. [#] [K-Pop]
│       └── LOOΠΔ ODD EYE CIRCLE - 2017. Max & Match [K-Pop]
└── labels
    ├── BlockBerry Creative
    │   ├── LOOΠΔ 1_3 - 2017. Love & Evil [K-Pop] {BlockBerry Creative}
    │   ├── LOOΠΔ - 2019. [X X] [K-Pop] {BlockBerry Creative}
    │   ├── LOOΠΔ - 2021. [&] [K-Pop] {BlockBerry Creative}
    │   └── LOOΠΔ ODD EYE CIRCLE - 2017. Max & Match [K-Pop]
    └── YG Entertainment
        ├── BLACKPINK - 2016. SQUARE ONE - Single [K-Pop] {YG Entertainment}
        └── BLACKPINK - 2016. SQUARE TWO - Single [K-Pop] {YG Entertainment}
```

## The Metadata Manager

Rosé constructs the virtual filesystem from the audio tags. However, audio tags
are frequently missing or incorrect. Thus, Rosé also provides a set of tools to
improve the audio tag metadata.

Which I have yet to write. Please check back later!

# Configuration

Rose is configured via a TOML file located at
`${XDG_CONFIG_HOME:-$HOME/.config}/rose/config.toml`.

The configuration parameters, with examples, are:

```toml
# The directory containing the music to manage.
music_source_dir = "~/.music-src"
# The directory to mount the library's virtual filesystem on.
fuse_mount_dir = "~/music"
# The directory to write the cache to. Defaults to `${XDG_CACHE_HOME:-$HOME/.cache}/rose`.
cache_dir = "~/.cache/rose"
```

The `--config/-c` flag overrides the config location.

## Library Conventions & Expectations

### Directory Structure

`$music_source_dir/albums/track.ogg`

### Supported Extensions

### Tag Structure

WIP

artist1;artist2 feat. artist3

BNF TODO

# Architecture

todo

- db is read cache, not source of truth
- filetags and files are source of truth
