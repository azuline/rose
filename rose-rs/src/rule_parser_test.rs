use crate::rule_parser::*;

#[test]
fn test_rule_str() {
    let rule = Rule::parse(
        "tracktitle:Track",
        vec!["releaseartist,genre/replace:lalala"],
    )
    .unwrap();
    assert_eq!(
        rule.to_string(),
        "matcher=tracktitle:Track action=releaseartist,genre/replace:lalala"
    );

    // Test that rules are quoted properly
    let rule = Rule::parse(r"tracktitle,releaseartist,genre::: ", vec![r"sed::::; "]).unwrap();
    assert_eq!(
        rule.to_string(),
        r"matcher='tracktitle,releaseartist,genre::: ' action='tracktitle,releaseartist,genre::: /sed::::; '"
    );

    // Test that custom action matcher is printed properly
    let rule = Rule::parse("tracktitle:Track", vec!["genre:lala/replace:lalala"]).unwrap();
    assert_eq!(
        rule.to_string(),
        "matcher=tracktitle:Track action=genre:lala/replace:lalala"
    );

    // Test that we print `matched` when action pattern is not null
    let rule = Rule::parse("genre:b", vec!["genre:h/replace:hi"]).unwrap();
    assert_eq!(
        rule.to_string(),
        r"matcher=genre:b action=genre:h/replace:hi"
    );
}

#[test]
fn test_rule_parse_matcher() {
    let matcher = Matcher::parse("tracktitle:Track").unwrap();
    assert_eq!(matcher.tags, vec![Tag::TrackTitle]);
    assert_eq!(matcher.pattern.needle, "Track");
    assert!(!matcher.pattern.case_insensitive);

    let matcher = Matcher::parse("tracktitle,tracknumber:Track").unwrap();
    assert_eq!(matcher.tags, vec![Tag::TrackTitle, Tag::TrackNumber]);
    assert_eq!(matcher.pattern.needle, "Track");

    let matcher = Matcher::parse(r"tracktitle,tracknumber:Tr::ck").unwrap();
    assert_eq!(matcher.tags, vec![Tag::TrackTitle, Tag::TrackNumber]);
    assert_eq!(matcher.pattern.needle, "Tr:ck");

    let matcher = Matcher::parse("tracktitle,tracknumber:Track:i").unwrap();
    assert_eq!(matcher.tags, vec![Tag::TrackTitle, Tag::TrackNumber]);
    assert_eq!(matcher.pattern.needle, "Track");
    assert!(matcher.pattern.case_insensitive);

    let matcher = Matcher::parse(r"tracktitle:").unwrap();
    assert_eq!(matcher.tags, vec![Tag::TrackTitle]);
    assert_eq!(matcher.pattern.needle, "");

    let matcher = Matcher::parse("tracktitle:^Track").unwrap();
    assert_eq!(matcher.tags, vec![Tag::TrackTitle]);
    assert_eq!(matcher.pattern.needle, "Track");
    assert!(matcher.pattern.strict_start);
    assert!(!matcher.pattern.strict_end);

    let matcher = Matcher::parse("tracktitle:Track$").unwrap();
    assert_eq!(matcher.tags, vec![Tag::TrackTitle]);
    assert_eq!(matcher.pattern.needle, "Track");
    assert!(!matcher.pattern.strict_start);
    assert!(matcher.pattern.strict_end);

    let matcher = Matcher::parse(r"tracktitle:\^Track").unwrap();
    assert_eq!(matcher.tags, vec![Tag::TrackTitle]);
    assert_eq!(matcher.pattern.needle, r"^Track");
    assert!(!matcher.pattern.strict_start);
    assert!(!matcher.pattern.strict_end);

    let matcher = Matcher::parse(r"tracktitle:Track\$").unwrap();
    assert_eq!(matcher.tags, vec![Tag::TrackTitle]);
    assert_eq!(matcher.pattern.needle, r"Track$");
    assert!(!matcher.pattern.strict_start);
    assert!(!matcher.pattern.strict_end);

    let matcher = Matcher::parse(r"tracktitle:\^Track\$").unwrap();
    assert_eq!(matcher.tags, vec![Tag::TrackTitle]);
    assert_eq!(matcher.pattern.needle, r"^Track$");
    assert!(!matcher.pattern.strict_start);
    assert!(!matcher.pattern.strict_end);
}

