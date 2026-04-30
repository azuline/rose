//! Rule DSL parser for Rose's metadata matching and transformation engine.
//!
//! This module parses a text DSL for rules that match music metadata tags and
//! apply actions (replace, sed, split, add, delete). It is pure parsing logic
//! with no I/O.
//!
//! # Regex replacement syntax
//!
//! **Important difference from the Python version:** The `sed` action stores a
//! compiled regex and a replacement string. Rust's `regex` crate uses `$1` /
//! `${name}` for capture-group references in replacements, whereas Python uses
//! `\1` / `\g<name>`. Users migrating rule definitions from the Python edition
//! must update their sed replacement strings accordingly.

use std::fmt;

use regex::Regex;

use crate::common::{uniq, RoseError};

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// An error indicating a rule is invalid (user-facing, expected).
#[derive(Debug)]
pub struct InvalidRuleError {
    pub message: String,
}

impl fmt::Display for InvalidRuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for InvalidRuleError {}

impl From<InvalidRuleError> for RoseError {
    fn from(e: InvalidRuleError) -> Self {
        RoseError::Internal(e.message)
    }
}

/// A syntax error at a specific position in a rule string.
#[derive(Debug)]
pub struct RuleSyntaxError {
    pub rule_name: String,
    pub rule: String,
    pub index: usize,
    pub feedback: String,
}

impl fmt::Display for RuleSyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Failed to parse {}, invalid syntax:\n\n    {}\n    {}^\n    {}{}\n",
            self.rule_name,
            self.rule,
            " ".repeat(self.index),
            " ".repeat(self.index),
            self.feedback,
        )
    }
}

impl std::error::Error for RuleSyntaxError {}

impl From<RuleSyntaxError> for RoseError {
    fn from(e: RuleSyntaxError) -> Self {
        RoseError::Internal(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Tag definitions
// ---------------------------------------------------------------------------

/// All concrete tag identifiers.
pub const TAG_TRACKTITLE: &str = "tracktitle";
pub const TAG_TRACKARTIST_MAIN: &str = "trackartist[main]";
pub const TAG_TRACKARTIST_GUEST: &str = "trackartist[guest]";
pub const TAG_TRACKARTIST_REMIXER: &str = "trackartist[remixer]";
pub const TAG_TRACKARTIST_PRODUCER: &str = "trackartist[producer]";
pub const TAG_TRACKARTIST_COMPOSER: &str = "trackartist[composer]";
pub const TAG_TRACKARTIST_CONDUCTOR: &str = "trackartist[conductor]";
pub const TAG_TRACKARTIST_DJMIXER: &str = "trackartist[djmixer]";
pub const TAG_TRACKNUMBER: &str = "tracknumber";
pub const TAG_TRACKTOTAL: &str = "tracktotal";
pub const TAG_DISCNUMBER: &str = "discnumber";
pub const TAG_DISCTOTAL: &str = "disctotal";
pub const TAG_RELEASETITLE: &str = "releasetitle";
pub const TAG_RELEASEARTIST_MAIN: &str = "releaseartist[main]";
pub const TAG_RELEASEARTIST_GUEST: &str = "releaseartist[guest]";
pub const TAG_RELEASEARTIST_REMIXER: &str = "releaseartist[remixer]";
pub const TAG_RELEASEARTIST_PRODUCER: &str = "releaseartist[producer]";
pub const TAG_RELEASEARTIST_COMPOSER: &str = "releaseartist[composer]";
pub const TAG_RELEASEARTIST_CONDUCTOR: &str = "releaseartist[conductor]";
pub const TAG_RELEASEARTIST_DJMIXER: &str = "releaseartist[djmixer]";
pub const TAG_RELEASETYPE: &str = "releasetype";
pub const TAG_RELEASEDATE: &str = "releasedate";
pub const TAG_ORIGINALDATE: &str = "originaldate";
pub const TAG_COMPOSITIONDATE: &str = "compositiondate";
pub const TAG_CATALOGNUMBER: &str = "catalognumber";
pub const TAG_EDITION: &str = "edition";
pub const TAG_GENRE: &str = "genre";
pub const TAG_SECONDARYGENRE: &str = "secondarygenre";
pub const TAG_DESCRIPTOR: &str = "descriptor";
pub const TAG_LABEL: &str = "label";
pub const TAG_NEW: &str = "new";
pub const TAG_FAVORITE: &str = "favorite";
pub const TAG_RATING: &str = "rating";

// Expandable aliases (not actual concrete tags, but accepted in the DSL).
const ALIAS_ARTIST: &str = "artist";
const ALIAS_TRACKARTIST: &str = "trackartist";
const ALIAS_RELEASEARTIST: &str = "releaseartist";

/// A concrete tag type, represented as a string slice for simplicity.
pub type Tag = &'static str;

/// An expandable tag — either a concrete tag or an alias.
type ExpandableTag = &'static str;

/// Ordered list of `(expandable_key, resolved_tags)` entries.  We use a slice
/// of tuples instead of a HashMap to preserve iteration order (important for
/// the parser, which tries longer prefixes first).
///
/// **Ordering rule:** longer keys appear *before* shorter keys that are
/// prefixes.  For example `"trackartist[main]"` before `"trackartist"`.
const ALL_TAGS: &[(ExpandableTag, &[Tag])] = &[
    (TAG_TRACKTITLE, &[TAG_TRACKTITLE]),
    // trackartist[*] before trackartist
    (TAG_TRACKARTIST_MAIN, &[TAG_TRACKARTIST_MAIN]),
    (TAG_TRACKARTIST_GUEST, &[TAG_TRACKARTIST_GUEST]),
    (TAG_TRACKARTIST_REMIXER, &[TAG_TRACKARTIST_REMIXER]),
    (TAG_TRACKARTIST_PRODUCER, &[TAG_TRACKARTIST_PRODUCER]),
    (TAG_TRACKARTIST_COMPOSER, &[TAG_TRACKARTIST_COMPOSER]),
    (TAG_TRACKARTIST_CONDUCTOR, &[TAG_TRACKARTIST_CONDUCTOR]),
    (TAG_TRACKARTIST_DJMIXER, &[TAG_TRACKARTIST_DJMIXER]),
    (
        ALIAS_TRACKARTIST,
        &[
            TAG_TRACKARTIST_MAIN,
            TAG_TRACKARTIST_GUEST,
            TAG_TRACKARTIST_REMIXER,
            TAG_TRACKARTIST_PRODUCER,
            TAG_TRACKARTIST_COMPOSER,
            TAG_TRACKARTIST_CONDUCTOR,
            TAG_TRACKARTIST_DJMIXER,
        ],
    ),
    (TAG_TRACKNUMBER, &[TAG_TRACKNUMBER]),
    (TAG_TRACKTOTAL, &[TAG_TRACKTOTAL]),
    (TAG_DISCNUMBER, &[TAG_DISCNUMBER]),
    (TAG_DISCTOTAL, &[TAG_DISCTOTAL]),
    (TAG_RELEASETITLE, &[TAG_RELEASETITLE]),
    // releaseartist[*] before releaseartist
    (TAG_RELEASEARTIST_MAIN, &[TAG_RELEASEARTIST_MAIN]),
    (TAG_RELEASEARTIST_GUEST, &[TAG_RELEASEARTIST_GUEST]),
    (TAG_RELEASEARTIST_REMIXER, &[TAG_RELEASEARTIST_REMIXER]),
    (TAG_RELEASEARTIST_PRODUCER, &[TAG_RELEASEARTIST_PRODUCER]),
    (TAG_RELEASEARTIST_COMPOSER, &[TAG_RELEASEARTIST_COMPOSER]),
    (TAG_RELEASEARTIST_CONDUCTOR, &[TAG_RELEASEARTIST_CONDUCTOR]),
    (TAG_RELEASEARTIST_DJMIXER, &[TAG_RELEASEARTIST_DJMIXER]),
    (
        ALIAS_RELEASEARTIST,
        &[
            TAG_RELEASEARTIST_MAIN,
            TAG_RELEASEARTIST_GUEST,
            TAG_RELEASEARTIST_REMIXER,
            TAG_RELEASEARTIST_PRODUCER,
            TAG_RELEASEARTIST_COMPOSER,
            TAG_RELEASEARTIST_CONDUCTOR,
            TAG_RELEASEARTIST_DJMIXER,
        ],
    ),
    (TAG_RELEASETYPE, &[TAG_RELEASETYPE]),
    (TAG_RELEASEDATE, &[TAG_RELEASEDATE]),
    (TAG_ORIGINALDATE, &[TAG_ORIGINALDATE]),
    (TAG_COMPOSITIONDATE, &[TAG_COMPOSITIONDATE]),
    (TAG_EDITION, &[TAG_EDITION]),
    (TAG_CATALOGNUMBER, &[TAG_CATALOGNUMBER]),
    (TAG_GENRE, &[TAG_GENRE]),
    (TAG_SECONDARYGENRE, &[TAG_SECONDARYGENRE]),
    (TAG_DESCRIPTOR, &[TAG_DESCRIPTOR]),
    (TAG_LABEL, &[TAG_LABEL]),
    (TAG_NEW, &[TAG_NEW]),
    (TAG_FAVORITE, &[TAG_FAVORITE]),
    (TAG_RATING, &[TAG_RATING]),
    (
        ALIAS_ARTIST,
        &[
            TAG_TRACKARTIST_MAIN,
            TAG_TRACKARTIST_GUEST,
            TAG_TRACKARTIST_REMIXER,
            TAG_TRACKARTIST_PRODUCER,
            TAG_TRACKARTIST_COMPOSER,
            TAG_TRACKARTIST_CONDUCTOR,
            TAG_TRACKARTIST_DJMIXER,
            TAG_RELEASEARTIST_MAIN,
            TAG_RELEASEARTIST_GUEST,
            TAG_RELEASEARTIST_REMIXER,
            TAG_RELEASEARTIST_PRODUCER,
            TAG_RELEASEARTIST_COMPOSER,
            TAG_RELEASEARTIST_CONDUCTOR,
            TAG_RELEASEARTIST_DJMIXER,
        ],
    ),
];

/// Tags that can be modified by actions.
pub const MODIFIABLE_TAGS: &[Tag] = &[
    TAG_TRACKTITLE,
    TAG_TRACKARTIST_MAIN,
    TAG_TRACKARTIST_GUEST,
    TAG_TRACKARTIST_REMIXER,
    TAG_TRACKARTIST_PRODUCER,
    TAG_TRACKARTIST_COMPOSER,
    TAG_TRACKARTIST_CONDUCTOR,
    TAG_TRACKARTIST_DJMIXER,
    TAG_TRACKNUMBER,
    TAG_DISCNUMBER,
    TAG_RELEASETITLE,
    TAG_RELEASEARTIST_MAIN,
    TAG_RELEASEARTIST_GUEST,
    TAG_RELEASEARTIST_REMIXER,
    TAG_RELEASEARTIST_PRODUCER,
    TAG_RELEASEARTIST_COMPOSER,
    TAG_RELEASEARTIST_CONDUCTOR,
    TAG_RELEASEARTIST_DJMIXER,
    TAG_RELEASETYPE,
    TAG_RELEASEDATE,
    TAG_ORIGINALDATE,
    TAG_COMPOSITIONDATE,
    TAG_EDITION,
    TAG_CATALOGNUMBER,
    TAG_GENRE,
    TAG_SECONDARYGENRE,
    TAG_DESCRIPTOR,
    TAG_LABEL,
    TAG_NEW,
    TAG_FAVORITE,
    TAG_RATING,
];

/// Tags that hold a single value (as opposed to multi-value tags like genre).
pub const SINGLE_VALUE_TAGS: &[Tag] = &[
    TAG_TRACKTITLE,
    TAG_TRACKNUMBER,
    TAG_TRACKTOTAL,
    TAG_DISCNUMBER,
    TAG_DISCTOTAL,
    TAG_RELEASETITLE,
    TAG_RELEASETYPE,
    TAG_RELEASEDATE,
    TAG_ORIGINALDATE,
    TAG_COMPOSITIONDATE,
    TAG_EDITION,
    TAG_CATALOGNUMBER,
    TAG_NEW,
    TAG_FAVORITE,
    TAG_RATING,
];

/// Tags that are release-level (as opposed to track-level).
#[allow(dead_code)]
pub const RELEASE_TAGS: &[Tag] = &[
    TAG_RELEASETITLE,
    TAG_RELEASEARTIST_MAIN,
    TAG_RELEASEARTIST_GUEST,
    TAG_RELEASEARTIST_REMIXER,
    TAG_RELEASEARTIST_PRODUCER,
    TAG_RELEASEARTIST_COMPOSER,
    TAG_RELEASEARTIST_CONDUCTOR,
    TAG_RELEASEARTIST_DJMIXER,
    TAG_RELEASETYPE,
    TAG_RELEASETYPE, // duplicated in Python source
    TAG_RELEASEDATE,
    TAG_ORIGINALDATE,
    TAG_COMPOSITIONDATE,
    TAG_EDITION,
    TAG_CATALOGNUMBER,
    TAG_GENRE,
    TAG_SECONDARYGENRE,
    TAG_DESCRIPTOR,
    TAG_LABEL,
    TAG_DISCTOTAL,
    TAG_NEW,
    TAG_FAVORITE,
    TAG_RATING,
];

// Helper to get the keys for the ALL_TAGS list (for error messages).
fn all_tags_keys() -> Vec<&'static str> {
    ALL_TAGS.iter().map(|(k, _)| *k).collect()
}

/// Look up an expandable tag key and return its resolved tags.
pub fn resolve_tag(key: &str) -> Option<&'static [Tag]> {
    ALL_TAGS.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

// ---------------------------------------------------------------------------
// Action types
// ---------------------------------------------------------------------------

/// Replaces the matched tag value with `replacement`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceAction {
    pub replacement: String,
}

/// Executes a regex substitution on a tag value.
///
/// **Note:** Rust's `regex` crate uses `$1`/`${name}` for capture-group
/// back-references in the replacement string, whereas the Python version uses
/// `\1`/`\g<name>`.
#[derive(Debug, Clone)]
pub struct SedAction {
    pub src: Regex,
    pub dst: String,
}

impl PartialEq for SedAction {
    fn eq(&self, other: &Self) -> bool {
        self.src.as_str() == other.src.as_str() && self.dst == other.dst
    }
}
impl Eq for SedAction {}

/// Splits a multi-value tag on a delimiter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitAction {
    pub delimiter: String,
}

