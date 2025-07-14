use crate::datafiles::*;
use crate::error::RoseExpectedError;
use std::fs;
use std::path::Path;
use tempfile::TempDir;
use uuid::Uuid;

#[test]
fn test_find_datafile_by_pattern() {
    let temp_dir = TempDir::new().unwrap();
    let dir = temp_dir.path();

    // No datafile initially
    assert!(find_release_datafile(dir).unwrap().is_none());

    // Create a valid datafile
    let uuid = Uuid::now_v7();
    let datafile_path = dir.join(format!(".rose.{uuid}.toml"));
    fs::write(
        &datafile_path,
        "new = true\nadded_at = \"2023-01-01T00:00:00+00:00\"",
    )
    .unwrap();

    // Should find the datafile
    let result = find_release_datafile(dir).unwrap();
    assert!(result.is_some());
    let (path, found_uuid) = result.unwrap();
    assert_eq!(path, datafile_path);
    assert_eq!(found_uuid, uuid);

    // Create another file that doesn't match the pattern
    fs::write(dir.join("not-a-datafile.toml"), "test").unwrap();

    // Should still find only the valid datafile
    let result = find_release_datafile(dir).unwrap();
    assert!(result.is_some());
    let (path, _) = result.unwrap();
    assert_eq!(path, datafile_path);
}

#[test]
fn test_read_valid_datafile() {
    let temp_dir = TempDir::new().unwrap();
    let datafile_path = temp_dir.path().join(".rose.test.toml");

    let content = r#"
new = false
added_at = "2023-10-23T00:00:00-04:00"
"#;
    fs::write(&datafile_path, content).unwrap();

    let datafile = read_datafile(&datafile_path).unwrap();
    assert!(!datafile.new);
    assert_eq!(datafile.added_at, "2023-10-23T00:00:00-04:00");
}

#[test]
fn test_read_missing_datafile() {
    let temp_dir = TempDir::new().unwrap();
    let datafile_path = temp_dir.path().join(".rose.missing.toml");

    let result = read_datafile(&datafile_path);
    assert!(result.is_err());
    match result.unwrap_err() {
        crate::error::RoseError::Expected(RoseExpectedError::FileNotFound { path }) => {
            assert_eq!(path, datafile_path);
        }
        _ => panic!("Expected FileNotFound error"),
    }
}

#[test]
fn test_read_corrupt_datafile() {
    let temp_dir = TempDir::new().unwrap();
    let datafile_path = temp_dir.path().join(".rose.corrupt.toml");

    // Write invalid TOML
    fs::write(&datafile_path, "this is not valid toml { ] }").unwrap();

    // Should return default datafile on corrupt data
    let datafile = read_datafile(&datafile_path).unwrap();
    assert!(datafile.new); // default
    assert!(!datafile.added_at.is_empty()); // should have a timestamp
}

#[test]
fn test_create_new_datafile() {
    let temp_dir = TempDir::new().unwrap();
    let dir = temp_dir.path();

    let (path, uuid, datafile) = create_datafile(dir).unwrap();

    // Verify the file was created
    assert!(path.exists());
    assert!(path.starts_with(dir));

    // Verify the filename format
    let filename = path.file_name().unwrap().to_str().unwrap();
    assert!(filename.starts_with(".rose."));
    assert!(filename.ends_with(".toml"));

    // Verify the UUID is valid
    assert_eq!(uuid.get_version(), Some(uuid::Version::SortRand));

    // Verify the datafile contents
    assert!(datafile.new);
    assert!(!datafile.added_at.is_empty());

    // Verify we can read it back
    let read_back = read_datafile(&path).unwrap();
    assert_eq!(read_back, datafile);
}

#[test]
fn test_write_datafile() {
    let temp_dir = TempDir::new().unwrap();
    let datafile_path = temp_dir.path().join(".rose.test.toml");

    let datafile = StoredDataFile {
        new: false,
        added_at: "2023-05-15T12:34:56+00:00".to_string(),
    };

    write_datafile(&datafile_path, &datafile).unwrap();

    // Read back and verify
    let content = fs::read_to_string(&datafile_path).unwrap();
    assert!(content.contains("new = false"));
    assert!(content.contains("added_at = \"2023-05-15T12:34:56+00:00\""));

    // Verify we can parse it back
    let read_back = read_datafile(&datafile_path).unwrap();
    assert_eq!(read_back, datafile);
}

#[test]
fn test_upgrade_missing_fields() {
    let temp_dir = TempDir::new().unwrap();
    let datafile_path = temp_dir.path().join(".rose.upgrade.toml");

    // Write a datafile with missing fields
    fs::write(&datafile_path, "new = false\n").unwrap();

    let upgraded = upgrade_datafile(&datafile_path).unwrap();
    assert!(!upgraded.new);
    assert!(!upgraded.added_at.is_empty()); // Should have been filled in

    // Verify it was written back
    let read_back = read_datafile(&datafile_path).unwrap();
    assert_eq!(read_back, upgraded);
}

