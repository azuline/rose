# Rose-py Architecture Document

## Overview

Rose-py is a sophisticated music library management system designed with performance, flexibility, and maintainability at its core. This document describes the architectural decisions, design patterns, and system structure of rose-py v0.5.0.

## System Architecture

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     User Interface Layer                     │
│                  (CLI Commands / Python API)                 │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│                    Application Logic Layer                   │
│         (Business Logic, Rules Engine, Templates)           │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│                      Cache Layer (SQLite)                    │
│              (Indexes, FTS, Optimized Queries)              │
└─────────────────────┬───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│                    Data Access Layer                         │
│              (Audio Tags, File System, Metadata)            │
└─────────────────────────────────────────────────────────────┘
```

### Core Design Principles

1. **Cache-First Architecture**: All operations go through a SQLite cache for performance
2. **Immutable IDs**: UUIDs ensure stable references across file moves/renames
3. **Flexible Metadata**: Support for complex artist roles and relationships
4. **Rule-Based Operations**: Declarative rules for bulk metadata management
5. **Type Safety**: Full type annotations with strict mypy checking
6. **Error Resilience**: Comprehensive error handling with graceful degradation

## Component Architecture

### 1. Configuration System (`config.py`)

The configuration system provides a centralized way to manage all application settings.

**Key Features:**
- TOML-based configuration with schema validation
- Hierarchical configuration search (user → system)
- Type-safe configuration objects
- Environment variable support
- Default values with override capability

**Architecture:**
```
Config
├── PathTemplates
├── ArtistAliases
├── Rules
├── CoverArtPatterns
└── SystemSettings
```

### 2. Cache System (`cache.py`, `cache.sql`)

The cache is the heart of Rose's performance strategy, using SQLite as a high-performance metadata store.

**Design Decisions:**
- **SQLite for ACID guarantees**: Ensures data consistency
- **Normalized schema**: Efficient storage and queries
- **Full-text search**: Fast content searching
- **Strategic indexes**: Optimized for common query patterns
- **Write-ahead logging**: Better concurrent performance

**Schema Design:**
```
releases
├── tracks (1:N)
├── releases_artists (M:N)
├── releases_genres (M:N)
├── releases_labels (M:N)
└── release_sources (1:1)

tracks
├── tracks_artists (M:N)
└── track_sources (1:1)

playlists
└── playlists_tracks (M:N)

collages
└── collages_releases (M:N)
```

**Performance Optimizations:**
- Indexes on all foreign keys
- Compound indexes for common join patterns
- FTS5 tables for text search
- Prepared statements for repeated queries
- Connection pooling per thread

### 3. Audio Metadata Abstraction (`audiotags.py`)

A unified interface for handling different audio formats while preserving format-specific capabilities.

**Architecture Pattern**: Abstract Factory
```
AudioTags (ABC)
├── ID3Tags (MP3)
├── MP4Tags (M4A/MP4)
├── VorbisTags (OGG/OPUS)
└── FLACTags (FLAC)
```

**Design Benefits:**
- Format-agnostic API
- Extensible for new formats
- Preserves format-specific features
- Consistent error handling

### 4. Rule Engine (`rule_parser.py`, `rules.py`)

A domain-specific language for bulk metadata operations with a two-phase execution model.

**Architecture:**
```
Rule Definition (DSL)
    ↓
Parser (Tokenizer → AST)
    ↓
Matcher (Query Generation)
    ↓
Actions (Metadata Updates)
```

**Execution Flow:**
1. Parse rule into AST
2. Generate optimized SQL queries from matchers
3. Execute actions on matched items
4. Update cache and flush to disk

**Design Decisions:**
- **Two-phase execution**: Fast matching via SQL, then detailed processing
- **Composable matchers**: Boolean operations for complex queries
- **Atomic actions**: All-or-nothing updates
- **Batch processing**: Efficient for large collections

### 5. Template System (`templates.py`)

Jinja2-based path generation for flexible file organization.

**Features:**
- Context-aware templates (release vs track)
- Safe path generation (no directory traversal)
- Custom filters for common operations
- Validation and error handling

**Template Context:**
```
Release Context:
- All release metadata
- Artist formatting helpers
- Genre/label lists
- Date/year formatting

