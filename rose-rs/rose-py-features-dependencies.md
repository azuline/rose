# Rose-py Features and Dependencies

## Feature Dependency Graph

This document lists all major features of rose-py and sequences them in a logical implementation order based on their dependencies.

## Implementation Phases

### Phase 1: Foundation (No Dependencies)
These features have no dependencies and form the base layer.

#### 1.1 Common Utilities
- **Module**: `common.py`
- **Description**: Base data structures and utility functions
- **Key Components**:
  - `Artist` and `ArtistMapping` data classes
  - Exception hierarchy (`RoseError`, `RoseExpectedError`, etc.)
  - Utility functions (`valid_uuid`, `sanitize_filename`, etc.)
  - Type definitions and constants
- **Dependencies**: None

#### 1.2 Genre Hierarchy
- **Module**: `genre_hierarchy.py`
- **Description**: Static genre classification and relationships
- **Key Components**:
  - `GENRES` list (24,000+ genre names)
  - `GENRE_PARENTS` mapping (genre inheritance relationships)
- **Dependencies**: None

### Phase 2: Core Infrastructure
Features that depend only on Phase 1.

#### 2.1 Rule Parser
- **Module**: `rule_parser.py`
- **Description**: DSL parser for metadata transformation rules
- **Key Components**:
  - Tokenizer for rule syntax
  - Parser to build AST
  - Matcher types (`TracksMatched`, `ReleasesMatched`)
  - Action types (`ReplaceAction`, `SedAction`, etc.)
- **Dependencies**: `common`

#### 2.2 Templates
- **Module**: `templates.py`
- **Description**: Jinja2-based path templating system
- **Key Components**:
  - Template parsing and validation
  - Custom Jinja2 filters
  - Path resolution functions
  - Template context building
- **Dependencies**: `common`

### Phase 3: Configuration & Audio
Features that build on core infrastructure.

#### 3.1 Configuration
- **Module**: `config.py`
- **Description**: Application configuration management
- **Key Components**:
  - TOML configuration parsing
  - Configuration validation
  - Default value management
  - Path resolution
  - Artist alias configuration
- **Dependencies**: `common`, `rule_parser`, `templates`

#### 3.2 Audio Tags
- **Module**: `audiotags.py`
- **Description**: Unified audio metadata interface
- **Key Components**:
  - Abstract `AudioTags` base class
  - Format-specific implementations (ID3, MP4, Vorbis, FLAC)
  - Tag reading/writing
  - Cover art extraction
  - Multi-value tag support
- **Dependencies**: `common`, `genre_hierarchy`

### Phase 4: Data Layer
The central cache system that most features depend on.

#### 4.1 Cache
- **Module**: `cache.py`, `cache.sql`
- **Description**: SQLite-based metadata cache and data access layer
- **Key Components**:
  - Database schema management
  - Connection pooling
  - CRUD operations for all entities
  - Full-text search
  - Transaction management
  - Index optimization
- **Dependencies**: `common`, `config`, `audiotags`, `templates`, `genre_hierarchy`

### Phase 5: Business Logic
Core business logic that operates on cached data.

#### 5.1 Rules Engine
- **Module**: `rules.py`
- **Description**: Executes metadata transformation rules
- **Key Components**:
  - Rule execution engine
  - Matcher evaluation
  - Action application
  - Batch processing
  - Transaction handling
- **Dependencies**: `common`, `config`, `audiotags`, `cache`, `rule_parser`

### Phase 6: Entity Management
High-level operations on music entities.

#### 6.1 Releases
- **Module**: `releases.py`
- **Description**: Album/release management operations
- **Key Components**:
  - Release creation/deletion
  - Metadata editing
  - Cover art management
  - Directory operations
  - Release templates
- **Dependencies**: `common`, `config`, `audiotags`, `cache`, `rules`, `rule_parser`, `templates`

#### 6.2 Tracks
- **Module**: `tracks.py`
- **Description**: Individual track management
- **Key Components**:
  - Track metadata editing
  - Audio file replacement
  - Track deletion
  - Loose track support
- **Dependencies**: `common`, `config`, `audiotags`, `cache`, `rules`, `rule_parser`

### Phase 7: Collections
Features for organizing releases and tracks.

#### 7.1 Collages
- **Module**: `collages.py`
- **Description**: Curated collections of releases
- **Key Components**:
  - Collage creation/deletion
  - Release addition/removal
  - Position management
  - Collage metadata
- **Dependencies**: `common`, `config`, `cache`, `releases`

#### 7.2 Playlists
- **Module**: `playlists.py`
- **Description**: Track playlists with M3U support
- **Key Components**:
  - Playlist creation/deletion
  - Track management
  - M3U file generation
  - Cover art support
  - Virtual filesystem integration
- **Dependencies**: `common`, `config`, `cache`, `collages`, `releases`, `templates`, `tracks`

### Phase 8: Frontend Interfaces
User-facing interfaces that depend on all core features.

#### 8.1 CLI (Command-Line Interface)
- **Module**: `rose-cli/`
- **Description**: Click-based command-line interface
- **Key Components**:
  - Command structure
  - Argument parsing
  - Output formatting
  - Progress indicators
  - Interactive editors
- **Dependencies**: All core modules

#### 8.2 Virtual Filesystem
- **Module**: `rose-vfs/`
- **Description**: FUSE-based virtual filesystem
- **Key Components**:
  - Directory structure generation
  - File operations
  - Dynamic path resolution
  - Symlink support
- **Dependencies**: All core modules

#### 8.3 File Watcher
- **Module**: `rose-watch/`
- **Description**: Filesystem monitoring daemon
- **Key Components**:
  - Inotify-based file watching
  - Automatic cache updates
  - Event handling
- **Dependencies**: `config`, `cache`

## Dependency Summary

```
Phase 1: Foundation
├── Common Utilities
└── Genre Hierarchy

Phase 2: Core Infrastructure  
├── Rule Parser → Common
└── Templates → Common

Phase 3: Configuration & Audio
├── Configuration → Common, Rule Parser, Templates
└── Audio Tags → Common, Genre Hierarchy

Phase 4: Data Layer
└── Cache → Common, Config, Audio Tags, Templates, Genre Hierarchy

Phase 5: Business Logic
└── Rules Engine → Common, Config, Audio Tags, Cache, Rule Parser

Phase 6: Entity Management
├── Releases → Common, Config, Audio Tags, Cache, Rules, Rule Parser, Templates
└── Tracks → Common, Config, Audio Tags, Cache, Rules, Rule Parser

Phase 7: Collections
├── Collages → Common, Config, Cache, Releases
└── Playlists → Common, Config, Cache, Collages, Releases, Templates, Tracks

Phase 8: Frontend Interfaces
├── CLI → All core modules
├── Virtual Filesystem → All core modules
└── File Watcher → Config, Cache
```

## Implementation Strategy

1. **Bottom-Up Approach**: Start with Phase 1 and work upward
2. **Feature Completeness**: Fully implement and test each feature before moving to the next
3. **API Stability**: Lock down APIs for lower phases before building higher phases
4. **Test-First**: Write tests for each feature before implementation
5. **Integration Points**: Validate integration between phases at each boundary

This ordering ensures that each feature has all its dependencies available when implementation begins, allowing for a smooth and systematic port from Python to Rust.