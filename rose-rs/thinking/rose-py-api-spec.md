# Rose-py API Specification

## Overview

Rose-py is a music library management system that provides a comprehensive API for organizing, tagging, and managing digital music collections. This document specifies the public API surface of rose-py v0.5.0.

## Core Data Types

### Artist
```python
@dataclass
class Artist:
    name: str
    alias: bool = False
```

### ArtistMapping
```python
@dataclass
class ArtistMapping:
    main: list[Artist]
    guest: list[Artist]
    remixer: list[Artist]
    producer: list[Artist]
    composer: list[Artist]
    conductor: list[Artist]
    djmixer: list[Artist]
```

### Release
Represents an album, single, EP, or other music release.

**Attributes:**
- `id`: str - Unique identifier
- `source_path`: Path - File system path
- `title`: str - Release title
- `release_type`: str - Type (album, single, ep, etc.)
- `release_year`: int | None
- `new`: bool - Whether newly added
- `artists`: ArtistMapping
- `genres`: list[str]
- `labels`: list[str]
- `tracks`: list[Track]

### Track
Represents an individual audio file.

**Attributes:**
- `id`: str - Unique identifier
- `source_path`: Path - File system path
- `title`: str - Track title
- `release_id`: str - Parent release ID
- `track_number`: str
- `disc_number`: str
- `duration_seconds`: int
- `artists`: ArtistMapping
- `audio_format`: str - File format (mp3, flac, etc.)

### Playlist
A collection of tracks.

**Attributes:**
- `name`: str
- `tracks`: list[Track]

### Collage
A curated collection of releases.

**Attributes:**
- `name`: str
- `releases`: list[Release]

## Configuration API

### Config
Central configuration object loaded from TOML files.

**Key Methods:**
```python
def parse(config_path_override: Path | None = None) -> Config
```

**Configuration Structure:**
- `music_source_dir`: Path
- `cache_dir`: Path
- `max_proc`: int
- `artist_aliases`: list[ArtistAlias]
- `rules`: list[Rule]
- `cover_art_regexes`: list[str]
- `multi_disc_toggle_flag`: str
- `path_templates`: PathTemplates

## Cache Management API

### Cache Operations

#### Update Operations
```python
def update_cache(c: Config, force: bool = False) -> None
def update_cache_for_releases(c: Config, release_dirs: list[Path], force: bool = False) -> UpdateCacheResult
def update_cache_for_tracks(c: Config, track_paths: list[Path], force: bool = False) -> None
def update_cache_for_collages(c: Config, force: bool = False) -> None
def update_cache_for_playlists(c: Config, force: bool = False) -> None
```

#### Query Operations
```python
def list_releases(c: Config, matcher: Matcher | None = None) -> Iterator[CachedRelease]
def list_tracks(c: Config, matcher: Matcher | None = None) -> Iterator[CachedTrack]
def list_playlists(c: Config) -> Iterator[str]
def list_collages(c: Config) -> Iterator[str]

def get_release(c: Config, release_id: str) -> CachedRelease | None
def get_track(c: Config, track_id: str) -> CachedTrack | None
def get_playlist(c: Config, playlist_name: str) -> CachedPlaylist | None
def get_collage(c: Config, collage_name: str) -> CachedCollage | None
```

#### Statistics
```python
def get_release_logtext(c: Config, release_id: str) -> str
def get_track_logtext(c: Config, track_id: str) -> str
```

## Content Management API

### Release Operations
```python
def create_single_release(c: Config, artist: str, title: str, track_paths: list[Path]) -> None
def delete_release(c: Config, release_id: str) -> None
def delete_release_ignore_fs(c: Config, release_id: str) -> None
def set_release_cover_art(c: Config, release_id: str, cover_path: Path | None) -> None
def extract_release_cover_art(c: Config, release_id: str, output_path: Path) -> None
def edit_release(c: Config, release_id: str) -> None
def edit_release_with_file(c: Config, release_id: str, edit_file: Path) -> None
def toggle_release_new(c: Config, release_id: str) -> None
```

### Track Operations
```python
def delete_track(c: Config, track_id: str) -> None
def delete_track_ignore_fs(c: Config, track_id: str) -> None
def edit_track(c: Config, track_id: str) -> None
def edit_track_with_file(c: Config, track_id: str, edit_file: Path) -> None
def extract_track_art(c: Config, track_id: str, output_path: Path) -> None
def set_track_audio(c: Config, track_id: str, audio_path: Path) -> None
```