Track Context:
- Inherits release context
- Track-specific metadata
- Disc/track number formatting
```

### 6. Virtual Filesystem (`virtualfs.py`)

FUSE-based virtual filesystem for transparent access to organized music.

**Architecture:**
- Read-only filesystem
- Dynamic path resolution
- Cache-backed metadata
- Lazy loading

**Design Benefits:**
- No file duplication
- Real-time organization changes
- Compatible with any music player
- Zero maintenance overhead

## Data Flow Architecture

### Update Flow
```
1. File System Scan
   ↓
2. Audio Tag Reading
   ↓
3. Metadata Extraction
   ↓
4. Cache Update (Transaction)
   ↓
5. Index Updates
```

### Query Flow
```
1. User Query
   ↓
2. SQL Generation
   ↓
3. Cache Query
   ↓
4. Result Hydration
   ↓
5. Response Formatting
```

### Edit Flow
```
1. Load Current State
   ↓
2. User Edits (TOML)
   ↓
3. Validation
   ↓
4. Diff Computation
   ↓
5. Cache Update
   ↓
6. Tag Writing
```

## Concurrency Model

### Locking Strategy
- **File-based locks**: For cross-process synchronization
- **Lock hierarchy**: Prevents deadlocks
- **Granular locking**: Release-level for maximum concurrency
- **Read-write separation**: Multiple readers, exclusive writers

### Thread Safety
- **Thread-local storage**: For database connections
- **Immutable data structures**: Where possible
- **Explicit synchronization**: For shared state

## Error Handling Architecture

### Exception Hierarchy
```
RoseError
├── RoseExpectedError (User-facing errors)
│   ├── ConfigNotFoundError
│   ├── InvalidPathTemplateError
│   └── UnsupportedAudioFormatError
└── RoseUnexpectedError (Internal errors)
```

### Error Handling Strategy
1. **Fail-fast for corruption**: Prevent data loss
2. **Graceful degradation**: Continue with partial results
3. **Detailed logging**: For debugging
4. **User-friendly messages**: For expected errors

## Extension Points

### 1. Audio Format Support
Add new format by implementing `AudioTags` interface

### 2. Rule Actions
Extend rule engine with custom actions

### 3. Template Functions
Add Jinja2 filters/functions for path generation

### 4. Cache Modules
Additional tables/indexes for new features

## Performance Architecture

### Optimization Strategies
1. **Batch Operations**: Minimize I/O overhead
2. **Lazy Loading**: Load data only when needed
3. **Caching**: Multiple levels (SQL, Python objects)
4. **Index Design**: Optimized for query patterns
5. **Connection Pooling**: Reuse database connections

### Scalability Considerations
- **Horizontal**: Multiple Rose instances can share cache
- **Vertical**: Efficient memory usage, streaming APIs
- **Large Collections**: Tested with 100k+ tracks

## Security Architecture

### Security Measures
1. **Path Validation**: Prevent directory traversal
2. **SQL Injection Prevention**: Parameterized queries
3. **File Permissions**: Respect system permissions
4. **Safe File Operations**: Use trash instead of delete
5. **Input Sanitization**: For all user inputs

## Testing Architecture

### Test Strategy
1. **Unit Tests**: For individual components
2. **Integration Tests**: For component interactions
3. **Property Tests**: For parser/rule engine
4. **Performance Tests**: For scalability validation
5. **Snapshot Tests**: For complex outputs

### Test Infrastructure
- **Fixtures**: Reusable test data
- **Mocks**: For file system/network
- **Coverage**: 100% target for critical paths

## Deployment Architecture

### Distribution
- **PyPI Package**: Standard Python packaging
- **Single Binary**: Via PyOxidizer (planned)
- **Docker**: Containerized deployment

### Dependencies
- **Required**: mutagen, click, jinja2, llfuse
- **Optional**: For extended features
- **Version Pinning**: For reproducibility
