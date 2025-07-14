# Rose-rs Test-Driven Development Implementation Plan

## Executive Summary

This revised plan follows strict Test-Driven Development (TDD) principles for implementing rose-rs. Each phase implements one feature following the dependency graph, with two stages:
1. **Red Phase**: Port Python tests to Rust with stub implementations that fail
2. **Green Phase**: Implement functionality until all tests pass

The implementation follows a breadth-first traversal of the feature dependency graph, ensuring foundational features are complete before dependent features.

## TDD Methodology

### Core Principles
1. **Write tests first**: Port Python tests before any implementation
2. **Red-Green-Refactor**: Tests fail → Implementation → Tests pass → Refactor
3. **One feature at a time**: Complete each feature before moving to the next
4. **Maintain test parity**: Every Python test gets a Rust equivalent
5. **Continuous validation**: All previous tests must keep passing

### Test Implementation Strategy
- Use `#[should_panic]` or `Result` for expected failures
- Implement stubs that return `unimplemented!()` or `todo!()`
- Group tests by feature in separate modules
- Use property-based testing where applicable
- Maintain test data compatibility with Python

## Phase 1: Foundation Layer

### Checkpoint 1.1: Common Utilities (Week 1)

#### Stage 1: Port Tests (Days 1-2)
Port from `rose-py/rose_test.py` (common utility tests):
```rust
// tests/common_test.rs
#[test]
fn test_valid_uuid() { todo!() }
#[test]
fn test_sanitize_filename() { todo!() }
#[test]
fn test_artist_dataclass() { todo!() }
#[test]
fn test_artist_mapping_dataclass() { todo!() }
#[test]
fn test_error_hierarchy() { todo!() }
```

#### Stage 2: Implementation (Days 3-5)
Implement in `rose-core/src/common.rs`:
- [ ] Artist and ArtistMapping structs with serde
- [ ] Error types matching Python hierarchy
- [ ] Utility functions (UUID validation, path sanitization)
- [ ] Constants and type definitions

**Validation**: All common tests pass, structs serialize/deserialize correctly

### Checkpoint 1.2: Genre Hierarchy (Week 1)

#### Stage 1: Port Tests (Day 5)
```rust
// tests/genre_hierarchy_test.rs
#[test]
fn test_genres_list_loaded() { todo!() }
#[test]
fn test_genre_parents_loaded() { todo!() }
#[test]
fn test_genre_lookup() { todo!() }
#[test]
fn test_parent_relationships() { todo!() }
```

#### Stage 2: Implementation (Days 6-7)
- [ ] Generate genre data from Python source
- [ ] Implement efficient lookup structures
- [ ] Validate against Python genre hierarchy

**Validation**: Genre lookups match Python exactly

## Phase 2: Core Infrastructure

### Checkpoint 2.1: Rule Parser (Week 2)

#### Stage 1: Port Tests (Days 1-2)
Port all 44 tests from `rule_parser_test.py`:
```rust
// tests/rule_parser_test.rs
mod tokenizer {
    #[test]
    fn test_tokenize_single_value() { todo!() }
    #[test]
    fn test_tokenize_multi_value() { todo!() }
    // ... 40 more tokenizer tests
}

mod parser {
    #[test]
    fn test_parse_tag() { todo!() }
    #[test]
    fn test_parse_matcher_pattern() { todo!() }
    // ... remaining parser tests
}
```

#### Stage 2: Implementation (Days 3-5)
- [ ] Implement tokenizer with exact Python compatibility
- [ ] Build parser generating same AST structure
- [ ] Create matcher and action types
- [ ] Handle all edge cases from tests

**Validation**: All 44 parser tests pass

### Checkpoint 2.2: Templates (Week 2-3)

#### Stage 1: Port Tests (Days 6-7)
Port from `templates_test.py`:
```rust
// tests/templates_test.rs
#[test]
fn test_execute_release_template() { todo!() }
#[test]
fn test_execute_track_template() { todo!() }
```

#### Stage 2: Implementation (Week 3, Days 1-2)
- [ ] Integrate Tera templating engine
- [ ] Implement custom filters
- [ ] Build template contexts
- [ ] Path safety validation

**Validation**: Template output matches Python exactly

## Phase 3: Configuration & Audio

### Checkpoint 3.1: Configuration (Week 3)

#### Stage 1: Port Tests (Days 3-4)
Port all 6 tests from `config_test.py`:
```rust
// tests/config_test.rs
#[test]
fn test_config_full() { todo!() }
#[test]
fn test_config_minimal() { todo!() }
#[test]
fn test_config_not_found() { todo!() }
#[test]
fn test_config_path_templates_error() { todo!() }
#[test]
fn test_config_validate_artist_aliases_resolve_to_self() { todo!() }
#[test]
fn test_config_validate_duplicate_artist_aliases() { todo!() }
```

#### Stage 2: Implementation (Days 5-7)
- [ ] TOML parsing with serde
- [ ] Configuration discovery logic
- [ ] Validation routines
- [ ] Default handling

**Validation**: Config files from Python work unchanged

### Checkpoint 3.2: Audio Tags (Week 4)