### Playlist Operations
```python
def create_playlist(c: Config, playlist_name: str) -> None
def delete_playlist(c: Config, playlist_name: str) -> None
def add_track_to_playlist(c: Config, playlist_name: str, track_id: str, position: int | None = None) -> None
def delete_track_from_playlist(c: Config, playlist_name: str, track_id: str) -> None
def edit_playlist(c: Config, playlist_name: str) -> None
def edit_playlist_with_file(c: Config, playlist_name: str, edit_file: Path) -> None
def set_playlist_cover_art(c: Config, playlist_name: str, cover_path: Path | None) -> None
def extract_playlist_cover_art(c: Config, playlist_name: str, output_path: Path) -> None
```

### Collage Operations
```python
def create_collage(c: Config, collage_name: str) -> None
def delete_collage(c: Config, collage_name: str) -> None
def add_release_to_collage(c: Config, collage_name: str, release_id: str, position: int | None = None) -> None
def delete_release_from_collage(c: Config, collage_name: str, release_id: str) -> None
def edit_collage(c: Config, collage_name: str) -> None
def edit_collage_with_file(c: Config, collage_name: str, edit_file: Path) -> None
```

## Metadata and Tagging API

### AudioTags
Abstract base class for audio metadata handling.

```python
class AudioTags(ABC):
    @classmethod
    def supports_file(cls, p: Path) -> bool
    
    @abstractmethod
    def dump(self) -> dict[str, Any]
    
    @abstractmethod
    def title(self) -> str | None
    @abstractmethod
    def album(self) -> str | None
    @abstractmethod
    def artist(self) -> ArtistMapping | None
    @abstractmethod
    def date(self) -> int | None
    @abstractmethod
    def track_number(self) -> int | None
    @abstractmethod
    def disc_number(self) -> int | None
    @abstractmethod
    def duration_seconds(self) -> int | None
    
    @classmethod
    @abstractmethod
    def from_file(cls, p: Path) -> AudioTags
    
    @abstractmethod
    def flush(self, p: Path) -> None
```

### Tag Reading/Writing
```python
def read_tags(p: Path) -> RoseAudioTags
def write_tags(c: Config, release_id: str) -> None
def write_release_log_templates(c: Config, release_id: str) -> None
```

## Rule Engine API

### Rule Parser
```python
class Rule:
    matcher: Matcher
    actions: list[Action]
    
def parse_rule(raw: str) -> Rule
```

### Matcher Types
- `TrackMatcher`: Match tracks based on criteria
- `ReleaseMatcher`: Match releases based on criteria
- `BooleanMatcher`: Combine matchers with boolean logic

### Action Types
- `ReplaceAction`: Replace tag values
- `SedAction`: Regex-based tag modification
- `SplitAction`: Split tag values
- `AddAction`: Add new tag values
- `DeleteAction`: Remove tag values

### Rule Execution
```python
def execute_rule(c: Config, rule: Rule) -> None
def fast_search_for_matching_releases(c: Config, matcher: Matcher) -> list[str]
def fast_search_for_matching_tracks(c: Config, matcher: Matcher) -> list[str]
```

## Path Templating API

```python
def resolve_release_template(c: Config, release: CachedRelease, track: CachedTrack | None = None) -> Path
def resolve_track_template(c: Config, track: CachedTrack) -> Path
def resolve_all_patterns(c: Config, release: CachedRelease, track: CachedTrack | None = None) -> Path
```

## File System Operations

### Virtual Filesystem
```python
def mount_virtualfs(c: Config, mount_dir: Path) -> None
```

### File Operations
```python
def dump_releases(c: Config, matcher: Matcher, output_path: Path) -> None
def dump_tracks(c: Config, matcher: Matcher, output_path: Path) -> None
```

## Utility Functions

### Locking
```python
def lock(c: Config, name: str) -> FileLock
```

### Genre Hierarchy
```python
GENRES: list[str]  # All known genres
GENRE_PARENTS: dict[str, list[str]]  # Genre parent relationships
```

### Path Helpers
```python
def musicfile(p: Path) -> bool  # Check if path is a music file
def valid_uuid(x: str) -> bool  # Validate UUID format
```

## Error Handling

### Custom Exceptions
```python
class RoseError(Exception): pass
class RoseExpectedError(RoseError): pass
class ConfigNotFoundError(RoseExpectedError): pass
class ConfigDecodeError(RoseExpectedError): pass
class InvalidPathTemplateError(RoseExpectedError): pass
class UnsupportedAudioFormatError(RoseExpectedError): pass
class TagNotAllowedError(RoseExpectedError): pass
class UnknownArtistRoleError(RoseExpectedError): pass
```

## Version Information

```python
__version__ = "0.5.0"
```

## Thread Safety

Most operations are thread-safe through file-based locking. The cache uses SQLite with appropriate transaction handling. Concurrent operations on different releases/tracks are safe.

## Performance Considerations

- Cache operations use SQLite with optimized indexes
- Batch operations available for bulk updates
- Lazy loading for large result sets
- Full-text search capabilities for fast queries