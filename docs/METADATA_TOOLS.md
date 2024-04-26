# Improving Your Music Metadata

Rosé relies on the metadata embedded in your music files to organize your music into a useful
virtual filesystem. This means that the quality of the music tags is important for getting the most
out of Rosé.

Therefore, Rosé also provides a text-based interface for manually modifying metadata and a rules
engine for bulk updating metadata to improve the tags of your music library.

> [!NOTE]
> Rosé has opinionated conventions for how metadata is stored in audio tags. See
> [Tagging Conventions](./TAGGING_CONVENTIONS.md) for documentation.

# Text-Based Release Editing

Rosé supports editing a release's metadata as a text file via the `rose releases edit` command. This
command accepts a Release ID or a Release's virtual filesystem directory name.

So for example:

```bash
$ rose releases edit "$fuse_mount_dir/1. Releases/LOOΠΔ ODD EYE CIRCLE - 2017. Mix & Match - EP"
$ rose releases edit "018b4ff1-acdf-7ff1-bcd6-67757aea0fed"
```

This command opens up a TOML representation of the release's metadata in your `$EDITOR`. Upon save
and exit, the TOML's metadata is written to the file tags.

> [!NOTE]
> The Artist Role and Release Type fields must be one of the supported enum values. The supported
> values are documented in [Tagging Conventions](./TAGGING_CONVENTIONS.md).

An example of the release's TOML representation:

```toml
title = "Mix & Match"
new = false
releasetype = "ep"
releaseyear = 2017
originalyear = 2017
compositionyear = -9999
genres = [
    "K-Pop",
    "Dance-Pop",
    "Future Bass",
]
secondarygenres = [
    "Electropop",
    "Alternative R&B",
    "Synthpop",
]
descriptors = [
    "melodic",
    "energetic",
    "female vocalist",
    "sensual",
    "love",
    "playful",
    "romantic",
    "eclectic",
    "lush",
    "rhythmic",
    "optimistic",
    "warm",
    "urban",
    "uplifting",
]
labels = [
    "BlockBerryCreative",
]
edition = ""
catalognumber = "WMED0709"
artists = [
    { name = "LOOΠΔ ODD EYE CIRCLE", role = "main" },
]

[tracks.018b6514-6fb8-729f-bf86-7590187ff377]
discnumber = "1"
tracknumber = "1"
title = "ODD"
artists = [
    { name = "LOOΠΔ ODD EYE CIRCLE", role = "main" },
]

[tracks.018b6514-6fba-7508-8576-c8e82ad4b7bc]
discnumber = "1"
tracknumber = "2"
title = "Girl Front"
artists = [
    { name = "LOOΠΔ ODD EYE CIRCLE", role = "main" },
]

[tracks.018b6514-6fb9-73f1-a139-18ecefcf55da]
discnumber = "1"
tracknumber = "3"
title = "LOONATIC"
artists = [
    { name = "LOOΠΔ ODD EYE CIRCLE", role = "main" },
]

[tracks.018b6514-6fb7-7cc6-9d23-8eaf0b1beee8]
discnumber = "1"
tracknumber = "4"
title = "Chaotic"
artists = [
    { name = "LOOΠΔ ODD EYE CIRCLE", role = "main" },
]

[tracks.018b6514-6fb6-766f-8430-c6ea3f48966d]
discnumber = "1"
tracknumber = "5"
title = "Starlight"
artists = [
    { name = "LOOΠΔ ODD EYE CIRCLE", role = "main" },
]
```

# Rules Engine

Rosé's rule engine allows you to update metadata in bulk across your library.

Rules consist of a _track matcher_, which matches against tracks in your library, and one or more
_actions_, which modify the metadata of the matched tracks. The 5 available actions let you
_replace_ values, apply a regex substitution (_sed_), _split_ one value into multiple values,
_delete_ values, and _add_ new values.

To run an ad hoc rule from the command line, use the following command:

```bash
# Accepts one or more actions.
$ rose rules run [trackmatcher] [action]...
```

Rules can also be stored in the configuration file to be ran on future additions to the library. Add
one instance of the following block to the configuration file for each rule:

```toml
[[stored_metadata_rules]]
matcher = "genre:^Kpop$"  # An example track matcher.
actions = ["replace:K-Pop"]  # Example actions.
```

The `rose rules run-stored` command runs all stored rules. Note that Rosé runs rules and actions in
the order they're defined in. So if multiple rules would modify one track, the earliest defined rule
will be applied first, and later rules applied on the output of the first rule.

## Demo

