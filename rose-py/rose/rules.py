"""
The rules module implements the Rules Engine, which provides performant substring tag querying and
bulk metadata updating.

The first part of this file implements the Rule Engine pipeline, which:

1. Fetches a superset of possible tracks from the Read Cache.
2. Filters out false positives via tags.
3. Executes actions to update metadata.

Release tags are matched and acted on once per release; track tags once per track.

The second part of this file provides performant release/track querying entirely from the read
cache, which is used by other modules to provide release/track filtering capabilities.
"""

import copy
import dataclasses
import logging
import re
import shlex
import time
import tomllib
from pathlib import Path

import click
import tomli_w

from rose.audiotags import AudioTags, RoseDate
from rose.cache import (
    STORED_DATA_FILE_REGEX,
    Release,
    StoredDataFile,
    Track,
    connect,
    list_tracks,
    update_cache_for_releases,
)
from rose.common import Artist, RoseError, RoseExpectedError, uniq
from rose.config import Config
from rose.rule_parser import (
    RELEASE_TAGS,
    SINGLE_VALUE_TAGS,
    Action,
    AddAction,
    CopyAction,
    DeleteAction,
    Matcher,
    Pattern,
    ReplaceAction,
    Rule,
    SedAction,
    SplitAction,
    Tag,
)

logger = logging.getLogger(__name__)


class TrackTagNotAllowedError(RoseExpectedError):
    pass


class InvalidReplacementValueError(RoseExpectedError):
    pass


def execute_stored_metadata_rules(
    c: Config,
    *,
    dry_run: bool = False,
    confirm_yes: bool = False,
) -> None:
    for rule in c.stored_metadata_rules:
        click.secho(f"Executing stored metadata rule {rule}", dim=True)
        execute_metadata_rule(c, rule, dry_run=dry_run, confirm_yes=confirm_yes)


def execute_metadata_rule(
    c: Config,
    rule: Rule,
    *,
    dry_run: bool = False,
    confirm_yes: bool = False,
    enter_number_to_confirm_above_count: int = 25,
) -> None:
    """
    This function executes a metadata update rule. It runs in five parts:

    1. Run a search query on our Full Text Search index. This is far more performant than the SQL
       LIKE operation; however, it is also less precise. It produces false positives, but should not
       produce false negatives. So we then run:
    2. Read the files returned from the search query and remove all false positives.
    3. We then run the actions on each valid matched file and store all the intended changes
       in-memory. No changes are written to disk.
    4. We then prompt the user to confirm the changes, assuming confirm_yes is True.
    5. We then flush the intended changes to disk.

    Tags are either release tags or track tags. Release tags are matched and acted on once per
    release, and a change to a release tag is written to every track of the release. Track tags are
    matched and acted on once per track. See `RuleRelease`.
    """
    # Newline for appearance.
    click.echo()
    fast_search_results = fast_search_for_matching_tracks(c, rule.matcher)
    if not fast_search_results:
        click.secho("No matching tracks found", dim=True, italic=True)
        click.echo()
        return
    # If there are more than 400 tracks matched, first filter the matched tracks using the cache,
    # has a sublinear time complexity (but higher baseline). Only then run the tag filter, which has
    # linear time complexity.
    if len(fast_search_results) > 400:
        time_start = time.time()
        tracks = list_tracks(c, [t.id for t in fast_search_results])
        logger.debug(f"Fetched tracks from cache for filtering in {time.time() - time_start} seconds")
        tracks = filter_track_false_positives_using_read_cache(rule.matcher, tracks)
        track_ids = {x.id for x in tracks}
        fast_search_results = [t for t in fast_search_results if t.id in track_ids]
    if not fast_search_results:
        click.secho("No matching tracks found", dim=True, italic=True)
        click.echo()
        return

    matcher_audiotags = filter_track_false_positives_using_tags(
        c,
        rule.matcher,
        fast_search_results,
        rule.ignore,
    )
    if not matcher_audiotags:
        click.secho("No matching tracks found", dim=True, italic=True)
        click.echo()
        return

    execute_metadata_actions(
        c,
        rule.actions,
        matcher_audiotags,
        dry_run=dry_run,
        confirm_yes=confirm_yes,
        enter_number_to_confirm_above_count=enter_number_to_confirm_above_count,
    )


TAG_ROLE_REGEX = re.compile(r"\[[^\]]+\]$")


@dataclasses.dataclass(slots=True)
class FastSearchResult:
    id: str
    path: Path


