# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust port of the Rose music library management system. The codebase is being translated from Python to Rust while maintaining the same data structures and control flow patterns where possible, only making changes necessary to satisfy Rust's borrow checker and type system.

## Build and Development Commands

```bash
# Build the project
make build

# Run type checking
make typecheck

# Run tests
make test

# Run a single test
cargo test test_name

# Check formatting and linting
make lintcheck

# Auto-fix formatting and linting issues
make lint

# Clean build artifacts
make clean

# Run all checks (typecheck, test, lintcheck)
make check
```

## Architecture and Code Structure

### Core Dependencies
- **Error Handling**: Uses `thiserror` with enum-based errors (`RoseError` and `RoseExpectedError`). Use the project's `Result<T>` type alias from common.rs
- **Logging**: Uses `tracing` and `tracing-subscriber` (not log/log4rs)
- **Serialization**: `serde` with JSON and TOML support
- **Database**: SQLite via `rusqlite` with bundled SQLite
- **Audio**: `lofty` for audio file metadata
- **Templates**: `tera` for templating engine

### Database Schema
The project uses SQLite with a schema defined in `src/cache.sql`.
- `releases` - Music releases with metadata
- `tracks` - Individual tracks
- `releases_artists`, `tracks_artists` - Artist relationships
- `playlists`, `collages` - Collections
- Full-text search tables for efficient querying

### Translation Guidelines
- Preserve Python docstrings as Rust doc comments
- Maintain the same function names and signatures where possible
- Use idiomatic Rust patterns (iterators, pattern matching, etc.)
- Keep data structures similar but use Rust idioms (Vec instead of list, HashMap instead of dict)
- Only modify control flow when necessary for the borrow checker
- Prefer concise Rust code without sacrificing clarity

### Key Patterns
- Use `once_cell::sync::Lazy` for lazy static initialization
- Use enum-based errors with `#[derive(Error)]` from thiserror
- Use the project's `Result<T>` type alias (defined in common.rs) for error propagation
- Return `RoseExpectedError` for user-facing errors (shown without traceback)
- Return `RoseError` for internal/system errors
- Preserve Unicode normalization for filesystem operations
- Use SHA256 hashing for cache keys and data integrity

# Conventions

- All log lines should be in lowercase.
