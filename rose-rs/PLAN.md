# Rose Rust Port Implementation Plan

## Overview
This document outlines the plan for completing the Rust port of the Rose music library management system. The porting strategy follows a bottom-up approach based on module dependencies, starting with foundational modules and building up to higher-level functionality.

Our approach is a test driven development approach. We want to port over all the tests from rose-py and then make sure that they are all implemented effectively.

## Current Status (Updated: 2025-07-16)

### ✅ Completed Modules (100% Feature Parity)
1. **common.rs** - Core utilities, error types, and basic data structures
2. **genre_hierarchy.rs** - Genre relationship data and lookups
3. **testing.rs** - Test utilities and helpers
4. **config.rs** - Configuration parsing with full test coverage
5. **templates.rs** - Path templating system with tera integration
6. **rule_parser.rs** - Rules DSL parser with comprehensive parsing logic

### ⚠️ Partially Completed (Limited Feature Parity)
1. **audiotags.rs** - Audio file metadata reading/writing
   - ✅ Basic tag reading/writing for standard tags
   - ✅ Multi-value tag support
   - ✅ Artist role parsing
   - ❌ Custom tag writing (lofty v0.22+ limitation - see DEBT.md)
   - Tests: 13/26 passing (13 ignored due to lofty limitations)

2. **cache.rs** - SQLite database layer  
   - ✅ Basic database connection and schema
   - ✅ Eviction functions (collages, playlists, releases)
   - ✅ get_track, list_tracks, list_tracks_with_filter
   - ✅ list_collages, list_playlists
   - ✅ list_descriptors, list_labels
   - ✅ artist_exists, genre_exists, descriptor_exists, label_exists
   - ✅ update_cache_for_releases with track handling
   - ✅ update_cache_for_collages with TOML parsing
   - ✅ update_cache_for_playlists with TOML parsing
   - ✅ Full cache update logic (update_cache function)
   - ✅ Helper functions for stored data files
   - ❌ Full-text search update functions
   - ❌ File renaming logic (rename_source_files)
   - Tests: 14/66 implemented (3 more passing)

### ❌ Not Started
1. **rules.rs** - Rules execution engine
2. **releases.rs** - Release management
3. **tracks.rs** - Track management
4. **collages.rs** - Collection management
5. **playlists.rs** - Playlist management

## Module Dependency Graph

```
Layer 0 (No dependencies):
├── common.rs ✅
└── genre_hierarchy.rs ✅

Layer 1:
├── audiotags.rs (→ common, genre_hierarchy)
├── rule_parser.rs (→ common)
└── templates.rs 🚧 (→ common, audiotags)

Layer 2:
└── config.rs 🚧 (→ common, rule_parser, templates)

Layer 3:
└── cache.rs (→ audiotags, common, config, genre_hierarchy, templates)

Layer 4:
└── rules.rs (→ audiotags, cache, common, config, rule_parser)

Layer 5:
├── releases.rs (→ audiotags, cache, common, config, rule_parser, rules, templates)
└── tracks.rs (→ audiotags, cache, common, config, rule_parser, rules)

Layer 6:
└── collages.rs (→ cache, common, config, releases)

Layer 7:
└── playlists.rs (→ cache, collages, common, config, releases, templates, tracks)
```

## Implementation Order

### Phase 1: Complete In-Progress Modules (High Priority)
1. **rule_parser.rs**
   - Port the DSL parser for rules engine
   - Implement Pattern, Matcher, Action types
   - Port all action types (Replace, Sed, Split, Add, Delete)
   - Implement parsing logic with proper error handling
   - Translate comprehensive test suite

2. **config.rs**
   - Complete translation of VirtualFSConfig
   - Implement Config struct with all parsing logic
   - Port validation and error handling
   - Translate tests

3. **templates.rs**
   - Set up tera templating engine integration
   - Implement PathTemplate and PathTemplateTriad
   - Port template evaluation functions
   - Implement custom filters (arrayfmt, artistsfmt, etc.)
   - Translate tests

### Phase 2: Foundation Layer (High Priority)
4. **audiotags.rs**
   - Integrate lofty crate for audio metadata
   - Implement tag reading/writing interfaces
   - Port genre hierarchy integration
   - Handle various audio formats

### Phase 3: Data Layer (High Priority)
5. **cache.rs**
   - Set up rusqlite integration
   - Implement database schema from cache.sql
   - Port all database operations
   - Implement caching logic and update mechanisms
   - Handle concurrent access patterns

### Phase 4: Business Logic (Medium Priority)
6. **rules.rs**
   - Implement rule execution engine
   - Port matcher/action execution logic
   - Integrate with cache for metadata updates

7. **releases.rs**
   - Port release management functionality
   - Implement file system operations
   - Handle release metadata and organization

8. **tracks.rs**
   - Port track management logic
   - Implement track-specific operations

### Phase 5: Collections (Medium/Low Priority)
9. **collages.rs**
   - Port collection management
   - Implement collage file handling

10. **playlists.rs**
   - Port playlist functionality
   - Implement M3U generation and management

## Key Technical Considerations