/// Adds a value to a multi-value tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddAction {
    pub value: String,
}

/// Deletes the tag value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteAction;

/// The behavior of an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionBehavior {
    Replace(ReplaceAction),
    Sed(SedAction),
    Split(SplitAction),
    Add(AddAction),
    Delete(DeleteAction),
}

// ---------------------------------------------------------------------------
// Pattern
// ---------------------------------------------------------------------------

/// A substring-match pattern with optional `^` (strict start) / `$` (strict
/// end) anchors and a case-insensitivity flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    pub needle: String,
    pub strict_start: bool,
    pub strict_end: bool,
    pub case_insensitive: bool,
}

impl Pattern {
    /// Create a new pattern.  If `strict_start`/`strict_end` are `false`, the
    /// constructor parses leading `^` and trailing `$` from the needle.
    /// Escaped `\^` and `\$` produce literal characters.
    pub fn new(
        needle: &str,
        strict: bool,
        strict_start: bool,
        strict_end: bool,
        case_insensitive: bool,
    ) -> Self {
        let mut n = needle.to_owned();
        let mut ss = strict_start || strict;
        let mut se = strict_end || strict;

        if !ss {
            if n.starts_with('^') {
                ss = true;
                n = n[1..].to_owned();
            } else if n.starts_with("\\^") {
                n = n[1..].to_owned();
            }
        }
        if !se {
            if n.ends_with("\\$") {
                n = n[..n.len() - 2].to_owned() + "$";
            } else if n.ends_with('$') {
                se = true;
                n = n[..n.len() - 1].to_owned();
            }
        }

        Pattern {
            needle: n,
            strict_start: ss,
            strict_end: se,
            case_insensitive,
        }
    }

