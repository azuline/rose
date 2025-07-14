# Rose-py Unit Tests Documentation

This document contains every unit test in rose-py, organized by feature/module with names and descriptions.

## 1. Rule Parser Tests (`rule_parser_test.py`)

### Basic Parsing Tests
- **test_parse_tag**: Tests parsing of field matchers (e.g., `artist:foo`)
- **test_parse_tag_regex**: Tests regex field matchers (e.g., `artist:/foo/`)
- **test_parse_tag_quotes**: Tests quoted string matchers (e.g., `artist:"foo:bar"`)
- **test_parse_bool_ops**: Tests boolean operators (and/or/not)
- **test_parse_empty_parens**: Tests handling of empty parentheses
- **test_parse_unmatched_parens**: Tests error handling for unmatched parentheses
- **test_parse_action_replace**: Tests replace action parsing (e.g., `artist:"foo"`)
- **test_parse_action_delete**: Tests delete action parsing (e.g., `artist:`)
- **test_parse_action_delete_tag**: Tests deleting entire tags (e.g., `artist::`)
- **test_parse_action_add**: Tests add action parsing (e.g., `artist+:"foo"`)
- **test_parse_action_split**: Tests split action parsing (e.g., `artist/"foo"`)
- **test_parse_action_sed**: Tests sed/regex replacement parsing (e.g., `artist/find/replace/`)

### Matcher Tests
- **test_parse_matcher_pattern**: Tests pattern matching for releases/tracks
- **test_release_matcher_year**: Tests year matching for releases
- **test_no_bloat_parse**: Tests performance of matcher parsing
- **test_no_bloat_execute**: Tests execution performance of matchers

### Token Tests
- **test_tokenize_single_value**: Tests tokenization of simple values
- **test_tokenize_multi_value**: Tests tokenization of lists
- **test_tokenize_quoted_value**: Tests quoted string tokenization
- **test_tokenize_regex**: Tests regex tokenization
- **test_tokenize_bad_pattern**: Tests error handling for invalid patterns
- **test_tokenize_bad_values**: Tests error handling for malformed values
- **test_tokenize_escaped_quotes**: Tests escaped quote handling
- **test_tokenize_escaped_delimiter**: Tests escaped delimiter handling
- **test_tokenize_escaped_slash**: Tests escaped slash handling
- **test_tokenize_weird_slash_escape**: Tests edge cases in slash escaping
- **test_tokenize_double_slash**: Tests double slash handling
- **test_tokenize_weird_double_slash**: Tests double slash edge cases
- **test_tokenize_sed_delimiter**: Tests sed pattern delimiter handling
- **test_tokenize_actions**: Tests action tokenization

### Action Parsing Tests
- **test_action_parse_replace**: Tests replace action construction
- **test_action_parse_add**: Tests add action construction
- **test_action_parse_delete**: Tests delete action construction
- **test_action_parse_sed**: Tests sed action construction
- **test_action_parse_split**: Tests split action construction
- **test_action_parse_sed_groups**: Tests sed with capture groups
- **test_action_parse_sed_global**: Tests global sed replacements
- **test_action_parse_sed_caseinsensitive**: Tests case-insensitive sed
- **test_action_parse_sed_multiline**: Tests multiline sed patterns
- **test_action_parse_bad_sed**: Tests sed error handling
- **test_action_parse_bad_flags**: Tests invalid flag handling
- **test_execute_sed_artist**: Tests sed execution on artist fields
- **test_execute_sed_global**: Tests global sed execution
- **test_execute_sed_case_insensitive**: Tests case-insensitive sed execution

## 2. Templates Tests (`templates_test.py`)

- **test_execute_release_template**: Tests release path template execution
- **test_execute_track_template**: Tests track path template execution

## 3. Collages Tests (`collages_test.py`)

### Basic Operations
- **test_lifecycle**: Tests create → add releases → read → delete lifecycle
- **test_edit**: Tests collage metadata editing
- **test_duplicate_name**: Tests error handling for duplicate collage names
- **test_add_release_resets_release_added_at**: Tests timestamp updates when adding releases
- **test_remove_release_from_collage**: Tests release removal functionality

### Advanced Operations
- **test_add_releases_in_middle**: Tests position management when inserting
- **test_collages_are_updated_on_general_cache_update**: Tests cache synchronization

## 4. Playlists Tests (`playlists_test.py`)