### Error Handling
- Use `thiserror` with enum-based errors (RoseError, RoseExpectedError)
- Maintain consistent error propagation using project's Result<T> type
- Preserve Python's error message clarity

### Dependencies
- **Templating**: tera (Jinja2-like syntax)
- **Audio**: lofty for metadata operations (⚠️ v0.22+ has limitations - see below)
- **Database**: rusqlite with bundled SQLite
- **Serialization**: serde with JSON/TOML support
- **Logging**: tracing (not log/log4rs)

### Known Limitations

#### Lofty v0.22+ Custom Tag Writing
The lofty crate (v0.22+) cannot write custom/unknown tags to Vorbis comment-based formats (FLAC, Ogg Vorbis, Opus). This is a breaking change from earlier versions and affects:
- Rose IDs (roseid/rosereleaseid)
- Release type
- Composition date
- Secondary genres
- Descriptors
- Edition

These tags can be read if they exist (created by other tools) but cannot be written. This impacts full feature parity with the Python implementation. See DEBT.md for detailed analysis.

### Translation Guidelines
1. **DO NOT DELETE PYTHON CODE** - Keep original Python code as comments in the Rust files until fully translated
   - **CRITICAL**: Never move Python code to separate files - always keep it commented in the same file being translated
   - If a Python function is too large, add a reference comment with line numbers to cache_py.rs
   - Comment out Python code using `//` line comments to ensure it doesn't interfere with Rust compilation
2. Preserve Python docstrings as Rust doc comments
3. Maintain same function names and signatures where possible
4. Use idiomatic Rust patterns:
   - Iterators instead of loops where appropriate
   - Pattern matching for control flow
   - Option/Result for nullable/fallible operations
5. Only modify control flow for borrow checker compliance
6. Keep data structures similar but use Rust idioms:
   - Vec instead of list
   - HashMap instead of dict
   - PathBuf instead of Path strings
7. Maintain exact same behavior and testing behavior
8. Do not modify control flow or guarantees under test

### Testing Strategy
1. Port all Python tests to Rust
2. Ensure test coverage remains comprehensive
3. Add Rust-specific tests for ownership/borrowing edge cases
4. Use the testing utilities in testing.rs

## Lessons Learned

1. **Test-Driven Porting Works Well**: Porting tests first helps catch subtle behavioral differences
2. **Library Limitations Are Critical**: The lofty v0.22+ limitation on custom tags was unexpected and impacts core functionality
3. **Type System Differences**: Rust's type system requires careful handling of:
   - Arc<T> for shared ownership (e.g., Release objects in cache)
   - Result type aliases vs std::result::Result in closures
   - Lifetime management in database queries
4. **Incremental Progress**: Even with limitations, we can make progress by:
   - Marking failing tests as ignored with clear reasons
   - Documenting technical debt (DEBT.md)
   - Implementing what's possible while tracking what's blocked

## Next Steps

### Immediate Priority
1. Complete remaining cache.rs functionality:
   - Full-text search update functions
   - File renaming logic (rename_source_files)
   - Remaining test implementations (52 tests to go)
   - Add cover art functionality

### Medium Priority
2. Implement rules.rs for metadata operations
3. Implement releases.rs for release management
4. Implement tracks.rs for track operations

### Long Term
5. Research alternatives to lofty for custom tag support:
   - Consider using mutagen bindings
   - Evaluate other audio metadata libraries
   - Potentially contribute custom tag support to lofty

### Blocked/Deferred
- Full audiotags.rs parity (blocked by lofty limitations)
- ID assignment functionality
- Custom metadata persistence

## Success Criteria

### Achieved
- ✅ Core modules (common, config, templates, rule_parser) fully ported
- ✅ Clean, idiomatic Rust code following project conventions
- ✅ Test framework established

### In Progress
- 🚧 Database layer implementation (cache.rs)
- 🚧 Audio metadata handling (limited by lofty)

### Not Yet Achieved
- ❌ All modules successfully ported with tests passing
- ❌ Full feature parity (blocked by lofty custom tag limitation)
- ❌ CLI interface implementation
- ❌ Performance benchmarking

## Module Implementation Status Summary

| Module | Lines of Code | Tests | Status | Notes |
|--------|---------------|-------|---------|-------|
| common.rs | ~200 | ✅ | 100% | Fully implemented |
| config.rs | ~400 | ✅ | 100% | Fully implemented |
| templates.rs | ~300 | ✅ | 100% | Fully implemented |
| rule_parser.rs | ~600 | ✅ | 100% | Fully implemented |
| genre_hierarchy.rs | ~100 | ✅ | 100% | Data module |
| audiotags.rs | ~900 | 13/26 | 70% | Limited by lofty |
| cache.rs | ~2100 | 14/66 | 60% | Major update functions implemented, track/release updates working |
| rules.rs | 0 | 0 | 0% | Not started |
| releases.rs | 0 | 0 | 0% | Not started |
| tracks.rs | 0 | 0 | 0% | Not started |
| collages.rs | 0 | 0 | 0% | Not started |
| playlists.rs | 0 | 0 | 0% | Not started |