def fast_search_for_matching_tracks(
    c: Config,
    matcher: Matcher,
) -> list[FastSearchResult]:
    """
    Run a search for tracks with the matcher on the Full Text Search index. This is _fast_, but will
    produce false positives. The caller must filter out the false positives after pulling the
    results.
    """
    time_start = time.time()
    matchsql = _convert_matcher_to_fts_query(matcher.pattern)
    logger.debug(f"Converted match {matcher=} to {matchsql=}")

    # Build the query to fetch a superset of tracks to attempt to execute the rules against. Note
    # that we directly use string interpolation here instead of prepared queries, because we are
    # constructing a complex match string and everything is escaped and spaced-out with a random
    # paragraph character, so there's no risk of SQL being interpreted.
    #
    # Remove the "artist role" from the tag, as we do not track the role information in the FTS
    # table. The false positives should be minimal enough that performance should be roughly the
    # same if we filter them out in the tag checking step.
    columns = uniq([TAG_ROLE_REGEX.sub("", t) for t in matcher.tags])
    ftsquery = f"{{{' '.join(columns)}}} : {matchsql}"
    query = f"""
        SELECT DISTINCT t.id, t.source_path
        FROM rules_engine_fts
        JOIN tracks t ON rules_engine_fts.rowid = t.rowid
        WHERE rules_engine_fts MATCH '{ftsquery}'
        ORDER BY t.source_path
    """
    logger.debug(f"Constructed matching query {query}")
    # And then execute the SQL query. Note that we don't pull the tag values here. This query is
    # only used to identify the matching tracks. Afterwards, we will read each track's tags from
    # disk and apply the action on those tag values.
    results: list[FastSearchResult] = []
    with connect(c) as conn:
        for row in conn.execute(query):
            results.append(FastSearchResult(id=row["id"], path=Path(row["source_path"]).resolve()))
    logger.debug(f"Matched {len(results)} tracks from the read cache in {time.time() - time_start} seconds")
    return results


def _convert_matcher_to_fts_query(pattern: Pattern) -> str:
    # Convert the matcher to a SQL expression for SQLite FTS. We won't be doing the precise
    # prefix/suffix matching here: for performance, we abuse SQLite FTS by making every character
    # its own token, which grants us the ability to search for arbitrary substrings. However, FTS
    # cannot guarantee ordering, which means that a search for `BLACKPINK` will also match
    # `PINKBLACK`. So we first pull all matching results, and then we use the previously written
    # precise Python matcher to ignore the false positives and only modify the tags we care about.
    # Construct the SQL string for the matcher. Escape quotes in the match string.
    matchsql = "¬".join(pattern.needle).replace("'", "''").replace('"', '""')
    # NEAR restricts the query such that the # of tokens in between the first and last tokens of the
    # matched substring must be less than or equal to a given number. For us, that number is
    # len(matchsqlstr) - 2, as we subtract the first and last characters.
    return f'NEAR("{matchsql}", {max(0, len(pattern.needle) - 2)})'


TagValue = str | int | bool | RoseDate | None
# The value of a multi-value tag is a list of strings.
AnyTagValue = TagValue | list[str]
Changes = tuple[str, AnyTagValue, AnyTagValue]

# The release tags that are stored in the release's datafile instead of in the audio files.
DATAFILE_TAGS: list[Tag] = ["new", "favorite", "rating"]
DATE_TAGS: list[Tag] = ["releasedate", "originaldate", "compositiondate"]
ARTIST_TAG_REGEX = re.compile(r"^(trackartist|releaseartist)\[(\w+)\]$")


def _get_audiotags_value(tags: AudioTags, tag: Tag) -> AnyTagValue:
    if m := ARTIST_TAG_REGEX.match(tag):
        # NOTE: Nothing should be an alias here because we get data from tags.
        return [a.name for a in getattr(getattr(tags, f"{m[1]}s"), m[2])]
    value: AnyTagValue = getattr(tags, tag)
    return list(value) if isinstance(value, list) else value


def _set_audiotags_value(tags: AudioTags, tag: Tag, value: AnyTagValue) -> None:
    if m := ARTIST_TAG_REGEX.match(tag):
        assert isinstance(value, list)
        setattr(getattr(tags, f"{m[1]}s"), m[2], [Artist(x) for x in value])
        return
    setattr(tags, tag, list(value) if isinstance(value, list) else value)