### Basic Operations
- **test_lifecycle**: Tests create → add tracks → read → delete lifecycle
- **test_duplicate_name**: Tests error handling for duplicate playlist names
- **test_add_track_resets_track_added_at**: Tests timestamp updates when adding tracks
- **test_remove_track_from_playlist**: Tests track removal functionality
- **test_edit**: Tests playlist metadata editing

### Cover Art Operations
- **test_playlist_cover_art**: Tests adding/retrieving/deleting cover art
- **test_playlist_cover_art_square**: Tests cover art dimension validation

### Advanced Operations
- **test_add_tracks_in_middle**: Tests position management when inserting
- **test_playlists_are_updated_on_general_cache_update**: Tests cache synchronization

## 5. Releases Tests (`releases_test.py`)

### Basic Operations
- **test_create_releases**: Tests release creation from directory structure
- **test_create_single_releases**: Tests single release creation
- **test_delete_release**: Tests release deletion

### Metadata Operations
- **test_edit_release**: Tests release metadata editing
- **test_set_release_cover_art**: Tests cover art management

### Rule Application
- **test_run_rule_on_release**: Tests applying rules to releases
- **test_toggle_new_flag**: Tests new release flag toggling
- **test_dump_releases**: Tests serialization/export functionality

## 6. Tracks Tests (`tracks_test.py`)

- **test_dump_tracks**: Tests track serialization/export
- **test_set_track_one**: Tests searching/filtering for track number 1

## 7. Rules Tests (`rules_test.py`)

### Tag Operations
- **test_update_tag_constant**: Tests updating tags with constant values
- **test_update_tag_regex**: Tests regex-based tag updates
- **test_update_tag_sed_replace**: Tests sed-style replacements
- **test_update_tag_delete**: Tests tag deletion
- **test_update_tag_add**: Tests adding new tag values
- **test_update_tag_split**: Tests splitting tag values

### Artist Tag Operations  
- **test_artist_tag_replace**: Tests artist replacement
- **test_artist_tag_sed**: Tests sed operations on artists
- **test_artist_tag_delete**: Tests artist deletion
- **test_artist_tag_split**: Tests artist splitting
- **test_artist_tag_multi_delete**: Tests deleting multiple artists
- **test_artist_tag_role_delete**: Tests role-specific artist deletion

### Special Tag Operations
- **test_releasedate_update**: Tests date field updates
- **test_tracknum_update**: Tests track number updates
- **test_genre_update**: Tests genre updates with hierarchy
- **test_genre_update_insert_parent**: Tests parent genre insertion
- **test_label_update**: Tests label updates

### Matching Tests
- **test_matcher_release**: Tests release matching
- **test_matcher_release_all**: Tests matching all releases
- **test_matcher_track**: Tests track matching
- **test_matcher_artist**: Tests artist matching
- **test_matcher_pattern**: Tests pattern matching
- **test_fast_search_release_matcher**: Tests optimized release search
- **test_fast_search_track_matcher**: Tests optimized track search
- **test_execute_stored_rule**: Tests stored rule execution
- **test_execute_stored_rename_rule**: Tests renaming rules

## 8. Audiotags Tests (`audiotags_test.py`)

### Format-Specific Tests
- **test_mp3**: Tests MP3/ID3 tag reading and writing
- **test_m4a**: Tests M4A/MP4 tag reading and writing  
- **test_ogg**: Tests OGG Vorbis tag reading and writing
- **test_opus**: Tests OPUS tag reading and writing
- **test_flac**: Tests FLAC tag reading and writing

### Feature Tests
- **test_unsupported_text_file**: Tests handling of non-audio files
- **test_id3_delete_explicit_v1**: Tests ID3v1 tag deletion
- **test_preserve_unknown_tags**: Tests preservation of unrecognized tags

## 9. Config Tests (`config_test.py`)

### Basic Configuration
- **test_config_full**: Tests full configuration parsing
- **test_config_minimal**: Tests minimal configuration requirements
- **test_config_not_found**: Tests missing configuration handling
- **test_config_path_templates_error**: Tests template validation

### Validation Tests
- **test_config_validate_artist_aliases_resolve_to_self**: Tests alias loop detection
- **test_config_validate_duplicate_artist_aliases**: Tests duplicate alias detection

## 10. Cache Tests (`cache_test.py`)

### Basic Operations
- **test_create**: Tests cache database creation
- **test_update**: Tests basic cache update from filesystem
- **test_update_releases_and_delete_orphans**: Tests orphan cleanup
- **test_force_update**: Tests forced cache refresh
- **test_evict_nonexistent_releases**: Tests cleanup of missing releases

