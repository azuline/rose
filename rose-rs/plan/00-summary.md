# Rose-RS Implementation Summary

## Project Overview
Rose is a music library manager that uses SQLite as its source of truth, with audio files and metadata synchronized from the database. This is a complete Rust rewrite of the Python implementation, maintaining 100% behavioral compatibility.

## Architecture Principles
1. **Cache-First**: SQLite database is the source of truth, files are updated from it
2. **Transactional**: All operations use database transactions for consistency
3. **Incremental**: Only changed files are rescanned based on mtime
4. **Cross-Platform**: Supports Linux, macOS, and Windows with platform-specific behaviors
5. **Test-Driven**: Each module has comprehensive tests matching or exceeding Python coverage

## Implementation Order
The milestones are ordered by dependency - each builds on the previous:

1. **Foundation**: Common types, errors, and utilities
2. **Genre System**: Hierarchical genre classification
3. **Configuration**: TOML-based config with validation
4. **Templates**: Path generation engine
5. **Datafiles**: Release metadata persistence
6. **Rule Parser**: DSL for metadata transformations  
7. **Audio Tags**: Unified metadata interface
8. **Cache Core**: SQLite database and operations
9. **Rules Engine**: Execute parsed rules
10. **Releases**: High-level release operations
11. **Tracks**: Individual track management
12. **Collages**: Virtual collections
13. **Playlists**: M3U8 generation

## Critical Behaviors to Preserve
- Track totals are calculated by counting tracks per disc, never stored
- All deletions use trash, never permanent delete
- File operations retry on lock failures with exponential backoff
- Datafiles auto-upgrade when fields are missing
- In-progress directories (`.in-progress.*`) are skipped
- Empty strings in tags are treated as None
- Artist aliases are display-only, never written to files

## Testing Strategy
- Port ALL Python tests as a baseline
- Add Rust-specific tests for ownership/borrowing edge cases
- Integration tests between modules
- Platform-specific behavior tests
- Performance benchmarks vs Python implementation

## Validation
Success criteria:
1. Can read/write Python-created cache and datafiles
2. Produces identical output for all operations
3. Passes all ported Python tests
4. No regressions in performance
5. Cross-platform compatibility verified

## Getting Started
1. Read the Python source for implementation details
2. Start with Milestone 1 (foundation)
3. Use existing Python tests as specification. Your logic should attempt to match the control flow
   of the Python as MUCH as possible.
4. Run against Python test data for validation
5. Each milestone should be a complete, tested unit
