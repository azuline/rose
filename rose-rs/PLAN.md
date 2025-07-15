# Rose Rust Port Implementation Plan

## Overview
This document outlines the plan for completing the Rust port of the Rose music library management system. The porting strategy follows a bottom-up approach based on module dependencies, starting with foundational modules and building up to higher-level functionality.

## Current Status

### ✅ Completed Modules
1. **common.rs** - Core utilities, error types, and basic data structures
2. **genre_hierarchy.rs** - Genre relationship data and lookups
3. **testing.rs** - Test utilities and helpers

### 🚧 In Progress
1. **config.rs** - Configuration parsing (Python code present, needs translation)
2. **templates.rs** - Path templating system (Python code present, needs translation)

### ❌ Not Started (Python code present)
1. **rule_parser.rs** - Rules DSL parser (Python code present, needs translation)
2. **audiotags.rs** - Audio file metadata reading/writing (Placeholder implementation only - needs full implementation with lofty crate and all tests from audiotags_test.py)
2. **cache.rs** - SQLite database layer
3. **rules.rs** - Rules execution engine
4. **releases.rs** - Release management
5. **tracks.rs** - Track management
6. **collages.rs** - Collection management
7. **playlists.rs** - Playlist management

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
- **Audio**: lofty for metadata operations
- **Database**: rusqlite with bundled SQLite
- **Serialization**: serde with JSON/TOML support
- **Logging**: tracing (not log/log4rs)

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

## Next Steps

1. Port rule_parser.rs module  
2. Complete config.rs translation
3. Complete templates.rs translation  
4. Run tests to ensure correctness
5. Begin audiotags.rs implementation
6. Continue with cache.rs as the core data layer

## Success Criteria

- All modules successfully ported with tests passing
- Performance equal to or better than Python version
- Maintains same CLI interface and functionality
- Preserves all features and behaviors
- Clean, idiomatic Rust code following project conventions