### Metadata Matching
- **test_release_type_albumartist_writeback**: Tests albumartist field handling
- **test_release_type_compilation_propagates**: Tests compilation flag propagation
- **test_year_uses_original_date_and_falls_back_to_date**: Tests date field precedence
- **test_year_is_set_on_releases_and_tracks**: Tests year propagation

### Track Management
- **test_track_number_parsing**: Tests track number format parsing
- **test_track_disc_parsing**: Tests disc number parsing
- **test_disc_number_filter_toggle**: Tests disc number filtering
- **test_catalog_number_parsing**: Tests catalog number extraction
- **test_track_id_and_source_path_match_on_filenames**: Tests ID generation

### Artist Management
- **test_read_artist_fields_album**: Tests album artist reading
- **test_read_artist_fields_compilation**: Tests compilation artist handling
- **test_handle_albumartists_field**: Tests multi-artist handling
- **test_handle_artists_field**: Tests artist list parsing
- **test_read_corr_artist_fields**: Tests various artist roles
- **test_artist_aliases**: Tests artist alias resolution

### Genre Management
- **test_genre_aliases**: Tests genre alias handling
- **test_genre_parent_genres_not_assigned**: Tests parent genre exclusion
- **test_genre_unknown_do_not_exist**: Tests unknown genre filtering

### Release Management
- **test_get_release**: Tests release retrieval
- **test_get_release_nonexistent**: Tests missing release handling
- **test_get_tracks_of_release**: Tests track listing for releases
- **test_list_releases**: Tests release enumeration
- **test_get_release_cover_art**: Tests cover art retrieval
- **test_cover_art_path_detection**: Tests various cover art filename patterns

### Duplicate Handling
- **test_album_duplicate_detection**: Tests duplicate album detection
- **test_album_multi_disc_duplicate_detection**: Tests multi-disc duplicate handling
- **test_compilation_duplicate_detection**: Tests compilation duplicate handling
- **test_single_duplicate_detection**: Tests single duplicate handling

### Performance and Optimization
- **test_null_cover_art_saved_as_null**: Tests null value optimization
- **test_update_incremental_album**: Tests incremental updates
- **test_multiprocessing**: Tests parallel processing
- **test_locking**: Tests concurrent access control

### Full-Text Search
- **test_fts_search_query**: Tests basic text search
- **test_fts_parse_query**: Tests query parsing
- **test_matcher_query**: Tests integrated matcher queries
- **test_matcher_names**: Tests name-based matching

### Query Building
- **test_build_release_query**: Tests release query generation
- **test_list_collages**: Tests collage enumeration
- **test_resolve_release_ids**: Tests ID resolution
- **test_path_templates**: Tests template-based queries

### Edge Cases and Validation
- **test_catalog_artists_are_albumartists**: Tests artist type validation
- **test_catalog_with_explicit_track_artists**: Tests explicit artist handling
- **test_catalog_multidisc_empty_track_artists**: Tests empty artist handling
- **test_catalog_releases_without_years**: Tests missing year handling
- **test_release_source_actions**: Tests release source modifications
- **test_metafile_index_ignore**: Tests metadata file exclusion
- **test_get_release_logtext**: Tests log generation

### Collection Management
- **test_create_collage**: Tests collage creation
- **test_already_exists_collage**: Tests duplicate collage handling
- **test_add_release_to_nonexistent_collage**: Tests invalid collage operations
- **test_create_playlist**: Tests playlist creation
- **test_already_exists_playlist**: Tests duplicate playlist handling
- **test_add_track_to_nonexistent_playlist**: Tests invalid playlist operations

### Release Source Management
- **test_get_release_source_paths**: Tests source path retrieval
- **test_release_source_actions**: Tests source action handling

### Special Cases
- **test_release_id_can_be_read_after_update**: Tests ID persistence
- **test_cover_cannot_be_read_after_update**: Tests cover art invalidation
- **test_track_ids_cannot_have_prefix_collisions**: Tests ID uniqueness
- **test_tracktotal**: Tests total track count handling
- **test_release_written_to_logfile**: Tests logging functionality

## Summary Statistics

- **Total Test Cases**: ~200
- **Most Tested Module**: Cache (87 tests)
- **Least Tested Module**: Templates (2 tests)
- **Test Coverage Areas**:
  - Data integrity and validation
  - Error handling and edge cases
  - Performance and concurrency
  - Feature completeness
  - Cross-component integration