    /// Convenience: pattern from needle with default flags.
    pub fn simple(needle: &str) -> Self {
        Self::new(needle, false, false, false, false)
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let escaped = escape(&self.needle);

        let mut r = String::new();
        if self.strict_start {
            r.push('^');
            r.push_str(&escaped);
        } else if self.needle.starts_with('^') {
            r.push('\\');
            r.push_str(&escaped);
        } else {
            r.push_str(&escaped);
        }

        if self.strict_end {
            r.push('$');
        } else if self.needle.ends_with('$') {
            // Replace the trailing "$" (which was escaped to "$" by `escape`)
            // with r"\$".
            // The escaped string ends with "$" (escape doesn't touch "$").
            r = r[..r.len() - 1].to_owned() + "\\$";
        }

        if self.case_insensitive {
            r.push_str(":i");
        }
        write!(f, "{}", r)
    }
}

// ---------------------------------------------------------------------------
// Matcher
// ---------------------------------------------------------------------------

/// A matcher: a set of tags and a pattern to test them against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matcher {
    pub tags: Vec<Tag>,
    pub pattern: Pattern,
}

impl Matcher {
    /// Build a Matcher from expandable tag names and a pattern.
    pub fn from_expandable(tags: &[ExpandableTag], pattern: Pattern) -> Self {
        let mut resolved: Vec<Tag> = Vec::new();
        for &t in tags {
            if let Some(r) = resolve_tag(t) {
                resolved.extend_from_slice(r);
            }
        }
        Matcher {
            tags: uniq(resolved),
            pattern,
        }
    }

    /// Parse a matcher from a DSL string like `"tag1,tag2:pattern:flags"`.
    pub fn parse(raw: &str) -> Result<Matcher, RuleSyntaxError> {
        Self::parse_with_name(raw, "matcher")
    }

    /// Parse with a custom rule_name for error messages.
    pub fn parse_with_name(raw: &str, rule_name: &str) -> Result<Matcher, RuleSyntaxError> {
        let mut idx = 0;

        // Parse tags.
        let mut tags: Vec<Tag> = Vec::new();
        let mut found_colon = false;
        loop {
            let mut matched_any = false;
            for &(t, resolved) in ALL_TAGS {
                if !raw[idx..].starts_with(t) {
                    continue;
                }
                let after = idx + t.len();
                let next_ch = raw.get(after..after + 1);
                match next_ch {
                    Some(":") | Some(",") => {}
                    Some(_) => continue,
                    None => {
                        return Err(RuleSyntaxError {
                            rule_name: rule_name.to_owned(),
                            rule: raw.to_owned(),
                            index: after,
                            feedback: "Expected to find ',' or ':', found end of string."
                                .to_owned(),
                        });
                    }
                }
                tags.extend_from_slice(resolved);
                idx = after + 1;
                found_colon = raw.as_bytes()[idx - 1] == b':';
                matched_any = true;
                break;
            }
            if !matched_any {
                return Err(RuleSyntaxError {
                    rule_name: rule_name.to_owned(),
                    rule: raw.to_owned(),
                    index: idx,
                    feedback: format!(
                        "Invalid tag: must be one of {{{}}}. The next character after a tag must be ':' or ','.",
                        all_tags_keys().join(", ")
                    ),
                });
            }
            if found_colon {
                break;
            }
        }

        // Parse the pattern.
        let (pattern_str, fwd) = take_until(&raw[idx..], ':', false);
        idx += fwd;

        // Optional flags section.
        let mut case_insensitive = false;
        if idx < raw.len() {
            let (probe, _) = take_until(&raw[idx..], ':', true);
            if probe.is_empty() {
                // We have an empty probe, meaning raw[idx] == ':', consume it.
                idx += 1;
                let (flags, fwd2) = take_until(&raw[idx..], ':', true);
                if flags.is_empty() {
                    return Err(RuleSyntaxError {
                        rule_name: rule_name.to_owned(),
                        rule: raw.to_owned(),
                        index: idx,
                        feedback: "No flags specified: Please remove this section (by deleting the colon) or specify one of the supported flags: `i` (case insensitive).".to_owned(),
                    });
                }
                for (i, flag) in flags.chars().enumerate() {
                    if flag == 'i' {
                        case_insensitive = true;
                        continue;
                    }
                    return Err(RuleSyntaxError {
                        rule_name: rule_name.to_owned(),
                        rule: raw.to_owned(),
                        index: idx + i,
                        feedback: "Unrecognized flag: Please specify one of the supported flags: `i` (case insensitive).".to_owned(),
                    });
                }
                idx += fwd2;
            }
        }

        if idx < raw.len() {
            return Err(RuleSyntaxError {
                rule_name: rule_name.to_owned(),
                rule: raw.to_owned(),
                index: idx,
                feedback:
                    "Extra input found after end of matcher. Perhaps you meant to escape this colon?"
                        .to_owned(),
            });
        }

        Ok(Matcher {
            tags: uniq(tags),
            pattern: Pattern::new(&pattern_str, false, false, false, case_insensitive),
        })
    }
}

impl fmt::Display for Matcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", stringify_tags(&self.tags), self.pattern)
    }
}

// ---------------------------------------------------------------------------
// Action
// ---------------------------------------------------------------------------

/// An action to apply to tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub tags: Vec<Tag>,
    pub behavior: ActionBehavior,
    pub pattern: Option<Pattern>,
}

impl Action {
    /// Build an Action from expandable tag names.
    pub fn from_expandable(
        tags: &[ExpandableTag],
        behavior: ActionBehavior,
        pattern: Option<Pattern>,
    ) -> Self {
        let mut resolved: Vec<Tag> = Vec::new();
        for &t in tags {
            if let Some(r) = resolve_tag(t) {
                resolved.extend_from_slice(r);
            }
        }
        Action {
            tags: uniq(resolved),
            behavior,
            pattern,
        }
    }

