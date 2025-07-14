use crate::common::{Artist, ArtistMapping};
use crate::config::PathTemplate;
use crate::templates::*;
use std::path::PathBuf;

fn empty_cached_release() -> Release {
    Release {
        id: String::new(),
        source_path: PathBuf::new(),
        cover_image_path: None,
        added_at: "0000-01-01T00:00:00Z".to_string(),
        datafile_mtime: "999".to_string(),
        releasetitle: String::new(),
        releasetype: "unknown".to_string(),
        releasedate: None,
        originaldate: None,
        compositiondate: None,
        edition: None,
        catalognumber: None,
        new: false,
        disctotal: 1,
        genres: vec![],
        parent_genres: vec![],
        secondary_genres: vec![],
        parent_secondary_genres: vec![],
        descriptors: vec![],
        labels: vec![],
        releaseartists: ArtistMapping::new(),
        metahash: "0".to_string(),
    }
}

fn empty_cached_track() -> Track {
    Track {
        id: String::new(),
        source_path: PathBuf::from("hi.m4a"),
        source_mtime: String::new(),
        tracktitle: String::new(),
        tracknumber: String::new(),
        tracktotal: 1,
        discnumber: String::new(),
        duration_seconds: 0,
        trackartists: ArtistMapping::new(),
        metahash: "0".to_string(),
        release: empty_cached_release(),
    }
}

#[test]
fn test_default_templates() {
    // Test release template with artists
    let mut release = empty_cached_release();
    release.releasetitle = "Title".to_string();
    release.releasedate = Some(RoseDate::year_only(2023));
    release.releaseartists = ArtistMapping {
        main: vec![
            Artist::new("A1".to_string()),
            Artist::new("A2".to_string()),
            Artist::new("A3".to_string()),
        ],
        guest: vec![Artist::new("BB".to_string())],
        producer: vec![Artist::new("PP".to_string())],
        ..ArtistMapping::new()
    };
    release.releasetype = "single".to_string();

    let template = PathTemplate(
        r#"
{{ releaseartists | artistsfmt }} -
{% if releasedate %}{{ releasedate.year }}.{% endif %}
{{ releasetitle }}
{% if releasetype == "single" %} - {{ releasetype | releasetypefmt }}{% endif %}
{% if new %}[NEW]{% endif %}
"#
        .to_string(),
    );

    let result = evaluate_release_template(&template, &release, None, None).unwrap();
    assert_eq!(
        result,
        "A1, A2 & A3 (feat. BB) (prod. PP) - 2023. Title - Single"
    );

    // Test with position
    let collage_template = PathTemplate(
        r#"{{ position }}. {{ releaseartists | artistsfmt }} -
{% if releasedate %}{{ releasedate.year }}.{% endif %}
{{ releasetitle }}
{% if releasetype == "single" %} - {{ releasetype | releasetypefmt }}{% endif %}
{% if new %}[NEW]{% endif %}"#
            .to_string(),
    );

    let result = evaluate_release_template(&collage_template, &release, None, Some("4")).unwrap();
    assert_eq!(
        result,
        "4. A1, A2 & A3 (feat. BB) (prod. PP) - 2023. Title - Single"
    );

    // Test empty release
    let mut release = empty_cached_release();
    release.releasetitle = "Title".to_string();

    let result = evaluate_release_template(&template, &release, None, None).unwrap();
    assert_eq!(result, "Unknown Artists - Title");

    let result = evaluate_release_template(&collage_template, &release, None, Some("4")).unwrap();
    assert_eq!(result, "4. Unknown Artists - Title");

    // Test track template
    let track_template = PathTemplate(
        r#"{% if disctotal > 1 %}{{ discnumber | rjust(width=2, fillchar='0') }}-{% endif %}{{ tracknumber | rjust(width=2, fillchar='0') }}.
{{ tracktitle }}
{% if trackartists.guest %}(feat. {{ trackartists.guest | artistsarrayfmt }}){% endif %}"#.to_string()
    );

    let mut track = empty_cached_track();
    track.tracknumber = "2".to_string();
    track.tracktitle = "Trick".to_string();

    let result = evaluate_track_template(&track_template, &track, None, None).unwrap();
    assert_eq!(result, "02. Trick.m4a");

    // Test playlist template
    let playlist_template = PathTemplate(
        r#"{{ position }}.
{{ trackartists | artistsfmt }} -
{{ tracktitle }}"#
            .to_string(),
    );

    let result = evaluate_track_template(&playlist_template, &track, None, Some("4")).unwrap();
    assert_eq!(result, "4. Unknown Artists - Trick.m4a");

    // Test multi-disc track
    let mut track = empty_cached_track();
    track.release.disctotal = 2;
    track.discnumber = "4".to_string();
    track.tracknumber = "2".to_string();
    track.tracktitle = "Trick".to_string();
    track.trackartists = ArtistMapping {
        main: vec![Artist::new("Main".to_string())],
        guest: vec![
            Artist::new("Hi".to_string()),
            Artist::new("High".to_string()),
            Artist::new("Hye".to_string()),
        ],
        ..ArtistMapping::new()
    };

    let result = evaluate_track_template(&track_template, &track, None, None).unwrap();
    assert_eq!(result, "04-02. Trick (feat. Hi, High & Hye).m4a");

    let result = evaluate_track_template(&playlist_template, &track, None, Some("4")).unwrap();
    assert_eq!(result, "4. Main (feat. Hi, High & Hye) - Trick.m4a");
}

