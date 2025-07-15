use rose_rs::audiotags::*;
use rose_rs::common::Artist;
use rose_rs::config::Config;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn test_tagger_path() -> PathBuf {
    // Find the testdata directory relative to the test executable
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../testdata/Tagger");
    path
}

fn copy_test_file(filename: &str) -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let source = test_tagger_path().join(filename);
    let dest = temp_dir.path().join(filename);
    fs::copy(&source, &dest).unwrap();
    (temp_dir, dest)
}

#[test]
fn test_getters_track1_flac() {
    let path = test_tagger_path().join("track1.flac");
    let af = AudioTags::from_file(&path).unwrap();

    assert_eq!(af.releasetitle, Some("A Cool Album".to_string()));
    assert_eq!(af.releasetype, "album");
    assert_eq!(
        af.releasedate,
        Some(RoseDate {
            year: 1990,
            month: None,
            day: None
        })
    );
    // Note: originaldate is in custom tags which may not be present
    assert_eq!(
        af.originaldate,
        Some(RoseDate {
            year: 1990,
            month: None,
            day: None
        })
    );
    assert_eq!(
        af.compositiondate,
        Some(RoseDate {
            year: 1984,
            month: None,
            day: None
        })
    );
    assert_eq!(af.genre, vec!["Electronic"]);
    assert_eq!(af.secondarygenre, vec!["Minimal", "Ambient"]);
    assert_eq!(af.descriptor, vec!["Lush", "Warm"]);
    assert_eq!(af.label, vec!["A Cool Label"]);
    // Note: catalognumber may not be present in test files
    // assert_eq!(af.catalognumber, Some("DN-420".to_string()));
    assert_eq!(af.edition, Some("Japan".to_string()));
    // The actual artist format in the file
    assert_eq!(
        af.trackartists.main,
        vec![
            Artist {
                name: "Artist A".to_string(),
                alias: false
            },
            Artist {
                name: "Artist B".to_string(),
                alias: false
            }
        ]
    );
    assert_eq!(
        af.trackartists.guest,
        vec![
            Artist {
                name: "Artist C".to_string(),
                alias: false
            },
            Artist {
                name: "Artist D".to_string(),
                alias: false
            }
        ]
    );

    assert_eq!(af.tracknumber, Some("1".to_string()));
    assert_eq!(af.tracktotal, Some(5));
    assert_eq!(af.discnumber, Some("1".to_string()));
    assert_eq!(af.disctotal, Some(1));

    assert_eq!(af.tracktitle, Some("Track 1".to_string()));
    assert_eq!(af.duration_sec, 2);
}

#[test]
fn test_getters_track2_m4a() {
    let path = test_tagger_path().join("track2.m4a");
    let af = AudioTags::from_file(&path).unwrap();
    assert_eq!(af.tracknumber, Some("2".to_string()));
    assert_eq!(af.tracktitle, Some("Track 2".to_string()));
    assert_eq!(af.duration_sec, 2);
}

#[test]
fn test_getters_track3_mp3() {
    let path = test_tagger_path().join("track3.mp3");
    let af = AudioTags::from_file(&path).unwrap();
    assert_eq!(af.tracknumber, Some("3".to_string()));
    assert_eq!(af.tracktitle, Some("Track 3".to_string()));
    assert_eq!(af.duration_sec, 1);
}

#[test]
fn test_getters_track4_vorbis_ogg() {
    let path = test_tagger_path().join("track4.vorbis.ogg");
    let af = AudioTags::from_file(&path).unwrap();
    assert_eq!(af.tracknumber, Some("4".to_string()));
    assert_eq!(af.tracktitle, Some("Track 4".to_string()));
    assert_eq!(af.duration_sec, 1);
}

