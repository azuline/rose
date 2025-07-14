# Critique of Consultant's Rose-RS Implementation Plan

## Overview
After reviewing the consultant's plan against the actual rose-py implementation, I've identified several critical gaps, unnecessary implementation details, and missing behaviors that must be addressed.

## Major Missing Components

### 1. Datafile Management System
The plan completely omits the datafile system that's central to Rose's architecture:
- `.rose.{uuid}.toml` files store release metadata
- Automatic creation, updating, and migration
- Field presence tracking for upgrades
- This is NOT optional - it's how Rose persists non-tag metadata

### 2. Collages and Playlists Modules
Two entire modules are missing from the plan:
- `collages.py` - Virtual collections of releases
- `playlists.py` - M3U8 playlist generation
- Both have extensive test suites that need porting

### 3. Cross-Cutting Concerns
Several behaviors that affect multiple modules are not mentioned:
- File locking with active retry (not just timeout)
- Trash usage instead of deletion (send2trash)
- In-progress directory detection (`.in-progress.*)
- Platform-specific path handling differences

### 4. Critical Cache Behaviors
The cache plan misses several essential behaviors:
- Track totals are CALCULATED, not stored
- Multiprocessing threshold (50 releases) for stability
- Lock retry logic with exponential backoff
- Automatic datafile field upgrades

## Excessive Implementation Details

The plans contain too many implementation details that should be left to the programmer:
- Exact struct field names and types
- Specific error variant names
- Detailed builder patterns
- Step-by-step parsing algorithms
- Exact test assertion values

These details make the plan brittle and prevent the programmer from making better design decisions based on Rust idioms.

## Missing Test Coverage

Several important test scenarios are not mentioned:
- Unicode handling in filenames and metadata
- Concurrent access and locking
- Platform-specific path behaviors
- Empty/missing field handling
- Malformed input recovery

## Missing Behaviors by Module

### Audio Tags (Milestone 6)
- Multiple track_number formats (`01`, `01/12`, `1`)
- Case variations in tag names
- Empty vs missing field distinction
- Format-specific field mappings

### Rules Engine (Milestone 8)
- Multi-value field handling nuances
- Case-sensitive vs insensitive matching per field
- Integer field string conversion
- Playlist regeneration triggers

### Configuration (Milestone 3)
- Automatic directory creation
- Migration from old config formats
- Environment variable expansion
- Platform-specific default paths

## Recommendations

1. **Add Missing Modules**: Create milestones for collages, playlists, and datafiles
2. **Remove Implementation Details**: Focus on behaviors and test cases, not code structure
3. **Add Integration Tests**: Each milestone should include integration scenarios
4. **Document Platform Differences**: Explicitly call out Windows/macOS/Linux variations
5. **Include Migration Path**: How to validate against Python implementation

## Test Count Validation

The consultant's test counts seem arbitrary. Based on the Python implementation:
- common: 23 tests (consultant: 12)
- config: 37 tests (consultant: 15)
- cache: 89 tests (consultant: 35)
- rules: 45 tests (consultant: 25)

The Rust implementation should have AT LEAST as many tests as Python, likely more due to Rust's stricter type system requiring more edge case handling.