#[test]
fn test_preserve_unknown_fields() {
    let temp_dir = TempDir::new().unwrap();
    let datafile_path = temp_dir.path().join(".rose.preserve.toml");

    // Write a datafile with an unknown field
    let content = r#"
new = true
added_at = "2023-01-01T00:00:00+00:00"
unknown_field = "should be ignored"
"#;
    fs::write(&datafile_path, content).unwrap();

    // Read should succeed, ignoring unknown field
    let datafile = read_datafile(&datafile_path).unwrap();
    assert!(datafile.new);
    assert_eq!(datafile.added_at, "2023-01-01T00:00:00+00:00");

    // Write back should not include unknown field
    write_datafile(&datafile_path, &datafile).unwrap();
    let new_content = fs::read_to_string(&datafile_path).unwrap();
    assert!(!new_content.contains("unknown_field"));
}

#[test]
fn test_uuid_validation() {
    assert!(Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").is_ok());
    assert!(Uuid::parse_str("not-a-uuid").is_err());

    // Test extract_uuid_from_path
    let path = Path::new("/some/dir/.rose.123e4567-e89b-12d3-a456-426614174000.toml");
    let uuid = extract_uuid_from_path(path).unwrap();
    assert_eq!(uuid.to_string(), "123e4567-e89b-12d3-a456-426614174000");

    // Invalid UUID in filename
    let path = Path::new("/some/dir/.rose.not-a-uuid.toml");
    assert!(extract_uuid_from_path(path).is_none());

    // Not a datafile
    let path = Path::new("/some/dir/regular-file.toml");
    assert!(extract_uuid_from_path(path).is_none());
}

#[test]
fn test_filename_format() {
    // Test is_datafile
    assert!(is_datafile(Path::new(
        ".rose.123e4567-e89b-12d3-a456-426614174000.toml"
    )));
    assert!(is_datafile(Path::new("/path/to/.rose.abc123.toml")));
    assert!(!is_datafile(Path::new("rose.uuid.toml")));
    assert!(!is_datafile(Path::new(".rose.toml")));
    assert!(!is_datafile(Path::new(".rose.uuid.txt")));

    // Test datafile_path
    let uuid = Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap();
    let path = datafile_path(Path::new("/some/dir"), &uuid);
    assert_eq!(
        path,
        Path::new("/some/dir/.rose.123e4567-e89b-12d3-a456-426614174000.toml")
    );
}

#[test]
fn test_toggle_new_flag() {
    let temp_dir = TempDir::new().unwrap();
    let datafile_path = temp_dir.path().join(".rose.toggle.toml");

    // Create a datafile with new = true
    let datafile = StoredDataFile {
        new: true,
        added_at: "2023-01-01T00:00:00+00:00".to_string(),
    };
    write_datafile(&datafile_path, &datafile).unwrap();

    // Toggle the flag
    toggle_new_flag(&datafile_path).unwrap();

    // Verify it was toggled
    let read_back = read_datafile(&datafile_path).unwrap();
    assert!(!read_back.new);
    assert_eq!(read_back.added_at, datafile.added_at);

    // Toggle again
    toggle_new_flag(&datafile_path).unwrap();
    let read_back = read_datafile(&datafile_path).unwrap();
    assert!(read_back.new);
}

#[test]
fn test_update_added_at() {
    let temp_dir = TempDir::new().unwrap();
    let datafile_path = temp_dir.path().join(".rose.update.toml");

    // Create a datafile
    let datafile = StoredDataFile::default();
    write_datafile(&datafile_path, &datafile).unwrap();

    // Update the timestamp
    let new_timestamp = "2024-01-01T12:00:00+00:00";
    update_added_at(&datafile_path, new_timestamp).unwrap();

    // Verify it was updated
    let read_back = read_datafile(&datafile_path).unwrap();
    assert_eq!(read_back.added_at, new_timestamp);
    assert_eq!(read_back.new, datafile.new);
}

#[test]
fn test_read_or_create_datafile() {
    let temp_dir = TempDir::new().unwrap();
    let dir = temp_dir.path();

    // First call should create
    let (path1, uuid1, datafile1) = read_or_create_datafile(dir).unwrap();
    assert!(path1.exists());
    assert!(datafile1.new);

    // Second call should read existing
    let (path2, uuid2, datafile2) = read_or_create_datafile(dir).unwrap();
    assert_eq!(path1, path2);
    assert_eq!(uuid1, uuid2);
    assert_eq!(datafile1, datafile2);

    // Modify the datafile
    toggle_new_flag(&path1).unwrap();

    // Third call should read the modified version
    let (path3, uuid3, datafile3) = read_or_create_datafile(dir).unwrap();
    assert_eq!(path1, path3);
    assert_eq!(uuid1, uuid3);
    assert!(!datafile3.new); // Should be toggled
}
