# 009 — Final Review of 008-FINAL-PLAN.md

Fourth and final review pass. The document is a well-consolidated, self-contained
reference. All previous feedback has been incorporated. This pass checks for any remaining
gaps and assesses whether an agent could execute from this document alone.

---

## Verdict

**Ready to execute.** The plan is comprehensive, the constraints are explicit, the known
bugs and quirks are catalogued, and the "easy to miss" section covers the non-obvious
behaviors. An agent reading only this document would have sufficient context to begin work
on any milestone.

There are a few remaining items — none are plan-breaking, but addressing them would
prevent an agent from having to re-discover things by reading the Python source.

---

## Issues Found

### 1. Artist String Parse/Format Is a Complex Bidirectional Protocol — Not Mentioned

`audiotags.py:553-629` — The `parse_artist_string` / `format_artist_string` functions
implement a **bidirectional encoding** for artist roles in a single tag string. This is one
of the most subtle parts of the audio tag contract.

**Parsing** (read path): The main artist string is destructured by keywords:
- `"feat. "` -> splits into main + guest
- `"remixed by "` -> splits into main + remixer
- `"produced by "` -> splits into main + producer
- `"pres. "` -> splits into DJ + main (note: reversed order)
- `"performed by "` -> splits into composer + main (reversed)
- `"under. "` -> splits into main + conductor

Order matters: `produced by` is checked before `remixed by` before `feat.`. Individual
artists within each role are split by `TAG_SPLITTER_REGEX`: ` \\ `, ` / `, `; ` (with
optional trailing space), or ` vs. `.

**Formatting** (write path): Reverses the parse. Composes the string in a specific order:
composer `performed by` DJ `pres.` main `under.` conductor `feat.` guest `remixed by`
remixer `produced by` producer. Only non-alias artists are included.

This is a lossless roundtrip protocol. If the Rust port gets the parse or format wrong,
artist roles will be corrupted on every tag write. The golden file tests (M-2.0) will
catch field-level mismatches, but the parse/format functions need their own dedicated unit
tests covering:
- All 6 keyword delimiters
- Multiple artists per role
- Combined strings (main + guest + producer)
- Roundtrip: `format(parse(s)) == s`

**Fix:** Add a note to M-2.1: "Port `parse_artist_string` and `format_artist_string` — a
bidirectional encoding using keyword delimiters (`feat.`, `remixed by`, `produced by`,
`pres.`, `performed by`, `under.`). Must roundtrip losslessly. Test all 6 delimiters and
combinations."

### 2. ID3 Tag Reading Uses Fallback Keys

The read path for MP3 tries multiple tag keys for some fields:
- Release date: `TDRC`, then `TYER`, then `TDAT` (`audiotags.py:179`)
- Original date: `TDOR`, then `TORY` (`audiotags.py:180`)
- Release type: `TXXX:RELEASETYPE`, then `TXXX:MusicBrainz Album Type` (`audiotags.py:194`)

Similarly for MP4:
- Original date: `----:net.sunsetglow.rose:ORIGINALDATE`, then
  `----:com.apple.iTunes:ORIGINALDATE`, then `----:com.apple.iTunes:ORIGINALYEAR`
  (`audiotags.py:226-229`)
- Release type: `----:com.apple.iTunes:RELEASETYPE`, then
  `----:com.apple.iTunes:MusicBrainz Album Type` (`audiotags.py:250-251`)

And for Vorbis/FLAC:
- Date: `date`, then `year` (`audiotags.py:273`)
- Original date: `originaldate`, then `originalyear` (`audiotags.py:274`)
- Label: `label`, then `organization`, then `recordlabel` (`audiotags.py:284`)

But the **write** path only writes to the primary key. So files imported with
MusicBrainz-style tags (`TXXX:MusicBrainz Album Type`) will be read correctly, then on
the next write, the value is written to `TXXX:RELEASETYPE` instead. The MusicBrainz tag
is not cleaned up.

The golden file tests (M-2.0) will use `testdata/Tagger/` files which presumably have
Rose-native tags. They won't cover the fallback key paths. Add test cases that exercise
fallback keys.

**Fix:** Add a note to M-2.1: "Some fields read from fallback keys (e.g., `TYER`/`TDAT`
for date in old ID3v2.3 files, `MusicBrainz Album Type` for release type). The write path
only uses the primary key. Test fallback key reads."

### 3. ID3 `TRCK`/`TPOS` Use `num/total` Format

MP3 stores track and disc numbers in a combined `tracknumber/tracktotal` format in
`TRCK`/`TPOS` tags (`audiotags.py:149-162`). The read path splits on `/` to extract both.
The write path writes `tracknumber` only (no total) at `audiotags.py:352-353`, meaning
**tracktotal and disctotal are lost on MP3 tag write**. This is an existing behavior, not a
bug — but the Rust port must match it.

**This is already implicitly covered by the roundtrip tests in M-2.2**, but worth noting
for the implementing agent.

### 4. ID3 Paired Text Frames (TIPL/IPLS) for Producer/DJ Roles