    /// Parse an action from a DSL string.
    pub fn parse(
        raw: &str,
        action_number: Option<usize>,
        matcher: Option<&Matcher>,
    ) -> Result<Action, RoseError> {
        let mut idx = 0;
        let rule_name = match action_number {
            Some(n) => format!("action {}", n),
            None => "action".to_owned(),
        };

        // Determine whether we have a tags+pattern section (unescaped `/`).
        let (_, action_idx) = take_until(raw, '/', true);
        let has_tags_pattern_section = action_idx != raw.len();

        let tags: Vec<Tag>;
        let pattern: Option<Pattern>;

        if !has_tags_pattern_section {
            // No tags/pattern section — inherit from matcher.
            match matcher {
                None => {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_owned(),
                        index: idx,
                        feedback: "Tags/pattern section not found. Must specify tags to modify, since there is no matcher to default to. Make sure you are formatting your action like {tags}:{pattern}/{kind}:{args} (where `:{pattern}` is optional)".to_owned(),
                    }.into());
                }
                Some(m) => {
                    tags = m
                        .tags
                        .iter()
                        .copied()
                        .filter(|t| MODIFIABLE_TAGS.contains(t))
                        .collect();
                    pattern = Some(m.pattern.clone());
                }
            }
        } else {
            // Parse tags.
            if raw[idx..].starts_with("matched:") {
                match matcher {
                    None => {
                        return Err(RuleSyntaxError {
                            rule_name,
                            rule: raw.to_owned(),
                            index: idx,
                            feedback: "Cannot use `matched` in this context: there is no matcher to default to.".to_owned(),
                        }.into());
                    }
                    Some(m) => {
                        idx += "matched:".len();
                        tags = m
                            .tags
                            .iter()
                            .copied()
                            .filter(|t| MODIFIABLE_TAGS.contains(t))
                            .collect();
                    }
                }
            } else {
                let mut parsed_tags: Vec<Tag> = Vec::new();
                let mut found_end = false;
                loop {
                    let mut matched_any = false;
                    for &(t, resolved) in ALL_TAGS {
                        if !raw[idx..].starts_with(t) {
                            continue;
                        }
                        let after = idx + t.len();
                        let next_ch = raw.get(after..after + 1);
                        match next_ch {
                            Some(":") | Some(",") | Some("/") => {}
                            _ => continue,
                        }
                        // Validate all resolved tags are modifiable.
                        for &rt in resolved {
                            if !MODIFIABLE_TAGS.contains(&rt) {
                                return Err(RuleSyntaxError {
                                    rule_name,
                                    rule: raw.to_owned(),
                                    index: idx,
                                    feedback: format!("Invalid tag: {} is not modifiable.", t),
                                }
                                .into());
                            }
                            parsed_tags.push(rt);
                        }
                        idx = after + 1;
                        let sep = raw.as_bytes()[idx - 1];
                        found_end = sep == b':' || sep == b'/';
                        matched_any = true;
                        break;
                    }
                    if !matched_any {
                        // Build the list of modifiable expandable tags for error message.
                        let tags_to_print: Vec<&str> = ALL_TAGS
                            .iter()
                            .filter(|(_, resolved)| {
                                resolved.iter().all(|r| MODIFIABLE_TAGS.contains(r))
                            })
                            .map(|(k, _)| *k)
                            .collect();
                        let feedback = if matcher.is_some() {
                            format!(
                                "Invalid tag: must be one of matched, {{{}}}. (And if the value is matched, it must be alone.) The next character after a tag must be ':' or ','.",
                                tags_to_print.join(", ")
                            )
                        } else {
                            format!(
                                "Invalid tag: must be one of {{{}}}. The next character after a tag must be ':' or ','.",
                                tags_to_print.join(", ")
                            )
                        };
                        return Err(RuleSyntaxError {
                            rule_name,
                            rule: raw.to_owned(),
                            index: idx,
                            feedback,
                        }
                        .into());
                    }
                    if found_end {
                        break;
                    }
                }
                tags = parsed_tags;
            }

            // Parse the optional pattern.
            // Check if the previous separator was `/` (meaning no pattern to parse).
            let prev_sep = raw.as_bytes()[idx - 1];
            if prev_sep == b'/' {
                // `tracktitle/...` — inherit pattern from matcher if tags match.
                if let Some(m) = matcher {
                    if tags == m.tags {
                        pattern = Some(m.pattern.clone());
                    } else {
                        pattern = None;
                    }
                } else {
                    pattern = None;
                }
            } else {
                // Previous separator was `:`, check if the next thing is `/` (explicitly empty pattern).
                let (probe, probe_fwd) = take_until(&raw[idx..], '/', true);
                if probe.is_empty() && probe_fwd == 1 {
                    // Explicitly empty: `tracktitle:/...`
                    idx += 1;
                    pattern = None;
                } else {
                    // We have pattern content. Take the earliest of colon or slash.
                    let (colon_pat, colon_fwd) = take_until(&raw[idx..], ':', true);
                    let (slash_pat, slash_fwd) = take_until(&raw[idx..], '/', true);
                    let (needle, fwd, has_flags) = if colon_fwd < slash_fwd {
                        (colon_pat, colon_fwd, true)
                    } else {
                        (slash_pat, slash_fwd, false)
                    };
                    idx += fwd;

                    if !needle.is_empty() {
                        let mut case_insensitive = false;
                        if has_flags {
                            let (flags, fwd2) = take_until(&raw[idx..], '/', true);
                            if flags.is_empty() {
                                return Err(RuleSyntaxError {
                                    rule_name,
                                    rule: raw.to_owned(),
                                    index: idx,
                                    feedback: "No flags specified: Please remove this section (by deleting the colon) or specify one of the supported flags: `i` (case insensitive).".to_owned(),
                                }.into());
                            }
                            for (i, flag) in flags.chars().enumerate() {
                                if flag == 'i' {
                                    case_insensitive = true;
                                    continue;
                                }
                                return Err(RuleSyntaxError {
                                    rule_name,
                                    rule: raw.to_owned(),
                                    index: idx + i,
                                    feedback: "Unrecognized flag: Either you forgot a colon here (to end the matcher), or this is an invalid matcher flag. The only supported flag is `i` (case insensitive).".to_owned(),
                                }.into());
                            }
                            idx += fwd2;
                        }
                        pattern =
                            Some(Pattern::new(&needle, false, false, false, case_insensitive));
                    } else {
                        pattern = None;
                    }
                }
            }
        }

        // Parse the action kind.
        let valid_actions = ["replace", "sed", "split", "add", "delete"];
        let mut action_kind: Option<&str> = None;
        for va in &valid_actions {
            if raw[idx..].starts_with(&format!("{}:", va)) {
                action_kind = Some(va);
                idx += va.len() + 1;
                break;
            }
            if raw[idx..] == **va {
                action_kind = Some(va);
                idx += va.len();
                break;
            }
        }
        let action_kind = match action_kind {
            Some(k) => k,
            None => {
                let mut feedback = format!(
                    "Invalid action kind: must be one of {{{}}}.",
                    valid_actions.join(", ")
                );
                if idx == 0 && raw.contains(':') {
                    feedback += " If this is pointing at your pattern, you forgot to put a `/` between the matcher section and the action section.";
                }
                return Err(RuleSyntaxError {
                    rule_name,
                    rule: raw.to_owned(),
                    index: idx,
                    feedback,
                }
                .into());
            }
        };

        // Validate that split/add are not used on single-value tags.
        if action_kind == "split" || action_kind == "add" {
            let single_valued: Vec<Tag> = tags
                .iter()
                .copied()
                .filter(|t| SINGLE_VALUE_TAGS.contains(t))
                .collect();
            if !single_valued.is_empty() {
                return Err(InvalidRuleError {
                    message: format!(
                        "Single valued tags {} cannot be modified by multi-value action {}",
                        single_valued.join(", "),
                        action_kind
                    ),
                }
                .into());
            }
        }

        // Parse action-specific parameters.
        let behavior = match action_kind {
            "replace" => {
                let (replacement, fwd) = take_until(&raw[idx..], ':', false);
                idx += fwd;
                if replacement.is_empty() {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_owned(),
                        index: idx,
                        feedback: "Replacement not found: must specify a non-empty replacement. Use the delete action to remove a value.".to_owned(),
                    }.into());
                }
                if idx < raw.len() {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_owned(),
                        index: idx,
                        feedback: "Found another section after the replacement, but the replacement must be the last section. Perhaps you meant to escape this colon?".to_owned(),
                    }.into());
                }
                ActionBehavior::Replace(ReplaceAction { replacement })
            }
            "sed" => {
                let (src_str, fwd) = take_until(&raw[idx..], ':', false);
                if src_str.is_empty() {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_owned(),
                        index: idx,
                        feedback: format!(
                            "Empty sed pattern found: must specify a non-empty pattern. Example: {}:pattern:replacement",
                            raw
                        ),
                    }.into());
                }
                let src = match Regex::new(&src_str) {
                    Ok(r) => r,
                    Err(e) => {
                        return Err(RuleSyntaxError {
                            rule_name,
                            rule: raw.to_owned(),
                            index: idx,
                            feedback: format!(
                                "Failed to compile the sed pattern regex: invalid pattern: {}",
                                e
                            ),
                        }
                        .into());
                    }
                };
                idx += fwd;

                if idx >= raw.len() || raw.as_bytes()[idx] != b':' {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_owned(),
                        index: idx,
                        feedback: format!(
                            "Sed replacement not found: must specify a sed replacement section. Example: {}:replacement.",
                            raw
                        ),
                    }.into());
                }
                idx += 1;

                let (dst, fwd) = take_until(&raw[idx..], ':', false);
                idx += fwd;
                if idx < raw.len() {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_owned(),
                        index: idx,
                        feedback: "Found another section after the sed replacement, but the sed replacement must be the last section. Perhaps you meant to escape this colon?".to_owned(),
                    }.into());
                }
                ActionBehavior::Sed(SedAction { src, dst })
            }
            "split" => {
                let (delimiter, fwd) = take_until(&raw[idx..], ':', false);
                idx += fwd;
                if delimiter.is_empty() {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_owned(),
                        index: idx,
                        feedback:
                            "Delimiter not found: must specify a non-empty delimiter to split on."
                                .to_owned(),
                    }
                    .into());
                }
                if idx < raw.len() {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_owned(),
                        index: idx,
                        feedback: "Found another section after the delimiter, but the delimiter must be the last section. Perhaps you meant to escape this colon?".to_owned(),
                    }.into());
                }
                ActionBehavior::Split(SplitAction { delimiter })
            }
            "add" => {
                let (value, fwd) = take_until(&raw[idx..], ':', false);
                idx += fwd;
                if value.is_empty() {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_owned(),
                        index: idx,
                        feedback: "Value not found: must specify a non-empty value to add."
                            .to_owned(),
                    }
                    .into());
                }
                if idx < raw.len() {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_owned(),
                        index: idx,
                        feedback: "Found another section after the value, but the value must be the last section. Perhaps you meant to escape this colon?".to_owned(),
                    }.into());
                }
                ActionBehavior::Add(AddAction { value })
            }
            "delete" => {
                if idx < raw.len() {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_owned(),
                        index: idx,
                        feedback: "Found another section after the action kind, but the delete action has no parameters. Please remove this section.".to_owned(),
                    }.into());
                }
                ActionBehavior::Delete(DeleteAction)
            }
            _ => {
                return Err(RoseError::Internal(format!(
                    "Impossible: unknown action_kind {}",
                    action_kind
                )));
            }
        };

        Ok(Action {
            tags: uniq(tags),
            behavior,
            pattern,
        })
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut r = String::new();
        r.push_str(&stringify_tags(&self.tags));
        if let Some(ref p) = self.pattern {
            r.push(':');
            r.push_str(&p.to_string());
        }
        if !r.is_empty() {
            r.push('/');
        }

        match &self.behavior {
            ActionBehavior::Replace(a) => {
                r.push_str("replace:");
                r.push_str(&a.replacement);
            }
            ActionBehavior::Sed(a) => {
                r.push_str("sed:");
                r.push_str(&escape(a.src.as_str()));
                r.push(':');
                r.push_str(&escape(&a.dst));
            }
            ActionBehavior::Split(a) => {
                r.push_str("split:");
                r.push_str(&a.delimiter);
            }
            ActionBehavior::Add(a) => {
                r.push_str("add:");
                r.push_str(&a.value);
            }
            ActionBehavior::Delete(_) => {
                r.push_str("delete");
            }
        }

        write!(f, "{}", r)
    }
}