@dataclasses.dataclass(slots=True)
class RuleRelease:
    """
    A release, as the rules engine operates on it.

    The rules engine matches and acts on tracks, but the tags in `RELEASE_TAGS` belong to the
    release rather than to any one track. They are stored in the release's datafile or redundantly
    in every audio file of the release. This class makes them a single unit: a release tag is read
    once per release (from the release's first track by path, which is the track that the read
    cache treats as the source of truth) and written to every track of the release.

    The audio files and datafile are only read on first access.
    """

    id: str
    source_path: Path
    # The tracks of the release, as a map of track ID to path, ordered by path.
    track_paths: dict[str, Path]
    # The number of discs is computed by the read cache from the tracks of the release. The
    # `disctotal` embedded in the audio files is not a source of truth.
    disctotal: int
    _tags: AudioTags | None = None
    _datafile: tuple[Path, StoredDataFile] | None = None

    @property
    def tags(self) -> AudioTags:
        """The audio tags that hold the release tags. Only the release tags on it are meaningful."""
        if self._tags is None:
            self._tags = AudioTags.from_file(next(iter(self.track_paths.values())))
        return self._tags

    @property
    def datafile(self) -> StoredDataFile:
        if self._datafile is None:
            for f in self.source_path.iterdir():
                if not STORED_DATA_FILE_REGEX.match(f.name):
                    continue
                with f.open("rb") as fp:
                    self._datafile = (f, StoredDataFile.parse(tomllib.load(fp)))
                break
            else:
                raise RoseError(f"Release data file not found in {self.source_path}. How is it in the library?")
        return self._datafile[1]

    def write_datafile(self) -> None:
        assert self._datafile is not None
        path, datafile = self._datafile
        with path.open("wb") as fp:
            tomli_w.dump(datafile.serialize(), fp)

    def get(self, tag: Tag) -> AnyTagValue:
        assert tag in RELEASE_TAGS
        if tag in DATAFILE_TAGS:
            value: AnyTagValue = getattr(self.datafile, tag)
            return value
        if tag == "disctotal":
            return self.disctotal
        return _get_audiotags_value(self.tags, tag)

    def set(self, tag: Tag, value: AnyTagValue) -> None:
        assert tag in RELEASE_TAGS
        if tag in DATAFILE_TAGS:
            setattr(self.datafile, tag, value)
        else:
            _set_audiotags_value(self.tags, tag, value)


def _fetch_releases_of_tracks(c: Config, track_ids: list[str]) -> dict[str, RuleRelease]:
    """Fetch the releases of the given tracks from the read cache, as a map of track ID to release."""
    rows_by_release: dict[str, list[tuple[Path, str]]] = {}
    release_paths: dict[str, Path] = {}
    release_disctotals: dict[str, int] = {}
    with connect(c) as conn:
        cursor = conn.execute(
            f"""
            SELECT
                t.id AS track_id
              , t.source_path AS track_path
              , r.id AS release_id
              , r.source_path AS release_path
              , r.disctotal
            FROM tracks t
            JOIN releases r ON r.id = t.release_id
            WHERE t.release_id IN (
                SELECT release_id FROM tracks WHERE id IN ({",".join(["?"] * len(track_ids))})
            )
            """,
            track_ids,
        )
        for row in cursor:
            release_paths[row["release_id"]] = Path(row["release_path"])
            release_disctotals[row["release_id"]] = row["disctotal"]
            rows_by_release.setdefault(row["release_id"], []).append((Path(row["track_path"]), row["track_id"]))

    rval: dict[str, RuleRelease] = {}
    requested = set(track_ids)
    for release_id, rows in rows_by_release.items():
        # Sort as paths, like the cache update does, so that we agree with it on the first track.
        release = RuleRelease(
            id=release_id,
            source_path=release_paths[release_id],
            track_paths={track_id: path for path, track_id in sorted(rows)},
            disctotal=release_disctotals[release_id],
        )
        for track_id in release.track_paths:
            if track_id in requested:
                rval[track_id] = release
    return rval


def filter_track_false_positives_using_tags(
    c: Config,
    matcher: Matcher,
    fast_search_results: list[FastSearchResult],
    ignore: list[Matcher],
) -> list[AudioTags]:
    """
    Read the tags of the tracks returned by the fast search and return the tracks that match the
    matcher and do not match any of the ignore matchers.

    Release tags are tested once per release: a release that matches on a release tag matches with
    all of its tracks. Track tags are tested on each track.
    """
    time_start = time.time()
    releases = _fetch_releases_of_tracks(c, [x.id for x in fast_search_results])
    matcher_has_track_tags = any(t not in RELEASE_TAGS for t in matcher.tags)
    # Map of release ID to whether the release matches the matcher and the ignore matchers.
    release_matches: dict[str, tuple[bool, bool]] = {}

    rval = []
    for fsr in fast_search_results:
        release = releases[fsr.id]
        try:
            matched, ignored = release_matches[release.id]
        except KeyError:
            matched = _release_matches(matcher, release)
            ignored = any(_release_matches(i, release) for i in ignore)
            release_matches[release.id] = matched, ignored
        # Avoid reading the track's tags if the release tags have already decided the result.
        if ignored or (not matched and not matcher_has_track_tags):
            continue
        tags = AudioTags.from_file(fsr.path)
        if not matched and not _track_matches(matcher, tags):
            continue
        if any(_track_matches(i, tags) for i in ignore):
            continue
        rval.append(tags)
    logger.debug(
        f"Filtered {len(fast_search_results)} tracks down to {len(rval)} tracks in {time.time() - time_start} seconds"
    )
    return rval