#### Stage 1: Port Tests (Days 1-2)
Port all 8 tests from `audiotags_test.py`:
```rust
// tests/audiotags_test.rs
#[test]
fn test_mp3() { todo!() }
#[test]
fn test_m4a() { todo!() }
#[test]
fn test_ogg() { todo!() }
#[test]
fn test_opus() { todo!() }
#[test]
fn test_flac() { todo!() }
#[test]
fn test_unsupported_text_file() { todo!() }
#[test]
fn test_id3_delete_explicit_v1() { todo!() }
#[test]
fn test_preserve_unknown_tags() { todo!() }
```

#### Stage 2: Implementation (Days 3-7)
- [ ] Abstract AudioTags trait
- [ ] Format-specific implementations
- [ ] Tag preservation logic
- [ ] Cover art handling

**Validation**: Tags written by Rust readable by Python

## Phase 4: Data Layer

### Checkpoint 4.1: Cache Core (Week 5-6)

#### Stage 1: Port Tests - Part 1 (Week 5, Days 1-3)
Port first 30 cache tests focusing on basic operations:
```rust
// tests/cache_test.rs
mod basic_operations {
    #[test]
    fn test_create() { todo!() }
    #[test]
    fn test_update() { todo!() }
    #[test]
    fn test_update_releases_and_delete_orphans() { todo!() }
    // ... more basic tests
}
```

#### Stage 2: Implementation - Part 1 (Week 5, Days 4-7)
- [ ] SQLite schema creation
- [ ] Basic CRUD operations
- [ ] Connection management
- [ ] Transaction handling

### Checkpoint 4.2: Cache Advanced (Week 6)

#### Stage 1: Port Tests - Part 2 (Days 1-2)
Port remaining 57 cache tests:
```rust
mod metadata_handling {
    #[test]
    fn test_release_type_albumartist_writeback() { todo!() }
    // ... metadata tests
}

mod performance {
    #[test]
    fn test_multiprocessing() { todo!() }
    #[test]
    fn test_locking() { todo!() }
    // ... performance tests
}
```

#### Stage 2: Implementation - Part 2 (Days 3-7)
- [ ] Complex queries and indexes
- [ ] Full-text search
- [ ] Concurrent access
- [ ] Cache optimization

**Validation**: All 87 cache tests pass, Python cache files work

## Phase 5: Business Logic

### Checkpoint 5.1: Rules Engine (Week 7)

#### Stage 1: Port Tests (Days 1-2)
Port all 27 tests from `rules_test.py`:
```rust
// tests/rules_test.rs
mod tag_operations {
    #[test]
    fn test_update_tag_constant() { todo!() }
    #[test]
    fn test_update_tag_regex() { todo!() }
    // ... more tag tests
}

mod matching {
    #[test]
    fn test_matcher_release() { todo!() }
    #[test]
    fn test_fast_search_release_matcher() { todo!() }
    // ... matching tests
}
```

#### Stage 2: Implementation (Days 3-7)
- [ ] Rule execution engine
- [ ] Matcher to SQL compilation
- [ ] Action application
- [ ] Batch processing

**Validation**: Rules produce identical results to Python

## Phase 6: Entity Management

### Checkpoint 6.1: Releases (Week 8)

#### Stage 1: Port Tests (Days 1-2)
Port all 8 tests from `releases_test.py`:
```rust
// tests/releases_test.rs
#[test]
fn test_create_releases() { todo!() }
#[test]
fn test_create_single_releases() { todo!() }
#[test]
fn test_delete_release() { todo!() }
#[test]
fn test_edit_release() { todo!() }
#[test]
fn test_set_release_cover_art() { todo!() }
#[test]
fn test_run_rule_on_release() { todo!() }
#[test]
fn test_toggle_new_flag() { todo!() }
#[test]
fn test_dump_releases() { todo!() }
```

#### Stage 2: Implementation (Days 3-7)
- [ ] Release CRUD operations
- [ ] Cover art management
- [ ] Metadata editing
- [ ] Rule integration

**Validation**: All release operations work identically

### Checkpoint 6.2: Tracks (Week 9, Days 1-3)

#### Stage 1: Port Tests (Day 1)
Port tests from `tracks_test.py`:
```rust
// tests/tracks_test.rs
#[test]
fn test_dump_tracks() { todo!() }
#[test]
fn test_set_track_one() { todo!() }
```

#### Stage 2: Implementation (Days 2-3)
- [ ] Track operations
- [ ] Track-specific queries

**Validation**: Track tests pass

## Phase 7: Collections

### Checkpoint 7.1: Collages (Week 9, Days 4-7)

#### Stage 1: Port Tests (Day 4)
Port all 7 tests from `collages_test.py`:
```rust
// tests/collages_test.rs
#[test]
fn test_lifecycle() { todo!() }
#[test]
fn test_edit() { todo!() }
#[test]
fn test_duplicate_name() { todo!() }
// ... remaining tests
```

#### Stage 2: Implementation (Days 5-7)
- [ ] Collage CRUD operations
- [ ] Release management
- [ ] Position handling

