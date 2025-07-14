use crate::common::uniq;
use lazy_static::lazy_static;
use regex::Regex;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidRuleError(pub String);

impl fmt::Display for InvalidRuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for InvalidRuleError {}

#[derive(Debug, Clone)]
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
            "Failed to parse {}, invalid syntax:\n\n    {}\n    {: <width$}^\n    {: <width$}{}",
            self.rule_name,
            self.rule,
            "",
            "",
            self.feedback,
            width = self.index
        )
    }
}

impl std::error::Error for RuleSyntaxError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tag {
    TrackTitle,
    TrackArtistMain,
    TrackArtistGuest,
    TrackArtistRemixer,
    TrackArtistProducer,
    TrackArtistComposer,
    TrackArtistConductor,
    TrackArtistDjMixer,
    TrackNumber,
    TrackTotal,
    DiscNumber,
    DiscTotal,
    ReleaseTitle,
    ReleaseArtistMain,
    ReleaseArtistGuest,
    ReleaseArtistRemixer,
    ReleaseArtistProducer,
    ReleaseArtistComposer,
    ReleaseArtistConductor,
    ReleaseArtistDjMixer,
    ReleaseType,
    ReleaseDate,
    OriginalDate,
    CompositionDate,
    CatalogNumber,
    Edition,
    Genre,
    SecondaryGenre,
    Descriptor,
    Label,
    New,
}

impl Tag {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "tracktitle" => Some(Self::TrackTitle),
            "trackartist[main]" => Some(Self::TrackArtistMain),
            "trackartist[guest]" => Some(Self::TrackArtistGuest),
            "trackartist[remixer]" => Some(Self::TrackArtistRemixer),
            "trackartist[producer]" => Some(Self::TrackArtistProducer),
            "trackartist[composer]" => Some(Self::TrackArtistComposer),
            "trackartist[conductor]" => Some(Self::TrackArtistConductor),
            "trackartist[djmixer]" => Some(Self::TrackArtistDjMixer),
            "tracknumber" => Some(Self::TrackNumber),
            "tracktotal" => Some(Self::TrackTotal),
            "discnumber" => Some(Self::DiscNumber),
            "disctotal" => Some(Self::DiscTotal),
            "releasetitle" => Some(Self::ReleaseTitle),
            "releaseartist[main]" => Some(Self::ReleaseArtistMain),
            "releaseartist[guest]" => Some(Self::ReleaseArtistGuest),
            "releaseartist[remixer]" => Some(Self::ReleaseArtistRemixer),
            "releaseartist[producer]" => Some(Self::ReleaseArtistProducer),
            "releaseartist[composer]" => Some(Self::ReleaseArtistComposer),
            "releaseartist[conductor]" => Some(Self::ReleaseArtistConductor),
            "releaseartist[djmixer]" => Some(Self::ReleaseArtistDjMixer),
            "releasetype" => Some(Self::ReleaseType),
            "releasedate" => Some(Self::ReleaseDate),
            "originaldate" => Some(Self::OriginalDate),
            "compositiondate" => Some(Self::CompositionDate),
            "catalognumber" => Some(Self::CatalogNumber),
            "edition" => Some(Self::Edition),
            "genre" => Some(Self::Genre),
            "secondarygenre" => Some(Self::SecondaryGenre),
            "descriptor" => Some(Self::Descriptor),
            "label" => Some(Self::Label),
            "new" => Some(Self::New),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::TrackTitle => "tracktitle",
            Self::TrackArtistMain => "trackartist[main]",
            Self::TrackArtistGuest => "trackartist[guest]",
            Self::TrackArtistRemixer => "trackartist[remixer]",
            Self::TrackArtistProducer => "trackartist[producer]",
            Self::TrackArtistComposer => "trackartist[composer]",
            Self::TrackArtistConductor => "trackartist[conductor]",
            Self::TrackArtistDjMixer => "trackartist[djmixer]",
            Self::TrackNumber => "tracknumber",
            Self::TrackTotal => "tracktotal",
            Self::DiscNumber => "discnumber",
            Self::DiscTotal => "disctotal",
            Self::ReleaseTitle => "releasetitle",
            Self::ReleaseArtistMain => "releaseartist[main]",
            Self::ReleaseArtistGuest => "releaseartist[guest]",
            Self::ReleaseArtistRemixer => "releaseartist[remixer]",
            Self::ReleaseArtistProducer => "releaseartist[producer]",
            Self::ReleaseArtistComposer => "releaseartist[composer]",
            Self::ReleaseArtistConductor => "releaseartist[conductor]",
            Self::ReleaseArtistDjMixer => "releaseartist[djmixer]",
            Self::ReleaseType => "releasetype",
            Self::ReleaseDate => "releasedate",
            Self::OriginalDate => "originaldate",
            Self::CompositionDate => "compositiondate",
            Self::CatalogNumber => "catalognumber",
            Self::Edition => "edition",
            Self::Genre => "genre",
            Self::SecondaryGenre => "secondarygenre",
            Self::Descriptor => "descriptor",
            Self::Label => "label",
            Self::New => "new",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpandableTag {
    Tag(Tag),
    Artist,
    TrackArtist,
    ReleaseArtist,
}

impl ExpandableTag {
    fn from_str(s: &str) -> Option<Self> {
        if let Some(tag) = Tag::from_str(s) {
            Some(Self::Tag(tag))
        } else {
            match s {
                "artist" => Some(Self::Artist),
                "trackartist" => Some(Self::TrackArtist),
                "releaseartist" => Some(Self::ReleaseArtist),
                _ => None,
            }
        }
    }