def _release_matches(matcher: Matcher, release: RuleRelease) -> bool:
    """Test the release tags of the matcher."""
    return any(_value_matches(matcher.pattern, release.get(t)) for t in matcher.tags if t in RELEASE_TAGS)


def _track_matches(matcher: Matcher, tags: AudioTags) -> bool:
    """Test the track tags of the matcher."""
    return any(
        _value_matches(matcher.pattern, _get_audiotags_value(tags, t)) for t in matcher.tags if t not in RELEASE_TAGS
    )


def _value_matches(pattern: Pattern, value: AnyTagValue) -> bool:
    if isinstance(value, list):
        return any(matches_pattern(pattern, x) for x in value)
    return matches_pattern(pattern, value)


@dataclasses.dataclass(slots=True)
class PlannedReleaseChanges:
    release: RuleRelease
    # Changes to the release tags, including the ones in the datafile.
    release_changes: list[Changes]
    # Changes to the track tags of each track.
    track_changes: list[tuple[AudioTags, list[Changes]]]
    # The tracks to flush to disk: the tracks whose track tags changed, and, if the release tags
    # changed, every track of the release that does not already have the new release tags.
    tracks_to_write: list[AudioTags]
    write_datafile: bool


def execute_metadata_actions(
    c: Config,
    actions: list[Action],
    audiotags: list[AudioTags],
    *,
    dry_run: bool = False,
    confirm_yes: bool = False,
    enter_number_to_confirm_above_count: int = 25,
) -> None:
    """
    This function executes steps 3-5 of the rule executor. See that function's docstring. This is
    split out to enable running actions on known releases/tracks.

    The audiotags are the matched tracks. Actions on track tags apply to each of them. Actions on
    release tags apply once to each release that has a matched track, and are written to every track
    of that release, matched or not.
    """
    # === Step 3: Prepare updates on in-memory tags ===

    track_ids: list[str] = []
    for tags in audiotags:
        if tags.id is None:
            raise RoseError(f"Cannot run actions on track {tags.path}: it is not in the read cache")
        track_ids.append(tags.id)
    releases = _fetch_releases_of_tracks(c, track_ids)

    # Group the matched tracks by their release.
    matched_tracks: dict[str, tuple[RuleRelease, dict[str, AudioTags]]] = {}
    for track_id, tags in zip(track_ids, audiotags, strict=True):
        try:
            release = releases[track_id]
        except KeyError as e:
            raise RoseError(f"Cannot run actions on track {tags.path}: it is not in the read cache") from e
        matched_tracks.setdefault(release.id, (release, {}))[1][track_id] = tags

    planned: list[PlannedReleaseChanges] = []
    for release, tracks in sorted(matched_tracks.values(), key=lambda x: x[0].source_path):
        # Order the matched tracks in the release's track order.
        tracks = {track_id: tracks[track_id] for track_id in release.track_paths if track_id in tracks}
        plan = _plan_release_changes(release, tracks, actions)
        if plan.tracks_to_write or plan.write_datafile:
            planned.append(plan)
        else:
            logger.debug(f"Skipping matched release {release.source_path}: no changes calculated off tags and datafile")

    if not planned:
        click.secho("No matching tracks found", dim=True, italic=True)
        click.echo()
        return

    # === Step 4: Display changes and ask for user confirmation ===

    # Compute the text to display: the release changes under the release directory, followed by the
    # track changes under each track.
    def pathtext(path: Path) -> str:
        text = str(path).removeprefix(str(c.music_source_dir) + "/")
        if len(text) >= 120:
            text = text[:59] + ".." + text[-59:]
        return text

    todisplay: list[tuple[str, list[Changes]]] = []
    for plan in planned:
        if plan.release_changes:
            todisplay.append((pathtext(plan.release.source_path), plan.release_changes))
        for tags, tag_changes in plan.track_changes:
            todisplay.append((pathtext(tags.path), tag_changes))

    # And then display it.
    for text, tag_changes in todisplay:
        click.secho(text, underline=True)
        for name, old, new in tag_changes:
            click.echo(f"      {name}: ", nl=False)
            click.secho(old, fg="red", nl=False)
            click.echo(" -> ", nl=False)
            click.secho(new, fg="green", bold=True)

    num_tracks = sum(len(plan.tracks_to_write) for plan in planned)

    # If we're dry-running, then abort here.
    if dry_run:
        click.echo()
        click.secho(
            f"This is a dry run, aborting. {num_tracks} tracks would have been modified.",
            dim=True,
        )
        return

    # And then let's go for the confirmation.
    num_changes = num_tracks + sum(plan.write_datafile for plan in planned)
    if confirm_yes:
        click.echo()
        if num_changes > enter_number_to_confirm_above_count:
            while True:
                userconfirmation = click.prompt(
                    f"Write changes to {num_changes} tracks? Enter {click.style(num_changes, bold=True)} to confirm (or 'no' to abort)"
                )
                if userconfirmation == "no":
                    logger.debug("Aborting planned tag changes after user confirmation")
                    return
                if userconfirmation == str(num_changes):
                    click.echo()
                    break
        else:
            if not click.confirm(
                f"Write changes to {click.style(num_changes, bold=True)} tracks?",
                default=True,
                prompt_suffix="",
            ):
                logger.debug("Aborting planned tag changes after user confirmation")
                return
            click.echo()

    # === Step 5: Flush writes to disk ===

    logger.info(f"Writing tag changes for actions {' '.join([shlex.quote(str(a)) for a in actions])}")
    for plan in planned:
        for tags in plan.tracks_to_write:
            tags.flush(c)
            logger.info(f"Wrote tag changes to {pathtext(tags.path)}")
        if plan.write_datafile:
            plan.release.write_datafile()
            logger.info(f"Wrote datafile changes to {pathtext(plan.release.source_path)}")

    click.echo()
    click.echo(f"Applied tag changes to {num_changes} tracks!")

    # == Step 6: Trigger cache update ===

    click.echo()
    update_cache_for_releases(c, [plan.release.source_path for plan in planned])