#[test]
fn test_rule_parse_matcher_errors() {
    let result = Matcher::parse("tracknumber^Track$");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Invalid tag"));

    let result = Matcher::parse("tracknumber");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err
        .to_string()
        .contains("Expected to find ',' or ':', found end of string"));

    let result = Matcher::parse("tracktitle:Tr:ck");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("Unrecognized flag"));

    let result = Matcher::parse("tracktitle:hi:i:hihi");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err
        .to_string()
        .contains("Extra input found after end of matcher"));
}

#[test]
fn test_rule_parse_action() {
    let matcher = Matcher::parse("tracktitle:haha").unwrap();

    let action = Action::parse("replace:lalala", 1, Some(&matcher)).unwrap();
    assert_eq!(action.tags, vec![Tag::TrackTitle]);
    assert_eq!(action.pattern.as_ref().unwrap().needle, "haha");
    match &action.behavior {
        ActionBehavior::Replace(r) => assert_eq!(r.replacement, "lalala"),
        _ => panic!("Expected Replace action"),
    }

    let action = Action::parse("genre/replace:lalala", 1, None).unwrap();
    assert_eq!(action.tags, vec![Tag::Genre]);
    assert!(action.pattern.is_none());
    match &action.behavior {
        ActionBehavior::Replace(r) => assert_eq!(r.replacement, "lalala"),
        _ => panic!("Expected Replace action"),
    }

    let action = Action::parse("tracknumber,genre/replace:lalala", 1, None).unwrap();
    assert_eq!(action.tags, vec![Tag::TrackNumber, Tag::Genre]);
    assert!(action.pattern.is_none());

    let action = Action::parse("genre:lala/replace:lalala", 1, None).unwrap();
    assert_eq!(action.tags, vec![Tag::Genre]);
    assert_eq!(action.pattern.as_ref().unwrap().needle, "lala");

    let action = Action::parse("genre:lala:i/replace:lalala", 1, None).unwrap();
    assert_eq!(action.tags, vec![Tag::Genre]);
    assert_eq!(action.pattern.as_ref().unwrap().needle, "lala");
    assert!(action.pattern.as_ref().unwrap().case_insensitive);

    let matcher = Matcher::parse("tracktitle:haha").unwrap();
    let action = Action::parse("matched:^x/replace:lalala", 1, Some(&matcher)).unwrap();
    assert_eq!(action.tags, vec![Tag::TrackTitle]);
    assert_eq!(action.pattern.as_ref().unwrap().needle, "x");
    assert!(action.pattern.as_ref().unwrap().strict_start);

    // Test case insensitivity inheritance
    let matcher = Matcher::parse("tracktitle:haha:i").unwrap();
    let action = Action::parse("replace:lalala", 1, Some(&matcher)).unwrap();
    assert!(action.pattern.as_ref().unwrap().case_insensitive);

    // Test excluding immutable *total tags
    let matcher = Matcher::parse("tracknumber,tracktotal,discnumber,disctotal:1").unwrap();
    let action = Action::parse("replace:5", 1, Some(&matcher)).unwrap();
    assert_eq!(action.tags, vec![Tag::TrackNumber, Tag::DiscNumber]);

    // Test sed action
    let matcher = Matcher::parse("genre:haha").unwrap();
    let action = Action::parse("sed:lalala:hahaha", 1, Some(&matcher)).unwrap();
    match &action.behavior {
        ActionBehavior::Sed(s) => {
            assert_eq!(s.src.as_str(), "lalala");
            assert_eq!(s.dst, "hahaha");
        }
        _ => panic!("Expected Sed action"),
    }

    // Test split action
    let action = Action::parse(r"split:::", 1, Some(&matcher)).unwrap();
    match &action.behavior {
        ActionBehavior::Split(s) => assert_eq!(s.delimiter, ":"),
        _ => panic!("Expected Split action"),
    }

    // Test add action
    let action = Action::parse(r"add:cute", 1, Some(&matcher)).unwrap();
    match &action.behavior {
        ActionBehavior::Add(a) => assert_eq!(a.value, "cute"),
        _ => panic!("Expected Add action"),
    }

    // Test delete action
    let action = Action::parse(r"delete", 1, Some(&matcher)).unwrap();
    match &action.behavior {
        ActionBehavior::Delete(_) => {}
        _ => panic!("Expected Delete action"),
    }
}