    #[allow(dead_code)]
    fn as_str(&self) -> &str {
        match self {
            Self::Tag(tag) => tag.as_str(),
            Self::Artist => "artist",
            Self::TrackArtist => "trackartist",
            Self::ReleaseArtist => "releaseartist",
        }
    }

    pub fn expand(&self) -> Vec<Tag> {
        match self {
            Self::Tag(tag) => vec![*tag],
            Self::Artist => vec![
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
            ],
            Self::TrackArtist => vec![
                Tag::TrackArtistMain,
                Tag::TrackArtistGuest,
                Tag::TrackArtistRemixer,
                Tag::TrackArtistProducer,
                Tag::TrackArtistComposer,
                Tag::TrackArtistConductor,
                Tag::TrackArtistDjMixer,
            ],
            Self::ReleaseArtist => vec![
                Tag::ReleaseArtistMain,
                Tag::ReleaseArtistGuest,
                Tag::ReleaseArtistRemixer,
                Tag::ReleaseArtistProducer,
                Tag::ReleaseArtistComposer,
                Tag::ReleaseArtistConductor,
                Tag::ReleaseArtistDjMixer,
            ],
        }
    }
}

lazy_static! {
    static ref ALL_TAG_STRINGS: Vec<&'static str> = {
        let mut tags = vec![
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
            "edition",
            "catalognumber",
            "genre",
            "secondarygenre",
            "descriptor",
            "label",
            "new",
            "artist",
            "trackartist",
            "releaseartist",
        ];
        tags.sort_by_key(|a| std::cmp::Reverse(a.len()));
        tags
    };
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    pub needle: String,
    pub strict_start: bool,
    pub strict_end: bool,
    pub case_insensitive: bool,
}

impl Pattern {
    pub fn new(needle: String) -> Self {
        let mut pattern = Self {
            needle,
            strict_start: false,
            strict_end: false,
            case_insensitive: false,
        };

        // Handle ^ prefix
        if pattern.needle.starts_with('^') && !pattern.needle.starts_with(r"\^") {
            pattern.strict_start = true;
            pattern.needle = pattern.needle[1..].to_string();
        } else if pattern.needle.starts_with(r"\^") {
            pattern.needle = pattern.needle[1..].to_string();
        }

        // Handle $ suffix
        if pattern.needle.ends_with('$') && !pattern.needle.ends_with(r"\$") {
            pattern.strict_end = true;
            pattern.needle.pop();
        } else if pattern.needle.ends_with(r"\$") {
            let prefix = &pattern.needle[..pattern.needle.len() - 2];
            pattern.needle = format!("{prefix}$");
        }

        pattern
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = escape(&self.needle);

        if self.strict_start {
            s = format!("^{s}");
        } else if self.needle.starts_with('^') {
            s = format!(r"\{s}");
        }

        if self.strict_end {
            s = format!("{s}$");
        } else if self.needle.ends_with('$') {
            s = s[..s.len() - 1].to_string();
            s = format!(r"{s}\$");
        }

        if self.case_insensitive {
            s = format!("{s}:i");
        }

        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matcher {
    pub tags: Vec<Tag>,
    pub pattern: Pattern,
}

impl fmt::Display for Matcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", stringify_tags(&self.tags), self.pattern)
    }
}