def _plan_release_changes(
    release: RuleRelease,
    matched_tracks: dict[str, AudioTags],
    actions: list[Action],
) -> PlannedReleaseChanges:
    """
    Run the actions on a release and its matched tracks (a map of track ID to tags, in the release's
    track order). The actions are applied in-memory; nothing is written to disk.
    """
    release_tags = uniq([t for a in actions for t in a.tags if t in RELEASE_TAGS])
    track_tags = uniq([t for a in actions for t in a.tags if t not in RELEASE_TAGS])

    # If the first track is one of the matched tracks, spare ourselves the second read. Make a copy
    # so that the release tags are independent from the track.
    first_track_id = next(iter(release.track_paths))
    if first_track_id in matched_tracks:
        release._tags = copy.deepcopy(matched_tracks[first_track_id])

    original_release_values = {t: release.get(t) for t in release_tags}
    original_track_values = {
        track_id: {t: _get_audiotags_value(tags, t) for t in track_tags} for track_id, tags in matched_tracks.items()
    }

    for act in actions:
        # A copy action reads its source tag once, before any of its destination tags are written,
        # so that a destination that is also the source does not feed into the other destinations.
        # A release tag cannot be copied from several tracks at once, so when a track tag is copied
        # to a release tag, the source is read from the first matched track.
        copy_sources: dict[str, str | list[str] | None] = dict.fromkeys(matched_tracks)
        if isinstance(act.behavior, CopyAction):
            source_tag = act.behavior.source
            for track_id, tags in matched_tracks.items():
                value = (
                    release.get(source_tag) if source_tag in RELEASE_TAGS else _get_audiotags_value(tags, source_tag)
                )
                copy_sources[track_id] = value if isinstance(value, list) else value_to_str(value)

        for tag in act.tags:
            if tag in RELEASE_TAGS:
                source = next(iter(copy_sources.values()))
                release.set(tag, execute_action(act, tag, source, release.get(tag)))
                continue
            for track_id, tags in matched_tracks.items():
                value = execute_action(act, tag, copy_sources[track_id], _get_audiotags_value(tags, tag))
                _set_audiotags_value(tags, tag, value)

    # Compute real changes by diffing the tags.
    release_changes: list[Changes] = [
        (t, original, new) for t, original in original_release_values.items() if (new := release.get(t)) != original
    ]
    track_changes: list[tuple[AudioTags, list[Changes]]] = []
    for track_id, tags in matched_tracks.items():
        tag_changes: list[Changes] = [
            (t, original, new)
            for t, original in original_track_values[track_id].items()
            if (new := _get_audiotags_value(tags, t)) != original
        ]
        if tag_changes:
            track_changes.append((tags, tag_changes))

    # A changed release tag is written to every track of the release. Unchanged release tags are
    # left alone, even on the tracks where they differ from the release's value.
    changed_release_tags = [t for t, original in original_release_values.items() if release.get(t) != original]
    changed_audio_tags = [t for t in changed_release_tags if t not in DATAFILE_TAGS]
    tracks_to_write = [tags for tags, _ in track_changes]
    if changed_audio_tags:
        tracks_to_write = []
        for track_id, path in release.track_paths.items():
            tags = matched_tracks.get(track_id) or AudioTags.from_file(path)
            dirty = any(tags is x for x, _ in track_changes)
            for tag in changed_audio_tags:
                new = release.get(tag)
                if _get_audiotags_value(tags, tag) != new:
                    _set_audiotags_value(tags, tag, new)
                    dirty = True
            if dirty:
                tracks_to_write.append(tags)

    return PlannedReleaseChanges(
        release=release,
        release_changes=release_changes,
        track_changes=track_changes,
        tracks_to_write=tracks_to_write,
        write_datafile=any(t in DATAFILE_TAGS for t in changed_release_tags),
    )