### Checkpoint 7.2: Playlists (Week 10)

#### Stage 1: Port Tests (Days 1-2)
Port all 9 tests from `playlists_test.py`:
```rust
// tests/playlists_test.rs
#[test]
fn test_lifecycle() { todo!() }
#[test]
fn test_playlist_cover_art() { todo!() }
#[test]
fn test_playlist_cover_art_square() { todo!() }
// ... remaining tests
```

#### Stage 2: Implementation (Days 3-7)
- [ ] Playlist operations
- [ ] M3U generation
- [ ] Cover art support

**Validation**: All collection tests pass

## Phase 8: Integration Testing (Week 11)

### Checkpoint 8.1: Cross-Feature Tests

#### Stage 1: Port Integration Tests (Days 1-3)
Create integration tests that verify feature interactions:
```rust
// tests/integration_test.rs
#[test]
fn test_cache_update_affects_playlists() { todo!() }
#[test]
fn test_rule_execution_updates_cache() { todo!() }
#[test]
fn test_template_with_updated_metadata() { todo!() }
```

#### Stage 2: Fix Integration Issues (Days 4-7)
- [ ] Resolve any integration bugs
- [ ] Optimize cross-feature performance
- [ ] Validate data consistency

## Phase 9: CLI Implementation (Week 12-13)

### Checkpoint 9.1: Basic Commands (Week 12)

#### Stage 1: CLI Tests (Days 1-3)
```rust
// tests/cli_test.rs
#[test]
fn test_cache_update_command() { todo!() }
#[test]
fn test_release_list_command() { todo!() }
// ... test each CLI command
```

#### Stage 2: Implementation (Days 4-7)
- [ ] Clap command structure
- [ ] Command implementations
- [ ] Output formatting

### Checkpoint 9.2: Advanced CLI Features (Week 13)

#### Stage 1: Interactive Tests (Days 1-2)
- [ ] Editor integration tests
- [ ] Progress indicator tests
- [ ] Error message tests

#### Stage 2: Implementation (Days 3-7)
- [ ] Interactive features
- [ ] Shell completions
- [ ] Help system

## Phase 10: Advanced Features (Week 14-15)

### Checkpoint 10.1: Virtual Filesystem (Week 14)

#### Stage 1: FUSE Tests (Days 1-3)
- [ ] Mount/unmount tests
- [ ] File operation tests
- [ ] Performance tests

#### Stage 2: Implementation (Days 4-7)
- [ ] FUSE integration
- [ ] Dynamic path generation
- [ ] Cache integration

### Checkpoint 10.2: File Watcher (Week 15)

#### Stage 1: Watcher Tests (Days 1-2)
- [ ] File change detection tests
- [ ] Update trigger tests

#### Stage 2: Implementation (Days 3-5)
- [ ] Inotify integration
- [ ] Event handling

## Phase 11: Performance & Polish (Week 16)

### Checkpoint 11.1: Performance Optimization

- [ ] Profile against Python implementation
- [ ] Optimize hot paths
- [ ] Parallel processing improvements
- [ ] Memory usage optimization

### Checkpoint 11.2: Final Validation

- [ ] Run entire Python test suite through Rust
- [ ] Performance benchmarks
- [ ] Documentation completion
- [ ] Release preparation

## Success Criteria

### Test Coverage
- 100% of Python tests ported to Rust
- All tests passing
- No regressions during development

### Performance Metrics
- All operations faster than Python
- Memory usage reduced by 50%+
- Startup time 5x faster

### Compatibility
- Python cache files work unchanged
- Configuration files compatible
- Command-line interface identical

## Risk Mitigation

### Technical Risks
1. **Test Incompatibility**: Some Python tests may rely on Python-specific behavior
   - Mitigation: Adapt tests while preserving intent
   
2. **Library Differences**: Rust libraries may behave differently than Python
   - Mitigation: Write adapters to match Python behavior

3. **Performance Regression**: Some Rust code might be slower initially
   - Mitigation: Profile early and often

### Process Risks
1. **Test Debt**: Skipping tests to make progress
   - Mitigation: Strict TDD discipline, no exceptions

2. **Feature Creep**: Adding improvements during implementation
   - Mitigation: Save enhancements for after v1.0

## Timeline Summary

- **Weeks 1-2**: Foundation & Core Infrastructure (Common, Genre, Parser, Templates)
- **Weeks 3-4**: Configuration & Audio (Config, AudioTags)
- **Weeks 5-6**: Cache Implementation (87 tests)
- **Week 7**: Rules Engine
- **Weeks 8-9**: Entity Management (Releases, Tracks)
- **Weeks 9-10**: Collections (Collages, Playlists)
- **Week 11**: Integration Testing
- **Weeks 12-13**: CLI Implementation
- **Weeks 14-15**: Advanced Features (VFS, Watcher)
- **Week 16**: Performance & Polish

Total: 16 weeks (4 months)

## Conclusion

This TDD approach ensures that rose-rs maintains complete compatibility with rose-py while leveraging Rust's advantages. By implementing features in dependency order and validating with tests at each step, we minimize risk and ensure quality throughout the development process.