#[test]
fn test_rule_parse_action_errors() {
    let result = Action::parse("tracktitle:hello/:delete", 1, None);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid action kind"));

    let result = Action::parse("haha/delete", 1, None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid tag"));

    let result = Action::parse("tracktitler/delete", 1, None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid tag"));

    let matcher = Matcher::parse("genre:haha").unwrap();
    let result = Action::parse("tracktitle:haha:delete", 1, Some(&matcher));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid action kind"));

    let result = Action::parse("tracktitle:haha:sed/hi:bye", 1, None);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Unrecognized flag"));

    let result = Action::parse("hahaha", 1, Some(&matcher));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid action kind"));

    let result = Action::parse("replace", 1, Some(&matcher));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Replacement not found"));

    let result = Action::parse("replace:haha:", 1, Some(&matcher));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Found another section after the replacement"));

    let result = Action::parse("sed", 1, Some(&matcher));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Empty sed pattern found"));

    let result = Action::parse("sed:hihi", 1, Some(&matcher));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Sed replacement not found"));

    let result = Action::parse("sed:invalid[", 1, Some(&matcher));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Failed to compile the sed pattern regex"));

    let result = Action::parse("sed:hihi:byebye:", 1, Some(&matcher));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Found another section after the sed replacement"));

    let result = Action::parse("split", 1, Some(&matcher));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Delimiter not found"));

    let result = Action::parse("split:hi:", 1, Some(&matcher));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Found another section after the delimiter"));

    let result = Action::parse("split:", 1, Some(&matcher));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Delimiter not found"));

    let result = Action::parse("add", 1, Some(&matcher));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Value not found"));

    let result = Action::parse("add:hi:", 1, Some(&matcher));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Found another section after the value"));

    let result = Action::parse("add:", 1, Some(&matcher));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Value not found"));

    let result = Action::parse("delete:h", 1, Some(&matcher));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Found another section after the action kind"));

    let result = Action::parse("delete", 1, None);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Tags/pattern section not found"));

    let result = Action::parse("tracktotal/replace:1", 1, None);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("is not modifiable"));

    let result = Action::parse("disctotal/replace:1", 1, None);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("is not modifiable"));
}

#[test]
fn test_rule_parsing_end_to_end_1() {
    let rule = Rule::parse("tracktitle:Track", vec!["delete"]).unwrap();
    assert_eq!(
        rule.to_string(),
        "matcher=tracktitle:Track action=tracktitle:Track/delete"
    );
}

#[test]
fn test_rule_parsing_end_to_end_2() {
    let test_cases = vec![
        (r"tracktitle:\^Track", "delete"),
        (r"tracktitle:Track\$", "delete"),
        (r"tracktitle:\^Track\$", "delete"),
    ];

    for (matcher, action) in test_cases {
        let rule = Rule::parse(matcher, vec![action]).unwrap();
        assert_eq!(
            rule.to_string(),
            format!("matcher='{matcher}' action='{matcher}/{action}'")
        );
    }
}

#[test]
fn test_rule_parsing_end_to_end_3() {
    let test_cases = vec![
        ("tracktitle:Track", "genre:lala/replace:lalala"),
        (
            "tracktitle,genre,trackartist:Track",
            "tracktitle,genre,artist/delete",
        ),
    ];

    for (matcher, action) in test_cases {
        let rule = Rule::parse(matcher, vec![action]).unwrap();
        assert_eq!(
            rule.to_string(),
            format!("matcher={matcher} action={action}")
        );
    }
}

#[test]
fn test_rule_parsing_multi_value_validation() {
    let result = Rule::parse("tracktitle:h", vec!["split:x"]);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Single valued tags tracktitle cannot be modified by multi-value action split"));

    let result = Rule::parse("genre:h", vec!["tracktitle/split:x"]);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Single valued tags tracktitle cannot be modified by multi-value action split"));

    let result = Rule::parse("genre:h", vec!["split:y", "tracktitle/split:x"]);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Single valued tags tracktitle cannot be modified by multi-value action split"));
}

#[test]
fn test_rule_parsing_defaults() {
    let rule = Rule::parse("tracktitle:Track", vec!["replace:hi"]).unwrap();
    assert!(rule.actions[0].pattern.is_some());
    assert_eq!(rule.actions[0].pattern.as_ref().unwrap().needle, "Track");

    let rule = Rule::parse("tracktitle:Track", vec!["tracktitle/replace:hi"]).unwrap();
    assert!(rule.actions[0].pattern.is_some());
    assert_eq!(rule.actions[0].pattern.as_ref().unwrap().needle, "Track");

    let rule = Rule::parse("tracktitle:Track", vec!["tracktitle:Lack/replace:hi"]).unwrap();
    assert!(rule.actions[0].pattern.is_some());
    assert_eq!(rule.actions[0].pattern.as_ref().unwrap().needle, "Lack");
}