Before diving into the mechanics and language of the rules engine, let's begin with a quick demo of
how the rule engine works.

Let's say that I am a LOOΠΔ fan (I mean, who isn't?). In my library, I have two of Chuu's releases,
but the first is tagged as `CHUU`, and the second as `Chuu`. I want to normalize the former to
`Chuu`. The following rule expresses this change:

```bash
$ rose rules run 'artist:^CHUU$' 'replace:Chuu'

CHUU - 2023. Howl/01. Howl.opus
      trackartist[main]: ['CHUU'] -> ['Chuu']
      releaseartist[main]: ['CHUU'] -> ['Chuu']
CHUU - 2023. Howl/02. Underwater.opus
      trackartist[main]: ['CHUU'] -> ['Chuu']
      releaseartist[main]: ['CHUU'] -> ['Chuu']
CHUU - 2023. Howl/03. My Palace.opus
      trackartist[main]: ['CHUU'] -> ['Chuu']
      releaseartist[main]: ['CHUU'] -> ['Chuu']
CHUU - 2023. Howl/04. Aliens.opus
      trackartist[main]: ['CHUU'] -> ['Chuu']
      releaseartist[main]: ['CHUU'] -> ['Chuu']
CHUU - 2023. Howl/05. Hitchhiker.opus
      trackartist[main]: ['CHUU'] -> ['Chuu']
      releaseartist[main]: ['CHUU'] -> ['Chuu']

Write changes to 5 tracks?  [Y/n] y

[01:10:58] INFO: Writing tag changes for rule matcher=trackartist,releaseartist:CHUU action=matched:CHUU/replace:Chuu
[01:10:58] INFO: Wrote tag changes to CHUU - 2023. Howl/01. Howl.opus
[01:10:58] INFO: Wrote tag changes to CHUU - 2023. Howl/02. Underwater.opus
[01:10:58] INFO: Wrote tag changes to CHUU - 2023. Howl/03. My Palace.opus
[01:10:58] INFO: Wrote tag changes to CHUU - 2023. Howl/04. Aliens.opus
[01:10:58] INFO: Wrote tag changes to CHUU - 2023. Howl/05. Hitchhiker.opus

Applied tag changes to 5 tracks!
```

And we now have only one Chuu in our library!

Let's go through one more example. I want all of Chuu's releases to have the K-Pop genre. The
following rule expresses that: for all releases with the releaseartist `Chuu`, add the `K-Pop` genre
tag.

```bash
$ rose rules run 'releaseartist:^Chuu$' 'genre/add:K-Pop'

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

[01:14:57] INFO: Writing tag changes for rule matcher=artist:Chuu action=genre/replace-all:K-Pop
[01:14:57] INFO: Wrote tag changes to CHUU - 2023. Howl/01. Howl.opus
[01:14:57] INFO: Wrote tag changes to CHUU - 2023. Howl/02. Underwater.opus
[01:14:57] INFO: Wrote tag changes to CHUU - 2023. Howl/03. My Palace.opus
[01:14:57] INFO: Wrote tag changes to CHUU - 2023. Howl/04. Aliens.opus
[01:14:57] INFO: Wrote tag changes to CHUU - 2023. Howl/05. Hitchhiker.opus
[01:14:57] INFO: Wrote tag changes to LOOΠΔ - 2017. Chuu/01. Heart Attack.opus
[01:14:57] INFO: Wrote tag changes to LOOΠΔ - 2017. Chuu/02. Girl's Talk.opus

Applied tag changes to 7 tracks!
```

Success! However, notice that one of Chuu's releases has the genre tag `Kpop`. Let's convert that
`Kpop` tag to `K-Pop`, across the board.