def value_to_str(value: TagValue) -> str:
    if isinstance(value, bool):
        return str(value).lower()
    if value:
        return str(value)
    return ""


def matches_pattern(pattern: Pattern, value: str | int | bool | RoseDate | None) -> bool:
    value = value_to_str(value)

    needle = pattern.needle
    haystack = value
    if pattern.case_insensitive:
        needle = needle.lower()
        haystack = haystack.lower()

    if pattern.strict_start and pattern.strict_end:
        return haystack == needle
    if pattern.strict_start:
        return haystack.startswith(needle)
    if pattern.strict_end:
        return haystack.endswith(needle)
    return needle in haystack


def execute_action(
    action: Action,
    tag: Tag,
    copy_source: str | list[str] | None,
    value: AnyTagValue,
) -> AnyTagValue:
    """Execute an action on the value of a tag and return the tag's new value."""
    if tag not in SINGLE_VALUE_TAGS:
        assert isinstance(value, list)
        return execute_multi_value_action(action, copy_source, value)

    assert not isinstance(value, list)
    v = execute_single_action(action, copy_source, value)
    if tag in ("new", "favorite"):
        if v != "true" and v != "false":
            raise InvalidReplacementValueError(
                f"Failed to assign new value {v} to {tag}: value must be string `true` or `false`"
            )
        return v == "true"
    if tag == "rating":
        if v is None or v == "":
            return None
        try:
            rating = int(v)
        except ValueError as e:
            raise InvalidReplacementValueError(
                f"Failed to assign new value {v} to rating: value must be an integer 1-100 or empty to clear"
            ) from e
        if rating < 1 or rating > 100:
            raise InvalidReplacementValueError(
                f"Failed to assign new value {v} to rating: value must be between 1 and 100"
            )
        return rating
    if tag in DATE_TAGS:
        try:
            return RoseDate.parse(v)
        except ValueError as e:
            raise InvalidReplacementValueError(
                f"Failed to assign new value {v} to {tag}: value must be date string"
            ) from e
    if tag == "releasetype":
        return v or "unknown"
    return v


# Factor out the logic for executing an action on a single-value tag and a multi-value tag.
def execute_single_action(
    action: Action,
    copy_source: str | list[str] | None,
    value: str | int | bool | RoseDate | None,
) -> str | None:
    bhv = action.behavior

    # The copy action ignores the destination value entirely: it writes the (optionally
    # sed-transformed) source value. The pattern, if set, filters the source value.
    if isinstance(bhv, CopyAction):
        source = copy_source if isinstance(copy_source, str) else "; ".join(copy_source or [])
        if action.pattern and not matches_pattern(action.pattern, source):
            return value_to_str(value)
        if bhv.sed is not None:
            return bhv.sed.src.sub(bhv.sed.dst, source)
        return source

    if action.pattern and not matches_pattern(action.pattern, value):
        return value_to_str(value)

    strvalue = value_to_str(value)

    if isinstance(bhv, ReplaceAction):
        return bhv.replacement
    elif isinstance(bhv, SedAction):
        if strvalue is None:
            return None
        return bhv.src.sub(bhv.dst, strvalue)
    elif isinstance(bhv, DeleteAction):
        return None
    raise RoseError(f"Invalid action {type(bhv)} for single-value tag: Should have been caught in parsing")