// ---------------------------------------------------------------------------
// Rule
// ---------------------------------------------------------------------------

/// A complete rule: a matcher, one or more actions, and optional ignore matchers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub matcher: Matcher,
    pub actions: Vec<Action>,
    pub ignore: Vec<Matcher>,
}

impl Rule {
    /// Parse a complete rule from DSL strings.
    pub fn parse(
        matcher: &str,
        actions: &[&str],
        ignore: Option<&[&str]>,
    ) -> Result<Rule, RoseError> {
        let parsed_matcher = Matcher::parse(matcher)?;
        let mut parsed_actions = Vec::new();
        for (i, a) in actions.iter().enumerate() {
            parsed_actions.push(Action::parse(a, Some(i + 1), Some(&parsed_matcher))?);
        }
        let mut parsed_ignore = Vec::new();
        if let Some(ign) = ignore {
            for v in ign {
                parsed_ignore.push(Matcher::parse_with_name(v, "ignore")?);
            }
        }
        Ok(Rule {
            matcher: parsed_matcher,
            actions: parsed_actions,
            ignore: parsed_ignore,
        })
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        parts.push(format!(
            "matcher={}",
            shell_quote(&self.matcher.to_string())
        ));
        for action in &self.actions {
            parts.push(format!("action={}", shell_quote(&action.to_string())));
        }
        write!(f, "{}", parts.join(" "))
    }
}

/// Minimally shell-quote a string: if it contains whitespace or special shell
/// characters, wrap it in single quotes.  This matches Python's `shlex.quote`.
fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    // If the string only contains "safe" characters, return as-is.
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || "/:.,_-=^[]@%+".contains(c))
    {
        return s.to_string();
    }
    // Wrap in single quotes, escaping any single quotes inside.
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

// ---------------------------------------------------------------------------
// Escaping protocol
// ---------------------------------------------------------------------------

/// Read from `x` until the next unescaped occurrence of `until` (or end of
/// string). Returns `(parsed_string, chars_consumed)`.
///
/// The consumed count includes the `until` character if `consume_until` is
/// true. The returned string has escape sequences resolved (`::` → `:`,
/// `//` → `/`).
pub fn take(x: &str, until: char) -> (String, usize) {
    take_until(x, until, true)
}

/// Internal `take` with configurable `consume_until`.
fn take_until(x: &str, until: char, consume_until: bool) -> (String, usize) {
    let mut result = String::new();
    let mut fwd: usize = 0;
    loop {
        let (part, part_fwd) = take_escaped(&x[fwd..], until, consume_until);
        // Unescape `::` -> `:` and `//` -> `/`.
        result.push_str(&part.replace("::", ":").replace("//", "/"));
        fwd += part_fwd;

        let next_idx = if consume_until { fwd } else { fwd + 1 };
        let escaped_special = x.get(next_idx..).is_some_and(|s| s.starts_with(until));
        if !escaped_special {
            break;
        }
        result.push(until);
        fwd = next_idx + 1;
    }
    (result, fwd)
}

/// Low-level helper — reads until `until` handling `::`/`//` escape pairs.
/// **Do not use directly**; use [`take_until`] instead.
fn take_escaped(x: &str, until: char, consume_until: bool) -> (String, usize) {
    let until_str: String = until.to_string();
    let mut r = String::new();
    let mut escaped: Option<char> = None;
    let mut seen_idx: usize = 0;
    let chars: Vec<char> = x.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // Check for `until` match.
        if x[byte_offset(x, i)..].starts_with(&until_str) {
            if consume_until {
                seen_idx += until_str.len();
            }
            break;
        }
        let c = chars[i];
        if (c == ':' || c == '/') && escaped.is_none() {
            escaped = Some(c);
            seen_idx += 1;
            i += 1;
            continue;
        }
        if let Some(esc) = escaped {
            if c != esc {
                r.push(esc);
            }
            escaped = None;
        }
        r.push(c);
        seen_idx += 1;
        i += 1;
    }

    (r, seen_idx)
}

/// Return the byte offset of the `n`-th char in `s`.
fn byte_offset(s: &str, n: usize) -> usize {
    s.char_indices().nth(n).map(|(i, _)| i).unwrap_or(s.len())
}

/// Escape special characters in a string for the DSL.
pub fn escape(x: &str) -> String {
    x.replace(':', "::").replace('/', "//")
}

