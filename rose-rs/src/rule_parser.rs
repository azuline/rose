use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

use crate::common::uniq;
use crate::errors::RoseExpectedError;

/// The rule_parser module provides a parser for the rules engine's DSL.
///
/// This is split out from the rules engine in order to avoid a dependency cycle between the config
/// module and the rules module.

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub struct InvalidRuleError(String);

impl fmt::Display for InvalidRuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<InvalidRuleError> for RoseExpectedError {
    fn from(err: InvalidRuleError) -> Self {
        RoseExpectedError::InvalidRule(err.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub struct RuleSyntaxError {
    rule_name: String,
    rule: String,
    index: usize,
    feedback: String,
}

impl fmt::Display for RuleSyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Failed to parse {}, invalid syntax:\n\n    {}\n    {}^\n    {}{}",
            self.rule_name,
            self.rule,
            " ".repeat(self.index),
            " ".repeat(self.index),
            self.feedback
        )
    }
}

impl From<RuleSyntaxError> for InvalidRuleError {
    fn from(err: RuleSyntaxError) -> Self {
        InvalidRuleError(err.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tag {
    #[serde(rename = "tracktitle")]
    TrackTitle,
    #[serde(rename = "trackartist[main]")]
    TrackArtistMain,
    #[serde(rename = "trackartist[guest]")]
    TrackArtistGuest,
    #[serde(rename = "trackartist[remixer]")]
    TrackArtistRemixer,
    #[serde(rename = "trackartist[producer]")]
    TrackArtistProducer,
    #[serde(rename = "trackartist[composer]")]
    TrackArtistComposer,
    #[serde(rename = "trackartist[conductor]")]
    TrackArtistConductor,
    #[serde(rename = "trackartist[djmixer]")]
    TrackArtistDjMixer,
    #[serde(rename = "tracknumber")]
    TrackNumber,
    #[serde(rename = "tracktotal")]
    TrackTotal,
    #[serde(rename = "discnumber")]
    DiscNumber,
    #[serde(rename = "disctotal")]
    DiscTotal,
    #[serde(rename = "releasetitle")]
    ReleaseTitle,
    #[serde(rename = "releaseartist[main]")]
    ReleaseArtistMain,
    #[serde(rename = "releaseartist[guest]")]
    ReleaseArtistGuest,
    #[serde(rename = "releaseartist[remixer]")]
    ReleaseArtistRemixer,
    #[serde(rename = "releaseartist[producer]")]
    ReleaseArtistProducer,
    #[serde(rename = "releaseartist[composer]")]
    ReleaseArtistComposer,
    #[serde(rename = "releaseartist[conductor]")]
    ReleaseArtistConductor,
    #[serde(rename = "releaseartist[djmixer]")]
    ReleaseArtistDjMixer,
    #[serde(rename = "releasetype")]
    ReleaseType,
    #[serde(rename = "releasedate")]
    ReleaseDate,
    #[serde(rename = "originaldate")]
    OriginalDate,
    #[serde(rename = "compositiondate")]
    CompositionDate,
    #[serde(rename = "catalognumber")]
    CatalogNumber,
    #[serde(rename = "edition")]
    Edition,
    #[serde(rename = "genre")]
    Genre,
    #[serde(rename = "secondarygenre")]
    SecondaryGenre,
    #[serde(rename = "descriptor")]
    Descriptor,
    #[serde(rename = "label")]
    Label,
    #[serde(rename = "new")]
    New,
}

impl Tag {
    fn as_str(&self) -> &'static str {
        match self {
            Tag::TrackTitle => "tracktitle",
            Tag::TrackArtistMain => "trackartist[main]",
            Tag::TrackArtistGuest => "trackartist[guest]",
            Tag::TrackArtistRemixer => "trackartist[remixer]",
            Tag::TrackArtistProducer => "trackartist[producer]",
            Tag::TrackArtistComposer => "trackartist[composer]",
            Tag::TrackArtistConductor => "trackartist[conductor]",
            Tag::TrackArtistDjMixer => "trackartist[djmixer]",
            Tag::TrackNumber => "tracknumber",
            Tag::TrackTotal => "tracktotal",
            Tag::DiscNumber => "discnumber",
            Tag::DiscTotal => "disctotal",
            Tag::ReleaseTitle => "releasetitle",
            Tag::ReleaseArtistMain => "releaseartist[main]",
            Tag::ReleaseArtistGuest => "releaseartist[guest]",
            Tag::ReleaseArtistRemixer => "releaseartist[remixer]",
            Tag::ReleaseArtistProducer => "releaseartist[producer]",
            Tag::ReleaseArtistComposer => "releaseartist[composer]",
            Tag::ReleaseArtistConductor => "releaseartist[conductor]",
            Tag::ReleaseArtistDjMixer => "releaseartist[djmixer]",
            Tag::ReleaseType => "releasetype",
            Tag::ReleaseDate => "releasedate",
            Tag::OriginalDate => "originaldate",
            Tag::CompositionDate => "compositiondate",
            Tag::CatalogNumber => "catalognumber",
            Tag::Edition => "edition",
            Tag::Genre => "genre",
            Tag::SecondaryGenre => "secondarygenre",
            Tag::Descriptor => "descriptor",
            Tag::Label => "label",
            Tag::New => "new",
        }
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExpandableTag {
    Tag(Tag),
    Artist,
    TrackArtist,
    ReleaseArtist,
}

impl ExpandableTag {
    fn as_str(&self) -> &str {
        match self {
            ExpandableTag::Tag(tag) => tag.as_str(),
            ExpandableTag::Artist => "artist",
            ExpandableTag::TrackArtist => "trackartist",
            ExpandableTag::ReleaseArtist => "releaseartist",
        }
    }
}

impl fmt::Display for ExpandableTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

lazy_static::lazy_static! {
    /// Map of a tag to its "resolved" tags. Most tags simply resolve to themselves; however, we let
    /// certain tags be aliases for multiple other tags, purely for convenience.
    pub static ref ALL_TAGS: HashMap<ExpandableTag, Vec<Tag>> = {
        let mut map = HashMap::new();

        // Single tags that resolve to themselves
        map.insert(ExpandableTag::Tag(Tag::TrackTitle), vec![Tag::TrackTitle]);
        map.insert(ExpandableTag::Tag(Tag::TrackNumber), vec![Tag::TrackNumber]);
        map.insert(ExpandableTag::Tag(Tag::TrackTotal), vec![Tag::TrackTotal]);
        map.insert(ExpandableTag::Tag(Tag::DiscNumber), vec![Tag::DiscNumber]);
        map.insert(ExpandableTag::Tag(Tag::DiscTotal), vec![Tag::DiscTotal]);
        map.insert(ExpandableTag::Tag(Tag::ReleaseTitle), vec![Tag::ReleaseTitle]);
        map.insert(ExpandableTag::Tag(Tag::ReleaseType), vec![Tag::ReleaseType]);
        map.insert(ExpandableTag::Tag(Tag::ReleaseDate), vec![Tag::ReleaseDate]);
        map.insert(ExpandableTag::Tag(Tag::OriginalDate), vec![Tag::OriginalDate]);
        map.insert(ExpandableTag::Tag(Tag::CompositionDate), vec![Tag::CompositionDate]);
        map.insert(ExpandableTag::Tag(Tag::CatalogNumber), vec![Tag::CatalogNumber]);
        map.insert(ExpandableTag::Tag(Tag::Edition), vec![Tag::Edition]);
        map.insert(ExpandableTag::Tag(Tag::Genre), vec![Tag::Genre]);
        map.insert(ExpandableTag::Tag(Tag::SecondaryGenre), vec![Tag::SecondaryGenre]);
        map.insert(ExpandableTag::Tag(Tag::Descriptor), vec![Tag::Descriptor]);
        map.insert(ExpandableTag::Tag(Tag::Label), vec![Tag::Label]);
        map.insert(ExpandableTag::Tag(Tag::New), vec![Tag::New]);

        // Track artist tags
        map.insert(ExpandableTag::Tag(Tag::TrackArtistMain), vec![Tag::TrackArtistMain]);
        map.insert(ExpandableTag::Tag(Tag::TrackArtistGuest), vec![Tag::TrackArtistGuest]);
        map.insert(ExpandableTag::Tag(Tag::TrackArtistRemixer), vec![Tag::TrackArtistRemixer]);
        map.insert(ExpandableTag::Tag(Tag::TrackArtistProducer), vec![Tag::TrackArtistProducer]);
        map.insert(ExpandableTag::Tag(Tag::TrackArtistComposer), vec![Tag::TrackArtistComposer]);
        map.insert(ExpandableTag::Tag(Tag::TrackArtistConductor), vec![Tag::TrackArtistConductor]);
        map.insert(ExpandableTag::Tag(Tag::TrackArtistDjMixer), vec![Tag::TrackArtistDjMixer]);

        // Release artist tags
        map.insert(ExpandableTag::Tag(Tag::ReleaseArtistMain), vec![Tag::ReleaseArtistMain]);
        map.insert(ExpandableTag::Tag(Tag::ReleaseArtistGuest), vec![Tag::ReleaseArtistGuest]);
        map.insert(ExpandableTag::Tag(Tag::ReleaseArtistRemixer), vec![Tag::ReleaseArtistRemixer]);
        map.insert(ExpandableTag::Tag(Tag::ReleaseArtistProducer), vec![Tag::ReleaseArtistProducer]);
        map.insert(ExpandableTag::Tag(Tag::ReleaseArtistComposer), vec![Tag::ReleaseArtistComposer]);
        map.insert(ExpandableTag::Tag(Tag::ReleaseArtistConductor), vec![Tag::ReleaseArtistConductor]);
        map.insert(ExpandableTag::Tag(Tag::ReleaseArtistDjMixer), vec![Tag::ReleaseArtistDjMixer]);

        // Expandable tags
        map.insert(ExpandableTag::TrackArtist, vec![
            Tag::TrackArtistMain,
            Tag::TrackArtistGuest,
            Tag::TrackArtistRemixer,
            Tag::TrackArtistProducer,
            Tag::TrackArtistComposer,
            Tag::TrackArtistConductor,
            Tag::TrackArtistDjMixer,
        ]);

        map.insert(ExpandableTag::ReleaseArtist, vec![
            Tag::ReleaseArtistMain,
            Tag::ReleaseArtistGuest,
            Tag::ReleaseArtistRemixer,
            Tag::ReleaseArtistProducer,
            Tag::ReleaseArtistComposer,
            Tag::ReleaseArtistConductor,
            Tag::ReleaseArtistDjMixer,
        ]);

        map.insert(ExpandableTag::Artist, vec![
            Tag::TrackArtistMain,
            Tag::TrackArtistGuest,
            Tag::TrackArtistRemixer,
            Tag::TrackArtistProducer,
            Tag::TrackArtistComposer,
            Tag::TrackArtistConductor,
            Tag::TrackArtistDjMixer,
            Tag::ReleaseArtistMain,
            Tag::ReleaseArtistGuest,
            Tag::ReleaseArtistRemixer,
            Tag::ReleaseArtistProducer,
            Tag::ReleaseArtistComposer,
            Tag::ReleaseArtistConductor,
            Tag::ReleaseArtistDjMixer,
        ]);

        map
    };
}

lazy_static::lazy_static! {
    static ref MODIFIABLE_TAGS: Vec<Tag> = vec![
        Tag::TrackTitle,
        Tag::TrackArtistMain,
        Tag::TrackArtistGuest,
        Tag::TrackArtistRemixer,
        Tag::TrackArtistProducer,
        Tag::TrackArtistComposer,
        Tag::TrackArtistConductor,
        Tag::TrackArtistDjMixer,
        Tag::TrackNumber,
        Tag::DiscNumber,
        Tag::ReleaseTitle,
        Tag::ReleaseArtistMain,
        Tag::ReleaseArtistGuest,
        Tag::ReleaseArtistRemixer,
        Tag::ReleaseArtistProducer,
        Tag::ReleaseArtistComposer,
        Tag::ReleaseArtistConductor,
        Tag::ReleaseArtistDjMixer,
        Tag::ReleaseType,
        Tag::ReleaseDate,
        Tag::OriginalDate,
        Tag::CompositionDate,
        Tag::Edition,
        Tag::CatalogNumber,
        Tag::Genre,
        Tag::SecondaryGenre,
        Tag::Descriptor,
        Tag::Label,
        Tag::New,
    ];
}

lazy_static::lazy_static! {
    static ref SINGLE_VALUE_TAGS: Vec<Tag> = vec![
        Tag::TrackTitle,
        Tag::TrackNumber,
        Tag::TrackTotal,
        Tag::DiscNumber,
        Tag::DiscTotal,
        Tag::ReleaseTitle,
        Tag::ReleaseType,
        Tag::ReleaseDate,
        Tag::OriginalDate,
        Tag::CompositionDate,
        Tag::Edition,
        Tag::CatalogNumber,
        Tag::New,
    ];
}

lazy_static::lazy_static! {
    static ref RELEASE_TAGS: Vec<Tag> = vec![
        Tag::ReleaseTitle,
        Tag::ReleaseArtistMain,
        Tag::ReleaseArtistGuest,
        Tag::ReleaseArtistRemixer,
        Tag::ReleaseArtistProducer,
        Tag::ReleaseArtistComposer,
        Tag::ReleaseArtistConductor,
        Tag::ReleaseArtistDjMixer,
        Tag::ReleaseType,
        Tag::ReleaseType,
        Tag::ReleaseDate,
        Tag::OriginalDate,
        Tag::CompositionDate,
        Tag::Edition,
        Tag::CatalogNumber,
        Tag::Genre,
        Tag::SecondaryGenre,
        Tag::Descriptor,
        Tag::Label,
        Tag::DiscTotal,
        Tag::New,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplaceAction {
    /// Replaces the matched tag with `replacement`. For multi-valued tags, `;` is treated as a
    /// delimiter between multiple replacement values.
    pub replacement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SedAction {
    /// Executes a regex substitution on a tag value.
    #[serde(with = "serde_regex")]
    pub src: Regex,
    pub dst: String,
}

impl PartialEq for SedAction {
    fn eq(&self, other: &Self) -> bool {
        self.src.as_str() == other.src.as_str() && self.dst == other.dst
    }
}

impl Eq for SedAction {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitAction {
    /// Splits a tag into multiple tags on the provided delimiter. This action is only allowed on
    /// multi-value tags.
    pub delimiter: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddAction {
    /// Adds a value to the tag. This action is only allowed on multi-value tags. If the value already
    /// exists, this action No-Ops.
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteAction;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pattern {
    // Substring match with support for `^$` strict start / strict end matching.
    pub needle: String,
    #[serde(default)]
    pub strict_start: bool,
    #[serde(default)]
    pub strict_end: bool,
    #[serde(default)]
    pub case_insensitive: bool,
}

impl Pattern {
    pub fn new(needle: String) -> Self {
        let mut pattern = Pattern {
            needle,
            strict_start: false,
            strict_end: false,
            case_insensitive: false,
        };

        // Parse ^ and $ from needle
        if !pattern.strict_start {
            if pattern.needle.starts_with("^") {
                pattern.strict_start = true;
                pattern.needle = pattern.needle[1..].to_string();
            } else if pattern.needle.starts_with(r"\^") {
                pattern.needle = pattern.needle[1..].to_string();
            }
        }

        if !pattern.strict_end {
            if pattern.needle.ends_with(r"\$") {
                pattern.needle = pattern.needle[..pattern.needle.len() - 2].to_string() + "$";
            } else if pattern.needle.ends_with("$") {
                pattern.strict_end = true;
                pattern.needle = pattern.needle[..pattern.needle.len() - 1].to_string();
            }
        }

        pattern
    }

    pub fn with_options(needle: String, strict: bool, strict_start: bool, strict_end: bool, case_insensitive: bool) -> Self {
        let mut pattern = Pattern {
            needle,
            strict_start: strict_start || strict,
            strict_end: strict_end || strict,
            case_insensitive,
        };

        // Parse ^ and $ from needle
        if !pattern.strict_start {
            if pattern.needle.starts_with("^") {
                pattern.strict_start = true;
                pattern.needle = pattern.needle[1..].to_string();
            } else if pattern.needle.starts_with(r"\^") {
                pattern.needle = pattern.needle[1..].to_string();
            }
        }

        if !pattern.strict_end {
            if pattern.needle.ends_with(r"\$") {
                pattern.needle = pattern.needle[..pattern.needle.len() - 2].to_string() + "$";
            } else if pattern.needle.ends_with("$") {
                pattern.strict_end = true;
                pattern.needle = pattern.needle[..pattern.needle.len() - 1].to_string();
            }
        }

        pattern
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut r = escape(&self.needle);

        if self.strict_start {
            r = format!("^{r}");
        } else if self.needle.starts_with("^") {
            r = format!(r"\{r}");
        }

        if self.strict_end {
            r = format!("{r}$");
        } else if self.needle.ends_with("$") {
            r = r[..r.len() - 1].to_string() + r"\$";
        }

        if self.case_insensitive {
            r = format!("{r}:i");
        }

        write!(f, "{r}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Matcher {
    /// Tags to test against the pattern. If any tags match the pattern, the action will be ran
    /// against the track.
    pub tags: Vec<Tag>,
    /// The pattern to test the tag against.
    pub pattern: Pattern,
}

impl Matcher {
    pub fn new(tags: Vec<ExpandableTag>, pattern: Pattern) -> Self {
        let mut expanded_tags = Vec::new();
        for t in tags {
            expanded_tags.extend(ALL_TAGS[&t].clone());
        }
        Matcher {
            tags: uniq(expanded_tags),
            pattern,
        }
    }

    pub fn parse(raw: &str) -> Result<Matcher, RuleSyntaxError> {
        Self::parse_with_name(raw, "matcher")
    }

    pub fn parse_with_name(raw: &str, rule_name: &str) -> Result<Matcher, RuleSyntaxError> {
        let mut idx = 0;
        let chars: Vec<char> = raw.chars().collect();

        // First, parse the tags.
        let mut tags = Vec::new();
        let mut found_colon = false;

        loop {
            let mut matched = false;

            // Try to match each tag
            for (expandable_tag, _) in ALL_TAGS.iter() {
                let tag_str = expandable_tag.as_str();
                let tag_chars: Vec<char> = tag_str.chars().collect();

                if idx + tag_chars.len() <= chars.len() {
                    let slice = &chars[idx..idx + tag_chars.len()];
                    if slice.iter().collect::<String>() == tag_str {
                        // Check next character is : or ,
                        if idx + tag_chars.len() < chars.len() {
                            let next_char = chars[idx + tag_chars.len()];
                            if next_char == ':' || next_char == ',' {
                                tags.push(*expandable_tag);
                                idx += tag_chars.len() + 1;
                                found_colon = next_char == ':';
                                matched = true;
                                break;
                            }
                        } else {
                            return Err(RuleSyntaxError {
                                rule_name: rule_name.to_string(),
                                rule: raw.to_string(),
                                index: idx + tag_chars.len(),
                                feedback: "Expected to find ',' or ':', found end of string.".to_string(),
                            });
                        }
                    }
                }
            }

            if !matched {
                let all_tags_str = vec![
                    "tracktitle",
                    "trackartist[main]",
                    "trackartist[guest]",
                    "trackartist[remixer]",
                    "trackartist[producer]",
                    "trackartist[composer]",
                    "trackartist[conductor]",
                    "trackartist[djmixer]",
                    "tracknumber",
                    "tracktotal",
                    "discnumber",
                    "disctotal",
                    "releasetitle",
                    "releaseartist[main]",
                    "releaseartist[guest]",
                    "releaseartist[remixer]",
                    "releaseartist[producer]",
                    "releaseartist[composer]",
                    "releaseartist[conductor]",
                    "releaseartist[djmixer]",
                    "releasetype",
                    "releasedate",
                    "originaldate",
                    "compositiondate",
                    "catalognumber",
                    "edition",
                    "genre",
                    "secondarygenre",
                    "descriptor",
                    "label",
                    "new",
                    "trackartist",
                    "releaseartist",
                    "artist",
                ]
                .join(", ");
                return Err(RuleSyntaxError {
                    rule_name: rule_name.to_string(),
                    rule: raw.to_string(),
                    index: idx,
                    feedback: format!("Invalid tag: must be one of {{{all_tags_str}}}. The next character after a tag must be ':' or ','."),
                });
            }

            if found_colon {
                break;
            }
        }

        // Then parse the pattern.
        let (pattern_str, fwd) = take(&raw[idx..], ":", false)?;
        idx += fwd;

        // If more input is remaining, it should be optional single-character flags.
        let mut case_insensitive = false;
        if idx < chars.len() && take(&raw[idx..], ":", true)?.0.is_empty() {
            idx += 1;
            let (flags, fwd) = take(&raw[idx..], ":", true)?;
            if flags.is_empty() {
                return Err(RuleSyntaxError {
                    rule_name: rule_name.to_string(),
                    rule: raw.to_string(),
                    index: idx,
                    feedback:
                        "No flags specified: Please remove this section (by deleting the colon) or specify one of the supported flags: `i` (case insensitive)."
                            .to_string(),
                });
            }
            for (i, flag) in flags.chars().enumerate() {
                if flag == 'i' {
                    case_insensitive = true;
                } else {
                    return Err(RuleSyntaxError {
                        rule_name: rule_name.to_string(),
                        rule: raw.to_string(),
                        index: idx + i,
                        feedback: "Unrecognized flag: Please specify one of the supported flags: `i` (case insensitive).".to_string(),
                    });
                }
            }
            idx += fwd;
        }

        if idx < chars.len() {
            return Err(RuleSyntaxError {
                rule_name: rule_name.to_string(),
                rule: raw.to_string(),
                index: idx,
                feedback: "Extra input found after end of matcher. Perhaps you meant to escape this colon?".to_string(),
            });
        }

        let pattern = if case_insensitive {
            Pattern::with_options(pattern_str, false, false, false, true)
        } else {
            Pattern::new(pattern_str)
        };
        let matcher = Matcher::new(tags, pattern);

        tracing::debug!("Parsed rule matcher raw={} as matcher={:?}", raw, matcher);
        Ok(matcher)
    }
}

impl fmt::Display for Matcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", stringify_tags(&self.tags), self.pattern)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "behavior", content = "params")]
pub enum ActionBehavior {
    Replace(ReplaceAction),
    Sed(SedAction),
    Split(SplitAction),
    Add(AddAction),
    Delete(DeleteAction),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    /// The tags to apply the action on. Defaults to the tag that the pattern matched.
    pub tags: Vec<Tag>,
    /// The behavior of the action, along with behavior-specific parameters.
    pub behavior: ActionBehavior,
    /// Only apply the action on values that match this pattern. None means that all values are acted
    /// upon.
    pub pattern: Option<Pattern>,
}

impl Action {
    pub fn new(tags: Vec<ExpandableTag>, behavior: ActionBehavior, pattern: Option<Pattern>) -> Self {
        let mut expanded_tags = Vec::new();
        for t in tags {
            expanded_tags.extend(ALL_TAGS[&t].clone());
        }
        Action {
            tags: uniq(expanded_tags),
            behavior,
            pattern,
        }
    }

    pub fn parse(raw: &str, action_number: Option<usize>, matcher: Option<&Matcher>) -> Result<Action, RuleSyntaxError> {
        let mut idx = 0;
        let chars: Vec<char> = raw.chars().collect();

        let rule_name = if let Some(num) = action_number {
            format!("action {num}")
        } else {
            "action".to_string()
        };

        // First, determine whether we have a matcher section or not.
        let (_, action_idx) = take(raw, "/", true)?;
        let has_tags_pattern_section = action_idx != raw.len();

        // Parse the (optional) tags+pattern section.
        let (tags, pattern) = if !has_tags_pattern_section {
            if let Some(m) = matcher {
                let tags: Vec<Tag> = m.tags.iter().filter(|t| MODIFIABLE_TAGS.contains(t)).cloned().collect();
                (tags, Some(m.pattern.clone()))
            } else {
                return Err(RuleSyntaxError {
                    rule_name,
                    rule: raw.to_string(),
                    index: idx,
                    feedback: "Tags/pattern section not found. Must specify tags to modify, since there is no matcher to default to. Make sure you are formatting your action like {tags}:{pattern}/{kind}:{args} (where `:{pattern}` is optional)".to_string(),
                });
            }
        } else {
            // First, parse the tags.
            let mut tags = Vec::new();

            if raw[idx..].starts_with("matched:") {
                if let Some(m) = matcher {
                    idx += "matched:".len();
                    tags = m.tags.iter().filter(|t| MODIFIABLE_TAGS.contains(t)).cloned().collect();
                } else {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_string(),
                        index: idx,
                        feedback: "Cannot use `matched` in this context: there is no matcher to default to.".to_string(),
                    });
                }
            } else {
                let mut found_end = false;
                loop {
                    let mut matched = false;

                    // Try to match each tag
                    for (expandable_tag, resolved) in ALL_TAGS.iter() {
                        let tag_str = expandable_tag.as_str();
                        let tag_chars: Vec<char> = tag_str.chars().collect();

                        if idx + tag_chars.len() <= chars.len() {
                            let slice = &chars[idx..idx + tag_chars.len()];
                            if slice.iter().collect::<String>() == tag_str {
                                // Check next character
                                if idx + tag_chars.len() < chars.len() {
                                    let next_char = chars[idx + tag_chars.len()];
                                    if next_char == ':' || next_char == ',' || next_char == '/' {
                                        // Check if all resolved tags are modifiable
                                        for resolved_tag in resolved {
                                            if !MODIFIABLE_TAGS.contains(resolved_tag) {
                                                return Err(RuleSyntaxError {
                                                    rule_name,
                                                    rule: raw.to_string(),
                                                    index: idx,
                                                    feedback: format!("Invalid tag: {tag_str} is not modifiable."),
                                                });
                                            }
                                            tags.push(*resolved_tag);
                                        }
                                        idx += tag_chars.len() + 1;
                                        found_end = next_char == ':' || next_char == '/';
                                        matched = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    if !matched {
                        let modifiable_tags_str = vec![
                            "tracktitle",
                            "trackartist[main]",
                            "trackartist[guest]",
                            "trackartist[remixer]",
                            "trackartist[producer]",
                            "trackartist[composer]",
                            "trackartist[conductor]",
                            "trackartist[djmixer]",
                            "tracknumber",
                            "discnumber",
                            "releasetitle",
                            "releaseartist[main]",
                            "releaseartist[guest]",
                            "releaseartist[remixer]",
                            "releaseartist[producer]",
                            "releaseartist[composer]",
                            "releaseartist[conductor]",
                            "releaseartist[djmixer]",
                            "releasetype",
                            "releasedate",
                            "originaldate",
                            "compositiondate",
                            "catalognumber",
                            "edition",
                            "genre",
                            "secondarygenre",
                            "descriptor",
                            "label",
                            "new",
                            "trackartist",
                            "releaseartist",
                            "artist",
                        ]
                        .join(", ");
                        let feedback = if matcher.is_some() {
                            format!("Invalid tag: must be one of matched, {{{modifiable_tags_str}}}. (And if the value is matched, it must be alone.) The next character after a tag must be ':' or ','.")
                        } else {
                            format!("Invalid tag: must be one of {{{modifiable_tags_str}}}. The next character after a tag must be ':' or ','.")
                        };
                        return Err(RuleSyntaxError {
                            rule_name,
                            rule: raw.to_string(),
                            index: idx,
                            feedback,
                        });
                    }

                    if found_end {
                        break;
                    }
                }
            }

            // Parse the optional pattern.
            let pattern = if idx > 0 && chars[idx - 1] == '/' {
                // Explicitly empty pattern or inherit
                if let Some(m) = matcher {
                    if tags == m.tags {
                        Some(m.pattern.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else if idx < chars.len() && chars[idx] == '/' {
                idx += 1;
                None
            } else {
                // Parse pattern
                let (colon_pattern, colon_fwd) = take(&raw[idx..], ":", true)?;
                let (slash_pattern, slash_fwd) = take(&raw[idx..], "/", true)?;

                let (needle, fwd, has_flags) = if colon_fwd < slash_fwd {
                    (colon_pattern, colon_fwd, true)
                } else {
                    (slash_pattern, slash_fwd, false)
                };
                idx += fwd;

                if !needle.is_empty() {
                    let mut case_insensitive = false;
                    if has_flags {
                        let (flags, fwd) = take(&raw[idx..], "/", true)?;
                        if flags.is_empty() {
                            return Err(RuleSyntaxError {
                                rule_name,
                                rule: raw.to_string(),
                                index: idx,
                                feedback: "No flags specified: Please remove this section (by deleting the colon) or specify one of the supported flags: `i` (case insensitive).".to_string(),
                            });
                        }
                        for (i, flag) in flags.chars().enumerate() {
                            if flag == 'i' {
                                case_insensitive = true;
                            } else {
                                return Err(RuleSyntaxError {
                                    rule_name,
                                    rule: raw.to_string(),
                                    index: idx + i,
                                    feedback: "Unrecognized flag: Either you forgot a colon here (to end the matcher), or this is an invalid matcher flag. The only supported flag is `i` (case insensitive).".to_string(),
                                });
                            }
                        }
                        idx += fwd;
                    }

                    if case_insensitive {
                        Some(Pattern::with_options(needle, false, false, false, true))
                    } else {
                        Some(Pattern::new(needle))
                    }
                } else {
                    None
                }
            };

            (tags, pattern)
        };

        // Parse the action kind.
        let valid_actions = ["replace", "sed", "split", "add", "delete"];
        let mut action_kind = None;

        for va in &valid_actions {
            if raw[idx..].starts_with(&format!("{va}:")) {
                action_kind = Some(*va);
                idx += va.len() + 1;
                break;
            } else if raw.get(idx..) == Some(va) {
                action_kind = Some(*va);
                idx += va.len();
                break;
            }
        }

        let action_kind = action_kind.ok_or_else(|| {
            let mut feedback = format!("Invalid action kind: must be one of {{{}}}.", valid_actions.join(", "));
            if idx == 0 && raw.contains(':') {
                feedback += " If this is pointing at your pattern, you forgot to put a `/` between the matcher section and the action section.";
            }
            RuleSyntaxError {
                rule_name: rule_name.clone(),
                rule: raw.to_string(),
                index: idx,
                feedback,
            }
        })?;

        // Validate that the action type is supported for the given tags.
        if action_kind == "split" || action_kind == "add" {
            let single_valued_tags: Vec<_> = tags.iter().filter(|t| SINGLE_VALUE_TAGS.contains(t)).map(|t| t.as_str()).collect();
            if !single_valued_tags.is_empty() {
                return Err(RuleSyntaxError {
                    rule_name: rule_name.clone(),
                    rule: raw.to_string(),
                    index: 0,
                    feedback: format!(
                        "Invalid rule: Single valued tags {} cannot be modified by multi-value action {}",
                        single_valued_tags.join(", "),
                        action_kind
                    ),
                });
            }
        }

        // Parse each action kind.
        let behavior = match action_kind {
            "replace" => {
                let (replacement, fwd) = take(&raw[idx..], ":", false)?;
                idx += fwd;
                if replacement.is_empty() {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_string(),
                        index: idx,
                        feedback: "Replacement not found: must specify a non-empty replacement. Use the delete action to remove a value.".to_string(),
                    });
                }
                if idx < chars.len() {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_string(),
                        index: idx,
                        feedback:
                            "Found another section after the replacement, but the replacement must be the last section. Perhaps you meant to escape this colon?"
                                .to_string(),
                    });
                }
                ActionBehavior::Replace(ReplaceAction { replacement })
            }
            "sed" => {
                let (src_str, fwd) = take(&raw[idx..], ":", false)?;
                if src_str.is_empty() {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_string(),
                        index: idx,
                        feedback: format!("Empty sed pattern found: must specify a non-empty pattern. Example: {raw}:pattern:replacement"),
                    });
                }
                let src = Regex::new(&src_str).map_err(|e| {
                    let err_msg = e.to_string();
                    // Extract just the error description without the full regex error format
                    let feedback = if err_msg.contains("unclosed character class") {
                        "Failed to compile the sed pattern regex: invalid pattern: unclosed character class".to_string()
                    } else {
                        format!("Failed to compile the sed pattern regex: invalid pattern: {e}")
                    };
                    RuleSyntaxError {
                        rule_name: rule_name.clone(),
                        rule: raw.to_string(),
                        index: idx,
                        feedback,
                    }
                })?;
                idx += fwd;

                if idx >= chars.len() || chars[idx] != ':' {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_string(),
                        index: idx,
                        feedback: format!("Sed replacement not found: must specify a sed replacement section. Example: {raw}:replacement."),
                    });
                }
                idx += 1;

                let (dst, fwd) = take(&raw[idx..], ":", false)?;
                idx += fwd;
                if idx < chars.len() {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_string(),
                        index: idx,
                        feedback: "Found another section after the sed replacement, but the sed replacement must be the last section. Perhaps you meant to escape this colon?".to_string(),
                    });
                }
                ActionBehavior::Sed(SedAction { src, dst })
            }
            "split" => {
                let (delimiter, fwd) = take(&raw[idx..], ":", false)?;
                idx += fwd;
                if delimiter.is_empty() {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_string(),
                        index: idx,
                        feedback: "Delimiter not found: must specify a non-empty delimiter to split on.".to_string(),
                    });
                }
                if idx < chars.len() {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_string(),
                        index: idx,
                        feedback:
                            "Found another section after the delimiter, but the delimiter must be the last section. Perhaps you meant to escape this colon?"
                                .to_string(),
                    });
                }
                ActionBehavior::Split(SplitAction { delimiter })
            }
            "add" => {
                let (value, fwd) = take(&raw[idx..], ":", false)?;
                idx += fwd;
                if value.is_empty() {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_string(),
                        index: idx,
                        feedback: "Value not found: must specify a non-empty value to add.".to_string(),
                    });
                }
                if idx < chars.len() {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_string(),
                        index: idx,
                        feedback: "Found another section after the value, but the value must be the last section. Perhaps you meant to escape this colon?"
                            .to_string(),
                    });
                }
                ActionBehavior::Add(AddAction { value })
            }
            "delete" => {
                if idx < chars.len() {
                    return Err(RuleSyntaxError {
                        rule_name,
                        rule: raw.to_string(),
                        index: idx,
                        feedback: "Found another section after the action kind, but the delete action has no parameters. Please remove this section."
                            .to_string(),
                    });
                }
                ActionBehavior::Delete(DeleteAction)
            }
            _ => unreachable!("unknown action_kind {}", action_kind),
        };

        let action = Action { behavior, tags, pattern };

        tracing::debug!("Parsed rule action raw={} matcher={:?} as action={:?}", raw, matcher, action);
        Ok(action)
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut result = String::new();
        result.push_str(&stringify_tags(&self.tags));
        if let Some(pattern) = &self.pattern {
            result.push(':');
            result.push_str(&pattern.to_string());
        }
        if !result.is_empty() {
            result.push('/');
        }

        match &self.behavior {
            ActionBehavior::Replace(r) => {
                result.push_str("replace:");
                result.push_str(&r.replacement);
            }
            ActionBehavior::Sed(s) => {
                result.push_str("sed:");
                result.push_str(&escape(s.src.as_str()));
                result.push(':');
                result.push_str(&escape(&s.dst));
            }
            ActionBehavior::Split(s) => {
                result.push_str("split:");
                result.push_str(&s.delimiter);
            }
            ActionBehavior::Add(a) => {
                result.push_str("add:");
                result.push_str(&a.value);
            }
            ActionBehavior::Delete(_) => {
                result.push_str("delete");
            }
        }

        write!(f, "{result}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub matcher: Matcher,
    pub actions: Vec<Action>,
    pub ignore: Vec<Matcher>,
}

impl Rule {
    pub fn parse(matcher: &str, actions: Vec<String>, ignore: Option<Vec<String>>) -> Result<Rule, RuleSyntaxError> {
        let parsed_matcher = Matcher::parse(matcher)?;
        let parsed_actions =
            actions.into_iter().enumerate().map(|(i, a)| Action::parse(&a, Some(i + 1), Some(&parsed_matcher))).collect::<Result<Vec<_>, _>>()?;
        let parsed_ignore = ignore.unwrap_or_default().into_iter().map(|v| Matcher::parse_with_name(&v, "ignore")).collect::<Result<Vec<_>, _>>()?;

        Ok(Rule {
            matcher: parsed_matcher,
            actions: parsed_actions,
            ignore: parsed_ignore,
        })
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        parts.push(format!("matcher={}", shell_escape(&self.matcher.to_string())));
        for action in &self.actions {
            parts.push(format!("action={}", shell_escape(&action.to_string())));
        }
        write!(f, "{}", parts.join(" "))
    }
}

/// Reads until the next unescaped `until` or end of string is found. Returns the read string and
/// the number of characters consumed from the input. `until` is counted (in the returned int) as
/// consumed if `consume_until` is true, though it is never included in the returned string.
///
/// The returned string is unescaped; that is, `//` become `/` and `::` become `:`.
fn take(x: &str, until: &str, consume_until: bool) -> Result<(String, usize), RuleSyntaxError> {
    let mut result = String::new();
    let mut fwd = 0;

    loop {
        let (match_str, fwd_) = take_escaped(&x[fwd..], until, consume_until);
        result.push_str(&match_str.replace("::", ":").replace("//", "/"));
        fwd += fwd_;

        let next_idx = fwd + if consume_until { 0 } else { 1 };
        let escaped_special_char = next_idx < x.len() && x[next_idx..].starts_with(until);
        if !escaped_special_char {
            break;
        }
        result.push_str(until);
        fwd = next_idx + 1;
    }

    Ok((result, fwd))
}

/// DO NOT USE THIS FUNCTION DIRECTLY. USE take.
fn take_escaped(x: &str, until: &str, consume_until: bool) -> (String, usize) {
    let mut result = String::new();
    let mut escaped: Option<char> = None;
    let mut seen_idx = 0;
    let chars: Vec<char> = x.chars().collect();

    for i in 0..chars.len() {
        if x[i..].starts_with(until) {
            if consume_until {
                seen_idx += until.len();
            }
            break;
        }

        let c = chars[i];

        // We have a potential escape here. Store the escaped character to verify it in the next
        // iteration.
        if (c == ':' || c == '/') && escaped.is_none() {
            escaped = Some(c);
            seen_idx += 1;
            continue;
        }

        // If this is true, then nothing was actually escaped. Write the first character and the
        // second character to the output.
        if let Some(esc) = escaped {
            if c != esc {
                result.push(esc);
                escaped = None;
            }
        }

        result.push(c);
        seen_idx += 1;
    }

    (result, seen_idx)
}

/// Escape the special characters in a string.
fn escape(x: &str) -> String {
    x.replace(":", "::").replace("/", "//")
}

/// Basically a ",".join(tags), except we collapse aliases down to their shorthand form.
fn stringify_tags(tags_input: &[Tag]) -> String {
    let mut tags: Vec<String> = tags_input.iter().map(|t| t.to_string()).collect();

    // Check if all artist tags are present
    let artist_tags: Vec<String> = ALL_TAGS[&ExpandableTag::Artist].iter().map(|t| t.to_string()).collect();
    if artist_tags.iter().all(|t| tags.contains(t)) {
        let idx = tags.iter().position(|t| t == &artist_tags[0]).unwrap();
        tags.retain(|t| !artist_tags.contains(t));
        tags.insert(idx, "artist".to_string());
    }

    // Check if all trackartist tags are present
    let trackartist_tags: Vec<String> = ALL_TAGS[&ExpandableTag::TrackArtist].iter().map(|t| t.to_string()).collect();
    if trackartist_tags.iter().all(|t| tags.contains(t)) {
        let idx = tags.iter().position(|t| t == &trackartist_tags[0]).unwrap();
        tags.retain(|t| !trackartist_tags.contains(t));
        tags.insert(idx, "trackartist".to_string());
    }

    // Check if all releaseartist tags are present
    let releaseartist_tags: Vec<String> = ALL_TAGS[&ExpandableTag::ReleaseArtist].iter().map(|t| t.to_string()).collect();
    if releaseartist_tags.iter().all(|t| tags.contains(t)) {
        let idx = tags.iter().position(|t| t == &releaseartist_tags[0]).unwrap();
        tags.retain(|t| !releaseartist_tags.contains(t));
        tags.insert(idx, "releaseartist".to_string());
    }

    tags.join(",")
}

/// Shell escape a string similar to Python's shlex.quote
fn shell_escape(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }

    // Check if string needs escaping
    let needs_escape = s.chars().any(|c| {
        matches!(
            c,
            ' ' | '\t'
                | '\n'
                | '\r'
                | '\\'
                | '\''
                | '"'
                | '`'
                | '$'
                | '!'
                | '('
                | ')'
                | '{'
                | '}'
                | '['
                | ']'
                | '<'
                | '>'
                | '|'
                | '&'
                | ';'
                | '*'
                | '?'
                | '~'
                | '#'
        )
    });

    if needs_escape {
        format!("'{}'", s.replace("'", "'\\''"))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_new() {
        // Test Pattern::new behavior
        let p1 = Pattern::new("^Track".to_string());
        assert_eq!(p1.needle, "Track");
        assert!(p1.strict_start);

        let p2 = Pattern::new(r"\^Track".to_string());
        assert_eq!(p2.needle, "^Track");
        assert!(!p2.strict_start);

        let p3 = Pattern::new(r"Track$".to_string());
        assert_eq!(p3.needle, "Track");
        assert!(p3.strict_end);

        let p4 = Pattern::new(r"Track\$".to_string());
        assert_eq!(p4.needle, "Track$");
        assert!(!p4.strict_end);
    }

    #[test]
    fn test_rule_str() {
        let rule = Rule::parse("tracktitle:Track", vec!["releaseartist,genre/replace:lalala".to_string()], None).unwrap();
        assert_eq!(rule.to_string(), "matcher=tracktitle:Track action=releaseartist,genre/replace:lalala");

        // Test that rules are quoted properly.
        let rule = Rule::parse(r"tracktitle,releaseartist,genre::: ", vec![r"sed::::; ".to_string()], None).unwrap();
        assert_eq!(rule.to_string(), r"matcher='tracktitle,releaseartist,genre::: ' action='tracktitle,releaseartist,genre::: /sed::::; '");

        // Test that custom action matcher is printed properly.
        let rule = Rule::parse("tracktitle:Track", vec!["genre:lala/replace:lalala".to_string()], None).unwrap();
        assert_eq!(rule.to_string(), "matcher=tracktitle:Track action=genre:lala/replace:lalala");

        // Test that we print `matched` when action pattern is not null.
        let rule = Rule::parse("genre:b", vec!["genre:h/replace:hi".to_string()], None).unwrap();
        assert_eq!(rule.to_string(), r"matcher=genre:b action=genre:h/replace:hi");
    }

    #[test]
    fn test_rule_parse_matcher() {
        assert_eq!(Matcher::parse("tracktitle:Track").unwrap(), Matcher::new(vec![ExpandableTag::Tag(Tag::TrackTitle)], Pattern::new("Track".to_string())));
        assert_eq!(
            Matcher::parse("tracktitle,tracknumber:Track").unwrap(),
            Matcher::new(vec![ExpandableTag::Tag(Tag::TrackTitle), ExpandableTag::Tag(Tag::TrackNumber)], Pattern::new("Track".to_string()))
        );
        assert_eq!(
            Matcher::parse(r"tracktitle,tracknumber:Tr::ck").unwrap(),
            Matcher::new(vec![ExpandableTag::Tag(Tag::TrackTitle), ExpandableTag::Tag(Tag::TrackNumber)], Pattern::new("Tr:ck".to_string()))
        );
        assert_eq!(
            Matcher::parse("tracktitle,tracknumber:Track:i").unwrap(),
            Matcher::new(
                vec![ExpandableTag::Tag(Tag::TrackTitle), ExpandableTag::Tag(Tag::TrackNumber)],
                Pattern::with_options("Track".to_string(), false, false, false, true)
            )
        );
        assert_eq!(Matcher::parse(r"tracktitle:").unwrap(), Matcher::new(vec![ExpandableTag::Tag(Tag::TrackTitle)], Pattern::new("".to_string())));

        assert_eq!(
            Matcher::parse("tracktitle:^Track").unwrap(),
            Matcher::new(vec![ExpandableTag::Tag(Tag::TrackTitle)], Pattern::with_options("Track".to_string(), false, true, false, false))
        );
        assert_eq!(
            Matcher::parse("tracktitle:Track$").unwrap(),
            Matcher::new(vec![ExpandableTag::Tag(Tag::TrackTitle)], Pattern::with_options("Track".to_string(), false, false, true, false))
        );
        assert_eq!(
            Matcher::parse(r"tracktitle:\^Track").unwrap(),
            Matcher::new(vec![ExpandableTag::Tag(Tag::TrackTitle)], Pattern::new(r"\^Track".to_string()))
        );
        assert_eq!(
            Matcher::parse(r"tracktitle:Track\$").unwrap(),
            Matcher::new(vec![ExpandableTag::Tag(Tag::TrackTitle)], Pattern::new(r"Track\$".to_string()))
        );
        assert_eq!(
            Matcher::parse(r"tracktitle:\^Track\$").unwrap(),
            Matcher::new(vec![ExpandableTag::Tag(Tag::TrackTitle)], Pattern::new(r"\^Track\$".to_string()))
        );
    }

    #[test]
    fn test_rule_parse_matcher_errors() {
        fn test_err(rule: &str, expected_err: &str) {
            let err = Matcher::parse(rule).unwrap_err();
            assert_eq!(err.to_string(), expected_err);
        }

        test_err(
            "tracknumber^Track$",
            "Failed to parse matcher, invalid syntax:\n\n    tracknumber^Track$\n    ^\n    Invalid tag: must be one of {tracktitle, trackartist[main], trackartist[guest], trackartist[remixer], trackartist[producer], trackartist[composer], trackartist[conductor], trackartist[djmixer], tracknumber, tracktotal, discnumber, disctotal, releasetitle, releaseartist[main], releaseartist[guest], releaseartist[remixer], releaseartist[producer], releaseartist[composer], releaseartist[conductor], releaseartist[djmixer], releasetype, releasedate, originaldate, compositiondate, catalognumber, edition, genre, secondarygenre, descriptor, label, new, trackartist, releaseartist, artist}. The next character after a tag must be ':' or ','."
        );

        test_err(
            "tracknumber",
            "Failed to parse matcher, invalid syntax:\n\n    tracknumber\n               ^\n               Expected to find ',' or ':', found end of string.",
        );

        test_err(
            "tracktitle:Tr:ck",
            "Failed to parse matcher, invalid syntax:\n\n    tracktitle:Tr:ck\n                  ^\n                  Unrecognized flag: Please specify one of the supported flags: `i` (case insensitive)."
        );

        test_err(
            "tracktitle:hi:i:hihi",
            "Failed to parse matcher, invalid syntax:\n\n    tracktitle:hi:i:hihi\n                    ^\n                    Extra input found after end of matcher. Perhaps you meant to escape this colon?"
        );
    }

    #[test]
    fn test_rule_parse_action() {
        let matcher = Matcher::new(vec![ExpandableTag::Tag(Tag::TrackTitle)], Pattern::new("haha".to_string()));

        assert_eq!(
            Action::parse("replace:lalala", Some(1), Some(&matcher)).unwrap(),
            Action {
                behavior: ActionBehavior::Replace(ReplaceAction {
                    replacement: "lalala".to_string()
                }),
                tags: vec![Tag::TrackTitle],
                pattern: Some(Pattern::new("haha".to_string())),
            }
        );

        assert_eq!(
            Action::parse("genre/replace:lalala", None, None).unwrap(),
            Action {
                behavior: ActionBehavior::Replace(ReplaceAction {
                    replacement: "lalala".to_string()
                }),
                tags: vec![Tag::Genre],
                pattern: None,
            }
        );

        assert_eq!(
            Action::parse("tracknumber,genre/replace:lalala", None, None).unwrap(),
            Action {
                behavior: ActionBehavior::Replace(ReplaceAction {
                    replacement: "lalala".to_string()
                }),
                tags: vec![Tag::TrackNumber, Tag::Genre],
                pattern: None,
            }
        );

        assert_eq!(
            Action::parse("genre:lala/replace:lalala", None, None).unwrap(),
            Action {
                behavior: ActionBehavior::Replace(ReplaceAction {
                    replacement: "lalala".to_string()
                }),
                tags: vec![Tag::Genre],
                pattern: Some(Pattern::new("lala".to_string())),
            }
        );

        assert_eq!(
            Action::parse("genre:lala:i/replace:lalala", None, None).unwrap(),
            Action {
                behavior: ActionBehavior::Replace(ReplaceAction {
                    replacement: "lalala".to_string()
                }),
                tags: vec![Tag::Genre],
                pattern: Some(Pattern::with_options("lala".to_string(), false, false, false, true)),
            }
        );

        assert_eq!(
            Action::parse("matched:^x/replace:lalala", Some(1), Some(&matcher)).unwrap(),
            Action {
                behavior: ActionBehavior::Replace(ReplaceAction {
                    replacement: "lalala".to_string()
                }),
                tags: vec![Tag::TrackTitle],
                pattern: Some(Pattern::new("^x".to_string())),
            }
        );

        // Test that case insensitivity is inherited from the matcher.
        let matcher_ci = Matcher::new(vec![ExpandableTag::Tag(Tag::TrackTitle)], Pattern::with_options("haha".to_string(), false, false, false, true));
        assert_eq!(
            Action::parse("replace:lalala", Some(1), Some(&matcher_ci)).unwrap(),
            Action {
                behavior: ActionBehavior::Replace(ReplaceAction {
                    replacement: "lalala".to_string()
                }),
                tags: vec![Tag::TrackTitle],
                pattern: Some(Pattern::with_options("haha".to_string(), false, false, false, true)),
            }
        );

        // Test that the action excludes the immutable *total tags.
        let matcher_totals = Matcher::new(
            vec![
                ExpandableTag::Tag(Tag::TrackNumber),
                ExpandableTag::Tag(Tag::TrackTotal),
                ExpandableTag::Tag(Tag::DiscNumber),
                ExpandableTag::Tag(Tag::DiscTotal),
            ],
            Pattern::new("1".to_string()),
        );
        assert_eq!(
            Action::parse("replace:5", Some(1), Some(&matcher_totals)).unwrap(),
            Action {
                behavior: ActionBehavior::Replace(ReplaceAction { replacement: "5".to_string() }),
                tags: vec![Tag::TrackNumber, Tag::DiscNumber],
                pattern: Some(Pattern::new("1".to_string())),
            }
        );

        let matcher_genre = Matcher::new(vec![ExpandableTag::Tag(Tag::Genre)], Pattern::new("haha".to_string()));
        assert_eq!(
            Action::parse("sed:lalala:hahaha", Some(1), Some(&matcher_genre)).unwrap(),
            Action {
                behavior: ActionBehavior::Sed(SedAction {
                    src: Regex::new("lalala").unwrap(),
                    dst: "hahaha".to_string(),
                }),
                tags: vec![Tag::Genre],
                pattern: Some(Pattern::new("haha".to_string())),
            }
        );

        assert_eq!(
            Action::parse(r"split:::", Some(1), Some(&matcher_genre)).unwrap(),
            Action {
                behavior: ActionBehavior::Split(SplitAction { delimiter: ":".to_string() }),
                tags: vec![Tag::Genre],
                pattern: Some(Pattern::new("haha".to_string())),
            }
        );

        assert_eq!(
            Action::parse(r"add:cute", Some(1), Some(&matcher_genre)).unwrap(),
            Action {
                behavior: ActionBehavior::Add(AddAction { value: "cute".to_string() }),
                tags: vec![Tag::Genre],
                pattern: Some(Pattern::new("haha".to_string())),
            }
        );

        assert_eq!(
            Action::parse(r"delete", Some(1), Some(&matcher_genre)).unwrap(),
            Action {
                behavior: ActionBehavior::Delete(DeleteAction),
                tags: vec![Tag::Genre],
                pattern: Some(Pattern::new("haha".to_string())),
            }
        );
    }

    #[test]
    fn test_rule_parse_action_errors() {
        fn test_err(rule: &str, expected_err: &str, matcher: Option<&Matcher>) {
            let err = Action::parse(rule, Some(1), matcher).unwrap_err();
            assert_eq!(err.to_string(), expected_err);
        }

        let matcher = Matcher::new(vec![ExpandableTag::Tag(Tag::Genre)], Pattern::new("haha".to_string()));

        test_err(
            "tracktitle:hello/:delete",
            "Failed to parse action 1, invalid syntax:\n\n    tracktitle:hello/:delete\n                     ^\n                     Invalid action kind: must be one of {replace, sed, split, add, delete}.",
            None
        );

        test_err(
            "haha/delete",
            "Failed to parse action 1, invalid syntax:\n\n    haha/delete\n    ^\n    Invalid tag: must be one of {tracktitle, trackartist[main], trackartist[guest], trackartist[remixer], trackartist[producer], trackartist[composer], trackartist[conductor], trackartist[djmixer], tracknumber, discnumber, releasetitle, releaseartist[main], releaseartist[guest], releaseartist[remixer], releaseartist[producer], releaseartist[composer], releaseartist[conductor], releaseartist[djmixer], releasetype, releasedate, originaldate, compositiondate, catalognumber, edition, genre, secondarygenre, descriptor, label, new, trackartist, releaseartist, artist}. The next character after a tag must be ':' or ','.",
            None
        );

        test_err(
            "tracktitler/delete",
            "Failed to parse action 1, invalid syntax:\n\n    tracktitler/delete\n    ^\n    Invalid tag: must be one of {tracktitle, trackartist[main], trackartist[guest], trackartist[remixer], trackartist[producer], trackartist[composer], trackartist[conductor], trackartist[djmixer], tracknumber, discnumber, releasetitle, releaseartist[main], releaseartist[guest], releaseartist[remixer], releaseartist[producer], releaseartist[composer], releaseartist[conductor], releaseartist[djmixer], releasetype, releasedate, originaldate, compositiondate, catalognumber, edition, genre, secondarygenre, descriptor, label, new, trackartist, releaseartist, artist}. The next character after a tag must be ':' or ','.",
            None
        );

        test_err(
            "tracktitle:haha:delete",
            "Failed to parse action 1, invalid syntax:\n\n    tracktitle:haha:delete\n    ^\n    Invalid action kind: must be one of {replace, sed, split, add, delete}. If this is pointing at your pattern, you forgot to put a `/` between the matcher section and the action section.",
            Some(&matcher)
        );

        test_err(
            "tracktitle:haha:sed/hi:bye",
            "Failed to parse action 1, invalid syntax:\n\n    tracktitle:haha:sed/hi:bye\n                    ^\n                    Unrecognized flag: Either you forgot a colon here (to end the matcher), or this is an invalid matcher flag. The only supported flag is `i` (case insensitive).",
            None
        );

        test_err(
            "hahaha",
            "Failed to parse action 1, invalid syntax:\n\n    hahaha\n    ^\n    Invalid action kind: must be one of {replace, sed, split, add, delete}.",
            Some(&matcher),
        );

        test_err(
            "replace",
            "Failed to parse action 1, invalid syntax:\n\n    replace\n           ^\n           Replacement not found: must specify a non-empty replacement. Use the delete action to remove a value.",
            Some(&matcher)
        );

        test_err(
            "replace:haha:",
            "Failed to parse action 1, invalid syntax:\n\n    replace:haha:\n                ^\n                Found another section after the replacement, but the replacement must be the last section. Perhaps you meant to escape this colon?",
            Some(&matcher)
        );

        test_err(
            "sed",
            "Failed to parse action 1, invalid syntax:\n\n    sed\n       ^\n       Empty sed pattern found: must specify a non-empty pattern. Example: sed:pattern:replacement",
            Some(&matcher)
        );

        test_err(
            "sed:hihi",
            "Failed to parse action 1, invalid syntax:\n\n    sed:hihi\n            ^\n            Sed replacement not found: must specify a sed replacement section. Example: sed:hihi:replacement.",
            Some(&matcher)
        );

        test_err(
            "sed:invalid[",
            "Failed to parse action 1, invalid syntax:\n\n    sed:invalid[\n        ^\n        Failed to compile the sed pattern regex: invalid pattern: unclosed character class",
            Some(&matcher)
        );

        test_err(
            "sed:hihi:byebye:",
            "Failed to parse action 1, invalid syntax:\n\n    sed:hihi:byebye:\n                   ^\n                   Found another section after the sed replacement, but the sed replacement must be the last section. Perhaps you meant to escape this colon?",
            Some(&matcher)
        );

        test_err(
            "split",
            "Failed to parse action 1, invalid syntax:\n\n    split\n         ^\n         Delimiter not found: must specify a non-empty delimiter to split on.",
            Some(&matcher),
        );

        test_err(
            "split:hi:",
            "Failed to parse action 1, invalid syntax:\n\n    split:hi:\n            ^\n            Found another section after the delimiter, but the delimiter must be the last section. Perhaps you meant to escape this colon?",
            Some(&matcher)
        );

        test_err(
            "split:",
            "Failed to parse action 1, invalid syntax:\n\n    split:\n          ^\n          Delimiter not found: must specify a non-empty delimiter to split on.",
            Some(&matcher)
        );

        test_err(
            "add",
            "Failed to parse action 1, invalid syntax:\n\n    add\n       ^\n       Value not found: must specify a non-empty value to add.",
            Some(&matcher),
        );

        test_err(
            "add:hi:",
            "Failed to parse action 1, invalid syntax:\n\n    add:hi:\n          ^\n          Found another section after the value, but the value must be the last section. Perhaps you meant to escape this colon?",
            Some(&matcher)
        );

        test_err(
            "add:",
            "Failed to parse action 1, invalid syntax:\n\n    add:\n        ^\n        Value not found: must specify a non-empty value to add.",
            Some(&matcher),
        );

        test_err(
            "delete:h",
            "Failed to parse action 1, invalid syntax:\n\n    delete:h\n           ^\n           Found another section after the action kind, but the delete action has no parameters. Please remove this section.",
            Some(&matcher)
        );

        test_err(
            "delete",
            "Failed to parse action 1, invalid syntax:\n\n    delete\n    ^\n    Tags/pattern section not found. Must specify tags to modify, since there is no matcher to default to. Make sure you are formatting your action like {tags}:{pattern}/{kind}:{args} (where `:{pattern}` is optional)",
            None
        );

        test_err(
            "tracktotal/replace:1",
            "Failed to parse action 1, invalid syntax:\n\n    tracktotal/replace:1\n    ^\n    Invalid tag: tracktotal is not modifiable.",
            None,
        );

        test_err(
            "disctotal/replace:1",
            "Failed to parse action 1, invalid syntax:\n\n    disctotal/replace:1\n    ^\n    Invalid tag: disctotal is not modifiable.",
            None,
        );
    }

    #[test]
    fn test_rule_parsing_end_to_end_1() {
        let test_cases = vec![("tracktitle:Track", "delete")];

        for (matcher, action) in test_cases {
            let rule = Rule::parse(matcher, vec![action.to_string()], None).unwrap();
            assert_eq!(rule.to_string(), format!("matcher={matcher} action={matcher}/{action}"));
        }
    }

    #[test]
    fn test_rule_parsing_end_to_end_2() {
        let test_cases = vec![(r"tracktitle:\^Track", "delete"), (r"tracktitle:Track\$", "delete"), (r"tracktitle:\^Track\$", "delete")];

        for (matcher, action) in test_cases {
            let rule = Rule::parse(matcher, vec![action.to_string()], None).unwrap();
            assert_eq!(rule.to_string(), format!("matcher='{matcher}' action='{matcher}/{action}'"));
        }
    }

    #[test]
    fn test_rule_parsing_end_to_end_3() {
        let test_cases = vec![("tracktitle:Track", "genre:lala/replace:lalala"), ("tracktitle,genre,trackartist:Track", "tracktitle,genre,artist/delete")];

        for (matcher, action) in test_cases {
            let rule = Rule::parse(matcher, vec![action.to_string()], None).unwrap();
            assert_eq!(rule.to_string(), format!("matcher={matcher} action={action}"));
        }
    }

    #[test]
    fn test_rule_parsing_multi_value_validation() {
        let err = Rule::parse("tracktitle:h", vec!["split:x".to_string()], None).unwrap_err();
        assert_eq!(err.to_string(), "Failed to parse action 1, invalid syntax:\n\n    split:x\n    ^\n    Invalid rule: Single valued tags tracktitle cannot be modified by multi-value action split");

        let err = Rule::parse("genre:h", vec!["tracktitle/split:x".to_string()], None).unwrap_err();
        assert_eq!(err.to_string(), "Failed to parse action 1, invalid syntax:\n\n    tracktitle/split:x\n    ^\n    Invalid rule: Single valued tags tracktitle cannot be modified by multi-value action split");

        let err = Rule::parse("genre:h", vec!["split:y".to_string(), "tracktitle/split:x".to_string()], None).unwrap_err();
        assert_eq!(err.to_string(), "Failed to parse action 2, invalid syntax:\n\n    tracktitle/split:x\n    ^\n    Invalid rule: Single valued tags tracktitle cannot be modified by multi-value action split");
    }

    #[test]
    fn test_rule_parsing_defaults() {
        let rule = Rule::parse("tracktitle:Track", vec!["replace:hi".to_string()], None).unwrap();
        assert!(rule.actions[0].pattern.is_some());
        assert_eq!(rule.actions[0].pattern.as_ref().unwrap().needle, "Track");

        let rule = Rule::parse("tracktitle:Track", vec!["tracktitle/replace:hi".to_string()], None).unwrap();
        assert!(rule.actions[0].pattern.is_some());
        assert_eq!(rule.actions[0].pattern.as_ref().unwrap().needle, "Track");

        let rule = Rule::parse("tracktitle:Track", vec!["tracktitle:Lack/replace:hi".to_string()], None).unwrap();
        assert!(rule.actions[0].pattern.is_some());
        assert_eq!(rule.actions[0].pattern.as_ref().unwrap().needle, "Lack");
    }

    #[test]
    fn test_parser_take() {
        assert_eq!(take("hello", ":", true).unwrap(), ("hello".to_string(), 5));
        assert_eq!(take("hello:hi", ":", true).unwrap(), ("hello".to_string(), 6));
        assert_eq!(take(r"h::lo:hi", ":", true).unwrap(), ("h:lo".to_string(), 6));
        assert_eq!(take(r"h:://lo:hi", ":", true).unwrap(), ("h:/lo".to_string(), 8));
        assert_eq!(take(r"h::lo/hi", "/", true).unwrap(), ("h:lo".to_string(), 6));
        assert_eq!(take(r"h:://lo/hi", "/", true).unwrap(), ("h:/lo".to_string(), 8));
    }
}