def execute_multi_value_action(
    action: Action,
    copy_source: str | list[str] | None,
    values: list[str],
) -> list[str]:
    bhv = action.behavior

    # The copy action overwrites the destination list wholesale with the source value. A
    # single-value source becomes a single-element list; a multi-value source is copied as a list.
    # The pattern, if set, filters which source values are copied.
    if isinstance(bhv, CopyAction):
        source_values = [copy_source] if isinstance(copy_source, str) else list(copy_source or [])
        copied: list[str] = []
        for v in source_values:
            if action.pattern and not matches_pattern(action.pattern, v):
                continue
            newvals = [v]
            if bhv.sed is not None:
                newvals = bhv.sed.src.sub(bhv.sed.dst, v).split(";")
            for nv in newvals:
                nv = nv.strip()
                if nv:
                    copied.append(nv)
        return uniq(copied)

    # If match_pattern is specified, check which values match. And if none match, bail out.
    matching_idx = list(range(len(values)))
    if action.pattern:
        matching_idx = []
        for i, v in enumerate(values):
            if matches_pattern(action.pattern, v):
                matching_idx.append(i)
        if not matching_idx:
            return values

    if isinstance(bhv, AddAction):
        return uniq([*values, bhv.value])

    rval: list[str] = []
    for i, v in enumerate(values):
        if i not in matching_idx:
            rval.append(v)
            continue
        if isinstance(bhv, DeleteAction):
            continue
        newvals = [v]
        if isinstance(bhv, ReplaceAction):
            newvals = bhv.replacement.split(";")
        elif isinstance(bhv, SedAction):
            newvals = bhv.src.sub(bhv.dst, v).split(";")
        elif isinstance(bhv, SplitAction):
            newvals = v.split(bhv.delimiter)
        for nv in newvals:
            nv = nv.strip()
            if nv:
                rval.append(nv)
    return uniq(rval)


# The following functions are for leveraging the rules engine as a performant query engine.


def fast_search_for_matching_releases(
    c: Config,
    matcher: Matcher,
    *,
    include_loose_tracks: bool = True,
) -> list[FastSearchResult]:
    """Basically the same thing as fast_search_for_matching_tracks but with releases."""
    time_start = time.time()
    if track_tags := [t for t in matcher.tags if t not in RELEASE_TAGS]:
        # But allow an exception if both trackartist and releaseartist are defined: means a shorthand
        # was used. Just ignore trackartist.
        if any(t.startswith("releaseartist") for t in matcher.tags):
            track_tags = [t for t in track_tags if not t.startswith("trackartist")]
        else:
            raise TrackTagNotAllowedError(
                f"Track tags are not allowed when matching against releases: {', '.join(track_tags)}"
            )

    matchsql = _convert_matcher_to_fts_query(matcher.pattern)
    logger.debug(f"Converted match {matcher=} to {matchsql=}")
    columns = uniq([TAG_ROLE_REGEX.sub("", t) for t in matcher.tags])
    ftsquery = f"{{{' '.join(columns)}}} : {matchsql}"
    query = f"""
        SELECT DISTINCT r.id, r.source_path
        FROM rules_engine_fts
        JOIN tracks t ON rules_engine_fts.rowid = t.rowid
        JOIN releases r ON r.id = t.release_id
        WHERE rules_engine_fts MATCH '{ftsquery}'
    """
    if not include_loose_tracks:
        query += " AND r.releasetype <> 'loosetrack'"
    query += " ORDER BY r.source_path"
    logger.debug(f"Constructed matching query {query}")
    results: list[FastSearchResult] = []
    with connect(c) as conn:
        for row in conn.execute(query):
            results.append(FastSearchResult(id=row["id"], path=Path(row["source_path"]).resolve()))
    logger.debug(f"Matched {len(results)} releases from the read cache in {time.time() - time_start} seconds")
    return results