```bash
$ rose rules run 'genre:^Kpop$' 'replace:K-Pop'

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

[14:47:26] INFO: Writing tag changes for rule matcher=genre:Kpop action=matched:Kpop/replace:K-Pop
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

And we also normalized a G-Dragon release on the way!

These rules were quite useful, so I'd like to store them, so that I can run them again in the future
when new music is added to the library. To do so, I add the following text to my configuration file:

```toml
[[stored_metadata_rules]]
matcher = "artist:^CHUU$"
actions = ["replace:Chuu"]
[[stored_metadata_rules]]
matcher = "releaseartist:^Chuu$"
actions = ["genre/add:K-Pop"]
[[stored_metadata_rules]]
matcher = "genre:^Kpop$"
actions = ["replace:K-Pop"]
```

The `rose rules run-stored` command will run the above three rules, along with any other rules I
have in my configuration file, on the entire library.

## Mechanics

Now that we've seen a bit of what the rules engine is capable of, let's explore its mechanics.

The rules engine runs in two steps: it first _matches_ tracks, and then _actions_ on those tracks.
Each rule has one single track matcher and one or more actions. If multiple actions are specified,
they are run in order of specification.

### Tags

The rules engine supports matching and acting on the following tags:

- `tracktitle`
- `trackartist[main]`
- `trackartist[guest]`
- `trackartist[remixer]`
- `trackartist[producer]`
- `trackartist[composer]`
- `trackartist[conductor]`
- `trackartist[djmixer]`
- `tracknumber`
- `tracktotal` (match only, actions not supported)
- `discnumber`
- `disctotal` (match only, actions not supported)
- `releasetitle`
- `releaseartist[main]`
- `releaseartist[guest]`
- `releaseartist[remixer]`
- `releaseartist[producer]`
- `releaseartist[composer]`
- `releaseartist[conductor]`
- `releaseartist[djmixer]`
- `releasetype`
- `releaseyear`
- `originalyear`
- `compositionyear`
- `genre`
- `parentgenre`
- `secondarygenre`
- `parentsecondarygenre`
- `descriptor`
- `label`
- `catalognumber`
- `edition`

The `trackartist[*]`, `releaseartist[*]`, `genre` (& parents), `secondarygenre` (& parents),
`descriptor`, and `label` tags are _multi-value_ tags, which have a slightly different behavior from
single-value tags for some of the actions. We'll explore this difference in the [Actions](#actions)
section.

For convenience, the rules parser also allows you to specify _tag aliases_ in
place of the above tags, which expand to multiple tags when matching. The
supported aliases are:

- `trackartist`: Expands to all the `trackartist[*]` tags.
- `releaseartist`: Expands to all the `releaseartist[*]` tags.
- `artist`: Expands to all `trackartist[*]` and `releaseartist[*]` tags.

The specific `artist[role]`-style tags are only needed when you want to match on a specific role.
The aliases provide a more convenient shorthand for most typical queries.

### Track Matchers

Track matchers are a tuple of `(tags, pattern, flags)`.

The tags are a list of [supported tags](#tags). The pattern is a string. Tracks _match_ the track
matcher if the `pattern` is a substring of one or more of the values in the given `tags`. Pattern
matching is case sensitive.

The pattern supports strict prefix and suffix matching with the `^` and `$` characters,
respectively. If the pattern starts with `^`, then the tag value must start with the pattern in
order to match. If the pattern ends with `$`, then the tag value must end with the pattern in order
to match.

Use both `^` and `$` for a string equality match. Favor using strict equality patterns if possible,
as they are less likely to match unrelated tags. For example, the pattern `^Chuu$` matches only the
value `Chuu`,

If your pattern actually starts with `^` or ends with `$`, you can escape them with backslashes. For
example, the pattern `\^.\$` matches the value `=^.$=`.

The flags allow you to configure the matching logic. The only available flag is `i`, which enables
case-insensitive matching.

### Actions

Actions are a tuple of `(tags, pattern, flags, kind, *kind_specific_args)`.

`tags`, `pattern`, and `flags` together consist the tag matcher. The tag matcher determines which
tags and values to modify on the matched track and has essentially the same semantics as the track
matcher. The tag matcher may differ from the `tags` and `pattern` of the track matcher. For example,
you can match tracks on `label:^SUNSEASKY$`, and modify their `genre` tags.

By default, `tags` is set to `matched`, which means "action on the tags of the track matcher." And
by default, `pattern` and `flags` are set to the `pattern` and `flags` of the track matcher, which
restricts the modified tags to those matched by the track matcher. However, `pattern` does not
default to the track matcher's pattern if `tags != matched`. In those cases, `pattern` defaults to
null, which matches all values.

`kind` determines which action is taken on the pattern-matched tags. There are five kinds of
actions, each of which has _kind-specific args_:

- `replace`: Replace the tag value. Has one argument: `replacement`. For
- `sed`: Executes a regex substitution (via Python's `re.sub`) on the tag
  value. Has two arguments: `pattern` and `replacement`.
- `split`: Splits a tag value into multiple values. Has one argument:
  `delimiter`, which is used to split the value. This action is only applicable to multi-value tags.
- `add`: Adds a value to the tag. Has one argument: `value`. This action is only applicable to
  multi-value tags.
- `delete`: Deletes the matched tag value. Takes no arguments.

### Multi-Value Tags

Single-valued tags are pretty straightforward: if the tag value matches, either replace the value,
sed the value, or delete the value.

Multi-valued tags are more complicated. In multi-value tags, only values matching the pattern are
acted upon. So, for example, given the genre tags `[K-Pop, Dance-Pop, Contemporary R&B]` and the
action `genre:Pop/sed:p:b`, the result will be `[K-Pob, Dance-Pob, Contemporary R&B]`
(`Contemporary R&B` is left untouched).

In order to act on all the values, the pattern can be set to null. Then the action will run over all
values in the tag. This can be used to, for example, fully replace a tag. For example, given the
above three genre tags, the action `genre:/replace:Hi;High` will result in `[Hi, High]`.

There are three additional mechanics affecting multi-value tags:

- If the new value in a multi-valued tag contains a `;`, the value will be split into multiple
  values, with `;` as the delimiter. This allows you to create multiple values from a single value
  (e.g. `[Hi, High]`!).
- If the new value is an empty string, it is removed from the result. This can be used, for example,
  in the `sed` action to remove values based on a regex pattern.
- The values in the tag are deduplicated. If this were not the case, we would have gotten
  `[Hi, High, Hi, High, Hi, High]` in the previous example. Instead, we got `[Hi, High]`.

## Rule Language

Rosé provides a Domain Specific Language (DSL) for defining rules. The DSL is the only way to
specify a rule.

Matchers are specified as `tags:pattern`. `tags` is a comma-delimited array of tags, and `pattern`
is a string. For example:

- `tracktitle:Hello`
- `tracktitle,releasetitle:^Hello`
- `tracktitle:Hello:i`

Actions are specified as `tags:pattern/kind:{kind_args}`. `tags` and `pattern` are optional, as
they default to the matcher's `tags` and `pattern`. `kind` is one of the five supported action
kinds. And `kind_args` are colon-delimited arguments for the specific kind of action. For example:

- `replace:Hi`
- `sed:.*:hi`
- `split: / `
- `add:Loony`
- `delete`
- `genre/replace:K-Pop;Dance-Pop` _(pattern is optional)_
- `matched:new-pattern/replace:Hi` _(but tags must be specified if pattern is specified)_
- `matched:new-pattern:i/replace:Hi`
- `label:/delete` _(null pattern)_

Any colon and slash characters that are not delimiters must be escaped. Colons and slashes can be
escaped by doubling them. For example: `sed:::://` replaces the `:` character with `/`.