#[test]
fn test_classical() {
    let template = PathTemplate(
        r#"
        {% if new %}{{ '{N}' }}{% endif %}
        {% for composer in releaseartists.composer %}{% if loop.first %}{{ composer.name | sortorder }}{% else %}, {{ composer.name | sortorder }}{% endif %}{% endfor %} -
        {% if compositiondate %}{{ compositiondate.year }}.{% endif %}
        {{ releasetitle }}
        performed by {{ releaseartists | artistsfmt(omit=["composer"]) }}
        {% if releasedate %}({{ releasedate.year }}){% endif %}
        "#.to_string()
    );

    let music_source_dir = PathBuf::from("/tmp");
    let ((_, _), (_, _), (debussy, _)) = get_sample_music(&music_source_dir);

    let result = evaluate_release_template(&template, &debussy, None, None).unwrap();
    assert_eq!(
        result,
        "Debussy, Claude - 1907. Images performed by Cleveland Orchestra under Pierre Boulez (1992)"
    );
}

#[test]
fn test_releasetypefmt() {
    assert_eq!(releasetypefmt("album"), "Album");
    assert_eq!(releasetypefmt("single"), "Single");
    assert_eq!(releasetypefmt("ep"), "EP");
    assert_eq!(releasetypefmt("compilation"), "Compilation");
    assert_eq!(releasetypefmt("anthology"), "Anthology");
    assert_eq!(releasetypefmt("soundtrack"), "Soundtrack");
    assert_eq!(releasetypefmt("live"), "Live");
    assert_eq!(releasetypefmt("remix"), "Remix");
    assert_eq!(releasetypefmt("djmix"), "DJ-Mix");
    assert_eq!(releasetypefmt("mixtape"), "Mixtape");
    assert_eq!(releasetypefmt("other"), "Other");
    assert_eq!(releasetypefmt("demo"), "Demo");
    assert_eq!(releasetypefmt("unknown"), "Unknown");
    assert_eq!(releasetypefmt("weird type"), "Weird Type");
}

#[test]
fn test_arrayfmt() {
    assert_eq!(arrayfmt(&[]), "");
    assert_eq!(arrayfmt(&["one".to_string()]), "one");
    assert_eq!(
        arrayfmt(&["one".to_string(), "two".to_string()]),
        "one & two"
    );
    assert_eq!(
        arrayfmt(&["one".to_string(), "two".to_string(), "three".to_string()]),
        "one, two & three"
    );
}