def filter_track_false_positives_using_read_cache(
    matcher: Matcher,
    tracks: list[Track],
) -> list[Track]:
    time_start = time.time()
    rval = []
    for t in tracks:
        for field in matcher.tags:
            match = False
            # fmt: off
            match = match or (field == "tracktitle" and matches_pattern(matcher.pattern, t.tracktitle))
            match = match or (field == "releasedate" and matches_pattern(matcher.pattern, t.release.releasedate))
            match = match or (field == "originaldate" and matches_pattern(matcher.pattern, t.release.originaldate))
            match = match or (field == "compositiondate" and matches_pattern(matcher.pattern, t.release.compositiondate))
            match = match or (field == "edition" and matches_pattern(matcher.pattern, t.release.edition))
            match = match or (field == "catalognumber" and matches_pattern(matcher.pattern, t.release.catalognumber))
            match = match or (field == "tracknumber" and matches_pattern(matcher.pattern, t.tracknumber))
            match = match or (field == "tracktotal" and matches_pattern(matcher.pattern, t.tracktotal))
            match = match or (field == "discnumber" and matches_pattern(matcher.pattern, t.discnumber))
            match = match or (field == "disctotal" and matches_pattern(matcher.pattern, t.release.disctotal))
            match = match or (field == "releasetitle" and matches_pattern(matcher.pattern, t.release.releasetitle))
            match = match or (field == "releasetype" and matches_pattern(matcher.pattern, t.release.releasetype))
            match = match or (field == "new" and matches_pattern(matcher.pattern, t.release.new))
            match = match or (field == "favorite" and matches_pattern(matcher.pattern, t.release.favorite))
            match = match or (field == "rating" and matches_pattern(matcher.pattern, str(t.release.rating) if t.release.rating is not None else ""))
            match = match or (field == "genre" and any(matches_pattern(matcher.pattern, x) for x in t.release.genres))
            match = match or (field == "secondarygenre" and any(matches_pattern(matcher.pattern, x) for x in t.release.secondary_genres))
            match = match or (field == "descriptor" and any(matches_pattern(matcher.pattern, x) for x in t.release.descriptors))
            match = match or (field == "label" and any(matches_pattern(matcher.pattern, x) for x in t.release.labels))
            match = match or (field == "trackartist[main]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.main))
            match = match or (field == "trackartist[guest]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.guest))
            match = match or (field == "trackartist[remixer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.remixer))
            match = match or (field == "trackartist[producer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.producer))
            match = match or (field == "trackartist[composer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.composer))
            match = match or (field == "trackartist[conductor]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.conductor))
            match = match or (field == "trackartist[djmixer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.trackartists.djmixer))
            match = match or (field == "releaseartist[main]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.releaseartists.main))
            match = match or (field == "releaseartist[guest]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.releaseartists.guest))
            match = match or (field == "releaseartist[remixer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.releaseartists.remixer))
            match = match or (field == "releaseartist[producer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.releaseartists.producer))
            match = match or (field == "releaseartist[composer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.releaseartists.composer))
            match = match or (field == "releaseartist[conductor]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.releaseartists.conductor))
            match = match or (field == "releaseartist[djmixer]" and any(matches_pattern(matcher.pattern, x.name) for x in t.release.releaseartists.djmixer))
            # fmt: on
            if match:
                rval.append(t)
                break
    logger.debug(f"Filtered {len(tracks)} tracks down to {len(rval)} tracks in {time.time() - time_start} seconds")
    return rval


def filter_release_false_positives_using_read_cache(
    matcher: Matcher,
    releases: list[Release],
    *,
    include_loose_tracks: bool = True,
) -> list[Release]:
    time_start = time.time()
    rval = []
    for r in releases:
        if not include_loose_tracks and r.releasetype == "loosetrack":
            continue
        for field in matcher.tags:
            match = False
            # Only attempt to match the release tags; ignore track tags.
            # fmt: off
            match = match or (field == "releasedate" and matches_pattern(matcher.pattern, r.releasedate))
            match = match or (field == "originaldate" and matches_pattern(matcher.pattern, r.originaldate))
            match = match or (field == "compositiondate" and matches_pattern(matcher.pattern, r.compositiondate))
            match = match or (field == "edition" and matches_pattern(matcher.pattern, r.edition))
            match = match or (field == "catalognumber" and matches_pattern(matcher.pattern, r.catalognumber))
            match = match or (field == "releasetitle" and matches_pattern(matcher.pattern, r.releasetitle))
            match = match or (field == "releasetype" and matches_pattern(matcher.pattern, r.releasetype))
            match = match or (field == "new" and matches_pattern(matcher.pattern, r.new))
            match = match or (field == "favorite" and matches_pattern(matcher.pattern, r.favorite))
            match = match or (field == "rating" and matches_pattern(matcher.pattern, str(r.rating) if r.rating is not None else ""))
            match = match or (field == "genre" and any(matches_pattern(matcher.pattern, x) for x in r.genres))
            match = match or (field == "secondarygenre" and any(matches_pattern(matcher.pattern, x) for x in r.secondary_genres))
            match = match or (field == "descriptor" and any(matches_pattern(matcher.pattern, x) for x in r.descriptors))
            match = match or (field == "label" and any(matches_pattern(matcher.pattern, x) for x in r.labels))
            match = match or (field == "releaseartist[main]" and any(matches_pattern(matcher.pattern, x.name) for x in r.releaseartists.main))
            match = match or (field == "releaseartist[guest]" and any(matches_pattern(matcher.pattern, x.name) for x in r.releaseartists.guest))
            match = match or (field == "releaseartist[remixer]" and any(matches_pattern(matcher.pattern, x.name) for x in r.releaseartists.remixer))
            match = match or (field == "releaseartist[conductor]" and any(matches_pattern(matcher.pattern, x.name) for x in r.releaseartists.conductor))
            match = match or (field == "releaseartist[producer]" and any(matches_pattern(matcher.pattern, x.name) for x in r.releaseartists.producer))
            match = match or (field == "releaseartist[composer]" and any(matches_pattern(matcher.pattern, x.name) for x in r.releaseartists.composer))
            match = match or (field == "releaseartist[djmixer]" and any(matches_pattern(matcher.pattern, x.name) for x in r.releaseartists.djmixer))
            # fmt: on
            if match:
                rval.append(r)
                break
    logger.debug(
        f"Filtered {len(releases)} releases down to {len(rval)} releases in {time.time() - time_start} seconds"
    )
    return rval