`audiotags.py:164-173` — For MP3, the `producer` and `DJ-mix` roles are read from ID3
**paired text frames** (`TIPL`/`IPLS`), not standard text frames. These are key-value
pairs like `[("producer", "Name"), ("DJ-mix", "Name")]`. On write, these frames are
**deleted** entirely (`audiotags.py:370-371`), and the producer/DJ info is folded into the
main artist string.

`lofty` may handle paired text frames differently from `mutagen`. This needs explicit
testing.

**Fix:** Add to M-2.1: "MP3 `producer` and `DJ-mix` roles are read from TIPL/IPLS paired
text frames — verify `lofty` support. On write, these frames are deleted."

### 5. `TAG_SPLITTER_REGEX` Is Defined Twice

`audiotags.py:32` defines `TAG_SPLITTER_REGEX = re.compile(r"\\\\| / |; ?| vs\. ")`
and `audiotags.py:550` redefines it as
`TAG_SPLITTER_REGEX = re.compile(r" \\\\ | / |; ?| vs\. ")`. These are **different
patterns**: the first matches `\\` (literal backslash-backslash), the second matches
` \\ ` (space-backslash-backslash-space). The second definition shadows the first. The
first is used by `_split_tag` and `_split_genre_tag` (line 32 import), while the second
is used by `parse_artist_string`.

This means genre/descriptor/label tags are split on `\\` (no spaces), while artist tags
are split on ` \\ ` (with spaces). This is presumably intentional but subtle. The Rust
port must reproduce both patterns.

**Fix:** Add to M-2.1: "Two different tag splitter regexes: genre/descriptor/label use
`\\\\| / |; ?| vs\\.` (no spaces around `\\\\`); artist parsing uses
` \\\\\\\\ | / |; ?| vs\\.` (with spaces around `\\\\`). Both must be ported."

### 6. `_split_genre_tag` Strips `\\PARENTS:\\` Before Splitting

`audiotags.py:480-486`:
```python
def _split_genre_tag(t: str | None) -> list[str]:
    if not t:
        return []
    t = t.split("\\\\PARENTS:\\\\")[0]
    return TAG_SPLITTER_REGEX.split(t)
```

This confirms the read path strips the parent genre section. The plan mentions
`\\PARENTS:\\` in constraint #1 and in M-2.1/M-2.2, which is correct. Just confirming.

### 7. Phases 12-15 Are Terse

Phases 12-15 (CLI, Watcher, PyO3, Finalization) are significantly less detailed than
Phases 1-11. M-12.3 through M-12.11 are one-liners. This is acceptable because:
- The CLI is a thin layer over `rose-core` — the complexity is in the core.
- The implementing agent can read `cli.py` directly when reaching Phase 12.
- The PyO3 shim is boilerplate.

But M-12.10 (fs commands with daemonization) and M-13.1 (watcher) are the exceptions —
they have real complexity and are appropriately detailed.

### 8. No Mention of `rating` Tag in Audio Tags

The `AudioTags` struct in the plan doesn't include `rating` in the field list for M-2.1.
Checking the Python: `AudioTags` has no `rating` field — ratings live exclusively in
`.rose.{uuid}.toml`, not in audio tags. However, the **FTS index** includes a `rating`
column (`cache.sql:192`), which gets its value from the `releases` table (populated from
the datafile, not from tags). This is consistent. No issue here.

---

## Things the Plan Gets Right

For the record, these are areas previous reviews flagged that are now correctly handled:

- Hard constraint #1 explicitly includes `\\PARENTS:\\` genre encoding
- Hard constraint #2 covers `.rose.{uuid}.toml` format
- Cross-cutting decisions cover: logging, path normalization, error recovery, SQLite
  PRAGMAs, VFS thread safety, rayon concurrency, regex flavor
- Known bugs section (4 bugs to fix)
- Known quirks section (4 things to NOT reproduce)
- "Key behaviors easy to miss" section (12 items)
- M-7.1e notes cache updates write to source TOML files
- M-8.1 lists the 5-phase pipeline and datafile mutations
- M-9.1 fully describes the `edit_release` protocol
- M-11.5 has the 6-step ghost protocol

---

## Summary

| # | Severity | Item |
|---|----------|------|
| 1 | High | Document `parse_artist_string`/`format_artist_string` bidirectional protocol in M-2.1 |
| 2 | Medium | Note ID3/MP4/Vorbis fallback tag keys for read in M-2.1 |
| 3 | Low | Note ID3 `TRCK`/`TPOS` num/total format and total loss on write |
| 4 | Medium | Note TIPL/IPLS paired text frame handling for producer/DJ in M-2.1 |
| 5 | Medium | Document the two different `TAG_SPLITTER_REGEX` patterns (with/without spaces) |
| 6 | Low | Already covered — confirming `_split_genre_tag` strips `\\PARENTS:\\` |
| 7 | Low | Phases 12-15 terse but acceptable |
| 8 | Low | No issue — ratings are datafile-only, not audio tags |

**Items 1, 2, 4, and 5 should be added to M-2.1.** They all relate to the audio tag
read/write contract, which is the single most data-corruption-prone area of the migration.
Everything else is minor or already covered.

The plan is ready.