impl Matcher {
    pub fn parse(raw: &str) -> std::result::Result<Self, RuleSyntaxError> {
        let mut idx = 0;
        let mut tags = Vec::new();

        // Parse tags
        loop {
            let mut found = false;
            for tag_str in ALL_TAG_STRINGS.iter() {
                if raw[idx..].starts_with(tag_str) {
                    let next_idx = idx + tag_str.len();
                    if next_idx < raw.len() {
                        let next_char = raw.chars().nth(next_idx).unwrap();
                        if next_char != ':' && next_char != ',' {
                            continue;
                        }
                    } else {
                        return Err(RuleSyntaxError {
                            rule_name: "matcher".to_string(),
                            rule: raw.to_string(),
                            index: next_idx,
                            feedback: "Expected to find ',' or ':', found end of string."
                                .to_string(),
                        });
                    }

                    if let Some(expandable) = ExpandableTag::from_str(tag_str) {
                        tags.extend(expandable.expand());
                    }

                    idx = next_idx + 1;
                    found = true;

                    if raw.chars().nth(next_idx) == Some(':') {
                        break;
                    }
                    break;
                }
            }

            if !found {
                return Err(RuleSyntaxError {
                    rule_name: "matcher".to_string(),
                    rule: raw.to_string(),
                    index: idx,
                    feedback: format!("Invalid tag: must be one of {{{}}}. The next character after a tag must be ':' or ','.", ALL_TAG_STRINGS.join(", ")),
                });
            }

            if idx > 0 && raw.chars().nth(idx - 1) == Some(':') {
                break;
            }
        }

        // Parse pattern
        let (pattern_str, fwd) = take(&raw[idx..], ":", false).unwrap();
        idx += fwd;

        let mut case_insensitive = false;

        // Check for flags
        if idx < raw.len() && &raw[idx..idx + 1] == ":" {
            idx += 1;
            let (flags, fwd) = take(&raw[idx..], ":", true).unwrap();
            if flags.is_empty() {
                return Err(RuleSyntaxError {
                    rule_name: "matcher".to_string(),
                    rule: raw.to_string(),
                    index: idx,
                    feedback: "No flags specified: Please remove this section (by deleting the colon) or specify one of the supported flags: `i` (case insensitive).".to_string(),
                });
            }

            for (i, flag) in flags.chars().enumerate() {
                match flag {
                    'i' => case_insensitive = true,
                    _ => return Err(RuleSyntaxError {
                        rule_name: "matcher".to_string(),
                        rule: raw.to_string(),
                        index: idx + i,
                        feedback: "Unrecognized flag: Please specify one of the supported flags: `i` (case insensitive).".to_string(),
                    }),
                }
            }
            idx += fwd;
        }

        if idx < raw.len() {
            return Err(RuleSyntaxError {
                rule_name: "matcher".to_string(),
                rule: raw.to_string(),
                index: idx,
                feedback: "Extra input found after end of matcher. Perhaps you meant to escape this colon?".to_string(),
            });
        }

        let mut pattern = Pattern::new(pattern_str);
        pattern.case_insensitive = case_insensitive;

        Ok(Matcher {
            tags: uniq(tags),
            pattern,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActionBehavior {
    Replace(ReplaceAction),
    Sed(SedAction),
    Split(SplitAction),
    Add(AddAction),
    Delete(DeleteAction),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceAction {
    pub replacement: String,
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitAction {
    pub delimiter: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddAction {
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteAction;

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub tags: Vec<Tag>,
    pub behavior: ActionBehavior,
    pub pattern: Option<Pattern>,
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = String::new();

        s.push_str(&stringify_tags(&self.tags));
        if let Some(pattern) = &self.pattern {
            s.push(':');
            s.push_str(&pattern.to_string());
        }

        if !s.is_empty() {
            s.push('/');
        }

        match &self.behavior {
            ActionBehavior::Replace(r) => {
                s.push_str("replace:");
                s.push_str(&r.replacement);
            }
            ActionBehavior::Sed(sed) => {
                s.push_str("sed:");
                s.push_str(&escape(sed.src.as_str()));
                s.push(':');
                s.push_str(&escape(&sed.dst));
            }
            ActionBehavior::Split(split) => {
                s.push_str("split:");
                s.push_str(&split.delimiter);
            }
            ActionBehavior::Add(add) => {
                s.push_str("add:");
                s.push_str(&add.value);
            }
            ActionBehavior::Delete(_) => {
                s.push_str("delete");
            }
        }

        write!(f, "{s}")
    }
}

impl Action {
    pub fn parse(
        raw: &str,
        action_number: usize,
        matcher: Option<&Matcher>,
    ) -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let mut idx = 0;

        // Check if we have a tags/pattern section
        let (_, action_idx) = take(raw, "/", true).unwrap();
        let has_tags_pattern = action_idx != raw.len();

        let (tags, pattern) = if !has_tags_pattern {
            // Use matcher defaults
            if let Some(m) = matcher {
                let modifiable_tags: Vec<Tag> = m
                    .tags
                    .iter()
                    .filter(|t| MODIFIABLE_TAGS.contains(t))
                    .copied()
                    .collect();
                (modifiable_tags, Some(m.pattern.clone()))
            } else {
                return Err(Box::new(RuleSyntaxError {
                    rule_name: format!("action {action_number}"),
                    rule: raw.to_string(),
                    index: idx,
                    feedback: "Tags/pattern section not found. Must specify tags to modify, since there is no matcher to default to. Make sure you are formatting your action like {tags}:{pattern}/{kind}:{args} (where `:{pattern}` is optional)".to_string(),
                }));
            }
        } else {
            // Parse tags and pattern
            let mut tags = Vec::new();

            if raw[idx..].starts_with("matched:") {
                if let Some(m) = matcher {
                    idx += "matched:".len();
                    tags = m
                        .tags
                        .iter()
                        .filter(|t| MODIFIABLE_TAGS.contains(t))
                        .copied()
                        .collect();
                } else {
                    return Err(Box::new(RuleSyntaxError {
                        rule_name: format!("action {action_number}"),
                        rule: raw.to_string(),
                        index: idx,
                        feedback: "Cannot use `matched` in this context: there is no matcher to default to.".to_string(),
                    }));
                }
            } else {
                // Parse tag list
                loop {
                    let mut found = false;
                    for tag_str in ALL_TAG_STRINGS.iter() {
                        if raw[idx..].starts_with(tag_str) {
                            let next_idx = idx + tag_str.len();
                            if next_idx < raw.len() {
                                let next_char = raw.chars().nth(next_idx).unwrap();
                                if next_char != ':' && next_char != ',' && next_char != '/' {
                                    continue;
                                }
                            }

                            if let Some(expandable) = ExpandableTag::from_str(tag_str) {
                                for tag in expandable.expand() {
                                    if !MODIFIABLE_TAGS.contains(&tag) {
                                        return Err(Box::new(RuleSyntaxError {
                                            rule_name: format!("action {action_number}"),
                                            rule: raw.to_string(),
                                            index: idx,
                                            feedback: format!(
                                                "Invalid tag: {tag_str} is not modifiable."
                                            ),
                                        }));
                                    }
                                    tags.push(tag);
                                }
                            }

                            idx = next_idx + 1;
                            found = true;

                            let prev_char = raw.chars().nth(next_idx);
                            if prev_char == Some(':') || prev_char == Some('/') {
                                break;
                            }
                            break;
                        }
                    }

                    if !found {
                        let modifiable_tag_strs: Vec<&str> = ALL_TAG_STRINGS
                            .iter()
                            .filter(|&&s| {
                                if let Some(expandable) = ExpandableTag::from_str(s) {
                                    expandable
                                        .expand()
                                        .iter()
                                        .all(|t| MODIFIABLE_TAGS.contains(t))
                                } else {
                                    false
                                }
                            })
                            .copied()
                            .collect();

                        let feedback = if matcher.is_some() {
                            format!("Invalid tag: must be one of matched, {{{}}}. (And if the value is matched, it must be alone.) The next character after a tag must be ':' or ','.", modifiable_tag_strs.join(", "))
                        } else {
                            format!("Invalid tag: must be one of {{{}}}. The next character after a tag must be ':' or ','.", modifiable_tag_strs.join(", "))
                        };

                        return Err(Box::new(RuleSyntaxError {
                            rule_name: format!("action {action_number}"),
                            rule: raw.to_string(),
                            index: idx,
                            feedback,
                        }));
                    }

                    if idx > 0 {
                        let prev = raw.chars().nth(idx - 1);
                        if prev == Some(':') || prev == Some('/') {
                            break;
                        }
                    }
                }
            }

            // Parse optional pattern
            let pattern = if idx > 0 && raw.chars().nth(idx - 1) == Some('/') {
                if let Some(m) = matcher {
                    if tags == m.tags {
                        Some(m.pattern.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else if idx < raw.len() && raw.chars().nth(idx) == Some('/') {
                idx += 1;
                None
            } else {
                // Parse pattern
                let (colon_pattern, colon_fwd) = take(&raw[idx..], ":", true).unwrap();
                let (slash_pattern, slash_fwd) = take(&raw[idx..], "/", true).unwrap();

                let (needle, fwd, has_flags) = if colon_fwd < slash_fwd {
                    (colon_pattern, colon_fwd, true)
                } else {
                    (slash_pattern, slash_fwd, false)
                };

                idx += fwd;

                if !needle.is_empty() {
                    let mut case_insensitive = false;
                    if has_flags {
                        let (flags, fwd) = take(&raw[idx..], "/", true).unwrap();
                        if flags.is_empty() {
                            return Err(Box::new(RuleSyntaxError {
                                rule_name: format!("action {action_number}"),
                                rule: raw.to_string(),
                                index: idx,
                                feedback: "No flags specified: Please remove this section (by deleting the colon) or specify one of the supported flags: `i` (case insensitive).".to_string(),
                            }));
                        }

                        for (i, flag) in flags.chars().enumerate() {
                            match flag {
                                'i' => case_insensitive = true,
                                _ => return Err(Box::new(RuleSyntaxError {
                                    rule_name: format!("action {action_number}"),
                                    rule: raw.to_string(),
                                    index: idx + i,
                                    feedback: "Unrecognized flag: Either you forgot a colon here (to end the matcher), or this is an invalid matcher flag. The only supported flag is `i` (case insensitive).".to_string(),
                                })),
                            }
                        }
                        idx += fwd;
                    }

                    let mut pattern = Pattern::new(needle);
                    pattern.case_insensitive = case_insensitive;
                    Some(pattern)
                } else {
                    None
                }
            };

            (tags, pattern)
        };

        // Parse action kind
        let valid_actions = ["replace", "sed", "split", "add", "delete"];
        let mut action_kind = None;

        for &action in &valid_actions {
            if raw[idx..].starts_with(&format!("{action}:")) {
                action_kind = Some(action);
                idx += action.len() + 1;
                break;
            } else if &raw[idx..] == action {
                action_kind = Some(action);
                idx += action.len();
                break;
            }
        }

        let action_kind = action_kind.ok_or_else(|| {
            let mut feedback = format!("Invalid action kind: must be one of {{{}}}.", valid_actions.join(", "));
            if idx == 0 && raw.contains(':') {
                feedback.push_str(" If this is pointing at your pattern, you forgot to put a `/` between the matcher section and the action section.");
            }
            RuleSyntaxError {
                rule_name: format!("action {action_number}"),
                rule: raw.to_string(),
                index: idx,
                feedback,
            }
        })?;

        // Validate single-value tags
        if action_kind == "split" || action_kind == "add" {
            let single_valued: Vec<&str> = tags
                .iter()
                .filter(|t| SINGLE_VALUE_TAGS.contains(t))
                .map(|t| t.as_str())
                .collect();

            if !single_valued.is_empty() {
                return Err(Box::new(InvalidRuleError(format!(
                    "Single valued tags {} cannot be modified by multi-value action {}",
                    single_valued.join(", "),
                    action_kind
                ))));
            }
        }

        // Parse action-specific parameters
        let behavior = match action_kind {
            "replace" => {
                let (replacement, fwd) = take(&raw[idx..], ":", false).unwrap();
                idx += fwd;

                if replacement.is_empty() {
                    return Err(Box::new(RuleSyntaxError {
                        rule_name: format!("action {action_number}"),
                        rule: raw.to_string(),
                        index: idx,
                        feedback: "Replacement not found: must specify a non-empty replacement. Use the delete action to remove a value.".to_string(),
                    }));
                }

                if idx < raw.len() {
                    return Err(Box::new(RuleSyntaxError {
                        rule_name: format!("action {action_number}"),
                        rule: raw.to_string(),
                        index: idx,
                        feedback: "Found another section after the replacement, but the replacement must be the last section. Perhaps you meant to escape this colon?".to_string(),
                    }));
                }

                ActionBehavior::Replace(ReplaceAction { replacement })
            }
            "sed" => {
                let (src_str, fwd) = take(&raw[idx..], ":", false).unwrap();
                if src_str.is_empty() {
                    return Err(Box::new(RuleSyntaxError {
                        rule_name: format!("action {action_number}"),
                        rule: raw.to_string(),
                        index: idx,
                        feedback: format!("Empty sed pattern found: must specify a non-empty pattern. Example: {raw}:pattern:replacement"),
                    }));
                }

                let src = Regex::new(&src_str).map_err(|e| RuleSyntaxError {
                    rule_name: format!("action {action_number}"),
                    rule: raw.to_string(),
                    index: idx,
                    feedback: format!(
                        "Failed to compile the sed pattern regex: invalid pattern: {e}"
                    ),
                })?;

                idx += fwd;

                if idx >= raw.len() || raw.chars().nth(idx) != Some(':') {
                    return Err(Box::new(RuleSyntaxError {
                        rule_name: format!("action {action_number}"),
                        rule: raw.to_string(),
                        index: idx,
                        feedback: format!("Sed replacement not found: must specify a sed replacement section. Example: {raw}:replacement."),
                    }));
                }
                idx += 1;

                let (dst, fwd) = take(&raw[idx..], ":", false).unwrap();
                idx += fwd;

                if idx < raw.len() {
                    return Err(Box::new(RuleSyntaxError {
                        rule_name: format!("action {action_number}"),
                        rule: raw.to_string(),
                        index: idx,
                        feedback: "Found another section after the sed replacement, but the sed replacement must be the last section. Perhaps you meant to escape this colon?".to_string(),
                    }));
                }

                ActionBehavior::Sed(SedAction { src, dst })
            }
            "split" => {
                let (delimiter, fwd) = take(&raw[idx..], ":", false).unwrap();
                idx += fwd;

                if delimiter.is_empty() {
                    return Err(Box::new(RuleSyntaxError {
                        rule_name: format!("action {action_number}"),
                        rule: raw.to_string(),
                        index: idx,
                        feedback:
                            "Delimiter not found: must specify a non-empty delimiter to split on."
                                .to_string(),
                    }));
                }

                if idx < raw.len() {
                    return Err(Box::new(RuleSyntaxError {
                        rule_name: format!("action {action_number}"),
                        rule: raw.to_string(),
                        index: idx,
                        feedback: "Found another section after the delimiter, but the delimiter must be the last section. Perhaps you meant to escape this colon?".to_string(),
                    }));
                }

                ActionBehavior::Split(SplitAction { delimiter })
            }
            "add" => {
                let (value, fwd) = take(&raw[idx..], ":", false).unwrap();
                idx += fwd;

                if value.is_empty() {
                    return Err(Box::new(RuleSyntaxError {
                        rule_name: format!("action {action_number}"),
                        rule: raw.to_string(),
                        index: idx,
                        feedback: "Value not found: must specify a non-empty value to add."
                            .to_string(),
                    }));
                }

                if idx < raw.len() {
                    return Err(Box::new(RuleSyntaxError {
                        rule_name: format!("action {action_number}"),
                        rule: raw.to_string(),
                        index: idx,
                        feedback: "Found another section after the value, but the value must be the last section. Perhaps you meant to escape this colon?".to_string(),
                    }));
                }

                ActionBehavior::Add(AddAction { value })
            }
            "delete" => {
                if idx < raw.len() {
                    return Err(Box::new(RuleSyntaxError {
                        rule_name: format!("action {action_number}"),
                        rule: raw.to_string(),
                        index: idx,
                        feedback: "Found another section after the action kind, but the delete action has no parameters. Please remove this section.".to_string(),
                    }));
                }
                ActionBehavior::Delete(DeleteAction)
            }
            _ => unreachable!(),
        };

        Ok(Action {
            tags: uniq(tags),
            behavior,
            pattern,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub matcher: Matcher,
    pub actions: Vec<Action>,
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let matcher_str = self.matcher.to_string();
        let needs_quotes = matcher_str.contains(' ') || matcher_str.contains('\\');

        if needs_quotes {
            write!(f, "matcher='{matcher_str}'")?;
        } else {
            write!(f, "matcher={matcher_str}")?;
        }

        for action in &self.actions {
            let action_str = action.to_string();
            let needs_quotes = action_str.contains(' ') || action_str.contains('\\');

            if needs_quotes {
                write!(f, " action='{action_str}'")?;
            } else {
                write!(f, " action={action_str}")?;
            }
        }

        Ok(())
    }
}

impl Rule {
    pub fn parse(
        matcher: &str,
        actions: Vec<&str>,
    ) -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let parsed_matcher = Matcher::parse(matcher)?;
        let mut parsed_actions = Vec::new();

        for (i, action) in actions.iter().enumerate() {
            parsed_actions.push(Action::parse(action, i + 1, Some(&parsed_matcher))?);
        }

        Ok(Rule {
            matcher: parsed_matcher,
            actions: parsed_actions,
        })
    }
}

pub fn take(
    x: &str,
    until: &str,
    consume_until: bool,
) -> std::result::Result<(String, usize), Box<dyn std::error::Error>> {
    let mut match_str = String::new();
    let mut fwd = 0;

    loop {
        let (match_, fwd_) = _take_escaped(&x[fwd..], until, consume_until);
        match_str.push_str(&match_.replace("::", ":").replace("//", "/"));
        fwd += fwd_;

        let next_idx = fwd + if consume_until { 0 } else { 1 };
        if next_idx < x.len() && x[next_idx..].starts_with(until) {
            match_str.push_str(until);
            fwd = next_idx + until.len();
        } else {
            break;
        }
    }

    Ok((match_str, fwd))
}

fn _take_escaped(x: &str, until: &str, consume_until: bool) -> (String, usize) {
    let mut result = String::new();
    let mut escaped: Option<char> = None;
    let mut seen_idx = 0;

    let chars: Vec<char> = x.chars().collect();
    for i in 0..chars.len() {
        if i + until.len() <= x.len() && &x[i..i + until.len()] == until {
            if consume_until {
                seen_idx = i + until.len();
            } else {
                seen_idx = i;
            }
            break;
        }

        let c = chars[i];
        if (c == ':' || c == '/') && escaped.is_none() {
            escaped = Some(c);
            seen_idx = i + 1;
            continue;
        }

        if let Some(esc) = escaped {
            if c != esc {
                result.push(esc);
            }
            escaped = None;
        }
        result.push(c);
        seen_idx = i + 1;
    }

    (result, seen_idx)
}

pub fn escape(x: &str) -> String {
    x.replace(':', "::").replace('/', "//")
}

pub fn stringify_tags(tags: &[Tag]) -> String {
    let mut tag_strs: Vec<String> = tags.iter().map(|t| t.as_str().to_string()).collect();

    // Check if we can collapse to artist shorthand
    let artist_tags: Vec<&str> = ExpandableTag::Artist
        .expand()
        .iter()
        .map(|t| t.as_str())
        .collect();
    if artist_tags
        .iter()
        .all(|t| tag_strs.contains(&t.to_string()))
    {
        let first_idx = tag_strs
            .iter()
            .position(|t| artist_tags.contains(&t.as_str()))
            .unwrap();
        tag_strs.retain(|t| !artist_tags.contains(&t.as_str()));
        tag_strs.insert(first_idx, "artist".to_string());
    } else {
        // Check trackartist
        let trackartist_tags: Vec<&str> = ExpandableTag::TrackArtist
            .expand()
            .iter()
            .map(|t| t.as_str())
            .collect();
        if trackartist_tags
            .iter()
            .all(|t| tag_strs.contains(&t.to_string()))
        {
            let first_idx = tag_strs
                .iter()
                .position(|t| trackartist_tags.contains(&t.as_str()))
                .unwrap();
            tag_strs.retain(|t| !trackartist_tags.contains(&t.as_str()));
            tag_strs.insert(first_idx, "trackartist".to_string());
        }

        // Check releaseartist
        let releaseartist_tags: Vec<&str> = ExpandableTag::ReleaseArtist
            .expand()
            .iter()
            .map(|t| t.as_str())
            .collect();
        if releaseartist_tags
            .iter()
            .all(|t| tag_strs.contains(&t.to_string()))
        {
            let first_idx = tag_strs
                .iter()
                .position(|t| releaseartist_tags.contains(&t.as_str()))
                .unwrap();
            tag_strs.retain(|t| !releaseartist_tags.contains(&t.as_str()));
            tag_strs.insert(first_idx, "releaseartist".to_string());
        }
    }

    tag_strs.join(",")
}