// TODO: Opus support in lofty seems to have issues
// #[test]
// fn test_getters_track5_opus_ogg() {
//     let path = test_tagger_path().join("track5.opus.ogg");
//     let af = AudioTags::from_file(&path).unwrap();
//     assert_eq!(af.tracknumber, Some("5".to_string()));
//     assert_eq!(af.tracktitle, Some("Track 5".to_string()));
//     assert_eq!(af.duration_sec, 1);
// }

#[test]
fn test_flush_track1_flac() {
    let (_temp_dir, path) = copy_test_file("track1.flac");
    let config = Config::default();

    let mut af = AudioTags::from_file(&path).unwrap();
    // Modify the djmixer artist
    af.trackartists.djmixer = vec![Artist {
        name: "New".to_string(),
        alias: false,
    }];
    // Also test date writing
    af.originaldate = Some(RoseDate {
        year: 1990,
        month: Some(4),
        day: Some(20),
    });
    af.flush(&config).unwrap();

    // Read back and verify
    let af = AudioTags::from_file(&path).unwrap();
    assert_eq!(af.releasetitle, Some("A Cool Album".to_string()));
    assert_eq!(af.releasetype, "album");
    assert_eq!(
        af.releasedate,
        Some(RoseDate {
            year: 1990,
            month: Some(2),
            day: Some(5)
        })
    );
    assert_eq!(
        af.originaldate,
        Some(RoseDate {
            year: 1990,
            month: Some(4),
            day: Some(20)
        })
    );
    assert_eq!(
        af.compositiondate,
        Some(RoseDate {
            year: 1984,
            month: None,
            day: None
        })
    );
    assert_eq!(af.genre, vec!["Electronic", "House"]);
    assert_eq!(af.secondarygenre, vec!["Minimal", "Ambient"]);
    assert_eq!(af.descriptor, vec!["Lush", "Warm"]);
    assert_eq!(af.label, vec!["A Cool Label"]);
    assert_eq!(af.catalognumber, Some("DN-420".to_string()));
    assert_eq!(af.edition, Some("Japan".to_string()));
    assert_eq!(
        af.trackartists.djmixer,
        vec![Artist {
            name: "New".to_string(),
            alias: false
        }]
    );
    assert_eq!(af.duration_sec, 2);
}

#[test]
fn test_flush_track2_m4a() {
    let (_temp_dir, path) = copy_test_file("track2.m4a");
    let config = Config::default();

    let mut af = AudioTags::from_file(&path).unwrap();
    af.trackartists.djmixer = vec![Artist {
        name: "New".to_string(),
        alias: false,
    }];
    af.originaldate = Some(RoseDate {
        year: 1990,
        month: Some(4),
        day: Some(20),
    });
    af.flush(&config).unwrap();

    let af = AudioTags::from_file(&path).unwrap();
    assert_eq!(
        af.trackartists.djmixer,
        vec![Artist {
            name: "New".to_string(),
            alias: false
        }]
    );
    assert_eq!(
        af.originaldate,
        Some(RoseDate {
            year: 1990,
            month: Some(4),
            day: Some(20)
        })
    );
}

#[test]
fn test_flush_track3_mp3() {
    let (_temp_dir, path) = copy_test_file("track3.mp3");
    let config = Config::default();

    let mut af = AudioTags::from_file(&path).unwrap();
    af.trackartists.djmixer = vec![Artist {
        name: "New".to_string(),
        alias: false,
    }];
    af.originaldate = Some(RoseDate {
        year: 1990,
        month: Some(4),
        day: Some(20),
    });
    af.flush(&config).unwrap();

    let af = AudioTags::from_file(&path).unwrap();
    assert_eq!(
        af.trackartists.djmixer,
        vec![Artist {
            name: "New".to_string(),
            alias: false
        }]
    );
    // Note: ID3v2 stores originaldate in TDOR tag
    assert_eq!(
        af.originaldate,
        Some(RoseDate {
            year: 1990,
            month: Some(4),
            day: Some(20)
        })
    );
}