#[test]
fn test_artistsarrayfmt() {
    let artists = vec![
        Artist::new("A1".to_string()),
        Artist::new("A2".to_string()),
        Artist::new("A3".to_string()),
    ];
    assert_eq!(artistsarrayfmt(&artists), "A1, A2 & A3");

    let artists = vec![
        Artist::new("A1".to_string()),
        Artist::new("A2".to_string()),
        Artist::new("A3".to_string()),
        Artist::new("A4".to_string()),
    ];
    assert_eq!(artistsarrayfmt(&artists), "A1 et al.");

    // Test with aliases (should be filtered out)
    let artists = vec![
        Artist::new("A1".to_string()),
        Artist::with_alias("A2".to_string(), true),
        Artist::new("A3".to_string()),
    ];
    assert_eq!(artistsarrayfmt(&artists), "A1 & A3");
}

#[test]
fn test_artistsfmt() {
    // Test main only
    let mapping = ArtistMapping {
        main: vec![Artist::new("Main".to_string())],
        ..ArtistMapping::new()
    };
    assert_eq!(artistsfmt(&mapping, None), "Main");

    // Test with guest
    let mapping = ArtistMapping {
        main: vec![Artist::new("Main".to_string())],
        guest: vec![Artist::new("Guest".to_string())],
        ..ArtistMapping::new()
    };
    assert_eq!(artistsfmt(&mapping, None), "Main (feat. Guest)");

    // Test with producer
    let mapping = ArtistMapping {
        main: vec![Artist::new("Main".to_string())],
        producer: vec![Artist::new("Producer".to_string())],
        ..ArtistMapping::new()
    };
    assert_eq!(artistsfmt(&mapping, None), "Main (prod. Producer)");

    // Test with djmixer
    let mapping = ArtistMapping {
        main: vec![Artist::new("Main".to_string())],
        djmixer: vec![Artist::new("DJ".to_string())],
        ..ArtistMapping::new()
    };
    assert_eq!(artistsfmt(&mapping, None), "DJ pres. Main");

    // Test with composer
    let mapping = ArtistMapping {
        main: vec![Artist::new("Main".to_string())],
        composer: vec![Artist::new("Composer".to_string())],
        ..ArtistMapping::new()
    };
    assert_eq!(artistsfmt(&mapping, None), "Composer performed by Main");

    // Test with conductor
    let mapping = ArtistMapping {
        main: vec![Artist::new("Main".to_string())],
        conductor: vec![Artist::new("Conductor".to_string())],
        ..ArtistMapping::new()
    };
    assert_eq!(artistsfmt(&mapping, None), "Main under Conductor");

    // Test empty
    let mapping = ArtistMapping::new();
    assert_eq!(artistsfmt(&mapping, None), "Unknown Artists");

    // Test omit
    let mapping = ArtistMapping {
        main: vec![Artist::new("Main".to_string())],
        guest: vec![Artist::new("Guest".to_string())],
        producer: vec![Artist::new("Producer".to_string())],
        ..ArtistMapping::new()
    };
    assert_eq!(
        artistsfmt(&mapping, Some(vec!["guest".to_string()])),
        "Main (prod. Producer)"
    );
}

#[test]
fn test_sortorder() {
    assert_eq!(sortorder("Claude Debussy"), "Debussy, Claude");
    assert_eq!(sortorder("Cher"), "Cher");
    assert_eq!(sortorder("Jean-Michel Jarre"), "Jarre, Jean-Michel");
}

#[test]
fn test_lastname() {
    assert_eq!(lastname("Claude Debussy"), "Debussy");
    assert_eq!(lastname("Cher"), "Cher");
    assert_eq!(lastname("Jean-Michel Jarre"), "Jarre");
}

#[test]
fn test_collapse_spacing() {
    assert_eq!(collapse_spacing("  hello   world  "), "hello world");
    assert_eq!(collapse_spacing("hello\n\nworld"), "hello world");
    assert_eq!(collapse_spacing("hello\t\tworld"), "hello world");
    assert_eq!(collapse_spacing("   "), "");
}
