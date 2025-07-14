# Milestone 6: Audio Tags

## Overview
This milestone implements the audio metadata abstraction layer. It provides a unified interface for reading and writing tags across different audio formats (MP3, M4A, FLAC, OGG Vorbis, OPUS).

## Dependencies
- lofty (primary audio metadata library)
- id3 (for specific ID3v2 features if needed)
- serde_json (for dump() method)

## Architecture Overview

```
AudioTags (trait)
├── ID3Tags (MP3)
├── MP4Tags (M4A/MP4)
├── VorbisTags (OGG Vorbis)
├── OpusTags (OGG Opus)
└── FLACTags (FLAC)
```

## Implementation Guide (`src/audiotags.rs`)

### 1. AudioTags Trait

```rust
pub trait AudioTags: Send + Sync {
    fn can_write(&self) -> bool { true }
    
    // Getters
    fn title(&self) -> Option<&str>;
    fn album(&self) -> Option<&str>;
    fn artist(&self) -> Option<ArtistMapping>;
    fn date(&self) -> Option<i32>;
    fn track_number(&self) -> Option<&str>;
    fn disc_number(&self) -> Option<&str>;
    fn duration_seconds(&self) -> Option<i32>;
    fn roseid(&self) -> Option<&str>;
    
    // Setters
    fn set_title(&mut self, value: Option<&str>) -> Result<()>;
    fn set_album(&mut self, value: Option<&str>) -> Result<()>;
    fn set_artist(&mut self, value: ArtistMapping) -> Result<()>;
    fn set_roseid(&mut self, id: &str) -> Result<()>;
    
    // Serialization
    fn dump(&self) -> HashMap<String, Value>;
    fn flush(&mut self, path: &Path) -> Result<()>;
}
```

### 2. Factory Function

```rust
pub fn read_tags(path: &Path) -> Result<Box<dyn AudioTags>> {
    todo!()
}
```

Implementation:
1. Get file extension (lowercase)
2. Match on extension:
   - "mp3" -> ID3Tags::from_file(path)
   - "m4a", "mp4" -> MP4Tags::from_file(path)
   - "ogg" -> Determine if Vorbis or Opus by reading file
   - "opus" -> OpusTags::from_file(path)
   - "flac" -> FLACTags::from_file(path)
3. Return as Box<dyn AudioTags>

### 3. Helper Functions

```rust
pub fn parse_artists(s: &str) -> Vec<Artist> {
    todo!()
}
```
- Split by ";" delimiter
- Trim whitespace
- Create Artist objects

```rust
pub fn format_artists(artists: &[Artist]) -> String {
    todo!()
}
```
- Join artist names with "; "

```rust
pub fn split_tag(value: &str, delimiter: &str) -> Vec<String> {
    todo!()
}
```
- Split string by delimiter
- Trim each part
- Filter empty strings

## Format-Specific Implementations

### ID3Tags (`src/audiotags_id3.rs`)

ID3v2 frame mappings:
- Title: TIT2
- Album: TALB
- Artist: TPE1
- Album Artist: TPE2
- Date: TDRC (recording date)
- Track: TRCK
- Disc: TPOS
- Genre: TCON
- Label: TPUB
- Custom tags: TXXX frames

Special handling:
- Rose ID stored in TXXX frame with description "ROSEID"
- Multi-value tags use null separator (\\0)
- Preserve all unknown frames

### MP4Tags (`src/audiotags_mp4.rs`)

MP4 atom mappings:
- Title: ©nam
- Album: ©alb
- Artist: ©ART
- Album Artist: aART
- Date: ©day
- Track: trkn (special format)
- Disc: disk (special format)
- Genre: ©gen
- Label: ©lab
- Custom: ---- atoms

Special handling:
- Track/disc stored as (current, total) tuples
- Rose ID in custom atom
- iTunes-style metadata

### VorbisTags (`src/audiotags_vorbis.rs`)

Vorbis comment mappings:
- Standard field names (TITLE, ALBUM, ARTIST, etc.)
- Multi-value support native to format
- Case-insensitive field names

Special handling:
- Multiple values naturally supported
- ROSEID as custom field

### OpusTags (similar to Vorbis)

Same as Vorbis but in Opus container

### FLACTags (`src/audiotags_flac.rs`)

Uses Vorbis comments embedded in FLAC
- Same field mappings as Vorbis
- Picture block for album art

## Test Implementation Guide (`src/audiotags_test.rs`)

### Parameterized Tests

The Python tests are parameterized across files. In Rust, create individual tests:

#### Getter Tests (5 tests)
- `test_getters_track1_flac`
- `test_getters_track2_m4a`
- `test_getters_track3_mp3`
- `test_getters_track4_vorbis_ogg`
- `test_getters_track5_opus_ogg`

Each test:
1. Read tags from testdata file
2. Verify expected values for all fields
3. Check track/disc numbers match expected

#### Flush Tests (5 tests)
- `test_flush_track1_flac`
- etc.

Each test:
1. Copy test file to temp directory
2. Read tags
3. Modify some fields
4. Flush to disk
5. Read again and verify changes persisted

#### ID Assignment Tests (5 tests)
Test writing and reading rose ID

#### Release Type Normalization Tests (5 tests)
Test that release types are normalized correctly

### Other Tests

#### `test_write_parent_genres`
- Test writing genres with parent relationships
- Verify parent genres are included

#### `test_split_tag`
- Test the tag splitting helper function

#### `test_parse_artist_string`
- Test artist string parsing

#### `test_format_artist_string`
- Test artist formatting

## Important Implementation Details

### 1. Tag Preservation
- Must preserve all existing tags when writing
- Only modify explicitly set fields
- Unknown/custom tags must be retained

### 2. Multi-Value Tags
- Artists can have multiple values
- Genres can have multiple values
- Different formats handle this differently

### 3. Character Encoding
- All strings are UTF-8
- Some formats (ID3) have encoding flags

### 4. Rose-Specific Tags
- ROSEID: Track UUID
- ROSERELEASEID: Release UUID
- Store in format-appropriate custom fields

### 5. Artist Roles
Artist mapping roles:
- main -> TPE1 (ID3), ©ART (MP4), ARTIST (Vorbis)
- albumartist -> TPE2 (ID3), aART (MP4), ALBUMARTIST (Vorbis)
- composer -> TCOM (ID3), ©wrt (MP4), COMPOSER (Vorbis)
- etc.

### 6. Error Handling
- Unsupported format -> UnsupportedAudioFormat error
- Corrupted file -> Appropriate error with context
- Missing required data -> Return None, don't error

## Performance Considerations

1. **Lazy Loading**: Don't parse all metadata unless requested
2. **Efficient Writing**: Only rewrite file if changes made
3. **Memory Usage**: Stream large files, don't load entirely

## Validation Checklist

- [ ] All 24 tests pass
- [ ] Round-trip tag preservation works
- [ ] Multi-value tags handled correctly
- [ ] Rose IDs persist across read/write
- [ ] Unknown tags are preserved
- [ ] All formats supported equally