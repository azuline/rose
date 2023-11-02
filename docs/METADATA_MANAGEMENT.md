# Managing Your Music Metadata

Rosé relies on the metadata embedded in your music files to organize your music
into a useful virtual filesystem. This means that the quality of the music tags
is important for getting the most out of Rosé.

Therefore, Rosé also provides tools to improve the metadata of your music.
Currently, Rosé provides:

- A text-based interface for manually modifying release metadata,
- Metadata importing from third-party sources,
- And a rules engine to bulk update metadata.

> [!NOTE]
> Rosé has opinionated conventions for how metadata is stored in audio tags.
> See [Tagging Conventions](./TAGGING_CONVENTIONS.md) for documentation.

# Text-Based Release Editing

Rosé supports editing a release's metadata as a text file via the
`rose releases edit` command. This command accepts a Release ID or a Release's
Virtual Filesystem Directory Name.

So for example:

```bash
$ rose releases edit "LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP [Dance-Pop;Future Bass;K-Pop]"
$ rose releases edit "018b4ff1-acdf-7ff1-bcd6-67757aea0fed"
```

This command opens up a TOML representation of the release's metadata in your
`$EDITOR`. Upon save and exit, the TOML's metadata is written to the file tags.

> [!NOTE]
> Rosé validates the Artist Role and Release Type fields upon metadata edit.
> The values provided must be one of the supported values. The supported values
> are documented in [Artist Tags](#artist-tags) and [Release Type Tags](#release-type-tags).

An example of the editing TOML:

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

# Rules Engine

Rosé's rule engine allows you to update metadata in bulk across your library.
The rule engine supports two methods of execution:

1. Running ad hoc rules in the command line.
2. Storing rules in the configuration to run repeatedly.

## Example

I have two artists in Rosé: `CHUU` and `Chuu`. They're actually the same
artist, but capitalized differently. To normalize them, I execute the following
ad hoc rule:

```bash
$ rose metadata run-rule 'trackartist,albumartist:CHUU' 'replace:Chuu'
CHUU - 2023. Howl/01. Howl.opus
      trackartist[main]: ['CHUU'] -> ['Chuu']
      albumartist[main]: ['CHUU'] -> ['Chuu']
CHUU - 2023. Howl/02. Underwater.opus
      trackartist[main]: ['CHUU'] -> ['Chuu']
      albumartist[main]: ['CHUU'] -> ['Chuu']
CHUU - 2023. Howl/03. My Palace.opus
      trackartist[main]: ['CHUU'] -> ['Chuu']
      albumartist[main]: ['CHUU'] -> ['Chuu']
CHUU - 2023. Howl/04. Aliens.opus
      trackartist[main]: ['CHUU'] -> ['Chuu']
      albumartist[main]: ['CHUU'] -> ['Chuu']
CHUU - 2023. Howl/05. Hitchhiker.opus
      trackartist[main]: ['CHUU'] -> ['Chuu']
      albumartist[main]: ['CHUU'] -> ['Chuu']

Write changes to 5 tracks?  [Y/n] y

[01:10:58] INFO: Writing tag changes for rule matcher=trackartist,albumartist:CHUU action=matched:CHUU::replace:Chuu
[01:10:58] INFO: Writing tag changes to CHUU - 2023. Howl/01. Howl.opus
[01:10:58] INFO: Writing tag changes to CHUU - 2023. Howl/02. Underwater.opus
[01:10:58] INFO: Writing tag changes to CHUU - 2023. Howl/03. My Palace.opus
[01:10:58] INFO: Writing tag changes to CHUU - 2023. Howl/04. Aliens.opus
[01:10:58] INFO: Writing tag changes to CHUU - 2023. Howl/05. Hitchhiker.opus

Applied tag changes to 5 tracks!
```

And we now have a single Chuu!

```bash
$ rose tracks print ...
TODO
```

And I also want to set all of Chuu's releases to the `K-Pop` genre:

```bash
$ rose metadata run-rule 'trackartist,albumartist:Chuu' 'genre::replace-all:K-Pop'
CHUU - 2023. Howl/01. Howl.opus
      genre: [] -> ['K-Pop']
CHUU - 2023. Howl/02. Underwater.opus
      genre: [] -> ['K-Pop']
CHUU - 2023. Howl/03. My Palace.opus
      genre: [] -> ['K-Pop']
CHUU - 2023. Howl/04. Aliens.opus
      genre: [] -> ['K-Pop']
CHUU - 2023. Howl/05. Hitchhiker.opus
      genre: [] -> ['K-Pop']
LOOΠΔ - 2017. Chuu/01. Heart Attack.opus
      genre: ['Kpop'] -> ['K-Pop']
LOOΠΔ - 2017. Chuu/02. Girl's Talk.opus
      genre: ['Kpop'] -> ['K-Pop']

Write changes to 7 tracks? [Y/n] y

[01:14:57] INFO: Writing tag changes for rule matcher=trackartist,albumartist:Chuu action=genre::replace-all:K-Pop
[01:14:57] INFO: Writing tag changes to CHUU - 2023. Howl/01. Howl.opus
[01:14:57] INFO: Writing tag changes to CHUU - 2023. Howl/02. Underwater.opus
[01:14:57] INFO: Writing tag changes to CHUU - 2023. Howl/03. My Palace.opus
[01:14:57] INFO: Writing tag changes to CHUU - 2023. Howl/04. Aliens.opus
[01:14:57] INFO: Writing tag changes to CHUU - 2023. Howl/05. Hitchhiker.opus
[01:14:57] INFO: Writing tag changes to LOOΠΔ - 2017. Chuu/01. Heart Attack.opus
[01:14:57] INFO: Writing tag changes to LOOΠΔ - 2017. Chuu/02. Girl's Talk.opus

Applied tag changes to 7 tracks!
```

Now that I've written these rules, I can also store them in Rosé's configuration in
order to apply them on all releases I add in the future. I do this by appending
the following to my configuration file:

```toml
[[stored_metadata_rules]]
matcher = "trackartist,albumartist:CHUU"
actions = ["replace:Chuu"]
[[stored_metadata_rules]]
matcher = "trackartist,albumartist:Chuu"
actions = ["genre::replace-all:K-Pop"]
```

And with the `rose metadata run-stored-rules` command, I can run these rules,
as well as the others, repeatedly again in the future.

## Mechanics

The rules engine operates in two steps:

1. Find all tracks matching a _matcher_.
2. Apply _actions_ to the matched tracks.

### Matchers

Matchers are `(tags, pattern)` tuples for selecting tracks. Tracks are selected
if the `pattern` matches one or more of the track's values for the given
`tags`.

Pattern matching is executed as a substring match. For example, the patterns
`Chuu`, `Chu`, `hu`, and `huu` all match `Chuu`. Regex is not supported for
pattern matching due to its performance.

The `^` and `$` characters enable strict prefix and strict suffix matching,
respectively. So for example, the pattern `^Chu` match `Chuu`, but not `AChuu`.
And the pattern `Chu$` matches `Chu`, but not `Chuu`.

### Actions

Actions are `(tags, pattern, all, kind, *args)` tuples for modifying the
metadata of a track.

Given a track, if the `pattern` matches the `tags`, by the same logic as the
matchers, the action is applied.

There are four kinds of actions: `replace`, `sed`, `split`, and `delete`. Each
action has its own set of additional arguments.

- `replace`:

For multi-valued tags, `all`...

The `tags` and `pattern`, usually by default, equivalent the `matcher`.

### Track-Based Paradigm

Each action is applied to the track _as a whole_. Rosé does not
inherently restrict the action solely to the matched tag. What does this mean?

Examples TODO

## Rule Language

Rosé provides a Domain Specific Language (DSL) for defining rules. Rosé's
language has two types of expressions: _matchers_ and _actions_.

TODO

The formal syntax is defined by the following grammar:

```
<matcher> ::= <tags> ':' <pattern>
<tags>    ::= string | string ',' <tags>
<pattern> ::= string | '^' string | string '$' | '^' string '$'

<action>         ::= <action-matcher> '::' <subaction> | <subaction>
<action-matcher> ::= <tags> | <tags> ':' <pattern>
<subaction>      ::= <replace-action> | <sed-action> | <split-action> | <delete-action>
<replace-action> ::= 'replace' <optional-all> ':' string
<sed-action>     ::= 'sed' <optional-all> ':' string ':' string
<split-action>   ::= 'split' <optional-all> ':' string
<delete-action>  ::= 'delete' <optional-all>
<optional-all>   ::= '' | '-all'
```

## Dry Runs

TODO

# Metadata Import & Cover Art Downloading

_In Development_

Sources: Discogs, MusicBrainz, Tidal, Deezer, Apple, Junodownload, Beatport,
fanart.tv, and RYM.