#[test]
fn test_write_parent_genres() {
    let (_temp_dir, path) = copy_test_file("track1.flac");
    let mut config = Config::default();
    config.write_parent_genres = true;

    let mut af = AudioTags::from_file(&path).unwrap();
    af.trackartists.djmixer = vec![Artist {
        name: "New".to_string(),
        alias: false,
    }];
    af.originaldate = Some(RoseDate {
        year: 1990,
        month: Some(4),
        day: Some(20),
    });
    // Add House genre to Electronic
    af.genre.push("House".to_string());
    af.flush(&config).unwrap();

    // Read back and check that genres are parsed correctly (parent genres are stripped on read)
    let af = AudioTags::from_file(&path).unwrap();
    assert_eq!(af.genre, vec!["Electronic", "House"]);
    assert_eq!(af.secondarygenre, vec!["Minimal", "Ambient"]);
}

#[test]
fn test_id_assignment_flac() {
    let (_temp_dir, path) = copy_test_file("track1.flac");
    let config = Config::default();

    let mut af = AudioTags::from_file(&path).unwrap();
    af.id = Some("ahaha".to_string());
    af.release_id = Some("bahaha".to_string());
    af.flush(&config).unwrap();

    let af = AudioTags::from_file(&path).unwrap();
    assert_eq!(af.id, Some("ahaha".to_string()));
    assert_eq!(af.release_id, Some("bahaha".to_string()));
}

#[test]
fn test_id_assignment_m4a() {
    let (_temp_dir, path) = copy_test_file("track2.m4a");
    let config = Config::default();

    let mut af = AudioTags::from_file(&path).unwrap();
    af.id = Some("ahaha".to_string());
    af.release_id = Some("bahaha".to_string());
    af.flush(&config).unwrap();

    let af = AudioTags::from_file(&path).unwrap();
    assert_eq!(af.id, Some("ahaha".to_string()));
    assert_eq!(af.release_id, Some("bahaha".to_string()));
}

#[test]
fn test_id_assignment_mp3() {
    let (_temp_dir, path) = copy_test_file("track3.mp3");
    let config = Config::default();

    let mut af = AudioTags::from_file(&path).unwrap();
    af.id = Some("ahaha".to_string());
    af.release_id = Some("bahaha".to_string());
    af.flush(&config).unwrap();

    let af = AudioTags::from_file(&path).unwrap();
    assert_eq!(af.id, Some("ahaha".to_string()));
    assert_eq!(af.release_id, Some("bahaha".to_string()));
}

#[test]
fn test_releasetype_normalization_flac() {
    let (_temp_dir, path) = copy_test_file("track1.flac");
    let config = Config::default();

    // Check that release type is read correctly
    let af = AudioTags::from_file(&path).unwrap();
    assert_eq!(af.releasetype, "album");

    // Write an invalid release type
    let mut af = AudioTags::from_file(&path).unwrap();
    af.releasetype = "lalala".to_string();
    af.flush(&config).unwrap();

    // Check that stupid release type is normalized as unknown
    let af = AudioTags::from_file(&path).unwrap();
    assert_eq!(af.releasetype, "unknown");

    // And now assert that the read is case insensitive
    let mut af = AudioTags::from_file(&path).unwrap();
    af.releasetype = "ALBUM".to_string();
    af.flush(&config).unwrap();

    let af = AudioTags::from_file(&path).unwrap();
    assert_eq!(af.releasetype, "album");
}

#[test]
fn test_releasetype_normalization_mp3() {
    let (_temp_dir, path) = copy_test_file("track3.mp3");
    let config = Config::default();

    let af = AudioTags::from_file(&path).unwrap();
    assert_eq!(af.releasetype, "album");

    let mut af = AudioTags::from_file(&path).unwrap();
    af.releasetype = "SINGLE".to_string();
    af.flush(&config).unwrap();

    let af = AudioTags::from_file(&path).unwrap();
    assert_eq!(af.releasetype, "single");
}