#[test]
fn test_parser_take() {
    assert_eq!(take("hello", ":", true).unwrap(), ("hello".to_string(), 5));
    assert_eq!(
        take("hello:hi", ":", true).unwrap(),
        ("hello".to_string(), 6)
    );
    assert_eq!(
        take(r"h::lo:hi", ":", true).unwrap(),
        ("h:lo".to_string(), 6)
    );
    assert_eq!(
        take(r"h:://lo:hi", ":", true).unwrap(),
        ("h:/lo".to_string(), 8)
    );
    assert_eq!(
        take(r"h::lo/hi", "/", true).unwrap(),
        ("h:lo".to_string(), 6)
    );
    assert_eq!(
        take(r"h:://lo/hi", "/", true).unwrap(),
        ("h:/lo".to_string(), 8)
    );
}

#[test]
fn test_escape() {
    assert_eq!(escape("hello"), "hello");
    assert_eq!(escape("hello:world"), "hello::world");
    assert_eq!(escape("hello/world"), "hello//world");
    assert_eq!(escape("hello:world/test"), "hello::world//test");
}

#[test]
fn test_stringify_tags() {
    // Test normal tags
    let tags = vec![Tag::TrackTitle, Tag::Genre];
    assert_eq!(stringify_tags(&tags), "tracktitle,genre");

    // Test artist shorthand
    let mut tags = vec![
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
    ];
    assert_eq!(stringify_tags(&tags), "artist");

    // Test trackartist shorthand
    tags = vec![
        Tag::TrackArtistMain,
        Tag::TrackArtistGuest,
        Tag::TrackArtistRemixer,
        Tag::TrackArtistProducer,
        Tag::TrackArtistComposer,
        Tag::TrackArtistConductor,
        Tag::TrackArtistDjMixer,
    ];
    assert_eq!(stringify_tags(&tags), "trackartist");

    // Test releaseartist shorthand
    tags = vec![
        Tag::ReleaseArtistMain,
        Tag::ReleaseArtistGuest,
        Tag::ReleaseArtistRemixer,
        Tag::ReleaseArtistProducer,
        Tag::ReleaseArtistComposer,
        Tag::ReleaseArtistConductor,
        Tag::ReleaseArtistDjMixer,
    ];
    assert_eq!(stringify_tags(&tags), "releaseartist");
}

#[test]
fn test_pattern_display() {
    let pattern = Pattern::new("hello".to_string());
    assert_eq!(pattern.to_string(), "hello");

    let pattern = Pattern::new("^hello".to_string());
    assert_eq!(pattern.to_string(), "^hello");
    assert!(pattern.strict_start);

    let pattern = Pattern::new("hello$".to_string());
    assert_eq!(pattern.to_string(), "hello$");
    assert!(pattern.strict_end);

    let pattern = Pattern::new(r"\^hello".to_string());
    assert_eq!(pattern.to_string(), r"\^hello");
    assert!(!pattern.strict_start);

    let pattern = Pattern::new(r"hello\$".to_string());
    assert_eq!(pattern.to_string(), r"hello\$");
    assert!(!pattern.strict_end);

    let mut pattern = Pattern::new("hello:world".to_string());
    pattern.case_insensitive = true;
    assert_eq!(pattern.to_string(), "hello::world:i");
}

#[test]
fn test_expandable_tags() {
    // Test artist expansion
    let artist = ExpandableTag::Artist;
    let expanded = artist.expand();
    assert_eq!(expanded.len(), 14);
    assert!(expanded.contains(&Tag::TrackArtistMain));
    assert!(expanded.contains(&Tag::ReleaseArtistMain));

    // Test trackartist expansion
    let trackartist = ExpandableTag::TrackArtist;
    let expanded = trackartist.expand();
    assert_eq!(expanded.len(), 7);
    assert!(expanded.contains(&Tag::TrackArtistMain));
    assert!(!expanded.contains(&Tag::ReleaseArtistMain));

    // Test releaseartist expansion
    let releaseartist = ExpandableTag::ReleaseArtist;
    let expanded = releaseartist.expand();
    assert_eq!(expanded.len(), 7);
    assert!(!expanded.contains(&Tag::TrackArtistMain));
    assert!(expanded.contains(&Tag::ReleaseArtistMain));
}
