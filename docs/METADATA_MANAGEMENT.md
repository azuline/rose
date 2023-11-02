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
Rules can be ran ad hoc from the command line, or stored in the configuration
file to be repeatedly ran over the entire library.

Rules consist of a _matcher_, which matches against tracks in your library, and
one or more _actions_, which modify the metadata of the matched tracks.

The 5 available actions let you _replace_ values, apply a regex substitution
(_sed_), _split_ one value into multiple values, _delete_ values, and _add_ new
values.

## Demo

Before diving into the mechanics and language of the rules engine, let's begin
with a quick demo of how the rule engine works.

Let's say that I am a LOOΠΔ fan (I mean, who isn't?). In my library, I have
two of Chuu's releases, but the first is tagged as `CHUU`, and the second as
`Chuu`. I want to normalize them to both be `Chuu`. The following rule
expresses this change:

```bash
# The first argument to run-rule is the matcher. We match artist tags with the
# value CHUU. The second argument is our action. We replace the matched artist
# tags with Chuu.
$ rose metadata run-rule 'trackartist,albumartist:^CHUU$' 'replace:Chuu'

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
[01:10:58] INFO: Wrote tag changes to CHUU - 2023. Howl/01. Howl.opus
[01:10:58] INFO: Wrote tag changes to CHUU - 2023. Howl/02. Underwater.opus
[01:10:58] INFO: Wrote tag changes to CHUU - 2023. Howl/03. My Palace.opus
[01:10:58] INFO: Wrote tag changes to CHUU - 2023. Howl/04. Aliens.opus
[01:10:58] INFO: Wrote tag changes to CHUU - 2023. Howl/05. Hitchhiker.opus

Applied tag changes to 5 tracks!
```

And we now have a single Chuu artist in our library!

Let's go through one more example. I want all of Chuu's releases to have the
K-Pop genre. The following rule expresses that: for all releases with the
albumartist `Chuu`, add the `K-Pop` genre tag.

```bash
$ rose metadata run-rule 'albumartist:^Chuu$' 'genre::add:K-Pop'

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
      genre: ['Kpop'] -> ['Kpop', 'K-Pop']
LOOΠΔ - 2017. Chuu/02. Girl's Talk.opus
      genre: ['Kpop'] -> ['Kpop', 'K-Pop']

Write changes to 7 tracks? [Y/n] y

[01:14:57] INFO: Writing tag changes for rule matcher=trackartist,albumartist:Chuu action=genre::replace-all:K-Pop
[01:14:57] INFO: Wrote tag changes to CHUU - 2023. Howl/01. Howl.opus
[01:14:57] INFO: Wrote tag changes to CHUU - 2023. Howl/02. Underwater.opus
[01:14:57] INFO: Wrote tag changes to CHUU - 2023. Howl/03. My Palace.opus
[01:14:57] INFO: Wrote tag changes to CHUU - 2023. Howl/04. Aliens.opus
[01:14:57] INFO: Wrote tag changes to CHUU - 2023. Howl/05. Hitchhiker.opus
[01:14:57] INFO: Wrote tag changes to LOOΠΔ - 2017. Chuu/01. Heart Attack.opus
[01:14:57] INFO: Wrote tag changes to LOOΠΔ - 2017. Chuu/02. Girl's Talk.opus

Applied tag changes to 7 tracks!
```

Success! However, notice that one of Chuu's releases has the genre tag `Kpop`.
Let's convert that `Kpop` tag to `K-Pop`, across the board.

```bash
$ rose metadata run-rule 'genre:^Kpop$' 'replace:K-Pop'
G‐Dragon - 2012. ONE OF A KIND/01. One Of A Kind.opus
      genre: ['Kpop'] -> ['K-Pop']
G‐Dragon - 2012. ONE OF A KIND/02. 크레용 (Crayon).opus
      genre: ['Kpop'] -> ['K-Pop']
G‐Dragon - 2012. ONE OF A KIND/03. 결국.opus
      genre: ['Kpop'] -> ['K-Pop']
G‐Dragon - 2012. ONE OF A KIND/04. 그 XX.opus
      genre: ['Kpop'] -> ['K-Pop']
G‐Dragon - 2012. ONE OF A KIND/05. Missing You.opus
      genre: ['Kpop'] -> ['K-Pop']
G‐Dragon - 2012. ONE OF A KIND/06. Today.opus
      genre: ['Kpop'] -> ['K-Pop']
G‐Dragon - 2012. ONE OF A KIND/07. 불 붙여봐라.opus
      genre: ['Kpop'] -> ['K-Pop']
LOOΠΔ - 2017. Chuu/01. Heart Attack.opus
      genre: ['Kpop', 'K-Pop'] -> ['K-Pop']
LOOΠΔ - 2017. Chuu/02. Girl's Talk.opus
      genre: ['Kpop', 'K-Pop'] -> ['K-Pop']

Write changes to 9 tracks? [Y/n] y

[14:47:26] INFO: Writing tag changes for rule matcher=genre:Kpop action=matched:Kpop::replace:K-Pop
[14:47:26] INFO: Wrote tag changes to G‐Dragon - 2012. ONE OF A KIND/01. One Of A Kind.opus
[14:47:26] INFO: Wrote tag changes to G‐Dragon - 2012. ONE OF A KIND/02. 크레용 (Crayon).opus
[14:47:26] INFO: Wrote tag changes to G‐Dragon - 2012. ONE OF A KIND/03. 결국.opus
[14:47:26] INFO: Wrote tag changes to G‐Dragon - 2012. ONE OF A KIND/04. 그 XX.opus
[14:47:26] INFO: Wrote tag changes to G‐Dragon - 2012. ONE OF A KIND/05. Missing You.opus
[14:47:26] INFO: Wrote tag changes to G‐Dragon - 2012. ONE OF A KIND/06. Today.opus
[14:47:26] INFO: Wrote tag changes to G‐Dragon - 2012. ONE OF A KIND/07. 불 붙여봐라.opus
[14:47:26] INFO: Wrote tag changes to LOOΠΔ - 2017. Chuu/01. Heart Attack.opus
[14:47:26] INFO: Wrote tag changes to LOOΠΔ - 2017. Chuu/02. Girl's Talk.opus

Applied tag changes to 7 tracks!
```

And we also normalized a G-Dragon release's genre tag on the way! I'd like to
run these rules again in the future, so that all new music I add to my library
is normalized according to these rules. To do so, the following text goes
inside my configuration file.

```toml
[[stored_metadata_rules]]
matcher = "trackartist,albumartist:^CHUU$"
actions = ["replace:Chuu"]
[[stored_metadata_rules]]
matcher = "albumartist:^Chuu$"
actions = ["genre::add:K-Pop"]
[[stored_metadata_rules]]
matcher = "genre:^Kpop$"
actions = ["replace:K-Pop"]
```

Now, the `rose metadata run-stored-rules` command will run the above three
rules, along with any other rules I have in my configuration file, on the
entire library.

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

Case sensitive

### Actions

Actions are `(tags, pattern, all, kind, *args)` tuples for modifying the
metadata of a track.

The `tags` and `pattern`, usually by default, equivalent the `matcher`. TODO

Given a track, if the `pattern` matches the `tags`, by the same logic as the
matchers, the action is applied.

There are five kinds of actions: `replace`, `sed`, `split`, `add`, and
`delete`. Each action has its own set of additional arguments.

- `replace`: Replace a tag. 

### Multi-Value Tags

For multi-valued tags, `all`...

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