/// Collapse expanded tag lists back to their shorthand alias form where
/// possible, then join with commas.
pub fn stringify_tags(tags: &[Tag]) -> String {
    let mut tags: Vec<&str> = tags.to_vec();

    // Try collapsing `artist` (all 14 artist tags).
    let artist_resolved = resolve_tag(ALIAS_ARTIST).unwrap();
    if artist_resolved.iter().all(|t| tags.contains(t)) {
        let idx = tags.iter().position(|t| *t == artist_resolved[0]).unwrap();
        for t in artist_resolved {
            tags.retain(|x| *x != *t);
        }
        tags.insert(idx, ALIAS_ARTIST);
    }

    // Try collapsing `trackartist` (7 trackartist tags).
    let ta_resolved = resolve_tag(ALIAS_TRACKARTIST).unwrap();
    if ta_resolved.iter().all(|t| tags.contains(t)) {
        let idx = tags.iter().position(|t| *t == ta_resolved[0]).unwrap();
        for t in ta_resolved {
            tags.retain(|x| *x != *t);
        }
        tags.insert(idx, ALIAS_TRACKARTIST);
    }

    // Try collapsing `releaseartist` (7 releaseartist tags).
    let ra_resolved = resolve_tag(ALIAS_RELEASEARTIST).unwrap();
    if ra_resolved.iter().all(|t| tags.contains(t)) {
        let idx = tags.iter().position(|t| *t == ra_resolved[0]).unwrap();
        for t in ra_resolved {
            tags.retain(|x| *x != *t);
        }
        tags.insert(idx, ALIAS_RELEASEARTIST);
    }

    tags.join(",")
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create Matcher without parse.
    fn make_matcher(tags: &[&'static str], pattern: Pattern) -> Matcher {
        Matcher::from_expandable(tags, pattern)
    }

    fn make_action(
        tags: &[&'static str],
        behavior: ActionBehavior,
        pattern: Option<Pattern>,
    ) -> Action {
        Action::from_expandable(tags, behavior, pattern)
    }

    // -----------------------------------------------------------------------
    // test_rule_str (from Python test_rule_str)
    // -----------------------------------------------------------------------

    #[test]
    fn test_rule_str() {
        let rule = Rule::parse(
            "tracktitle:Track",
            &["releaseartist,genre/replace:lalala"],
            None,
        )
        .unwrap();
        assert_eq!(
            rule.to_string(),
            "matcher=tracktitle:Track action=releaseartist,genre/replace:lalala"
        );

        // Test that rules are quoted properly.
        let rule =
            Rule::parse(r"tracktitle,releaseartist,genre::: ", &[r"sed::::; "], None).unwrap();
        assert_eq!(
            rule.to_string(),
            r"matcher='tracktitle,releaseartist,genre::: ' action='tracktitle,releaseartist,genre::: /sed::::; '"
        );

        // Test that custom action matcher is printed properly.
        let rule = Rule::parse("tracktitle:Track", &["genre:lala/replace:lalala"], None).unwrap();
        assert_eq!(
            rule.to_string(),
            "matcher=tracktitle:Track action=genre:lala/replace:lalala"
        );

        // Test that we print when action pattern is not null.
        let rule = Rule::parse("genre:b", &["genre:h/replace:hi"], None).unwrap();
        assert_eq!(
            rule.to_string(),
            r"matcher=genre:b action=genre:h/replace:hi"
        );
    }

    // -----------------------------------------------------------------------
    // test_rule_parse_matcher (from Python test_rule_parse_matcher)
    // -----------------------------------------------------------------------

    #[test]
    fn test_rule_parse_matcher() {
        assert_eq!(
            Matcher::parse("tracktitle:Track").unwrap(),
            make_matcher(&["tracktitle"], Pattern::simple("Track"))
        );
        assert_eq!(
            Matcher::parse("tracktitle,tracknumber:Track").unwrap(),
            make_matcher(&["tracktitle", "tracknumber"], Pattern::simple("Track"))
        );
        assert_eq!(
            Matcher::parse(r"tracktitle,tracknumber:Tr::ck").unwrap(),
            make_matcher(&["tracktitle", "tracknumber"], Pattern::simple("Tr:ck"))
        );
        assert_eq!(
            Matcher::parse("tracktitle,tracknumber:Track:i").unwrap(),
            make_matcher(
                &["tracktitle", "tracknumber"],
                Pattern::new("Track", false, false, false, true)
            )
        );
        assert_eq!(
            Matcher::parse(r"tracktitle:").unwrap(),
            make_matcher(&["tracktitle"], Pattern::simple(""))
        );

        assert_eq!(
            Matcher::parse("tracktitle:^Track").unwrap(),
            make_matcher(
                &["tracktitle"],
                Pattern::new("Track", false, true, false, false)
            )
        );
        assert_eq!(
            Matcher::parse("tracktitle:Track$").unwrap(),
            make_matcher(
                &["tracktitle"],
                Pattern::new("Track", false, false, true, false)
            )
        );
        // In the Python test, `Matcher.parse(r"tracktitle:\^Track")` produces
        // `Pattern(r"\^Track")` — i.e. needle is literally `\^Track` because the
        // `\^` prefix is interpreted as "escaped ^ → literal ^", with the needle
        // then being `^Track` (the backslash is consumed by Pattern.__init__).
        // But the Python test compares to `Pattern(r"\^Track")` which also goes
        // through __init__, so `\^` → needle=`^Track`.
        assert_eq!(
            Matcher::parse(r"tracktitle:\^Track").unwrap(),
            make_matcher(&["tracktitle"], Pattern::simple(r"\^Track"))
        );
        assert_eq!(
            Matcher::parse(r"tracktitle:Track\$").unwrap(),
            make_matcher(&["tracktitle"], Pattern::simple(r"Track\$"))
        );
        assert_eq!(
            Matcher::parse(r"tracktitle:\^Track\$").unwrap(),
            make_matcher(&["tracktitle"], Pattern::simple(r"\^Track\$"))
        );
    }

    #[test]
    fn test_rule_parse_matcher_errors() {
        fn test_err(rule: &str, expected: &str) {
            let err = Matcher::parse(rule).unwrap_err();
            assert_eq!(err.to_string(), expected);
        }

        test_err(
            "tracknumber^Track$",
            "Failed to parse matcher, invalid syntax:\n\n\
             \x20   tracknumber^Track$\n\
             \x20   ^\n\
             \x20   Invalid tag: must be one of {tracktitle, trackartist[main], trackartist[guest], trackartist[remixer], trackartist[producer], trackartist[composer], trackartist[conductor], trackartist[djmixer], trackartist, tracknumber, tracktotal, discnumber, disctotal, releasetitle, releaseartist[main], releaseartist[guest], releaseartist[remixer], releaseartist[producer], releaseartist[composer], releaseartist[conductor], releaseartist[djmixer], releaseartist, releasetype, releasedate, originaldate, compositiondate, edition, catalognumber, genre, secondarygenre, descriptor, label, new, favorite, rating, artist}. The next character after a tag must be ':' or ','.\n",
        );

        test_err(
            "tracknumber",
            "Failed to parse matcher, invalid syntax:\n\n\
             \x20   tracknumber\n\
             \x20              ^\n\
             \x20              Expected to find ',' or ':', found end of string.\n",
        );

        test_err(
            "tracktitle:Tr:ck",
            "Failed to parse matcher, invalid syntax:\n\n\
             \x20   tracktitle:Tr:ck\n\
             \x20                 ^\n\
             \x20                 Unrecognized flag: Please specify one of the supported flags: `i` (case insensitive).\n",
        );

        test_err(
            "tracktitle:hi:i:hihi",
            "Failed to parse matcher, invalid syntax:\n\n\
             \x20   tracktitle:hi:i:hihi\n\
             \x20                   ^\n\
             \x20                   Extra input found after end of matcher. Perhaps you meant to escape this colon?\n",
        );
    }

    // -----------------------------------------------------------------------
    // test_rule_parse_action (from Python test_rule_parse_action)
    // -----------------------------------------------------------------------

    #[test]
    fn test_rule_parse_action() {
        let m1 = make_matcher(&["tracktitle"], Pattern::simple("haha"));

        // Replace with matcher
        assert_eq!(
            Action::parse("replace:lalala", Some(1), Some(&m1)).unwrap(),
            make_action(
                &["tracktitle"],
                ActionBehavior::Replace(ReplaceAction {
                    replacement: "lalala".to_owned()
                }),
                Some(Pattern::simple("haha")),
            )
        );

        // Replace with explicit genre tag
        assert_eq!(
            Action::parse("genre/replace:lalala", Some(1), None).unwrap(),
            make_action(
                &["genre"],
                ActionBehavior::Replace(ReplaceAction {
                    replacement: "lalala".to_owned()
                }),
                None,
            )
        );

        // Replace with multiple tags
        assert_eq!(
            Action::parse("tracknumber,genre/replace:lalala", Some(1), None).unwrap(),
            make_action(
                &["tracknumber", "genre"],
                ActionBehavior::Replace(ReplaceAction {
                    replacement: "lalala".to_owned()
                }),
                None,
            )
        );

        // Replace with tag and pattern
        assert_eq!(
            Action::parse("genre:lala/replace:lalala", Some(1), None).unwrap(),
            make_action(
                &["genre"],
                ActionBehavior::Replace(ReplaceAction {
                    replacement: "lalala".to_owned()
                }),
                Some(Pattern::simple("lala")),
            )
        );

        // Replace with tag, pattern, and case-insensitive flag
        assert_eq!(
            Action::parse("genre:lala:i/replace:lalala", Some(1), None).unwrap(),
            make_action(
                &["genre"],
                ActionBehavior::Replace(ReplaceAction {
                    replacement: "lalala".to_owned()
                }),
                Some(Pattern::new("lala", false, false, false, true)),
            )
        );

        // Replace with matched tag and pattern
        assert_eq!(
            Action::parse("matched:^x/replace:lalala", Some(1), Some(&m1)).unwrap(),
            make_action(
                &["tracktitle"],
                ActionBehavior::Replace(ReplaceAction {
                    replacement: "lalala".to_owned()
                }),
                Some(Pattern::new("^x", false, false, false, false)),
            )
        );

        // Test that case insensitivity is inherited from the matcher.
        let m_ci = make_matcher(
            &["tracktitle"],
            Pattern::new("haha", false, false, false, true),
        );
        assert_eq!(
            Action::parse("replace:lalala", Some(1), Some(&m_ci)).unwrap(),
            make_action(
                &["tracktitle"],
                ActionBehavior::Replace(ReplaceAction {
                    replacement: "lalala".to_owned()
                }),
                Some(Pattern::new("haha", false, false, false, true)),
            )
        );

        // Test that the action excludes the immutable *total tags.
        let m_totals = make_matcher(
            &["tracknumber", "tracktotal", "discnumber", "disctotal"],
            Pattern::simple("1"),
        );
        assert_eq!(
            Action::parse("replace:5", Some(1), Some(&m_totals)).unwrap(),
            make_action(
                &["tracknumber", "discnumber"],
                ActionBehavior::Replace(ReplaceAction {
                    replacement: "5".to_owned()
                }),
                Some(Pattern::simple("1")),
            )
        );

        // Sed action
        let m_genre = make_matcher(&["genre"], Pattern::simple("haha"));
        assert_eq!(
            Action::parse("sed:lalala:hahaha", Some(1), Some(&m_genre)).unwrap(),
            make_action(
                &["genre"],
                ActionBehavior::Sed(SedAction {
                    src: Regex::new("lalala").unwrap(),
                    dst: "hahaha".to_owned()
                }),
                Some(Pattern::simple("haha")),
            )
        );

        // Split action with escaped colon
        assert_eq!(
            Action::parse(r"split:::", Some(1), Some(&m_genre)).unwrap(),
            make_action(
                &["genre"],
                ActionBehavior::Split(SplitAction {
                    delimiter: ":".to_owned()
                }),
                Some(Pattern::simple("haha")),
            )
        );

        // Add action
        assert_eq!(
            Action::parse(r"add:cute", Some(1), Some(&m_genre)).unwrap(),
            make_action(
                &["genre"],
                ActionBehavior::Add(AddAction {
                    value: "cute".to_owned()
                }),
                Some(Pattern::simple("haha")),
            )
        );

        // Delete action (with trailing colon as in Python test "delete:")
        assert_eq!(
            Action::parse(r"delete:", Some(1), Some(&m_genre)).unwrap(),
            make_action(
                &["genre"],
                ActionBehavior::Delete(DeleteAction),
                Some(Pattern::simple("haha")),
            )
        );
    }

    #[test]
    fn test_rule_parse_action_errors() {
        /// Build expected error string from components. This avoids manual space counting.
        fn err_str(rule: &str, index: usize, feedback: &str) -> String {
            format!(
                "Failed to parse action 1, invalid syntax:\n\n    {}\n    {}^\n    {}{}\n",
                rule,
                " ".repeat(index),
                " ".repeat(index),
                feedback,
            )
        }

        fn test_err(rule: &str, index: usize, feedback: &str, matcher: Option<&Matcher>) {
            let err = Action::parse(rule, Some(1), matcher).unwrap_err();
            assert_eq!(err.to_string(), err_str(rule, index, feedback));
        }

        let m_genre = make_matcher(&["genre"], Pattern::simple("haha"));

        let modifiable_tags_str = "tracktitle, trackartist[main], trackartist[guest], trackartist[remixer], trackartist[producer], trackartist[composer], trackartist[conductor], trackartist[djmixer], trackartist, tracknumber, discnumber, releasetitle, releaseartist[main], releaseartist[guest], releaseartist[remixer], releaseartist[producer], releaseartist[composer], releaseartist[conductor], releaseartist[djmixer], releaseartist, releasetype, releasedate, originaldate, compositiondate, edition, catalognumber, genre, secondarygenre, descriptor, label, new, favorite, rating, artist";

        test_err(
            "tracktitle:hello/:delete",
            17,
            "Invalid action kind: must be one of {replace, sed, split, add, delete}.",
            None,
        );

        test_err(
            "haha/delete",
            0,
            &format!("Invalid tag: must be one of {{{}}}. The next character after a tag must be ':' or ','.", modifiable_tags_str),
            None,
        );

        test_err(
            "tracktitler/delete",
            0,
            &format!("Invalid tag: must be one of {{{}}}. The next character after a tag must be ':' or ','.", modifiable_tags_str),
            None,
        );

        test_err(
            "tracktitle:haha:delete",
            0,
            "Invalid action kind: must be one of {replace, sed, split, add, delete}. If this is pointing at your pattern, you forgot to put a `/` between the matcher section and the action section.",
            Some(&m_genre),
        );

        test_err(
            "tracktitle:haha:sed/hi:bye",
            16,
            "Unrecognized flag: Either you forgot a colon here (to end the matcher), or this is an invalid matcher flag. The only supported flag is `i` (case insensitive).",
            None,
        );

        test_err(
            "hahaha",
            0,
            "Invalid action kind: must be one of {replace, sed, split, add, delete}.",
            Some(&m_genre),
        );

        test_err(
            "replace",
            7,
            "Replacement not found: must specify a non-empty replacement. Use the delete action to remove a value.",
            Some(&m_genre),
        );

        test_err(
            "replace:haha:",
            12,
            "Found another section after the replacement, but the replacement must be the last section. Perhaps you meant to escape this colon?",
            Some(&m_genre),
        );

        test_err(
            "sed",
            3,
            "Empty sed pattern found: must specify a non-empty pattern. Example: sed:pattern:replacement",
            Some(&m_genre),
        );

        test_err(
            "sed:hihi",
            8,
            "Sed replacement not found: must specify a sed replacement section. Example: sed:hihi:replacement.",
            Some(&m_genre),
        );

        // Note: Rust regex error messages differ from Python's. We check the prefix.
        let err = Action::parse("sed:invalid[", Some(1), Some(&m_genre)).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Failed to compile the sed pattern regex"),
            "Expected regex compile error, got: {}",
            msg
        );

        test_err(
            "sed:hihi:byebye:",
            15,
            "Found another section after the sed replacement, but the sed replacement must be the last section. Perhaps you meant to escape this colon?",
            Some(&m_genre),
        );

        test_err(
            "split",
            5,
            "Delimiter not found: must specify a non-empty delimiter to split on.",
            Some(&m_genre),
        );

        test_err(
            "split:hi:",
            8,
            "Found another section after the delimiter, but the delimiter must be the last section. Perhaps you meant to escape this colon?",
            Some(&m_genre),
        );

        test_err(
            "split:",
            6,
            "Delimiter not found: must specify a non-empty delimiter to split on.",
            Some(&m_genre),
        );

        test_err(
            "add",
            3,
            "Value not found: must specify a non-empty value to add.",
            Some(&m_genre),
        );

        test_err(
            "add:hi:",
            6,
            "Found another section after the value, but the value must be the last section. Perhaps you meant to escape this colon?",
            Some(&m_genre),
        );

        test_err(
            "add:",
            4,
            "Value not found: must specify a non-empty value to add.",
            Some(&m_genre),
        );

        test_err(
            "delete:h",
            7,
            "Found another section after the action kind, but the delete action has no parameters. Please remove this section.",
            Some(&m_genre),
        );

        // delete with no matcher
        test_err(
            "delete",
            0,
            "Tags/pattern section not found. Must specify tags to modify, since there is no matcher to default to. Make sure you are formatting your action like {tags}:{pattern}/{kind}:{args} (where `:{pattern}` is optional)",
            None,
        );

        // tracktotal is not modifiable
        test_err(
            "tracktotal/replace:1",
            0,
            "Invalid tag: tracktotal is not modifiable.",
            None,
        );

        test_err(
            "disctotal/replace:1",
            0,
            "Invalid tag: disctotal is not modifiable.",
            None,
        );
    }

    // -----------------------------------------------------------------------
    // test_rule_parsing_end_to_end (from Python parametrized tests)
    // -----------------------------------------------------------------------

    #[test]
    fn test_rule_parsing_end_to_end_1() {
        let rule = Rule::parse("tracktitle:Track", &["delete"], None).unwrap();
        assert_eq!(
            rule.to_string(),
            "matcher=tracktitle:Track action=tracktitle:Track/delete"
        );
    }

    #[test]
    fn test_rule_parsing_end_to_end_2() {
        // Escaped ^ and $ in matcher: these need quoting in the display.
        for (matcher, action) in &[
            (r"tracktitle:\^Track", "delete"),
            (r"tracktitle:Track\$", "delete"),
            (r"tracktitle:\^Track\$", "delete"),
        ] {
            let rule = Rule::parse(matcher, &[action], None).unwrap();
            let expected = format!("matcher='{}' action='{}/{}'", matcher, matcher, action);
            assert_eq!(rule.to_string(), expected);
        }
    }

    #[test]
    fn test_rule_parsing_end_to_end_3() {
        for (matcher, action) in &[
            ("tracktitle:Track", "genre:lala/replace:lalala"),
            (
                "tracktitle,genre,trackartist:Track",
                "tracktitle,genre,artist/delete",
            ),
        ] {
            let rule = Rule::parse(matcher, &[action], None).unwrap();
            assert_eq!(
                rule.to_string(),
                format!("matcher={} action={}", matcher, action)
            );
        }
    }

    // -----------------------------------------------------------------------
    // test_rule_parsing_multi_value_validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_rule_parsing_multi_value_validation() {
        let err = Rule::parse("tracktitle:h", &["split:x"], None).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Single valued tags tracktitle cannot be modified by multi-value action split"
        );

        let err = Rule::parse("genre:h", &["tracktitle/split:x"], None).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Single valued tags tracktitle cannot be modified by multi-value action split"
        );

        // Verify the error is raised for the second action in a list.
        let err = Rule::parse("genre:h", &["split:y", "tracktitle/split:x"], None).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Single valued tags tracktitle cannot be modified by multi-value action split"
        );
    }

    // -----------------------------------------------------------------------
    // test_rule_parsing_defaults
    // -----------------------------------------------------------------------

    #[test]
    fn test_rule_parsing_defaults() {
        let rule = Rule::parse("tracktitle:Track", &["replace:hi"], None).unwrap();
        assert!(rule.actions[0].pattern.is_some());
        assert_eq!(rule.actions[0].pattern.as_ref().unwrap().needle, "Track");

        let rule = Rule::parse("tracktitle:Track", &["tracktitle/replace:hi"], None).unwrap();
        assert!(rule.actions[0].pattern.is_some());
        assert_eq!(rule.actions[0].pattern.as_ref().unwrap().needle, "Track");

        let rule = Rule::parse("tracktitle:Track", &["tracktitle:Lack/replace:hi"], None).unwrap();
        assert!(rule.actions[0].pattern.is_some());
        assert_eq!(rule.actions[0].pattern.as_ref().unwrap().needle, "Lack");
    }

    // -----------------------------------------------------------------------
    // test_parser_take
    // -----------------------------------------------------------------------

    #[test]
    fn test_parser_take() {
        assert_eq!(take("hello", ':'), ("hello".to_owned(), 5));
        assert_eq!(take("hello:hi", ':'), ("hello".to_owned(), 6));
        assert_eq!(take(r"h::lo:hi", ':'), ("h:lo".to_owned(), 6));
        assert_eq!(take(r"h:://lo:hi", ':'), ("h:/lo".to_owned(), 8));
        assert_eq!(take(r"h::lo/hi", '/'), ("h:lo".to_owned(), 6));
        assert_eq!(take(r"h:://lo/hi", '/'), ("h:/lo".to_owned(), 8));
    }

    // -----------------------------------------------------------------------
    // Display roundtrip tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_display_roundtrip_matcher() {
        let cases = [
            "tracktitle:Track",
            "tracktitle,tracknumber:Track",
            "tracktitle:Track:i",
            "tracktitle:^Track",
            "tracktitle:Track$",
            "tracktitle:",
        ];
        for case in &cases {
            let parsed = Matcher::parse(case).unwrap();
            let displayed = parsed.to_string();
            let reparsed = Matcher::parse(&displayed).unwrap();
            assert_eq!(parsed, reparsed, "roundtrip failed for {}", case);
        }
    }

    #[test]
    fn test_display_roundtrip_action() {
        let m = make_matcher(&["genre"], Pattern::simple("haha"));
        let cases = [
            "genre/replace:lalala",
            "genre:lala/replace:lalala",
            "genre:lala:i/replace:lalala",
        ];
        for case in &cases {
            let parsed = Action::parse(case, Some(1), Some(&m)).unwrap();
            let displayed = parsed.to_string();
            let reparsed = Action::parse(&displayed, Some(1), Some(&m)).unwrap();
            assert_eq!(parsed, reparsed, "roundtrip failed for {}", case);
        }
    }

    // -----------------------------------------------------------------------
    // Proptest: arbitrary strings should never panic
    // -----------------------------------------------------------------------

    #[test]
    fn test_proptest_matcher_no_panic() {
        // Fuzz-like test: a selection of adversarial inputs should return Err, not panic.
        let adversarial = [
            "",
            ":",
            ":::",
            "/",
            "///",
            "a",
            "tracktitle",
            "tracktitle:",
            "tracktitle::",
            "tracktitle:::",
            "tracktitle:foo:bar:baz",
            "tracktitle:foo:bar:baz:qux",
            "unknown:tag",
            "\0\0\0",
            "tracktitle:hello\x00world",
            &"x".repeat(10000),
        ];
        for input in &adversarial {
            let _ = Matcher::parse(input);
        }
    }

    #[test]
    fn test_proptest_action_no_panic() {
        let m = make_matcher(&["genre"], Pattern::simple("haha"));
        let adversarial = [
            "",
            ":",
            ":::",
            "/",
            "///",
            "a",
            "replace",
            "replace:",
            "replace::",
            "sed",
            "sed:",
            "sed::",
            "sed:::",
            "split",
            "split:",
            "add",
            "add:",
            "delete",
            "delete:",
            "delete:hi",
            "genre/replace",
            "genre/replace:",
            "genre/",
            "/delete",
            "genre:lala/",
            "unknown/delete",
            "\0\0\0",
            &"x".repeat(10000),
        ];
        for input in &adversarial {
            let _ = Action::parse(input, Some(1), Some(&m));
            let _ = Action::parse(input, None, None);
        }
    }

    // -----------------------------------------------------------------------
    // Escape / stringify_tags
    // -----------------------------------------------------------------------

    #[test]
    fn test_escape() {
        assert_eq!(escape("hello"), "hello");
        assert_eq!(escape("a:b"), "a::b");
        assert_eq!(escape("a/b"), "a//b");
        assert_eq!(escape("a:b/c"), "a::b//c");
    }

    #[test]
    fn test_stringify_tags() {
        assert_eq!(stringify_tags(&["tracktitle"]), "tracktitle");
        assert_eq!(stringify_tags(&["tracktitle", "genre"]), "tracktitle,genre");

        // Collapse trackartist alias.
        let ta: Vec<Tag> = resolve_tag("trackartist").unwrap().to_vec();
        assert_eq!(stringify_tags(&ta), "trackartist");

        // Collapse releaseartist alias.
        let ra: Vec<Tag> = resolve_tag("releaseartist").unwrap().to_vec();
        assert_eq!(stringify_tags(&ra), "releaseartist");

        // Collapse artist alias.
        let a: Vec<Tag> = resolve_tag("artist").unwrap().to_vec();
        assert_eq!(stringify_tags(&a), "artist");
    }

    // -----------------------------------------------------------------------
    // Proptest: arbitrary strings passed to parse never panic
    // -----------------------------------------------------------------------

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn matcher_parse_never_panics(input in "\\PC{0,200}") {
                let _ = Matcher::parse(&input);
            }

            #[test]
            fn action_parse_never_panics_no_matcher(input in "\\PC{0,200}") {
                let _ = Action::parse(&input, None, None);
            }

            #[test]
            fn action_parse_never_panics_with_matcher(input in "\\PC{0,200}") {
                let m = Matcher::from_expandable(
                    &["genre"],
                    Pattern::simple("haha"),
                );
                let _ = Action::parse(&input, Some(1), Some(&m));
            }

            #[test]
            fn take_never_panics_colon(input in "\\PC{0,200}") {
                let _ = take(&input, ':');
            }

            #[test]
            fn take_never_panics_slash(input in "\\PC{0,200}") {
                let _ = take(&input, '/');
            }
        }
    }
}