> [!NOTE]
> When writing sed rules in the Shell and in TOML, carefully escape your backslashes. You may need
> to double-escape them, once for the shell/TOML, and another time for Rosé's sed.

The formal syntax is defined by the following grammar:

```
<trackmatcher> ::= <tags> ':' <pattern> | <tags> ':' <pattern> ':' <flags>
<tags>         ::= string | string ',' <tags>
<pattern>      ::= string | '^' string | string '$' | '^' string '$'
<flags>        ::= 'i' | ''

<action>            ::= <action-tagmatcher> '::' <subaction> | <subaction>
<action-tagmatcher> ::= <tags> | <tags> ':' <pattern> | <tags> ':' <pattern> ':' <flags>
<subaction>         ::= <replace-action> | <sed-action> | <split-action> | <add-action> | <delete-action>
<replace-action>    ::= 'replace' ':' string
<sed-action>        ::= 'sed' ':' string ':' string
<split-action>      ::= 'split' ':' string
<add-action>        ::= 'add' ':' string
<delete-action>     ::= 'delete'
```

## Ignoring Tracks

Tracks can be excluded from a specific rule by specifying an ignore matcher. Tracks which match an
ignore matcher are excluded from the rule even if they match the track matcher.

On the command line, ignore matchers may be specified via the `--ignore/-i` option:

```bash
$ rose rules run 'artist: & ' 'split: & ' --ignore 'artist:^Eli & Fur$'
# Multiple matchers may be specified by passing multiple --ignore/-i arguments.
$ rose rules run 'artist: & ' 'split: & ' --ignore 'artist:^Eli & Fur$' --ignore 'artist:^Above & Beyond$'
```

And in stored rules, ignore matchers may be specified with the `ignore` key:

```toml
[[stored_metadata_rules]]
matcher = "artist: & "
actions = ["split: & "]
ignore = ["artist:^Eli & Fur$"]
```

## Examples

_TODO_

## Dry Runs

You can preview a rule's changes with the `--dry-run` flag. For example:

```bash
$ rose rules run --dry-run 'releaseartist:^Chuu$' 'genre/add:K-Pop'

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

This is a dry run, aborting. 7 tracks would have been modified.
```
